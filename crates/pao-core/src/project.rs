//! The files an executable's modules unpack to, made readable: the tree that
//! `pao extract` writes and the GUI shows.
//!
//! [`build`] turns a [`Located`] payload into two trees:
//!
//! ```text
//! bundled/   each module's contents, at its name under Bun's virtual root
//! sources/   each original file from the modules' source maps
//! ```
//!
//! `sources/` is the readable one when it exists: the files as they were
//! before bundling. `bundled/` is what the executable runs, and the only tree
//! for a build without `--sourcemap`.
//!
//! Every path is computed and checked before a [`Project`] is returned, so a
//! caller that writes one to disk never has to undo a partial write.
//!
//! # Source paths
//!
//! Source map names are relative to the directory Bun was run from, and that
//! directory is not recorded anywhere in the executable (`docs/format.md`
//! section 6.1). A project that imports from outside it gets names such as
//! `../shared.ts`. Those are kept, not clamped: `sources/` is rooted at the
//! highest directory any name reaches, and the directories in between, whose
//! real names are lost, are named after the executable. For `app` built from
//! `web/` with `../shared.ts` and `index.ts`:
//!
//! ```text
//! sources/
//!   shared.ts        ../shared.ts
//!   app/
//!     index.ts       index.ts; the build directory, really named web/
//! ```
//!
//! Relative imports between the files still resolve. One level up names the
//! build directory `app`; each further level adds `app.up1`, `app.up2`, ...
//! above it.
//!
//! # Trade-offs
//!
//! - **Refusing unsafe names**: a module or source name that is absolute after
//!   its prefix is removed, or that leaves its root, fails the whole build
//!   instead of being rewritten. Bun does not write such names, so one is
//!   either a format change or a crafted executable, and a quiet rename would
//!   hide both.
//! - **Readable over byte-exact**: the files are changed so that they open in
//!   an editor, and are then not what the executable stores.
//!   [`Module::entry`](crate::locate::Module::entry) has the offsets of the
//!   stored bytes.
//!   - `latin1` and `utf16` modules are converted to UTF-8.
//!   - A `js` module whose name has no JavaScript extension gets `.js`. Bun
//!     names the entry module after the executable, `cli` or `app.exe`, and
//!     `app.exe` becomes `app.js` so a build for any platform unpacks alike.
//!     The loader decides, not the name: a `file` asset named `.js` is left
//!     alone, and so is everything that is not code.
//!   - With [`Options::decompress`], a module named `.zst` that is a zstd
//!     frame is decompressed and loses the suffix, `README.md.zst` to
//!     `README.md`. Bun stores files byte for byte, so this compression is the
//!     app's own; one large Bun 1.4.4 executable embeds 127 such files, nearly
//!     all text. One that fails to decompress is kept as stored, with a
//!     [`Warning`].
//! - **Contents held in memory**: every file is decompressed and converted up
//!   front, about 62 MB for a 2,898-module executable. A viewer could do it
//!   per file on demand, but then a collision or a bad name would surface
//!   only when that file was opened.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;

use crate::locate::Located;
use crate::names;
use crate::sourcemap::{self, SourceMapError};
use crate::version::BunVersion;
use crate::zstd_frame::{self, FrameError};

/// Prefixes of module names, from `docs/format.md` section 6. A name with
/// none of them still unpacks, from its first path segment.
const VIRTUAL_ROOTS: [&str; 3] = ["/$bunfs/root/", "B:/~BUN/root/", "compiled://root/"];

/// How [`build`] unpacks.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Decompress modules named `.zst`.
    pub decompress: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { decompress: true }
    }
}

/// The unpacked files, sorted by path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub files: Vec<ProjectFile>,
    /// Files kept as stored because they could not be made readable.
    pub warnings: Vec<Warning>,
}

/// Which tree a file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tree {
    /// A module's contents.
    Bundled,
    /// An original file from a source map.
    Sources,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    /// `/`-separated and relative, starting `bundled/` or `sources/`. No
    /// segment is empty, `.`, `..`, or contains `\` or NUL, so joining it
    /// below a directory stays below that directory.
    pub path: String,
    pub tree: Tree,
    /// Index into [`Located::modules`]: the module stored here, or for a
    /// source, the first module whose source map lists it.
    pub module: usize,
    pub contents: Vec<u8>,
}

/// A file kept as stored instead of being made readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub path: String,
    pub module_name: String,
    pub error: FrameError,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}; kept as stored", self.module_name, self.error)
    }
}

