//! `pao extract`: writes the embedded modules, and the original sources their
//! source maps carry, to a directory.
//!
//! The output has two trees:
//!
//! ```text
//! app.extracted/
//!   bundled/   each module's contents, at its name under Bun's virtual root
//!   sources/   each original file from the modules' source maps
//! ```
//!
//! `sources/` is the readable one when it exists: the files as they were
//! before bundling. `bundled/` is what the executable runs, and the only tree
//! for a build without `--sourcemap`.
//!
//! Every output path is computed and checked before anything is written, so a
//! refused name leaves no partial extraction behind.
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
//! build directory `app`; each further level adds `app.up1`, `app.up2`, ... above
//! it.
//!
//! # Trade-offs
//!
//! - **Refusing unsafe names**: a module or source name that is absolute after
//!   its prefix is removed, or that leaves its root, fails the whole run
//!   instead of being rewritten. Bun does not write such names, so one is
//!   either a format change or a crafted executable, and a quiet rename would
//!   hide both.
//! - **Text as UTF-8**: `latin1` and `utf16` modules are converted, so that
//!   every text file opens in an editor. The output is then not the stored
//!   bytes; `pao inspect` gives the offsets for anyone who needs those.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail, ensure};
use pao_core::locate::{Located, locate};
use pao_core::names;
use pao_core::sourcemap;
use pao_core::version::{BunVersion, find_version};

/// Prefixes of module names, from `docs/format.md` section 6. A name with
/// none of them still extracts, from its first path segment.
const VIRTUAL_ROOTS: [&str; 3] = ["/$bunfs/root/", "B:/~BUN/root/", "compiled://root/"];

pub fn run(binary: &Path, output: Option<&Path>, force: bool) -> anyhow::Result<()> {
    let bytes = fs::read(binary).with_context(|| format!("read {}", binary.display()))?;
    let located =
        locate(&bytes).with_context(|| format!("find the Bun payload in {}", binary.display()))?;
    let version = find_version(&bytes, &located)
        .with_context(|| format!("identify the Bun release that built {}", binary.display()))?;

    // Not `file_stem`: executables are often named with dots, `app-1.4.2`,
    // and only Windows' `.exe` is an extension Bun adds.
    let file_name = binary
        .file_name()
        .map_or("build".into(), |name| name.to_string_lossy());
    let stem = file_name.strip_suffix(".exe").unwrap_or(&file_name);
    let output = output.map_or_else(
        || PathBuf::from(format!("{stem}.extracted")),
        Path::to_path_buf,
    );

    let bundled = plan_bundled(&located, &version)?;
    let sources = plan_sources(&located, stem)?;

    prepare_output(&output, force)?;
    for (path, contents) in bundled.iter().chain(&sources) {
        write_new(&output.join(path), contents, force)?;
    }
    println!(
        "wrote {} module(s) to {} and {} source file(s) to {}",
        bundled.len(),
        output.join("bundled").display(),
        sources.len(),
        output.join("sources").display(),
    );
    if sources.is_empty() {
        println!("no module has a source map, so only the bundled code was extracted");
    }
    Ok(())
}

/// Output paths, relative to the output directory, and their contents. A map
/// so that two names landing on one path are caught before writing.
type Plan = BTreeMap<PathBuf, Vec<u8>>;

/// Each module's contents under `bundled/`, text converted to UTF-8.
fn plan_bundled(located: &Located<'_>, version: &BunVersion<'_>) -> anyhow::Result<Plan> {
    let mut plan = Plan::new();
    for entry in &located.entries {
        let name = located
            .payload
            .get(entry.name)
            .expect("locate checked every pointer");
        let name = std::str::from_utf8(name).expect("locate checked that names are UTF-8");
        let contents = located
            .payload
            .get(entry.contents)
            .expect("locate checked every pointer");

        let relative = VIRTUAL_ROOTS
            .iter()
            .find_map(|root| name.strip_prefix(root))
            .unwrap_or(name);
        let segments = safe_segments(relative).with_context(|| format!("place module {name}"))?;
        let path: PathBuf = ["bundled".to_string()]
            .into_iter()
            .chain(segments)
            .collect();

        let contents = to_utf8(
            contents,
            entry
                .encoding
                .and_then(|value| names::encoding(value, version)),
        )
        .with_context(|| format!("convert module {name} to UTF-8"))?;
        if plan.insert(path, contents).is_some() {
            bail!("two modules are named {name}; extracting both would lose one");
        }
    }
    Ok(plan)
}

