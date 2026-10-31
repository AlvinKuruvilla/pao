//! Reads the Bun release that built an executable from the version string in
//! its runtime image.
//!
//! No field in the payload records the release. The runtime carries it as the
//! literal `Bun v<major>.<minor>.<patch> (<commit>)`, `<commit>` being 8 or 9
//! lowercase hex digits. In each of the 74 `tools/bun-matrix` executables
//! (Bun 1.0.0 to 1.4.4) that pattern had exactly one distinct match, naming
//! the release that built the file.
//!
//! Only the runtime image is searched, the bytes outside the payload, because
//! bundled code can contain the same text. Anything other than exactly one
//! distinct match is an error. A missing or conflicting string means the file
//! was altered or built by a Bun this crate has not seen, and guessing a
//! release from the layout instead would make version-keyed tables, such as
//! the loader names (`docs/format.md` section 7.1), wrong without any sign.

use std::fmt;

use memchr::memmem;

use crate::locate::{Located, TRAILER};

const PREFIX: &[u8] = b"Bun v";

/// A Bun release as its runtime names itself.
///
/// `commit` identifies the build, not the release: a Bun built from a fork or
/// a non-release commit reports the release's numbers with its own commit. Key
/// version-dependent behaviour on the numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BunVersion<'a> {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub commit: &'a str,
}

impl BunVersion<'_> {
    /// `(major, minor, patch)`, which orders releases.
    pub fn release(&self) -> (u32, u32, u32) {
        (self.major, self.minor, self.patch)
    }
}

impl fmt::Display for BunVersion<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{} ({})", self.major, self.minor, self.patch, self.commit)
    }
}

/// Why [`find_version`] found no single release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// The runtime image has no version string.
    NotFound,
    /// The runtime image names more than one release; each as `BunVersion`
    /// displays it.
    Ambiguous(Vec<String>),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::NotFound => write!(
                f,
                "the runtime has no `Bun v<version> (<commit>)` string, so the Bun release that built \
                 this executable cannot be determined; the file may have been altered"
            ),
            VersionError::Ambiguous(versions) => write!(
                f,
                "the runtime names more than one Bun release ({}), so which one built this \
                 executable cannot be determined; please report this with the executable",
                versions.join(", ")
            ),
        }
    }
}

impl std::error::Error for VersionError {}

/// Finds the one Bun release named in `binary`'s runtime image.
///
/// # Panics
///
/// If `located` was not produced by [`locate`](crate::locate::locate) from
/// this `binary`, so that its payload lies outside it.
pub fn find_version<'a>(
    binary: &'a [u8],
    located: &Located<'_>,
) -> Result<BunVersion<'a>, VersionError> {
    let payload_end = located.trailer_pos + TRAILER.len();
    let runtime = [&binary[..located.base], &binary[payload_end..]];

    // A runtime holds the string several times (help text, crash reports),
    // so duplicates are expected and collapse here; only distinct values count.
    let mut found: Vec<BunVersion<'a>> = Vec::new();
    for region in runtime {
        for at in memmem::find_iter(region, PREFIX) {
            if let Some(version) = parse_after_prefix(&region[at + PREFIX.len()..])
                && !found.contains(&version)
            {
                found.push(version);
            }
        }
    }

    match found.len() {
        0 => Err(VersionError::NotFound),
        1 => Ok(found[0]),
        _ => Err(VersionError::Ambiguous(found.iter().map(ToString::to_string).collect())),
    }
}

/// Parses `<major>.<minor>.<patch> (<commit>)` at the start of `bytes`.
fn parse_after_prefix(bytes: &[u8]) -> Option<BunVersion<'_>> {
    let (major, rest) = parse_number(bytes)?;
    let (minor, rest) = parse_number(rest.strip_prefix(b".")?)?;
    let (patch, rest) = parse_number(rest.strip_prefix(b".")?)?;
    let rest = rest.strip_prefix(b" (")?;

    // 8 digits up to Bun 1.3.x, 9 in 1.4.x. Other lengths are other strings
    // that happen to start the same way, such as `Bun v1.2.12 (Windows x64)`.
    let hex_len = rest.iter().take_while(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')).count();
    if !(8..=9).contains(&hex_len) || rest.get(hex_len) != Some(&b')') {
        return None;
    }
    let commit = std::str::from_utf8(&rest[..hex_len]).expect("hex digits are ASCII");
    Some(BunVersion { major, minor, patch, commit })
}