/// Why [`build`] produced no project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// A module or source name could leave the directory it is placed in.
    UnsafeName { name: String, reason: String },
    /// Two files land on one path, so one would be lost.
    Collision { path: String, name: String },
    /// A module's text is not valid in the encoding it declares.
    BadText { name: String, reason: String },
    /// A module's source map does not decode.
    SourceMap {
        module: String,
        error: SourceMapError,
    },
    /// Two source maps list one file with different contents.
    ConflictingSource { name: String },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::UnsafeName { name, reason } => {
                write!(f, "cannot place {name}: {reason}")
            }
            ProjectError::Collision { path, name } => write!(
                f,
                "{name} lands on {path}, as another file does; unpacking both would lose one"
            ),
            ProjectError::BadText { name, reason } => {
                write!(f, "cannot convert module {name} to UTF-8: {reason}")
            }
            ProjectError::SourceMap { module, error } => {
                write!(
                    f,
                    "cannot decode the source map of module {module}: {error}"
                )
            }
            ProjectError::ConflictingSource { name } => {
                write!(f, "two source maps list {name} with different contents")
            }
        }
    }
}

impl std::error::Error for ProjectError {}

/// Unpacks `located`, which `version` built. `executable_name` is the
/// executable's file name, which names the directories a source map does not
/// record (see the module docs).
pub fn build(
    located: &Located<'_>,
    version: &BunVersion<'_>,
    executable_name: &str,
    options: Options,
) -> Result<Project, ProjectError> {
    // Only Windows' `.exe` is an extension Bun adds. Not a general
    // extension strip: executables are often named with dots, `app-1.4.2`.
    let stem = executable_name
        .strip_suffix(".exe")
        .unwrap_or(executable_name);

    let mut files = BTreeMap::new();
    let mut warnings = Vec::new();
    bundled(located, version, options, &mut files, &mut warnings)?;
    sources(located, stem, &mut files)?;
    Ok(Project {
        files: files.into_values().collect(),
        warnings,
    })
}

/// Inserts `file`, refusing a second file at the same path.
fn insert(
    files: &mut BTreeMap<String, ProjectFile>,
    file: ProjectFile,
    name: &str,
) -> Result<(), ProjectError> {
    match files.entry(file.path.clone()) {
        Entry::Vacant(slot) => {
            slot.insert(file);
            Ok(())
        }
        Entry::Occupied(_) => Err(ProjectError::Collision {
            path: file.path,
            name: name.to_string(),
        }),
    }
}

/// Each module's contents under `bundled/`.
fn bundled(
    located: &Located<'_>,
    version: &BunVersion<'_>,
    options: Options,
    files: &mut BTreeMap<String, ProjectFile>,
    warnings: &mut Vec<Warning>,
) -> Result<(), ProjectError> {
    for (index, module) in located.modules.iter().enumerate() {
        let name = module.name;
        let unsafe_name = |reason: String| ProjectError::UnsafeName {
            name: name.to_string(),
            reason,
        };
        let relative = VIRTUAL_ROOTS
            .iter()
            .find_map(|root| name.strip_prefix(root))
            .unwrap_or(name);
        let (up, mut segments) = split_parents(relative).map_err(unsafe_name)?;
        if up > 0 {
            return Err(unsafe_name(
                "it climbs out of its directory with `..`".into(),
            ));
        }
        let Some(file_name) = segments.last_mut() else {
            return Err(unsafe_name("nothing is left after the virtual root".into()));
        };

        let encoding = module
            .entry
            .encoding
            .and_then(|value| names::encoding(value, version));
        let mut contents =
            to_utf8(module.contents, encoding).map_err(|reason| ProjectError::BadText {
                name: name.to_string(),
                reason,
            })?;

        let mut kept_as_stored = None;
        if names::loader(module.entry.loader, version) == Some("js") {
            if ![".js", ".mjs", ".cjs"]
                .iter()
                .any(|ext| file_name.ends_with(ext))
            {
                let base = file_name.strip_suffix(".exe").unwrap_or(file_name);
                *file_name = format!("{base}.js");
            }
        } else if options.decompress
            && let Some(base) = file_name
                .strip_suffix(".zst")
                .filter(|base| !base.is_empty())
            && contents.starts_with(&zstd_frame::MAGIC)
        {
            match zstd_frame::decompress(&contents) {
                Ok(decompressed) => {
                    contents = decompressed;
                    *file_name = base.to_string();
                }
                Err(error) => kept_as_stored = Some(error),
            }
        }

        let path = format!("bundled/{}", segments.join("/"));
        if let Some(error) = kept_as_stored {
            warnings.push(Warning {
                path: path.clone(),
                module_name: name.to_string(),
                error,
            });
        }
        let file = ProjectFile {
            path,
            tree: Tree::Bundled,
            module: index,
            contents,
        };
        insert(files, file, name)?;
    }
    Ok(())
}

