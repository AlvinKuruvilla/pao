use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use pao_core::entry::ModuleEntry;
use pao_core::locate::{Located, locate};
use pao_core::string_pointer::StringPointer;
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

/// Writes a summary of the payload, then one row per module.
///
/// Numbers are left raw: sizes in bytes and enum fields as Bun stores them.
/// The loader's names depend on the release (`docs/format.md` section 7.1)
/// and are not wired in yet.
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

    // Summary. File offsets are hex so they can be pasted into a hex editor.
    let payload_end = *base as u64 + offsets.byte_count;
    writeln!(out, "bun          {version}")?;
    writeln!(out, "layout       {layout:?}")?;
    writeln!(out, "payload      {base:#x}..{payload_end:#x} ({} bytes)", offsets.byte_count)?;
    writeln!(out, "trailer      {trailer_pos:#x}")?;
    if let Some(flags) = offsets.flags {
        writeln!(out, "flags        {flags:#x}")?;
    }
    writeln!(out, "modules      {}", entries.len())?;
    writeln!(out, "entry point  {entry_point}  {}", name_of(&entries[entry_point]))?;
    writeln!(out)?;

    // One row per module. The name goes last because it is the only column
    // whose width varies.
    writeln!(
        out,
        "{:>6}  {:>10}  {:>10}  {:>10}  {:>10}  {:>6}  {:>3}  {:>3}  {:>4}  name",
        "#", "offset", "size", "sourcemap", "bytecode", "loader", "enc", "fmt", "side",
    )?;
    for (index, entry) in entries.iter().enumerate() {
        let marker = if index == entry_point { '*' } else { ' ' };
        let offset = format!("{:#x}", base + entry.contents.offset as usize);
        writeln!(
            out,
            "{marker}{index:>5}  {offset:>10}  {:>10}  {:>10}  {:>10}  {:>6}  {:>3}  {:>3}  {:>4}  {}",
            entry.contents.length,
            length_or_dash(Some(entry.sourcemap)),
            length_or_dash(entry.bytecode),
            entry.loader,
            byte_or_dash(entry.encoding),
            byte_or_dash(entry.module_format),
            byte_or_dash(entry.side),
            name_of(entry),
        )?;
    }

    writeln!(out)?;
    writeln!(out, "* entry point. offset: file offset of the contents. Sizes in bytes.")?;
    writeln!(out, "loader, enc, fmt, side: Bun's raw enum values; - means absent in this layout.")
}

/// A pointer's length, or `-` when the layout lacks the field or the module
/// has none (Bun writes an empty pointer for "no source map").
fn length_or_dash(pointer: Option<StringPointer>) -> String {
    match pointer {
        Some(pointer) if pointer.length > 0 => pointer.length.to_string(),
        _ => "-".to_string(),
    }
}

fn byte_or_dash(value: Option<u8>) -> String {
    value.map_or_else(|| "-".to_string(), |value| value.to_string())
}
