//! A summary of a native Node-API addon, the `.node` file Bun stores under
//! the `napi` loader.
//!
//! An addon is an ordinary shared library: Mach-O on macOS, possibly a
//! universal binary holding one image per architecture; ELF on Linux; PE on
//! Windows. Nothing here runs or disassembles it. The summary is what the
//! headers state outright:
//!
//! - **Architectures**: which machines the addon runs on. A universal binary
//!   has one [`Image`] per slice.
//! - **Own name**: the Mach-O install name (`LC_ID_DYLIB`), ELF `DT_SONAME` or
//!   PE export directory name. A Rust addon built by `cargo` on CI keeps the
//!   absolute path of its build directory here, which names the repository.
//! - **Libraries**: what the dynamic linker loads with it. On macOS the system
//!   frameworks say what the addon is for: `ScreenCaptureKit`, `CoreAudio`.
//! - **Node-API exports**: the entry points the runtime looks for, such as
//!   `napi_register_module_v1`. An addon with none of these is not a Node-API
//!   addon, whatever its loader says.
//!
//! Which toolchain built the addon, and which crate versions it carries, can
//! often be read from its strings, but only some of the time, so it is left
//! to a separate, explicitly heuristic pass.

use std::fmt;

use object::read::macho::{FatArch, MachOFatFile32, MachOFatFile64};
use object::{Architecture, FileKind, ImportLibraryFlags, Object, macho};

/// The container a native library arrived in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    MachO,
    /// A Mach-O universal ("fat") binary: several Mach-O images, one per
    /// architecture, behind a table of contents.
    MachOUniversal,
    Elf,
    Pe,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::MachO => "Mach-O",
            Format::MachOUniversal => "Mach-O universal",
            Format::Elf => "ELF",
            Format::Pe => "PE",
        })
    }
}

/// A native library's headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLibrary {
    pub format: Format,
    /// One per architecture slice of a universal binary, otherwise exactly one.
    pub images: Vec<Image>,
}

/// One loadable image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The architecture in Node's `process.arch` vocabulary (`arm64`, `x64`,
    /// `ia32`, `arm`), the names npm prebuilds are published under; any other
    /// architecture is named as `object` names it, in lower case.
    pub architecture: String,
    pub own_name: Option<String>,
    /// In load-command or import-table order, which is the order the dynamic
    /// linker resolves them in.
    pub libraries: Vec<Library>,
    /// Exported symbols in the Node-API namespace, without Mach-O's leading
    /// underscore, in export-table order.
    pub node_api_exports: Vec<String>,
}

/// A library an image depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub name: String,
    /// How the library is loaded, when that is not "always, at load time":
    /// `weak` (Mach-O; the image loads without it), `re-exported`, `lazy`,
    /// `upward` (Mach-O), or `delay-loaded` (PE).
    pub kind: Option<&'static str>,
}

impl Library {
    /// The name as a reader scans for it. A macOS system library is named by
    /// its last component, `CoreAudio` for
    /// `/System/Library/Frameworks/CoreAudio.framework/Versions/A/CoreAudio`,
    /// since the directory says nothing the name does not. Anything else is
    /// the full name: `@rpath/libComputerUseSwift.dylib` ships beside the
    /// addon, and where it is looked for is the point. ELF and PE names are
    /// bare file names already.
    #[must_use]
    pub fn short_name(&self) -> &str {
        let system = self.name.starts_with("/System/") || self.name.starts_with("/usr/lib/");
        match self.name.rsplit_once('/') {
            Some((_, last)) if system => last,
            _ => &self.name,
        }
    }
}

/// Why a module's bytes did not summarise.
#[derive(Debug)]
pub enum NativeError {
    /// The bytes are not a Mach-O, ELF or PE shared library. `kind` is what
    /// they are instead, when `object` recognises them.
    Unrecognized { kind: Option<FileKind> },
    /// The headers are recognised but do not parse.
    Malformed(object::Error),
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NativeError::Unrecognized { kind: None } => {
                write!(f, "not a Mach-O, ELF or PE library")
            }
            NativeError::Unrecognized { kind: Some(kind) } => {
                write!(f, "not a shared library (object reads it as {kind:?})")
            }
            NativeError::Malformed(error) => write!(f, "malformed headers: {error}"),
        }
    }
}

impl std::error::Error for NativeError {}

impl From<object::Error> for NativeError {
    fn from(error: object::Error) -> Self {
        NativeError::Malformed(error)
    }
}

/// Summarises the native library in `bytes`.
pub fn describe(bytes: &[u8]) -> Result<NativeLibrary, NativeError> {
    let kind = FileKind::parse(bytes).map_err(|_| NativeError::Unrecognized { kind: None })?;
    let format = match kind {
        FileKind::MachO32 | FileKind::MachO64 => Format::MachO,
        FileKind::MachOFat32 | FileKind::MachOFat64 => Format::MachOUniversal,
        FileKind::Elf32 | FileKind::Elf64 => Format::Elf,
        FileKind::Pe32 | FileKind::Pe64 => Format::Pe,
        other => return Err(NativeError::Unrecognized { kind: Some(other) }),
    };

    // A universal binary's slices are complete Mach-O files at the offsets
    // its table gives, so each is parsed on its own.
    let images = match kind {
        FileKind::MachOFat32 => slices(MachOFatFile32::parse(bytes)?.arches(), bytes)?,
        FileKind::MachOFat64 => slices(MachOFatFile64::parse(bytes)?.arches(), bytes)?,
        _ => vec![image(bytes)?],
    };
    Ok(NativeLibrary { format, images })
}

fn slices(arches: &[impl FatArch], bytes: &[u8]) -> Result<Vec<Image>, NativeError> {
    arches.iter().map(|arch| image(arch.data(bytes)?)).collect()
}

