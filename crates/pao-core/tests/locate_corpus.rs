//! `locate` against real executables, too large to check in.
//!
//! - `PAO_MATRIX_DIR`: binaries from `bun-matrix/build.sh`, named
//!   `<bun version>-bun-<os>-<arch>-<plain|bytecode>[.exe]`. Each embeds one
//!   module named after the file, plus, from Bun 1.4.4, the fixture's text
//!   import as a second entry.
//! - `PAO_EXTRA_BINARY`: any other `bun build --compile` executable. Its
//!   contents are unknown here, so the test checks only that it decodes.
//!
//! A test whose variable is unset passes without checking anything.

use std::env;
use std::fs;
use std::path::PathBuf;

use pao_core::layout_version::LayoutVersion;
use pao_core::locate::locate;
use pao_core::version::find_version;

/// The layout each tested Bun release uses, from `docs/format.md`.
fn expected_layout(bun_version: &str) -> LayoutVersion {
    match bun_version {
        "1.0.0" | "1.0.36" | "1.1.0" | "1.1.20" => LayoutVersion::V1_0,
        "1.1.30" | "1.1.45" | "1.2.0" | "1.2.12" => LayoutVersion::V1_1_30,
        "1.2.23" | "1.3.0" | "1.3.7" => LayoutVersion::V1_2_23,
        "1.3.14" | "1.4.0" | "1.4.4" => LayoutVersion::V1_3_14,
        other => panic!("no expected layout recorded for Bun {other}"),
    }
}

#[test]
fn matrix_binaries_decode_with_their_release_layout() {
    let Some(dir) = env::var_os("PAO_MATRIX_DIR").map(PathBuf::from) else {
        eprintln!("PAO_MATRIX_DIR unset; skipping");
        return;
    };

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("PAO_MATRIX_DIR is a readable directory")
        .map(|entry| entry.expect("directory entries are readable").path())
        .collect();
    paths.sort();
    // Catches an empty or wrong directory. 74 is the count built on 2026-10-29.
    assert_eq!(paths.len(), 74, "unexpected binary count in {}", dir.display());

    let mut failures = Vec::new();
    for path in &paths {
        let file_name = path.file_name().expect("read_dir yields named paths").to_string_lossy();
        let bun_version = file_name.split('-').next().expect("split yields at least one piece");
        let binary = fs::read(path).expect("matrix binaries are readable");

        let located = match locate(&binary) {
            Ok(located) => located,
            Err(error) => {
                failures.push(format!("{file_name}: {error}"));
                continue;
            }
        };
        if located.layout != expected_layout(bun_version) {
            failures.push(format!("{file_name}: decoded as {:?}", located.layout));
            continue;
        }

        let entry_point = located.entries[located.offsets.entry_point_id as usize];
        let name = located.payload.get(entry_point.name).expect("locate checked every pointer");
        let name = std::str::from_utf8(name).expect("locate checked names are UTF-8");
        if !name.ends_with(&*file_name) {
            failures.push(format!("{file_name}: entry point named {name}"));
        }

        match find_version(&binary, &located) {
            Ok(version) => {
                let numbers = format!("{}.{}.{}", version.major, version.minor, version.patch);
                if numbers != bun_version {
                    failures.push(format!("{file_name}: runtime says Bun {version}"));
                }
            }
            Err(error) => failures.push(format!("{file_name}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{} of {} failed:\n{}", failures.len(), paths.len(), failures.join("\n"));
}

#[test]
fn extra_binary_decodes() {
    let Some(path) = env::var_os("PAO_EXTRA_BINARY") else {
        eprintln!("PAO_EXTRA_BINARY unset; skipping");
        return;
    };
    let binary = fs::read(path).expect("PAO_EXTRA_BINARY is a readable file");
    let located = locate(&binary).unwrap_or_else(|error| panic!("{error}"));
    let version = find_version(&binary, &located).unwrap_or_else(|error| panic!("{error}"));
    eprintln!("Bun {version}, {:?}, {} modules", located.layout, located.entries.len());
}
