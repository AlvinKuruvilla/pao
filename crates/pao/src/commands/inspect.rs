//! `pao inspect`: prints a summary of the payload, then one row per module in
//! table order.
//!
//! Rows follow binwalk's shape: the file offset where the module's contents
//! start, in decimal and hex, then a description. A description lists only
//! the fields the layout has, so one wording never has to cover two meanings,
//! such as a bytecode length that is a real size in one build and runs to the
//! end of a shared payload in another.

use std::fmt::Display;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use anyhow::Context;
use pao_core::entry::ModuleEntry;
use pao_core::flags;
use pao_core::locate::{Located, locate};
use pao_core::names;
use pao_core::native::{self, Image, Library};
use pao_core::version::{BunVersion, find_version};

use crate::output::{Color, with_output};

pub fn run(binary: &Path, color: Color, no_pager: bool) -> anyhow::Result<()> {
    let bytes = fs::read(binary).with_context(|| format!("read {}", binary.display()))?;
    let located =
        locate(&bytes).with_context(|| format!("find the Bun payload in {}", binary.display()))?;
    let version = find_version(&bytes, &located)
        .with_context(|| format!("identify the Bun release that built {}", binary.display()))?;
    with_output(color, no_pager, |out| print(out, &located, version))
}

// Colours. Each marks something a reader scans for; the rest stays plain.
mod style {
    use anstyle::{AnsiColor, Style};

    pub const LABEL: Style = Style::new().dimmed();
    pub const VERSION: Style = Style::new().bold();
    pub const HEADER: Style = Style::new().bold();
    pub const RULE: Style = Style::new().dimmed();
    pub const DECIMAL: Style = Style::new().dimmed();
    pub const HEX: Style = AnsiColor::Cyan.on_default();
    pub const INDEX: Style = Style::new().dimmed();
    pub const NAME: Style = Style::new().bold();
    pub const ENTRY_POINT: Style = AnsiColor::Yellow.on_default().bold();
    /// Loaders whose contents are JavaScript or TypeScript source.
    pub const SOURCE: Style = AnsiColor::Green.on_default();
    /// Loaders for native code and databases.
    pub const NATIVE: Style = AnsiColor::Magenta.on_default();
    /// Every other loader: files, text, HTML, data.
    pub const ASSET: Style = AnsiColor::Blue.on_default();
    pub const BYTECODE: Style = AnsiColor::Yellow.on_default();
    /// A value the release does not define.
    pub const UNDEFINED: Style = AnsiColor::Red.on_default().bold();
}

/// `text` in `style`, reset afterwards. The output stream strips colour when
/// it should not be shown, so this always writes it.
fn paint(style: anstyle::Style, text: impl Display) -> String {
    format!("{style}{text}{style:#}")
}

/// Writes the inspection of `located`, which `version` built.
fn print(out: &mut dyn Write, located: &Located<'_>, version: BunVersion<'_>) -> io::Result<()> {
    let Located {
        layout,
        offsets,
        payload: _,
        base,
        trailer_pos,
        modules,
    } = located;
    let entry_point = offsets.entry_point_id as usize;
    let linked = offsets
        .flags
        .is_some_and(|raw| flags::is_set(raw, flags::HAS_LINKED_BYTECODE_PAYLOAD, &version));
    let label = |name: &str| paint(style::LABEL, format!("{name:<13}"));

    // Summary. File offsets are hex so they can be pasted into a hex editor.
    let payload_end = *base as u64 + offsets.byte_count;
    writeln!(out, "{}{}", label("bun"), paint(style::VERSION, version))?;
    writeln!(out, "{}{layout:?}", label("layout"))?;
    writeln!(
        out,
        "{}{base:#x}..{payload_end:#x}, {} bytes",
        label("payload"),
        grouped(offsets.byte_count)
    )?;
    writeln!(out, "{}{trailer_pos:#x}", label("trailer"))?;
    // Before 1.3.3 these bytes are padding, which is zero in every build
    // tested; the line is left out then, and shown if the padding is not zero.
    if let Some(raw) = offsets.flags
        && (raw != 0 || flags::defined(&version) != 0)
    {
        let mut parts: Vec<String> = flags::names(raw, &version).map(str::to_string).collect();
        let undefined = raw & !flags::defined(&version);
        if undefined != 0 {
            parts.push(paint(
                style::UNDEFINED,
                format!("{undefined:#x} not defined in Bun {version}"),
            ));
        }
        let parts = if parts.is_empty() {
            paint(style::LABEL, "none set")
        } else {
            parts.join(", ")
        };
        writeln!(out, "{}{raw:#x}: {parts}", label("flags"))?;
    }
    if linked {
        writeln!(
            out,
            "{}{}",
            label("bytecode"),
            paint(
                style::BYTECODE,
                "linked: modules share one payload, so rows give each entry, not a size"
            )
        )?;
    }
    writeln!(
        out,
        "{}{}, entry point #{entry_point} {}",
        label("modules"),
        grouped(modules.len() as u64),
        paint(style::ENTRY_POINT, modules[entry_point].name)
    )?;
    writeln!(out)?;

    writeln!(
        out,
        "{}",
        paint(
            style::HEADER,
            format!("{:<13}{:<14}DESCRIPTION", "DECIMAL", "HEXADECIMAL")
        )
    )?;
    writeln!(out, "{}", paint(style::RULE, "-".repeat(78)))?;
    for (index, module) in modules.iter().enumerate() {
        let entry = &module.entry;
        let offset = base + entry.contents.offset as usize;
        let (marker, name_style) = if index == entry_point {
            (paint(style::ENTRY_POINT, "* "), style::ENTRY_POINT)
        } else {
            (String::new(), style::NAME)
        };
        writeln!(
            out,
            "{}{}{marker}{} {}",
            paint(style::DECIMAL, format!("{offset:<13}")),
            paint(style::HEX, format!("{offset:<#14x}")),
            paint(style::INDEX, format!("#{index}")),
            paint(name_style, module.name),
        )?;
        writeln!(out, "{:27}{}", "", describe(entry, &version, *base, linked))?;
        if names::loader(entry.loader, &version) == Some("napi") {
            print_native(out, module.contents)?;
        }
    }
    writeln!(out)?;
    writeln!(
        out,
        "{} {}",
        paint(style::ENTRY_POINT, "*"),
        paint(style::LABEL, "entry point")
    )
}