fn image(bytes: &[u8]) -> Result<Image, NativeError> {
    let file = object::File::parse(bytes)?;
    let is_macho = matches!(file.format(), object::BinaryFormat::MachO);

    let libraries = file
        .import_libraries()?
        .map(|library| {
            let library = library?;
            Ok(Library {
                name: text(library.name()),
                kind: library_kind(library.flags()),
            })
        })
        .collect::<Result<_, object::Error>>()?;

    let mut node_api_exports = Vec::new();
    for export in file.exports()? {
        let export = export?;
        let name = export.name();
        let Some(name) = name.name() else {
            continue; // exported by ordinal only, which the Node-API never is
        };
        // Mach-O prefixes every C symbol with `_`; ELF and PE do not.
        let name = match (is_macho, name) {
            (true, [b'_', rest @ ..]) => rest,
            _ => name,
        };
        if is_node_api(name) {
            node_api_exports.push(text(name));
        }
    }

    Ok(Image {
        architecture: architecture(file.architecture()),
        own_name: own_name(&file)?,
        libraries,
        node_api_exports,
    })
}

/// Whether an export belongs to the Node-API's entry points: the module
/// registration hook `napi_register_module_v1`, `node_api_module_get_api_version_v1`
/// (which reports the Node-API version the addon targets), and the
/// `node_register_module_v<ABI>` hook of addons built against the V8 API
/// directly. Any other `napi_` or `node_api_` export is listed too, since an
/// addon has no reason to define symbols in that namespace except for the
/// runtime to find.
fn is_node_api(name: &[u8]) -> bool {
    name.starts_with(b"napi_")
        || name.starts_with(b"node_api_")
        || name.starts_with(b"node_register_module_v")
}

fn own_name(file: &object::File<'_>) -> Result<Option<String>, object::Error> {
    match file {
        object::File::MachO32(file) => macho_install_name(file),
        object::File::MachO64(file) => macho_install_name(file),
        object::File::Elf32(file) => elf_soname(file),
        object::File::Elf64(file) => elf_soname(file),
        object::File::Pe32(file) => pe_export_name(file),
        object::File::Pe64(file) => pe_export_name(file),
        _ => Ok(None),
    }
}

fn macho_install_name<Mach: object::read::macho::MachHeader>(
    file: &object::read::macho::MachOFile<'_, Mach>,
) -> Result<Option<String>, object::Error> {
    let endian = file.endian();
    let mut commands = file.macho_load_commands()?;
    while let Some(command) = commands.next()? {
        // `LoadCommandData::dylib` accepts only the `LC_LOAD_*` family, so the
        // identity command is matched through the variant.
        if let object::read::macho::LoadCommandVariant::IdDylib(dylib) = command.variant()? {
            return Ok(Some(text(command.string(endian, dylib.dylib.name)?)));
        }
    }
    Ok(None)
}

fn elf_soname<Elf: object::read::elf::FileHeader>(
    file: &object::read::elf::ElfFile<'_, Elf>,
) -> Result<Option<String>, object::Error> {
    let endian = file.endian();
    let table = file
        .elf_section_table()
        .dynamic_table(endian, file.data())?;
    for entry in table.iter() {
        if entry.tag == object::elf::DT_SONAME {
            return Ok(Some(text(table.string(entry)?)));
        }
    }
    Ok(None)
}

fn pe_export_name<Pe: object::read::pe::ImageNtHeaders>(
    file: &object::read::pe::PeFile<'_, Pe>,
) -> Result<Option<String>, object::Error> {
    let Some(table) = file.export_table()? else {
        return Ok(None);
    };
    let pointer = table.directory().name.get(object::LittleEndian);
    Ok(Some(text(table.name_from_pointer(pointer)?)))
}

fn library_kind(flags: ImportLibraryFlags) -> Option<&'static str> {
    match flags {
        ImportLibraryFlags::MachO { cmd, .. } => match cmd {
            macho::LC_LOAD_WEAK_DYLIB => Some("weak"),
            macho::LC_REEXPORT_DYLIB => Some("re-exported"),
            macho::LC_LAZY_LOAD_DYLIB => Some("lazy"),
            macho::LC_LOAD_UPWARD_DYLIB => Some("upward"),
            _ => None,
        },
        ImportLibraryFlags::Pe { delay: true } => Some("delay-loaded"),
        _ => None,
    }
}

fn architecture(architecture: Architecture) -> String {
    match architecture {
        Architecture::Aarch64 => "arm64".to_string(),
        Architecture::X86_64 => "x64".to_string(),
        Architecture::I386 => "ia32".to_string(),
        Architecture::Arm => "arm".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// Header strings are bytes with no declared encoding; paths in practice are
/// UTF-8, and anything else is shown with replacement characters rather than
/// dropped.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real addons are checked against the fixture builds in
    // `tests/loaders_corpus.rs`; these cover bytes that are not a library.

    #[test]
    fn webassembly_is_not_a_library() {
        // The 8-byte header `tools/bun-matrix/build.sh` writes as `module.wasm`.
        // The `wasm` feature of `object` is off, so it is not recognised at all.
        let error = describe(b"\0asm\x01\0\0\0").expect_err("wasm is not Mach-O, ELF or PE");
        assert!(matches!(error, NativeError::Unrecognized { kind: None }));
    }

    #[test]
    fn truncated_macho_is_malformed() {
        // MH_MAGIC_64, little-endian, padded to the 16 bytes `FileKind` reads
        // before deciding: enough to recognise, short of the 32-byte header.
        let mut bytes = [0u8; 16];
        bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        let error = describe(&bytes).expect_err("the header is cut off");
        assert!(matches!(error, NativeError::Malformed(_)), "{error:?}");
    }
}
