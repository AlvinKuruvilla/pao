//! Finds the payload in an executable without parsing Mach-O, PE or ELF.
//!
//! Every payload ends with [`TRAILER`]. The scanner tries each known layout
//! at each occurrence and accepts one only if every pointer in its module
//! table lands in the payload and every name is text. The runtime's own copy
//! of the trailer, a string literal, sits among machine code and fails within
//! a few checks.
//!
//! All (trailer, layout) pairs are tried, even after one decodes. A second
//! decoding means the checks cannot tell layouts apart, and
//! [`LocateError::Ambiguous`] reports it where a first-match scan would hide it.
//!
//! # Trade-offs
//!
//! - **No name-prefix check**: requiring `/$bunfs/root/` and its relatives
//!   would make false matches rarer, but a Bun release that changed the
//!   prefix would fail as [`LocateError::NotFound`] despite a known layout.
//!   Structural checks alone gave one decoding in each of the 110 binaries
//!   from `tools/bun-matrix` (Bun 1.0.0–1.4.4) and in one large production
//!   binary, both in this crate and in `tools/probe.py`.
//! - **Eager module table**: the checks read every entry anyway, so
//!   [`Located::modules`] keeps each one with the slices it names. Callers
//!   then read a module's name and bytes without re-checking pointers that
//!   were already checked, at the cost of three slice references per module.

use std::fmt;

use memchr::memmem;

use crate::entry::{ModuleEntry, parse_entry};
use crate::layout_version::LayoutVersion;
use crate::offsets::{Offsets, parse_offsets};
use crate::payload::Payload;

/// The 16 bytes that end every payload, in every Bun release since 1.0.0.
pub const TRAILER: &[u8] = b"\n---- Bun! ----\n";

/// The longest module name accepted, in bytes.
///
/// Linux's `PATH_MAX`, as in `probe.py`. Real names are far shorter. Set too
/// low, it rejects real binaries; too high, it only loosens the garbage filter.
const MAX_NAME_LEN: usize = 4096;

/// A payload found in an executable, and its decoded module table.
#[derive(Debug)]
pub struct Located<'a> {
    pub layout: LayoutVersion,
    pub offsets: Offsets,
    pub payload: Payload<'a>,
    /// File offset of payload byte 0. Add it to a pointer's offset to get the
    /// pointer's position in the file.
    pub base: usize,
    /// File offset of the trailer.
    pub trailer_pos: usize,
    /// The module table in file order. `offsets.entry_point_id` indexes it.
    pub modules: Vec<Module<'a>>,
}

/// One module table entry and the payload bytes it names.
///
/// The slices were cut by [`locate`] when it checked the entry, so reading
/// them cannot fail. Pointers the slices do not cover (bytecode, module info)
/// are still in `entry`, already checked to lie within the payload.
#[derive(Debug, Clone, Copy)]
pub struct Module<'a> {
    /// The raw entry. Its pointer offsets locate the slices in the file.
    pub entry: ModuleEntry,
    /// Non-empty, at most 4,096 bytes, no control characters.
    pub name: &'a str,
    pub contents: &'a [u8],
    /// Empty when the module has no source map.
    pub sourcemap: &'a [u8],
}

/// Why [`locate`] found no single payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocateError {
    /// No layout decoded. Zero `trailers` means not a Bun executable; more
    /// usually means a Bun release with an unknown layout.
    NotFound { trailers: usize },
    /// More than one (trailer position, layout) pair decoded: the checks are
    /// too weak, which is a bug in this crate.
    Ambiguous(Vec<(usize, LayoutVersion)>),
}

impl fmt::Display for LocateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocateError::NotFound { trailers: 0 } => {
                write!(
                    f,
                    "no Bun trailer found; this is not a `bun build --compile` executable"
                )
            }
            LocateError::NotFound { trailers } => write!(
                f,
                "found {trailers} Bun trailers but no known layout decodes at any of them; \
                 the executable may come from a Bun release newer than this parser"
            ),
            LocateError::Ambiguous(candidates) => {
                write!(
                    f,
                    "more than one layout decodes, so the result cannot be trusted:"
                )?;
                for (trailer_pos, layout) in candidates {
                    write!(f, " {layout:?} at trailer offset {trailer_pos:#x};")?;
                }
                write!(f, " please report this with the executable's Bun version")
            }
        }
    }
}

impl std::error::Error for LocateError {}

/// Finds the one payload in `binary`, a whole executable read into memory.
pub fn locate(binary: &[u8]) -> Result<Located<'_>, LocateError> {
    let mut trailers = 0;
    let mut found = Vec::new();
    for trailer_pos in memmem::rfind_iter(binary, TRAILER) {
        trailers += 1;
        for layout in LayoutVersion::LAYOUT_VERSIONS {
            found.extend(try_layout(binary, trailer_pos, layout));
        }
    }

    match found.len() {
        0 => Err(LocateError::NotFound { trailers }),
        1 => Ok(found.pop().expect("length is 1")),
        _ => Err(LocateError::Ambiguous(
            found
                .iter()
                .map(|located| (located.trailer_pos, located.layout))
                .collect(),
        )),
    }
}

/// Decodes `layout` at `trailer_pos`, or `None` at the first failed check.
fn try_layout(binary: &[u8], trailer_pos: usize, layout: LayoutVersion) -> Option<Located<'_>> {
    // Layout: payload, Offsets, trailer. `try_from`, not `as`: on a 32-bit
    // target a corrupt `byte_count` would truncate to a plausible value.
    let offsets_pos = trailer_pos.checked_sub(layout.offsets_size())?;
    let offsets = parse_offsets(&binary[offsets_pos..trailer_pos], layout)?;
    let base = offsets_pos.checked_sub(usize::try_from(offsets.byte_count).ok()?)?;
    let payload = Payload::new(&binary[base..offsets_pos]);

    // Not needed for decoding; checked because garbage rarely passes it.
    if let Some(argv) = offsets.compile_exec_argv_ptr {
        payload.get(argv)?;
    }

    let table = payload.get(offsets.modules_ptr)?;
    let entry_size = layout.entry_size();
    if table.is_empty() || table.len() % entry_size != 0 {
        return None;
    }
    if usize::try_from(offsets.entry_point_id).ok()? >= table.len() / entry_size {
        return None;
    }

    let modules = table
        .chunks_exact(entry_size)
        .map(|row| {
            let entry = parse_entry(row, layout)?;
            let optional = [entry.bytecode, entry.module_info, entry.bytecode_origin_path];
            for pointer in optional.into_iter().flatten() {
                payload.get(pointer)?;
            }

            // Names are paths. UTF-8 rather than probe.py's printable ASCII,
            // which would reject modules named in other scripts.
            let name = std::str::from_utf8(payload.get(entry.name)?).ok()?;
            if name.is_empty() || name.len() > MAX_NAME_LEN || name.chars().any(char::is_control) {
                return None;
            }

            Some(Module {
                entry,
                name,
                contents: payload.get(entry.contents)?,
                sourcemap: payload.get(entry.sourcemap)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    Some(Located {
        layout,
        offsets,
        payload,
        base,
        trailer_pos,
        modules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_missing_trailer() {
        let error = locate(b"\x7fELF not a bun binary").expect_err("input has no trailer");
        assert_eq!(error, LocateError::NotFound { trailers: 0 });
    }

    /// A trailer with nothing before it: every layout's `checked_sub` fails
    /// rather than panicking.
    #[test]
    fn rejects_trailer_at_file_start() {
        let error = locate(TRAILER).expect_err("no room for an Offsets record");
        assert_eq!(error, LocateError::NotFound { trailers: 1 });
    }
}
