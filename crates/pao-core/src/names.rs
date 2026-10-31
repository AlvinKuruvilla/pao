//! Names for the single-byte fields of a module entry, by Bun release.
//!
//! `loader` is the `u8` value of Bun's `Loader` enum, whose numbering changed
//! twice: each change inserted a variant in the middle and moved every value
//! above it up by one. So the same byte means different loaders in different
//! releases, and naming it needs the exact release ([`BunVersion`]); the
//! layout is not enough, because the 1.2.5 change falls inside layout
//! `V1_1_30`. Every other change appended a variant.
//!
//! The history below was read from the `Loader` enum at all 127 Bun 1.x
//! release tags, 1.0.0 to 1.4.4 (`src/options.zig`, then
//! `src/bundler/options.zig` from 1.3.14, then `src/ast/loader.rs` in 1.4).
//! The values that reach a module table in practice were checked on bytes by
//! `tests/loaders_corpus.rs`.
//!
//! ```text
//! release  change                                   commit
//! 1.0.0    jsx js ts tsx css file json toml wasm    822a00c4d508
//!          napi base64 dataurl text (0..=12)
//! 1.0.23   appended sqlite, sqlite_embedded         4ccf6fb28492
//! 1.0.24   inserted bunsh at 13                     d4794bbac82c
//! 1.1.43   appended html                            76800b049a70
//! 1.2.5    inserted jsonc at 7                      013fdddc6ed1
//! 1.2.21   appended yaml                            7c45ed97def1
//! 1.3.7    appended json5                           ba426210c28a
//! 1.3.8    appended md                              b64edcb490b4
//! 1.4.0    appended xml                             34cbb9a40b4b
//! ```
//!
//! A value that the release's enum does not define names nothing: the
//! functions return `None` rather than borrow a name from another release.

use crate::version::BunVersion;

// The three numberings, each in full; a release uses a prefix of one of them.
const LOADERS_BEFORE_1_0_24: &[&str] = &[
    "jsx",
    "js",
    "ts",
    "tsx",
    "css",
    "file",
    "json",
    "toml",
    "wasm",
    "napi",
    "base64",
    "dataurl",
    "text",
    "sqlite",
    "sqlite_embedded",
];
const LOADERS_BEFORE_1_2_5: &[&str] = &[
    "jsx",
    "js",
    "ts",
    "tsx",
    "css",
    "file",
    "json",
    "toml",
    "wasm",
    "napi",
    "base64",
    "dataurl",
    "text",
    "bunsh",
    "sqlite",
    "sqlite_embedded",
    "html",
];
const LOADERS_FROM_1_2_5: &[&str] = &[
    "jsx",
    "js",
    "ts",
    "tsx",
    "css",
    "file",
    "json",
    "jsonc",
    "toml",
    "wasm",
    "napi",
    "base64",
    "dataurl",
    "text",
    "bunsh",
    "sqlite",
    "sqlite_embedded",
    "html",
    "yaml",
    "json5",
    "md",
    "xml",
];

/// The loader names `version` defines, in value order.
///
/// Releases after 1.4.4 are untested and get 1.4.4's table: a loader they
/// append names nothing, and an insertion like 1.2.5's would misname values
/// until it is added here.
fn loaders(version: &BunVersion<'_>) -> &'static [&'static str] {
    let (table, defined) = match version.release() {
        release if release < (1, 0, 23) => (LOADERS_BEFORE_1_0_24, 13),
        release if release < (1, 0, 24) => (LOADERS_BEFORE_1_0_24, 15),
        release if release < (1, 1, 43) => (LOADERS_BEFORE_1_2_5, 16),
        release if release < (1, 2, 5) => (LOADERS_BEFORE_1_2_5, 17),
        release if release < (1, 2, 21) => (LOADERS_FROM_1_2_5, 18),
        release if release < (1, 3, 7) => (LOADERS_FROM_1_2_5, 19),
        release if release < (1, 3, 8) => (LOADERS_FROM_1_2_5, 20),
        release if release < (1, 4, 0) => (LOADERS_FROM_1_2_5, 21),
        _ => (LOADERS_FROM_1_2_5, 22),
    };
    &table[..defined]
}

/// The name of loader `value` in `version`, or `None` if that release does
/// not define it.
pub fn loader(value: u8, version: &BunVersion<'_>) -> Option<&'static str> {
    loaders(version).get(usize::from(value)).copied()
}

/// `module_format`; the same in every release that has the field.
pub fn module_format(value: u8) -> Option<&'static str> {
    ["none", "esm", "cjs"].get(usize::from(value)).copied()
}

/// `encoding`. Value 2 was `utf8`, defined but never written, until Bun 1.4.1
/// (`4661e494f052`) reused it for UTF-16 code units.
pub fn encoding(value: u8, version: &BunVersion<'_>) -> Option<&'static str> {
    let two = if version.release() < (1, 4, 1) {
        "utf8"
    } else {
        "utf16"
    };
    ["binary", "latin1", two].get(usize::from(value)).copied()
}

/// `side`; the same in every release that has the field.
pub fn side(value: u8) -> Option<&'static str> {
    ["server", "client"].get(usize::from(value)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bun(major: u32, minor: u32, patch: u32) -> BunVersion<'static> {
        BunVersion {
            major,
            minor,
            patch,
            commit: "00000000",
        }
    }

    /// Each insertion changes what one byte means between adjacent releases.
    #[test]
    fn insertions_renumber_at_their_release() {
        assert_eq!(loader(13, &bun(1, 0, 23)), Some("sqlite"));
        assert_eq!(loader(13, &bun(1, 0, 24)), Some("bunsh"));
        assert_eq!(loader(7, &bun(1, 2, 4)), Some("toml"));
        assert_eq!(loader(7, &bun(1, 2, 5)), Some("jsonc"));
        assert_eq!(loader(16, &bun(1, 2, 4)), Some("html"));
        assert_eq!(loader(16, &bun(1, 2, 5)), Some("sqlite_embedded"));
    }

    /// A value beyond what the release defines names nothing, even where a
    /// later release defines it.
    #[test]
    fn appended_loaders_start_at_their_release() {
        assert_eq!(loader(13, &bun(1, 0, 22)), None);
        assert_eq!(loader(16, &bun(1, 1, 42)), None);
        assert_eq!(loader(16, &bun(1, 1, 43)), Some("html"));
        assert_eq!(loader(21, &bun(1, 3, 14)), None);
        assert_eq!(loader(21, &bun(1, 4, 0)), Some("xml"));
        assert_eq!(loader(22, &bun(1, 4, 4)), None);
    }

    /// `loaders` slices a numbering to the release's length, which panics if
    /// a length outgrows its numbering; this visits every boundary.
    #[test]
    fn table_lengths_fit_their_numberings() {
        for release in [
            (1, 0, 0),
            (1, 0, 23),
            (1, 1, 0),
            (1, 1, 43),
            (1, 2, 5),
            (1, 2, 21),
            (1, 3, 7),
            (1, 3, 8),
            (1, 4, 4),
        ] {
            let table = loaders(&bun(release.0, release.1, release.2));
            assert!(!table.is_empty(), "{release:?}");
        }
    }

    #[test]
    fn encoding_two_changes_meaning_at_1_4_1() {
        assert_eq!(encoding(2, &bun(1, 4, 0)), Some("utf8"));
        assert_eq!(encoding(2, &bun(1, 4, 1)), Some("utf16"));
        assert_eq!(encoding(3, &bun(1, 4, 4)), None);
    }
}
