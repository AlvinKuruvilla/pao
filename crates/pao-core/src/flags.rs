//! The bits of [`Offsets::flags`](crate::offsets::Offsets::flags), by Bun release.
//!
//! The 32-byte `Offsets` record ends in 4 bytes that were padding until Bun
//! 1.3.3 made them a bit set. Releases added bits in four steps, so whether a
//! bit means anything depends on the exact release ([`BunVersion`]); the
//! layout is not enough, because the first two steps fall inside layout
//! `V1_2_23` (1.2.23 to 1.3.7).
//!
//! The history was read from the flags type at all 31 Bun release tags from
//! 1.2.13 to 1.4.4 (a Zig `packed struct(u32)` of bools until 1.3.x, a
//! `bitflags` type in the Rust port):
//!
//! ```text
//! release  bits   commit
//! 1.3.3    0-1    abb1b0c4d75a
//! 1.3.4    2-3    5eb2145b3104
//! 1.4.1    4-10   4661e494f052
//! 1.4.3    11-13  c6da4a4d3010
//! ```
//!
//! In every matrix build before 1.3.3 the padding bytes are zero.

use crate::version::BunVersion;

pub const DISABLE_DEFAULT_ENV_FILES: u32 = 1 << 0;
pub const DISABLE_AUTOLOAD_BUNFIG: u32 = 1 << 1;
pub const DISABLE_AUTOLOAD_TSCONFIG: u32 = 1 << 2;
pub const DISABLE_AUTOLOAD_PACKAGE_JSON: u32 = 1 << 3;
/// Every module's contents lie in one run that no other region overlaps.
pub const SOURCE_TEXT_CONTIGUOUS: u32 = 1 << 4;
pub const HAS_SOURCE_HASHES: u32 = 1 << 5;
pub const HAS_BUILTIN_BYTECODE: u32 = 1 << 6;
pub const HAS_BYTECODE_STRING_TABLE: u32 = 1 << 7;
pub const HAS_STARTUP_MODULE_COUNT: u32 = 1 << 8;
pub const HAS_MODULE_INFO_STRING_TABLE: u32 = 1 << 9;
/// The bytecode was written by another platform's JavaScriptCore.
pub const CROSS_COMPILED_BYTECODE: u32 = 1 << 10;
pub const HAS_PRELINKED_MODULE_GRAPH: u32 = 1 << 11;
pub const HAS_RUNTIME_OPTIONS: u32 = 1 << 12;
/// Every module's bytecode lives in one shared payload, and a module's
/// `bytecode` pointer runs from its own entry to the end of that payload, so
/// its length is not the module's bytecode size (`docs/format.md` section 8.3).
pub const HAS_LINKED_BYTECODE_PAYLOAD: u32 = 1 << 13;

/// Each bit with its name in Bun's source and the release that defined it.
const BITS: &[(u32, &str, (u32, u32, u32))] = &[
    (DISABLE_DEFAULT_ENV_FILES, "disable_default_env_files", (1, 3, 3)),
    (DISABLE_AUTOLOAD_BUNFIG, "disable_autoload_bunfig", (1, 3, 3)),
    (DISABLE_AUTOLOAD_TSCONFIG, "disable_autoload_tsconfig", (1, 3, 4)),
    (DISABLE_AUTOLOAD_PACKAGE_JSON, "disable_autoload_package_json", (1, 3, 4)),
    (SOURCE_TEXT_CONTIGUOUS, "source_text_contiguous", (1, 4, 1)),
    (HAS_SOURCE_HASHES, "has_source_hashes", (1, 4, 1)),
    (HAS_BUILTIN_BYTECODE, "has_builtin_bytecode", (1, 4, 1)),
    (HAS_BYTECODE_STRING_TABLE, "has_bytecode_string_table", (1, 4, 1)),
    (HAS_STARTUP_MODULE_COUNT, "has_startup_module_count", (1, 4, 1)),
    (HAS_MODULE_INFO_STRING_TABLE, "has_module_info_string_table", (1, 4, 1)),
    (CROSS_COMPILED_BYTECODE, "cross_compiled_bytecode", (1, 4, 1)),
    (HAS_PRELINKED_MODULE_GRAPH, "has_prelinked_module_graph", (1, 4, 3)),
    (HAS_RUNTIME_OPTIONS, "has_runtime_options", (1, 4, 3)),
    (HAS_LINKED_BYTECODE_PAYLOAD, "has_linked_bytecode_payload", (1, 4, 3)),
];

/// The bits `version` defines. Zero before 1.3.3, where the bytes are padding.
///
/// Releases after 1.4.4 are untested and get 1.4.4's bits, so a bit they add
/// shows up as undefined rather than misnamed.
pub fn defined(version: &BunVersion<'_>) -> u32 {
    BITS.iter()
        .filter(|(_, _, since)| version.release() >= *since)
        .fold(0, |mask, (bit, _, _)| mask | bit)
}

/// Whether `bit` is set in `flags` and means something in `version`.
pub fn is_set(flags: u32, bit: u32, version: &BunVersion<'_>) -> bool {
    flags & bit & defined(version) != 0
}

/// The names of the bits set in `flags` that `version` defines, low bit first.
/// Set bits it does not define are `flags & !defined(version)`.
pub fn names<'v>(flags: u32, version: &'v BunVersion<'_>) -> impl Iterator<Item = &'static str> + 'v {
    BITS.iter()
        .filter(move |(bit, _, since)| flags & bit != 0 && version.release() >= *since)
        .map(|(_, name, _)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bun(major: u32, minor: u32, patch: u32) -> BunVersion<'static> {
        BunVersion { major, minor, patch, commit: "00000000" }
    }

    #[test]
    fn bits_appear_at_their_release() {
        assert_eq!(defined(&bun(1, 3, 2)), 0);
        assert_eq!(defined(&bun(1, 3, 3)), 0b11);
        assert_eq!(defined(&bun(1, 3, 4)), 0b1111);
        assert_eq!(defined(&bun(1, 4, 0)), 0b1111);
        assert_eq!(defined(&bun(1, 4, 1)), 0b111_1111_1111);
        assert_eq!(defined(&bun(1, 4, 3)), 0b11_1111_1111_1111);
    }

    /// 1.3.7 builds write 0xc; 1.4.0 has no linked-bytecode bit to set.
    #[test]
    fn names_only_defined_bits() {
        let names_of = |flags, version| names(flags, &version).collect::<Vec<_>>();
        assert_eq!(
            names_of(0xc, bun(1, 3, 7)),
            ["disable_autoload_tsconfig", "disable_autoload_package_json"]
        );
        assert_eq!(names_of(HAS_LINKED_BYTECODE_PAYLOAD, bun(1, 4, 0)), Vec::<&str>::new());
        assert!(!is_set(HAS_LINKED_BYTECODE_PAYLOAD, HAS_LINKED_BYTECODE_PAYLOAD, &bun(1, 4, 0)));
        assert!(is_set(HAS_LINKED_BYTECODE_PAYLOAD, HAS_LINKED_BYTECODE_PAYLOAD, &bun(1, 4, 3)));
    }
}