/// A decimal `u32` at the start of `bytes`, and the bytes after it.
fn parse_number(bytes: &[u8]) -> Option<(u32, &[u8])> {
    let digits = bytes.iter().take_while(|byte| byte.is_ascii_digit()).count();
    // `parse` rejects the empty string and values past `u32::MAX`.
    let value = std::str::from_utf8(&bytes[..digits]).ok()?.parse().ok()?;
    Some((value, &bytes[digits..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_version::LayoutVersion;
    use crate::offsets::Offsets;
    use crate::payload::Payload;
    use crate::string_pointer::StringPointer;

    fn version(text: &str) -> Option<BunVersion<'_>> {
        parse_after_prefix(text.strip_prefix("Bun v").expect("test strings start with the prefix").as_bytes())
    }

    #[test]
    fn parses_both_commit_lengths() {
        assert_eq!(
            version("Bun v1.3.7 (ba426210) macOS"),
            Some(BunVersion { major: 1, minor: 3, patch: 7, commit: "ba426210" })
        );
        assert_eq!(
            version("Bun v1.4.4 (38afee6c8)"),
            Some(BunVersion { major: 1, minor: 4, patch: 4, commit: "38afee6c8" })
        );
    }

    /// Strings in the runtime that start like a version string and are not one.
    #[test]
    fn rejects_lookalikes() {
        assert_eq!(version("Bun v1.2.12 (Windows x64)"), None);
        assert_eq!(version("Bun v1.4.4 (38afee6c8a)"), None); // 10 digits
        assert_eq!(version("Bun v1.4.4 (38afee6)"), None); // 7 digits
        assert_eq!(version("Bun v1.4 (38afee6c8)"), None);
        assert_eq!(version("Bun v99999999999.0.0 (38afee6c8)"), None); // past u32::MAX
    }

    /// Runtime bytes, then `payload`, the trailer, and more runtime bytes;
    /// returns the bytes with the payload's `base` and `trailer_pos`.
    fn fake_executable(runtime_before: &str, payload: &str, runtime_after: &str) -> (Vec<u8>, usize, usize) {
        let mut binary = runtime_before.as_bytes().to_vec();
        let base = binary.len();
        binary.extend_from_slice(payload.as_bytes());
        let trailer_pos = binary.len();
        binary.extend_from_slice(TRAILER);
        binary.extend_from_slice(runtime_after.as_bytes());
        (binary, base, trailer_pos)
    }

    /// A `Located` for that payload. Only `base` and `trailer_pos` are
    /// meaningful, as they are all `find_version` reads.
    fn located(binary: &[u8], base: usize, trailer_pos: usize) -> Located<'_> {
        Located {
            layout: LayoutVersion::V1_3_14,
            offsets: Offsets {
                byte_count: 0,
                modules_ptr: StringPointer { offset: 0, length: 0 },
                entry_point_id: 0,
                compile_exec_argv_ptr: None,
                flags: None,
            },
            payload: Payload::new(&binary[base..trailer_pos]),
            base,
            trailer_pos,
            entries: Vec::new(),
        }
    }

    /// The same release named twice, and another named inside the payload:
    /// the payload's copy is bundled code, not the runtime, so it is ignored.
    #[test]
    fn ignores_duplicates_and_the_payload() {
        let (binary, base, trailer_pos) = fake_executable(
            "help: Bun v1.4.4 (38afee6c8) ... crash: Bun v1.4.4 (38afee6c8)",
            "const banner = 'Bun v0.9.0 (0123abcd)';",
            "",
        );
        let found = find_version(&binary, &located(&binary, base, trailer_pos));
        assert_eq!(found.expect("one distinct release").to_string(), "1.4.4 (38afee6c8)");
    }

    #[test]
    fn rejects_two_releases() {
        let (binary, base, trailer_pos) =
            fake_executable("Bun v1.4.4 (38afee6c8)", "", "Bun v1.4.0 (0123abcd4)");
        let found = find_version(&binary, &located(&binary, base, trailer_pos));
        assert_eq!(
            found,
            Err(VersionError::Ambiguous(vec!["1.4.4 (38afee6c8)".into(), "1.4.0 (0123abcd4)".into()]))
        );
    }

    #[test]
    fn reports_a_missing_version() {
        let (binary, base, trailer_pos) = fake_executable("no version here", "", "");
        let found = find_version(&binary, &located(&binary, base, trailer_pos));
        assert_eq!(found, Err(VersionError::NotFound));
    }
}
