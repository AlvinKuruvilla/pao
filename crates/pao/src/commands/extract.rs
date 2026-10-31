//! `pao extract`: writes every embedded module to a directory.

use std::path::Path;

use anyhow::bail;

// TODO(alvin): decide path safety for module names and whether to decompress
// `.zst` modules, then write each module under `output`.
pub fn run(_binary: &Path, _output: &Path) -> anyhow::Result<()> {
    bail!("`pao extract` is not implemented yet")
}