/// The headers of a `napi` module, under its row: format and architectures,
/// then the addon's own name, its Node-API exports and the libraries it links.
fn print_native(out: &mut dyn Write, contents: &[u8]) -> io::Result<()> {
    let indent = format!("{:27}", "");
    let library = match native::describe(contents) {
        Ok(library) => library,
        Err(error) => return writeln!(out, "{indent}{}", paint(style::UNDEFINED, error)),
    };
    let architectures: Vec<&str> = library
        .images
        .iter()
        .map(|image| image.architecture.as_str())
        .collect();
    writeln!(out, "{indent}{}: {}", library.format, architectures.join(", "))?;

    // Each field is listed once when every slice agrees on it, and once per
    // slice otherwise; slices of one build often differ only in a target
    // triple inside the build path.
    let width = architectures.iter().map(|name| name.len()).max().unwrap_or(0);
    let mut field = |name: &str, value: &dyn Fn(&Image) -> Option<String>| -> io::Result<()> {
        let values: Vec<(&str, Option<String>)> = library
            .images
            .iter()
            .map(|image| (image.architecture.as_str(), value(image)))
            .collect();
        let label = paint(style::LABEL, format!("{name:<9}"));
        if values.windows(2).all(|pair| pair[0].1 == pair[1].1) {
            if let Some((_, Some(value))) = values.first() {
                writeln!(out, "{indent}{label}{value}")?;
            }
            return Ok(());
        }
        for (architecture, value) in values {
            let value = value.unwrap_or_else(|| paint(style::LABEL, "none"));
            let architecture = paint(style::LABEL, format!("{architecture:<width$}"));
            writeln!(out, "{indent}{label}{architecture} {value}")?;
        }
        Ok(())
    };
    field("name", &|image| image.own_name.clone())?;
    field("exports", &|image| {
        Some(if image.node_api_exports.is_empty() {
            paint(style::UNDEFINED, "no Node-API entry points")
        } else {
            image.node_api_exports.join(", ")
        })
    })?;
    field("links", &|image| {
        let links: Vec<String> = image.libraries.iter().map(library_name).collect();
        (!links.is_empty()).then(|| links.join(", "))
    })?;
    Ok(())
}

/// `ScreenCaptureKit (weak)`: the short name, then how it is loaded.
fn library_name(library: &Library) -> String {
    let name = library.short_name();
    match library.kind {
        Some(kind) => format!("{name} ({kind})"),
        None => name.to_string(),
    }
}

/// `js, esm, latin1, server, 1,244 bytes, bytecode 688 bytes`: the entry's
/// fields named for `version`, then its sizes.
fn describe(entry: &ModuleEntry, version: &BunVersion<'_>, base: usize, linked: bool) -> String {
    // An undefined value is shown as a number, never given a name borrowed
    // from another release.
    let undefined = |field: &str, value: u8| {
        paint(
            style::UNDEFINED,
            format!("{field} {value} (not defined in Bun {version})"),
        )
    };
    let loader = match names::loader(entry.loader, version) {
        Some(name @ ("js" | "jsx" | "ts" | "tsx")) => paint(style::SOURCE, name),
        Some(name @ ("napi" | "wasm" | "sqlite" | "sqlite_embedded")) => paint(style::NATIVE, name),
        Some(name) => paint(style::ASSET, name),
        None => undefined("loader", entry.loader),
    };
    let mut parts = vec![loader];
    if let Some(value) = entry.module_format {
        parts.push(match names::module_format(value) {
            Some("none") => "no module format".to_string(),
            Some(name) => name.to_string(),
            None => undefined("module format", value),
        });
    }
    if let Some(value) = entry.encoding {
        parts.push(
            names::encoding(value, version)
                .map_or_else(|| undefined("encoding", value), str::to_string),
        );
    }
    if let Some(value) = entry.side {
        parts.push(names::side(value).map_or_else(|| undefined("side", value), str::to_string));
    }

    parts.push(format!("{} bytes", grouped(entry.contents.length.into())));
    // Bun writes an empty pointer for "none".
    if entry.sourcemap.length > 0 {
        parts.push(format!(
            "source map {} bytes",
            grouped(entry.sourcemap.length.into())
        ));
    }
    match entry.bytecode {
        Some(bytecode) if bytecode.length > 0 && linked => {
            parts.push(paint(
                style::BYTECODE,
                format!("bytecode entry at {:#x}", base + bytecode.offset as usize),
            ));
        }
        Some(bytecode) if bytecode.length > 0 => {
            parts.push(paint(
                style::BYTECODE,
                format!("bytecode {} bytes", grouped(bytecode.length.into())),
            ));
        }
        _ => {}
    }
    if let Some(module_info) = entry.module_info
        && module_info.length > 0
    {
        parts.push(format!(
            "module info {} bytes",
            grouped(module_info.length.into())
        ));
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
