//! The desktop viewer's backend: opens an executable, holds its unpacked
//! [`Project`] in memory, and serves the frontend in `gui/` through three
//! commands.
//!
//! - `open_binary` builds the project and returns the file list and module
//!   table, without file contents.
//! - `read_file` returns one file's contents.
//! - `export_to` writes every file into an empty directory, as
//!   `pao extract` does.
//!
//! Contents are sent one file at a time because a project can hold tens of
//! megabytes, and the frontend only ever shows one file.
//!
//! The opened executable's bytes are dropped once the project is built:
//! [`Project`] owns its contents, and everything else the frontend needs is
//! copied into [`OpenedBinary`].

// Without this, a release build on Windows opens a console beside the window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pao_core::flags;
use pao_core::locate::{Located, locate};
use pao_core::names;
use pao_core::project::{self, Options, Project, Tree};
use pao_core::version::{BunVersion, find_version};
use serde::Serialize;
use tauri::State;

/// The open executable's project, or `None` before the first open.
#[derive(Default)]
struct Opened(Mutex<Option<Project>>);

#[derive(Serialize)]
struct OpenedBinary {
    path: String,
    file_name: String,
    bun_version: String,
    layout: String,
    entry_point: usize,
    modules: Vec<ModuleSummary>,
    files: Vec<FileSummary>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct ModuleSummary {
    name: String,
    loader: String,
    module_format: Option<String>,
    encoding: Option<String>,
    side: Option<String>,
    contents_offset: u64,
    contents_size: u32,
    sourcemap_size: u32,
    bytecode: Option<String>,
}

#[derive(Serialize)]
struct FileSummary {
    path: String,
    tree: &'static str,
    module: usize,
    size: usize,
    text: bool,
}

#[derive(Serialize)]
struct FileContents {
    text: Option<String>,
    size: usize,
}

/// Errors cross to the frontend as their message; it only shows them.
type CommandResult<T> = Result<T, String>;

#[tauri::command]
fn open_binary(path: PathBuf, opened: State<'_, Opened>) -> CommandResult<OpenedBinary> {
    let shown = path.display();
    let bytes = fs::read(&path).map_err(|error| format!("cannot read {shown}: {error}"))?;
    let located = locate(&bytes)
        .map_err(|error| format!("cannot find the Bun payload in {shown}: {error}"))?;
    let version = find_version(&bytes, &located)
        .map_err(|error| format!("cannot identify the Bun release that built {shown}: {error}"))?;
    let file_name = path
        .file_name()
        .map_or("build".into(), |name| name.to_string_lossy().into_owned());
    let project = project::build(&located, &version, &file_name, Options::default())
        .map_err(|error| format!("cannot unpack {shown}: {error}"))?;

    let summary = OpenedBinary {
        path: path.display().to_string(),
        file_name,
        bun_version: version.to_string(),
        layout: format!("{:?}", located.layout),
        entry_point: located.offsets.entry_point_id as usize,
        modules: module_summaries(&located, &version),
        files: project
            .files
            .iter()
            .map(|file| FileSummary {
                path: file.path.clone(),
                tree: match file.tree {
                    Tree::Bundled => "bundled",
                    Tree::Sources => "sources",
                },
                module: file.module,
                size: file.contents.len(),
                text: as_text(&file.contents).is_some(),
            })
            .collect(),
        warnings: project.warnings.iter().map(ToString::to_string).collect(),
    };
    *opened
        .0
        .lock()
        .expect("no command panics while holding the lock") = Some(project);
    Ok(summary)
}

#[tauri::command]
fn read_file(path: String, opened: State<'_, Opened>) -> CommandResult<FileContents> {
    let opened = opened
        .0
        .lock()
        .expect("no command panics while holding the lock");
    let project = opened.as_ref().ok_or("no executable is open")?;
    // Sorted by path, so a binary search finds it.
    let file = project
        .files
        .binary_search_by(|file| file.path.as_str().cmp(&path))
        .map(|index| &project.files[index])
        .map_err(|_| format!("{path} is not in the open executable"))?;
    Ok(FileContents {
        text: as_text(&file.contents).map(str::to_string),
        size: file.contents.len(),
    })
}

/// Writes every file under `directory`, which must be empty or absent, and
/// returns how many were written. Refusing a non-empty directory, with no
/// `--force` equivalent, means an export can never overwrite anything.
#[tauri::command]
fn export_to(directory: PathBuf, opened: State<'_, Opened>) -> CommandResult<usize> {
    let opened = opened
        .0
        .lock()
        .expect("no command panics while holding the lock");
    let project = opened.as_ref().ok_or("no executable is open")?;
    let shown = directory.display();
    if let Ok(mut entries) = fs::read_dir(&directory)
        && entries.next().is_some()
    {
        return Err(format!("{shown} is not empty; choose an empty folder"));
    }
    for file in &project.files {
        // `project::build` checked every segment, so this stays inside
        // `directory`.
        let path: PathBuf = std::iter::once(directory.as_path())
            .chain(file.path.split('/').map(Path::new))
            .collect();
        write_new(&path, &file.contents)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(project.files.len())
}

/// Creates `path` and its parents. `create_new` also catches two project
/// paths that differ only in case on a case-insensitive file system.
fn write_new(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(contents)
}

/// `contents` as text if the viewer can show it: valid UTF-8 without NUL.
/// Every text module is UTF-8 by now, since `project::build` converts
/// Latin-1 and UTF-16; a NUL means a binary file that happens to decode.
fn as_text(contents: &[u8]) -> Option<&str> {
    std::str::from_utf8(contents)
        .ok()
        .filter(|text| !text.contains('\0'))
}

/// The module table as `pao inspect` describes it: values named for the
/// release that built the executable, and a number where it defines none.
fn module_summaries(located: &Located<'_>, version: &BunVersion<'_>) -> Vec<ModuleSummary> {
    let linked = located
        .offsets
        .flags
        .is_some_and(|raw| flags::is_set(raw, flags::HAS_LINKED_BYTECODE_PAYLOAD, version));
    let named = |name: Option<&str>, value: u8| {
        name.map_or_else(|| format!("value {value}"), str::to_string)
    };
    located
        .modules
        .iter()
        .map(|module| {
            let entry = &module.entry;
            let bytecode = entry
                .bytecode
                .filter(|bytecode| bytecode.length > 0)
                .map(|bytecode| {
                    // A linked payload's pointer runs to the payload's end,
                    // so its length is not this module's size.
                    if linked {
                        format!("entry at {:#x}", located.base + bytecode.offset as usize)
                    } else {
                        format!("{} bytes", bytecode.length)
                    }
                });
            ModuleSummary {
                name: module.name.to_string(),
                loader: named(names::loader(entry.loader, version), entry.loader),
                module_format: entry
                    .module_format
                    .map(|value| named(names::module_format(value), value)),
                encoding: entry
                    .encoding
                    .map(|value| named(names::encoding(value, version), value)),
                side: entry.side.map(|value| named(names::side(value), value)),
                contents_offset: (located.base + entry.contents.offset as usize) as u64,
                contents_size: entry.contents.length,
                sourcemap_size: entry.sourcemap.length,
                bytecode,
            }
        })
        .collect()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Opened::default())
        .invoke_handler(tauri::generate_handler![open_binary, read_file, export_to])
        .run(tauri::generate_context!())
        .expect("the window and webview start");
}