/// The original files from every module's source map, under `sources/`.
fn sources(
    located: &Located<'_>,
    stem: &str,
    files: &mut BTreeMap<String, ProjectFile>,
) -> Result<(), ProjectError> {
    // Keyed by each name split into how many `..` it starts with and the rest.
    // All of them are needed before any is placed, because the deepest `..`
    // decides where the build directory sits.
    type Key = (usize, Vec<String>);
    // The name as the map gives it, the first module listing it, the text.
    type Found = (String, usize, Vec<u8>);
    let mut found: BTreeMap<Key, Found> = BTreeMap::new();
    for (index, module) in located.modules.iter().enumerate() {
        if module.sourcemap.is_empty() {
            continue;
        }
        let map = sourcemap::parse(module.sourcemap).map_err(|error| ProjectError::SourceMap {
            module: module.name.to_string(),
            error,
        })?;
        for file in &map.files {
            let key = split_parents(file.name).map_err(|reason| ProjectError::UnsafeName {
                name: file.name.to_string(),
                reason,
            })?;
            let text = file.decompress().map_err(|error| ProjectError::SourceMap {
                module: module.name.to_string(),
                error,
            })?;
            // Code splitting can list one file in several chunks' maps.
            match found.entry(key) {
                Entry::Vacant(slot) => {
                    slot.insert((file.name.to_string(), index, text));
                }
                Entry::Occupied(slot) if slot.get().2 == text => {}
                Entry::Occupied(_) => {
                    return Err(ProjectError::ConflictingSource {
                        name: file.name.to_string(),
                    });
                }
            }
        }
    }

    let depth = found.keys().map(|(up, _)| *up).max().unwrap_or(0);
    // The directories from `sources/` down to the build directory, outermost
    // first: `app.up1/app` for a depth of 2.
    let invented: Vec<String> = (0..depth)
        .rev()
        .map(|level| match level {
            0 => stem.to_string(),
            level => format!("{stem}.up{level}"),
        })
        .collect();

    for ((up, rest), (name, module, contents)) in found {
        let segments: Vec<&str> = std::iter::once("sources")
            .chain(invented[..depth - up].iter().map(String::as_str))
            .chain(rest.iter().map(String::as_str))
            .collect();
        let file = ProjectFile {
            path: segments.join("/"),
            tree: Tree::Sources,
            module,
            contents,
        };
        insert(files, file, &name)?;
    }
    Ok(())
}

/// Splits `name` into the number of `..` it starts with and the segments
/// after them, with `.` and empty segments dropped. Refuses anything that
/// could leave the directory the segments are joined to: an absolute path, a
/// drive, a `..` later in the name, or a segment containing `\` or NUL. A
/// later `..` is refused rather than resolved because Bun's relative paths
/// never have one, so it would mean the name is not what this code assumes.
fn split_parents(name: &str) -> Result<(usize, Vec<String>), String> {
    if name.starts_with('/') {
        return Err("it is an absolute path".into());
    }
    let mut up = 0;
    let mut rest = Vec::new();
    for segment in name.split('/') {
        match segment {
            "" | "." => {}
            ".." if rest.is_empty() => up += 1,
            ".." => return Err("it has `..` after its first segments".into()),
            segment => {
                let drive = rest.is_empty() && up == 0 && segment.ends_with(':');
                if drive || segment.contains(['\\', '\0']) {
                    return Err(format!("{segment:?} is not a plain file name"));
                }
                rest.push(segment.to_string());
            }
        }
    }
    Ok((up, rest))
}

/// `contents` as UTF-8 for the text encodings, unchanged for `binary` and for
/// layouts without the field.
fn to_utf8(contents: &[u8], encoding: Option<&str>) -> Result<Vec<u8>, String> {
    Ok(match encoding {
        // Latin-1 maps each byte to the code point of the same value.
        Some("latin1") => contents
            .iter()
            .map(|&byte| char::from(byte))
            .collect::<String>()
            .into_bytes(),
        // Bun 1.4.1+ stores UTF-16 code units little-endian; checked on every
        // `utf16` module of one 1.4.4 executable.
        Some("utf16") => {
            if !contents.len().is_multiple_of(2) {
                return Err(format!(
                    "UTF-16 contents have an odd length, {}",
                    contents.len()
                ));
            }
            let units = contents
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_le_bytes(pair));
            char::decode_utf16(units)
                .collect::<Result<String, _>>()
                .map_err(|error| format!("contents are not valid UTF-16: {error}"))?
                .into_bytes()
        }
        _ => contents.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_leading_parents() {
        assert_eq!(
            split_parents("../../lib/a.ts"),
            Ok((2, vec!["lib".to_string(), "a.ts".to_string()]))
        );
        assert_eq!(
            split_parents("./src//b.ts"),
            Ok((0, vec!["src".to_string(), "b.ts".to_string()]))
        );
    }

    #[test]
    fn refuses_names_that_escape() {
        for name in ["/etc/passwd", "src/../../x", "C:/x", "a\\..\\b", "a\0b"] {
            assert!(split_parents(name).is_err(), "{name} was accepted");
        }
    }

    #[test]
    fn converts_text_encodings() {
        assert_eq!(to_utf8(&[0x63, 0xE9], Some("latin1")), Ok("cé".into()));
        assert_eq!(
            to_utf8(&[0x63, 0x00, 0xE9, 0x00], Some("utf16")),
            Ok("cé".into())
        );
        assert_eq!(to_utf8(&[0xFF, 0x00], Some("binary")), Ok(vec![0xFF, 0x00]));
    }
}
