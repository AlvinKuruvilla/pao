//! `pao extract`: writes the files [`pao_core::project`] unpacks an
//! executable into, `bundled/` and `sources/`, to a directory.
//!
//! The module docs of `pao_core::project` describe the trees, how source
//! paths outside the build directory are placed, and how files are made
//! readable. This command only adds the directory handling.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, ensure};
use pao_core::locate::locate;
use pao_core::project::{self, Options, Tree};
use pao_core::version::find_version;

pub fn run(
    binary: &Path,
    output: Option<&Path>,
    force: bool,
    decompress: bool,
) -> anyhow::Result<()> {
    let bytes = fs::read(binary).with_context(|| format!("read {}", binary.display()))?;
    let located =
        locate(&bytes).with_context(|| format!("find the Bun payload in {}", binary.display()))?;
    let version = find_version(&bytes, &located)
        .with_context(|| format!("identify the Bun release that built {}", binary.display()))?;

    let executable_name = binary
        .file_name()
        .map_or("build".into(), |name| name.to_string_lossy());
    let project = project::build(&located, &version, &executable_name, Options { decompress })
        .with_context(|| format!("unpack {}", binary.display()))?;

    let stem = executable_name
        .strip_suffix(".exe")
        .unwrap_or(&executable_name);
    let output = output.map_or_else(
        || PathBuf::from(format!("{stem}.extracted")),
        Path::to_path_buf,
    );

    prepare_output(&output, force)?;
    for file in &project.files {
        // Every segment was checked by `project::build`, so the join stays
        // inside `output`.
        let path: PathBuf = std::iter::once(output.as_path())
            .chain(file.path.split('/').map(Path::new))
            .collect();
        write_new(&path, &file.contents, force)?;
    }
    for warning in &project.warnings {
        eprintln!("warning: {warning}");
    }

    let count = |tree| {
        project
            .files
            .iter()
            .filter(|file| file.tree == tree)
            .count()
    };
    let sources = count(Tree::Sources);
    println!(
        "wrote {} module(s) to {} and {sources} source file(s) to {}",
        count(Tree::Bundled),
        output.join("bundled").display(),
        output.join("sources").display(),
    );
    if sources == 0 {
        println!("no module has a source map, so only the bundled code was extracted");
    }
    Ok(())
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
/// also catches two project paths that differ only in case on a
/// case-insensitive file system, which `project::build` cannot see.
fn write_new(path: &Path, contents: &[u8], force: bool) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .expect("project paths have at least two segments");
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
