//! Source maps in the `--sourcemap` builds from `bun-matrix/build.sh`, found
//! through `PAO_MATRIX_DIR` as in `locate_corpus.rs`.
//!
//! Every release from 1.1.30 embeds one map naming the fixture's two
//! TypeScript files, relative to the repository root that `build.sh` runs
//! from. The decompressed sources must equal those files on disk byte for
//! byte, which also checks that the names are the paths Bun read.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use pao_core::locate::locate;
use pao_core::sourcemap;

/// Bun 1.0.0 to 1.1.20 accept `--sourcemap` with `--compile` and embed
/// nothing (`docs/format.md` section 6.1), so their builds have no map.
fn embeds_source_map(bun_version: &str) -> bool {
    !matches!(bun_version, "1.0.0" | "1.0.36" | "1.1.0" | "1.1.20")
}

#[test]
fn matrix_source_maps_hold_the_fixture_sources() {
    let Some(dir) = env::var_os("PAO_MATRIX_DIR").map(PathBuf::from) else {
        eprintln!("PAO_MATRIX_DIR unset; skipping");
        return;
    };
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("PAO_MATRIX_DIR is a readable directory")
        .map(|entry| entry.expect("directory entries are readable").path())
        .filter(|path| path.to_string_lossy().contains("-sourcemap"))
        .collect();
    paths.sort();
    // 14 releases x 3 targets, less the six cross-compiles 1.0.0 to 1.1.0
    // cannot do. Counted on 2026-10-31.
    assert_eq!(
        paths.len(),
        36,
        "unexpected --sourcemap binary count in {}",
        dir.display()
    );

    let mut failures = Vec::new();
    let mut checked = 0;
    for path in &paths {
        let file_name = path
            .file_name()
            .expect("read_dir yields named paths")
            .to_string_lossy();
        let bun_version = file_name
            .split('-')
            .next()
            .expect("split yields at least one piece");
        let binary = fs::read(path).expect("matrix binaries are readable");
        let located = locate(&binary).expect("locate_corpus.rs checks these decode");

        let maps: Vec<_> = located
            .entries
            .iter()
            .filter(|entry| entry.sourcemap.length > 0)
            .collect();
        if !embeds_source_map(bun_version) {
            if !maps.is_empty() {
                failures.push(format!(
                    "{file_name}: Bun {bun_version} was not expected to embed a source map"
                ));
            }
            continue;
        }
        let [entry] = maps[..] else {
            failures.push(format!(
                "{file_name}: {} modules have source maps, expected 1",
                maps.len()
            ));
            continue;
        };

        let bytes = located
            .payload
            .get(entry.sourcemap)
            .expect("locate checked every pointer");
        let map = match sourcemap::parse(bytes) {
            Ok(map) => map,
            Err(error) => {
                failures.push(format!("{file_name}: {error}"));
                continue;
            }
        };
        let names: Vec<&str> = map.files.iter().map(|file| file.name).collect();
        if names
            != [
                "tools/bun-matrix/fixture/greet.ts",
                "tools/bun-matrix/fixture/index.ts",
            ]
        {
            failures.push(format!("{file_name}: source map names {names:?}"));
            continue;
        }
        for file in &map.files {
            let on_disk =
                fs::read(repository.join(file.name)).expect("fixture sources are readable");
            match file.decompress() {
                Ok(text) if text == on_disk => {}
                Ok(_) => failures.push(format!(
                    "{file_name}: {} differs from the fixture",
                    file.name
                )),
                Err(error) => failures.push(format!("{file_name}: {error}")),
            }
        }
        checked += 1;
    }
    assert!(
        failures.is_empty(),
        "{} of {} failed:\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n")
    );
    assert_eq!(checked, 30, "expected 30 binaries with a source map");
}
