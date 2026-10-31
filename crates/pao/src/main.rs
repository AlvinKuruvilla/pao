use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use pao_core::entry::ModuleEntry;
use pao_core::locate::{Located, locate};
use pao_core::names;
use pao_core::flags;
use pao_core::version::{BunVersion, find_version};

/// Inspect and extract Bun single-file executables.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the embedded module table without writing anything.
    Inspect {
        /// The executable produced by `bun build --compile`.
        binary: PathBuf,
    },
    /// Write every embedded module to a directory.
    Extract {
        /// The executable produced by `bun build --compile`.
        binary: PathBuf,
        /// Directory to write modules into.
        #[arg(short, long, default_value = "extracted")]
        output: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Inspect { binary } => {
            let bytes =
                fs::read(&binary).with_context(|| format!("read {}", binary.display()))?;
            let located = locate(&bytes)
                .with_context(|| format!("find the Bun payload in {}", binary.display()))?;
            let version = find_version(&bytes, &located)
                .with_context(|| format!("identify the Bun release that built {}", binary.display()))?;
            // Locked and buffered: an unbuffered write per row is slow on
            // tables with thousands of modules.
            let mut out = BufWriter::new(io::stdout().lock());
            match print_inspection(&mut out, &located, version).and_then(|()| out.flush()) {
                // The reader closed the pipe (`pao inspect app | head`); it
                // already has all it wanted.
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
                result => result.context("write the module table to stdout"),
            }
        }
        // TODO(alvin): decide path safety for module names and whether to
        // decompress `.zst` modules, then write each module under `output`.
        Command::Extract { .. } => bail!("`pao extract` is not implemented yet"),
    }
}

/// Writes a summary of the payload, then one row per module in table order.
///
/// Rows follow binwalk's shape: the file offset where the module's contents
/// start, in decimal and hex, then a description. A description lists only
/// the fields the layout has, so one wording never has to cover two meanings,
/// such as a bytecode length that is a real size in one build and runs to the
/// end of a shared payload in another.
fn print_inspection(
    out: &mut impl Write,
    located: &Located<'_>,
    version: BunVersion<'_>,
) -> io::Result<()> {
    let Located { layout, offsets, payload, base, trailer_pos, entries } = located;
    let name_of = |entry: &ModuleEntry| {
        let bytes = payload.get(entry.name).expect("locate checked every pointer");
        std::str::from_utf8(bytes).expect("locate checked that names are UTF-8")
    };
    let entry_point = offsets.entry_point_id as usize;
    let linked = offsets
        .flags
        .is_some_and(|raw| flags::is_set(raw, flags::HAS_LINKED_BYTECODE_PAYLOAD, &version));

    // Summary. File offsets are hex so they can be pasted into a hex editor.
    let payload_end = *base as u64 + offsets.byte_count;
    writeln!(out, "bun          {version}")?;
    writeln!(out, "layout       {layout:?}")?;
    writeln!(out, "payload      {base:#x}..{payload_end:#x}, {} bytes", grouped(offsets.byte_count))?;
    writeln!(out, "trailer      {trailer_pos:#x}")?;
    // Before 1.3.3 these bytes are padding, which is zero in every build
    // tested; the line is left out then, and shown if the padding is not zero.
    if let Some(raw) = offsets.flags
        && (raw != 0 || flags::defined(&version) != 0)
    {
        let mut parts: Vec<String> = flags::names(raw, &version).map(str::to_string).collect();
        let undefined = raw & !flags::defined(&version);
        if undefined != 0 {
            parts.push(format!("{undefined:#x} not defined in Bun {version}"));
        }
        let parts = if parts.is_empty() { "none set".to_string() } else { parts.join(", ") };
        writeln!(out, "flags        {raw:#x}: {parts}")?;
    }
    if linked {
        writeln!(out, "bytecode     linked: modules share one payload, so rows give each entry, not a size")?;
    }
    writeln!(
        out,
        "modules      {}, entry point #{entry_point} {}",
        grouped(entries.len() as u64),
        name_of(&entries[entry_point])
    )?;
    writeln!(out)?;

    writeln!(out, "{:<13}{:<14}DESCRIPTION", "DECIMAL", "HEXADECIMAL")?;
    writeln!(out, "{}", "-".repeat(78))?;
    for (index, entry) in entries.iter().enumerate() {
        let offset = base + entry.contents.offset as usize;
        let marker = if index == entry_point { "* " } else { "" };
        writeln!(out, "{offset:<13}{offset:<#14x}{marker}#{index} {}", name_of(entry))?;
        writeln!(out, "{:27}{}", "", describe(entry, &version, *base, linked))?;
    }
    writeln!(out)?;
    writeln!(out, "* entry point")
}

/// `js, esm, latin1, server, 1,244 bytes, bytecode 688 bytes`: the entry's
/// fields named for `version`, then its sizes.
fn describe(entry: &ModuleEntry, version: &BunVersion<'_>, base: usize, linked: bool) -> String {
    // An undefined value is shown as a number, never given a name borrowed
    // from another release.
    let undefined = |field: &str, value: u8| format!("{field} {value} (not defined in Bun {version})");
    let mut parts = vec![
        names::loader(entry.loader, version).map_or_else(|| undefined("loader", entry.loader), str::to_string),
    ];
    if let Some(value) = entry.module_format {
        parts.push(match names::module_format(value) {
            Some("none") => "no module format".to_string(),
            Some(name) => name.to_string(),
            None => undefined("module format", value),
        });
    }
    if let Some(value) = entry.encoding {
        parts.push(names::encoding(value, version).map_or_else(|| undefined("encoding", value), str::to_string));
    }
    if let Some(value) = entry.side {
        parts.push(names::side(value).map_or_else(|| undefined("side", value), str::to_string));
    }

    parts.push(format!("{} bytes", grouped(entry.contents.length.into())));
    // Bun writes an empty pointer for "none".
    if entry.sourcemap.length > 0 {
        parts.push(format!("source map {} bytes", grouped(entry.sourcemap.length.into())));
    }
    match entry.bytecode {
        Some(bytecode) if bytecode.length > 0 && linked => {
            parts.push(format!("bytecode entry at {:#x}", base + bytecode.offset as usize));
        }
        Some(bytecode) if bytecode.length > 0 => {
            parts.push(format!("bytecode {} bytes", grouped(bytecode.length.into())));
        }
        _ => {}
    }
    if let Some(module_info) = entry.module_info
        && module_info.length > 0
    {
        parts.push(format!("module info {} bytes", grouped(module_info.length.into())));
    }
    parts.join(", ")
}

/// `182024748` as `182,024,748`.
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