/// The original files from every module's source map, under `sources/`.
fn plan_sources(located: &Located<'_>, stem: &str) -> anyhow::Result<Plan> {
    // Each name split into how many `..` it starts with and the rest. All of
    // them are needed before any is placed, because the deepest `..` decides
    // where the build directory sits.
    let mut files: BTreeMap<(usize, Vec<String>), (String, Vec<u8>)> = BTreeMap::new();
    for entry in located
        .entries
        .iter()
        .filter(|entry| entry.sourcemap.length > 0)
    {
        let bytes = located
            .payload
            .get(entry.sourcemap)
            .expect("locate checked every pointer");
        let map = sourcemap::parse(bytes).context("decode a module's source map")?;
        for file in &map.files {
            let (up, rest) =
                split_parents(file.name).with_context(|| format!("place source {}", file.name))?;
            let text = file.decompress().context("decompress a source file")?;
            // Code splitting can list one file in several chunks' maps.
            match files.entry((up, rest)) {
                Entry::Vacant(slot) => {
                    slot.insert((file.name.to_string(), text));
                }
                Entry::Occupied(slot) => ensure!(
                    slot.get().1 == text,
                    "two source maps list {} with different contents",
                    file.name
                ),
            }
        }
    }

    let depth = files.keys().map(|(up, _)| *up).max().unwrap_or(0);
    // The directories from `sources/` down to the build directory, outermost
    // first: `app.up1/app` for a depth of 2.
    let invented: Vec<String> = (0..depth)
        .rev()
        .map(|level| match level {
            0 => stem.to_string(),
            level => format!("{stem}.up{level}"),
        })
        .collect();

    let mut plan = Plan::new();
    for ((up, rest), (name, text)) in files {
        let path: PathBuf = ["sources"]
            .into_iter()
            .map(String::from)
            .chain(invented[..depth - up].iter().cloned())
            .chain(rest)
            .collect();
        if plan.insert(path.clone(), text).is_some() {
            bail!(
                "source {name} lands on {}, as another file does; the names that stand in for \
                 directories above the build directory collide with a real one",
                path.display()
            );
        }
    }
    Ok(plan)
}

/// `name`'s segments, refusing anything that could leave the directory it is
/// joined to: an absolute path, a drive, `..`, or a segment the platform would
/// read as a separator.
fn safe_segments(name: &str) -> anyhow::Result<Vec<String>> {
    let (up, rest) = split_parents(name)?;
    ensure!(up == 0, "{name} climbs out of its directory with `..`");
    ensure!(!rest.is_empty(), "the name is empty");
    Ok(rest)
}

/// Splits `name` into the number of `..` it starts with and the segments
/// after them, with `.` and empty segments dropped. A `..` later in the name
/// is refused rather than resolved: Bun's relative paths never have one, so it
/// would mean the name is not what this code assumes.
fn split_parents(name: &str) -> anyhow::Result<(usize, Vec<String>)> {
    ensure!(!name.starts_with('/'), "{name} is an absolute path");
    let mut up = 0;
    let mut rest = Vec::new();
    for segment in name.split('/') {
        match segment {
            "" | "." => {}
            ".." if rest.is_empty() => up += 1,
            ".." => bail!("{name} has `..` after its first segments"),
            segment => {
                ensure!(
                    !segment.contains(['\\', '\0'])
                        && !(rest.is_empty() && up == 0 && segment.ends_with(':')),
                    "{name} has a segment, {segment:?}, that is not a plain file name"
                );
                rest.push(segment.to_string());
            }
        }
    }
    Ok((up, rest))
}

/// `contents` as UTF-8 for the text encodings, unchanged for `binary` and for
/// layouts without the field.
fn to_utf8(contents: &[u8], encoding: Option<&str>) -> anyhow::Result<Vec<u8>> {
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
            ensure!(
                contents.len().is_multiple_of(2),
                "UTF-16 contents have an odd length, {}",
                contents.len()
            );
            let units = contents
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_le_bytes(pair));
            char::decode_utf16(units)
                .collect::<Result<String, _>>()
                .context("contents are not valid UTF-16")?
                .into_bytes()
        }
        _ => contents.to_vec(),
    })
}

/// Creates `output`, or with `force` accepts one that already has files.
fn prepare_output(output: &Path, force: bool) -> anyhow::Result<()> {
    match fs::read_dir(output) {
        Ok(mut entries) => ensure!(
            force || entries.next().is_none(),
            "{} already exists and is not empty; pass --force to write into it",
            output.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("read {}", output.display())),
    }
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))
}

/// Writes `path`, failing if it exists unless `force`. Without `force` this
/// also catches two plan paths that differ only in case on a case-insensitive
/// file system, which the plan's own check cannot see.
fn write_new(path: &Path, contents: &[u8], force: bool) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .expect("plan paths are under the output directory");
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let mut options = OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(contents)
        .with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_leading_parents() {
        assert_eq!(
            split_parents("../../lib/a.ts").unwrap(),
            (2, vec!["lib".to_string(), "a.ts".to_string()])
        );
        assert_eq!(
            split_parents("./src//b.ts").unwrap(),
            (0, vec!["src".to_string(), "b.ts".to_string()])
        );
    }

    #[test]
    fn refuses_names_that_escape() {
        for name in ["/etc/passwd", "src/../../x", "C:/x", "a\\..\\b", "a\0b"] {
            assert!(split_parents(name).is_err(), "{name} was accepted");
        }
        assert!(safe_segments("../x").is_err());
    }

    #[test]
    fn converts_text_encodings() {
        assert_eq!(
            to_utf8(&[0x63, 0xE9], Some("latin1")).unwrap(),
            "cé".as_bytes()
        );
        assert_eq!(
            to_utf8(&[0x63, 0x00, 0xE9, 0x00], Some("utf16")).unwrap(),
            "cé".as_bytes()
        );
        assert_eq!(
            to_utf8(&[0xFF, 0x00], Some("binary")).unwrap(),
            [0xFF, 0x00]
        );
    }
}
