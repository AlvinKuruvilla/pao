//! Loader names against the binaries `tools/bun-matrix/build.sh` builds from
//! `fixture-loaders`, one per Bun release, named `<bun version>-loaders`.
//!
//! - `PAO_LOADERS_DIR`: `tools/bun-matrix/out/bin-loaders`.
//!
//! The test passes without checking anything if the variable is unset.
//!
//! Each fixture file is stored as its own module entry. Its loader byte,
//! named by `names::loader` for the release the runtime reports, must be the
//! loader that release was measured to use for that file. The fixture's
//! `.db`, `.node` and `.wasm` entries have values above 7, so they fail if
//! the 1.2.5 renumbering is applied to the wrong releases.

use std::env;
use std::fs;
use std::path::PathBuf;

use pao_core::locate::locate;
use pao_core::names;
use pao_core::native;
use pao_core::version::find_version;

/// The loader `release` uses for a fixture file with this extension, as
/// measured on the fixture builds. Releases that changed how a file is stored
/// are the boundaries below.
fn expected_loader(extension: &str, release: (u32, u32, u32)) -> &'static str {
    match extension {
        "js" => "js",
        "db" if release < (1, 0, 36) => "file", // 1.0.0 has no `sqlite_embedded`
        "db" => "sqlite_embedded",
        "node" => "napi",
        "wasm" => "wasm",
        "html" if release < (1, 2, 23) => "file",
        "html" => "html",
        "sh" if release < (1, 1, 45) => "file",
        "sh" => "bunsh",
        "txt" => "text",
        other => panic!("fixture-loaders has no `.{other}` file"),
    }
}

#[test]
fn loader_names_match_the_release() {
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
    assert_eq!(
        paths.len(),
        14,
        "unexpected binary count in {}",
        dir.display()
    );

    let mut failures = Vec::new();
    for path in &paths {
        let file_name = path
            .file_name()
            .expect("read_dir yields named paths")
            .to_string_lossy();
        let binary = fs::read(path).expect("fixture binaries are readable");
        let located = locate(&binary).unwrap_or_else(|error| panic!("{file_name}: {error}"));
        let version =
            find_version(&binary, &located).unwrap_or_else(|error| panic!("{file_name}: {error}"));
        let numbers = format!("{}.{}.{}", version.major, version.minor, version.patch);
        if file_name.trim_end_matches("-loaders") != numbers {
            failures.push(format!("{file_name}: runtime says Bun {version}"));
            continue;
        }

        let mut extensions = Vec::new();
        for module in &located.modules {
            let name = module.name;
            // From 1.0.36 the entry module is named after the output file,
            // `1.2.0-loaders`, whose dots are not an extension; it is the
            // bundled JavaScript.
            let base_name = name
                .rsplit_once('/')
                .map_or(name, |(_, base_name)| base_name);
            let extension = match base_name.rsplit_once('.') {
                _ if base_name == file_name => "js",
                Some((_, extension)) => extension,
                None => "js",
            };
            extensions.push(extension);

            let want = expected_loader(extension, version.release());
            let got = names::loader(module.entry.loader, &version).unwrap_or("<undefined>");
            if got != want {
                failures.push(format!(
                    "{file_name}: {name} has loader {} ({got}), want {want}",
                    module.entry.loader
                ));
            }
        }

        // These four are stored as their own entries by every release.
        for required in ["db", "node", "wasm", "html"] {
            if !extensions.contains(&required) {
                failures.push(format!("{file_name}: no `.{required}` entry"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The fixture's `addon.node` is `cc -shared` output defining one function,
/// `pao_fixture`, and no Node-API entry point. Bun stores it without loading
/// it, so every release must hand back a library `native::describe` reads as
/// a host Mach-O image that exports nothing in the Node-API namespace: the
/// case `pao inspect` marks as "no Node-API entry points".
#[test]
fn fixture_addon_describes_with_no_node_api_exports() {
    let Some(dir) = env::var_os("PAO_LOADERS_DIR").map(PathBuf::from) else {
        eprintln!("PAO_LOADERS_DIR unset; skipping");
        return;
    };

    let mut checked = 0;
    for entry in fs::read_dir(&dir).expect("PAO_LOADERS_DIR is a readable directory") {
        let path = entry.expect("directory entries are readable").path();
        let binary = fs::read(&path).expect("fixture binaries are readable");
        let located = locate(&binary).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let addon = located
            .modules
            .iter()
            .find(|module| module.name.ends_with(".node"))
            .unwrap_or_else(|| panic!("{}: no `.node` entry", path.display()));

        let library = native::describe(addon.contents)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(library.format, native::Format::MachO, "{}", path.display());
        assert_eq!(library.images.len(), 1, "{}", path.display());
        assert_eq!(library.images[0].architecture, "arm64", "{}", path.display());
        assert!(
            library.images[0].node_api_exports.is_empty(),
            "{}: {:?}",
            path.display(),
            library.images[0].node_api_exports
        );
        checked += 1;
    }
    assert_eq!(checked, 14, "one binary per Bun release in the matrix");
}
