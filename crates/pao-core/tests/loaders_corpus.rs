//! Loader values against the binaries `tools/bun-matrix/build.sh` builds from
//! `fixture-loaders`, one per Bun release, named `<bun version>-loaders`.
//!
//! - `PAO_LOADERS_DIR`: `tools/bun-matrix/out/bin-loaders`.
//!
//! The test passes without checking anything if the variable is unset.
//!
//! Each fixture file is stored as its own module entry, and its loader byte
//! must name the loader that release is known to use for that file. The
//! names come from two tables, because Bun 1.2.5 inserted `jsonc` at 7 and
//! renumbered everything above it; the fixture's `.db`, `.node` and `.wasm`
//! entries sit above 7, so they fail if the wrong table is applied.

use std::env;
use std::fs;
use std::path::PathBuf;

use pao_core::locate::locate;

// `bun.options.Loader` at Bun 1.2.4 (`fd9a5ea668e9`), `src/options.zig`. Older
// releases define a prefix of this list: 1.0.0 stops at `text`.
const LOADERS_BEFORE_1_2_5: &[&str] = &[
    "jsx", "js", "ts", "tsx", "css", "file", "json", "toml", "wasm", "napi", "base64", "dataurl",
    "text", "bunsh", "sqlite", "sqlite_embedded", "html",
];

// `bun.options.Loader` from Bun 1.2.5 (`013fdddc6ed1`), up to `bun_ast::Loader`
// at 1.4.4 (`38afee6c84fc`, `src/ast/loader.rs`). Later releases only append.
const LOADERS_FROM_1_2_5: &[&str] = &[
    "jsx", "js", "ts", "tsx", "css", "file", "json", "jsonc", "toml", "wasm", "napi", "base64",
    "dataurl", "text", "bunsh", "sqlite", "sqlite_embedded", "html", "yaml", "json5", "md", "xml",
];

type Version = (u32, u32, u32);

fn parse_version(text: &str) -> Version {
    let mut parts = text.split('.').map(|part| part.parse().expect("versions are numeric"));
    let mut next = || parts.next().expect("versions have three parts");
    (next(), next(), next())
}

/// The loader `version` uses for a fixture file with this extension, as
/// measured on the fixture builds. Releases that changed how a file is stored
/// are the boundaries below.
fn expected_loader(extension: &str, version: Version) -> &'static str {
    match extension {
        "js" => "js",
        "db" if version < (1, 0, 36) => "file", // 1.0.0 has no `sqlite_embedded`
        "db" => "sqlite_embedded",
        "node" => "napi",
        "wasm" => "wasm",
        "html" if version < (1, 2, 23) => "file",
        "html" => "html",
        "sh" if version < (1, 1, 45) => "file",
        "sh" => "bunsh",
        "txt" => "text",
        other => panic!("fixture-loaders has no `.{other}` file"),
    }
}

#[test]
fn loader_values_match_the_release_table() {
    let Some(dir) = env::var_os("PAO_LOADERS_DIR").map(PathBuf::from) else {
        eprintln!("PAO_LOADERS_DIR unset; skipping");
        return;
    };

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("PAO_LOADERS_DIR is a readable directory")
        .map(|entry| entry.expect("directory entries are readable").path())
        .collect();
    paths.sort();
    // One binary per Bun release in the matrix, 1.0.0 to 1.4.4.
    assert_eq!(paths.len(), 14, "unexpected binary count in {}", dir.display());

    let mut failures = Vec::new();
    for path in &paths {
        let file_name = path.file_name().expect("read_dir yields named paths").to_string_lossy();
        let version = parse_version(file_name.trim_end_matches("-loaders"));
        let table = if version < (1, 2, 5) { LOADERS_BEFORE_1_2_5 } else { LOADERS_FROM_1_2_5 };

        let binary = fs::read(path).expect("fixture binaries are readable");
        let located = locate(&binary).unwrap_or_else(|error| panic!("{file_name}: {error}"));

        let mut extensions = Vec::new();
        for entry in &located.entries {
            let name = located.payload.get(entry.name).expect("locate checked every pointer");
            let name = std::str::from_utf8(name).expect("locate checked that names are UTF-8");
            // From 1.0.36 the entry module is named after the output file,
            // `1.2.0-loaders`, whose dots are not an extension; it is the
            // bundled JavaScript.
            let base_name = name.rsplit_once('/').map_or(name, |(_, base_name)| base_name);
            let extension = match base_name.rsplit_once('.') {
                _ if base_name == file_name => "js",
                Some((_, extension)) => extension,
                None => "js",
            };
            extensions.push(extension);

            let want = expected_loader(extension, version);
            let got = table.get(usize::from(entry.loader)).copied().unwrap_or("<out of range>");
            if got != want {
                failures.push(format!("{file_name}: {name} has loader {} ({got}), want {want}", entry.loader));
            }
        }

        // These four are stored as their own entries by every release.
        for required in ["db", "node", "wasm", "html"] {
            if !extensions.contains(&required) {
                failures.push(format!("{file_name}: no `.{required}` entry"));
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
