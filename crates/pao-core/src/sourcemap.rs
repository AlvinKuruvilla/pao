//! The serialized source map a module's `sourcemap` pointer names: the
//! original source files the module was bundled from, each compressed with
//! zstd, and the mappings from the module's code back into them.
//!
//! `docs/format.md` section 6.1 specifies the layout. It has been the same
//! from Bun 1.1.30, the first release to embed one, to 1.4.4:
//!
//! ```text
//!  0       4       8                    8+8n                 8+16n          8+16n+m
//!  +-------+-------+--------------------+--------------------+--------------+---------+
//!  | n     | m     | names: n x         | sources: n x       | mappings     | strings |
//!  | u32   | u32   | StringPointer      | StringPointer      | m bytes      |         |
//!  +-------+-------+--------------------+--------------------+--------------+---------+
//! ```
//!
//! Pointers here are relative to the start of the source map, not the
//! payload, so they are resolved against the source map's own bytes.
//!
//! The mappings are not decoded. They changed from VLQ text to Bun's
//! `InternalSourceMap` blob between 1.3.7 and 1.3.14, and recovering the
//! original sources needs only the names and the compressed files.
//!
//! # Trade-offs
//!
//! - **Decompression on request**: [`SourceFile::decompress`] runs per file,
//!   so a caller that lists names pays nothing for the sources.
//! - **A size cap**: a file whose zstd frame claims more than
//!   [`MAX_SOURCE_LEN`] bytes is refused rather than allocated, because the
//!   claim comes from the executable and is not checked against anything.

use std::fmt;

use crate::byte_reader::LittleEndianByteReader;
use crate::string_pointer::StringPointer;

/// The largest decompressed source file accepted. Arbitrary: 256 MiB is far
/// above any hand-written source file, and low enough that a forged frame
/// size cannot exhaust memory. Raising it only matters for generated files.
pub const MAX_SOURCE_LEN: u64 = 256 << 20;

/// `n` and `m`, the two `u32`s before the pointer arrays.
const HEADER_LEN: usize = 8;
const POINTER_LEN: usize = 8;

/// A decoded source map, borrowing from the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMap<'a> {
    /// In the order Bun listed them, which is the order the mappings index.
    pub files: Vec<SourceFile<'a>>,
}

/// One original source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceFile<'a> {
    /// The path Bun's JSON source map gives under `sources`: relative to the
    /// directory Bun was run from, `/`-separated, and possibly starting with
    /// `../`. The directory itself is not recorded anywhere in the executable.
    pub name: &'a str,
    /// One zstd frame holding the file's UTF-8 text.
    pub compressed: &'a [u8],
}

/// Why a source map did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMapError {
    /// Shorter than its header and pointer arrays.
    Truncated,
    /// Pointer `index` (names first, then sources) runs past the source map.
    PointerOutOfRange { index: usize },
    /// File `index`'s name is not UTF-8.
    NameNotUtf8 { index: usize },
    /// The file's zstd frame does not record its decompressed size. Bun's
    /// writer always records it.
    UnknownSize { name: String },
    /// The file's zstd frame claims more than [`MAX_SOURCE_LEN`] bytes.
    TooLarge { name: String, claimed: u64 },
    /// zstd rejected the file's frame.
    Decompress { name: String, message: String },
}

impl fmt::Display for SourceMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceMapError::Truncated => {
                write!(f, "source map is shorter than its own header says")
            }
            SourceMapError::PointerOutOfRange { index } => {
                write!(
                    f,
                    "source map pointer {index} runs past the end of the source map"
                )
            }
            SourceMapError::NameNotUtf8 { index } => {
                write!(f, "source map file name {index} is not UTF-8")
            }
            SourceMapError::UnknownSize { name } => {
                write!(
                    f,
                    "compressed source {name} does not record its size; Bun always writes one"
                )
            }
            SourceMapError::TooLarge { name, claimed } => write!(
                f,
                "compressed source {name} claims {claimed} bytes, over the {MAX_SOURCE_LEN}-byte limit"
            ),
            SourceMapError::Decompress { name, message } => {
                write!(f, "compressed source {name} does not decompress: {message}")
            }
        }
    }
}

impl std::error::Error for SourceMapError {}

/// Decodes the source map in `bytes`, the slice an entry's `sourcemap`
/// pointer names.
pub fn parse(bytes: &[u8]) -> Result<SourceMap<'_>, SourceMapError> {
    let count = bytes.u32_at(0).ok_or(SourceMapError::Truncated)? as usize;
    // The pointer arrays must fit before anything is read from them, so a
    // forged count fails here instead of driving a long loop.
    let pointers_end = count
        .checked_mul(2 * POINTER_LEN)
        .and_then(|len| len.checked_add(HEADER_LEN))
        .filter(|&end| end <= bytes.len())
        .ok_or(SourceMapError::Truncated)?;
    debug_assert!(pointers_end <= bytes.len());

    let slice = |index: usize| -> Result<&[u8], SourceMapError> {
        let pointer: StringPointer = bytes
            .pointer_at(HEADER_LEN + index * POINTER_LEN)
            .expect("the pointer arrays were checked to fit");
        pointer
            .range()
            .and_then(|range| bytes.get(range))
            .ok_or(SourceMapError::PointerOutOfRange { index })
    };

    let files = (0..count)
        .map(|index| {
            let name = std::str::from_utf8(slice(index)?)
                .map_err(|_| SourceMapError::NameNotUtf8 { index })?;
            Ok(SourceFile {
                name,
                compressed: slice(count + index)?,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(SourceMap { files })
}

impl SourceFile<'_> {
    /// The file's original text. Bun wrote it from a JSON string, so it is
    /// UTF-8, but that is not checked here.
    pub fn decompress(&self) -> Result<Vec<u8>, SourceMapError> {
        let name = || self.name.to_string();
        let claimed = zstd::zstd_safe::get_frame_content_size(self.compressed)
            .map_err(|_| SourceMapError::Decompress {
                name: name(),
                message: "not a zstd frame".to_string(),
            })?
            .ok_or_else(|| SourceMapError::UnknownSize { name: name() })?;
        if claimed > MAX_SOURCE_LEN {
            return Err(SourceMapError::TooLarge {
                name: name(),
                claimed,
            });
        }
        // `bulk::decompress` fails rather than writing past `claimed` bytes,
        // so the cap holds even if the frame lies about its size.
        let text = zstd::bulk::decompress(self.compressed, claimed as usize).map_err(|error| {
            SourceMapError::Decompress {
                name: name(),
                message: error.to_string(),
            }
        })?;
        if text.len() as u64 != claimed {
            return Err(SourceMapError::Decompress {
                name: name(),
                message: format!(
                    "decompressed to {} bytes, frame claims {claimed}",
                    text.len()
                ),
            });
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a source map the way Bun's `serializeJsonSourceMapForStandalone`
    /// does: names, then compressed sources, then the mappings, then strings.
    fn serialize(files: &[(&str, &str)], mappings: &[u8]) -> Vec<u8> {
        let compressed: Vec<Vec<u8>> = files
            .iter()
            .map(|(_, text)| zstd::bulk::compress(text.as_bytes(), 1).expect("compresses"))
            .collect();
        let strings_start = HEADER_LEN + files.len() * 2 * POINTER_LEN + mappings.len();
        let mut header: Vec<u8> = Vec::new();
        let mut strings: Vec<u8> = Vec::new();
        header.extend((files.len() as u32).to_le_bytes());
        header.extend((mappings.len() as u32).to_le_bytes());
        let blobs = files
            .iter()
            .map(|(name, _)| name.as_bytes())
            .chain(compressed.iter().map(Vec::as_slice));
        for blob in blobs {
            header.extend(((strings_start + strings.len()) as u32).to_le_bytes());
            header.extend((blob.len() as u32).to_le_bytes());
            strings.extend(blob);
        }
        header.extend(mappings);
        header.extend(strings);
        header
    }

    #[test]
    fn decodes_names_and_sources() {
        let bytes = serialize(
            &[
                ("../shared.ts", "export const a = 1;\n"),
                ("index.ts", "import \"../shared\";\n"),
            ],
            b"AAAA;",
        );
        let map = parse(&bytes).expect("decodes");
        let names: Vec<&str> = map.files.iter().map(|file| file.name).collect();
        assert_eq!(names, ["../shared.ts", "index.ts"]);
        assert_eq!(
            map.files[1].decompress().expect("decompresses"),
            b"import \"../shared\";\n"
        );
    }

    #[test]
    fn rejects_a_count_the_bytes_cannot_hold() {
        let mut bytes = serialize(&[("index.ts", "x")], b"");
        bytes[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&bytes), Err(SourceMapError::Truncated));
    }

    #[test]
    fn rejects_a_pointer_past_the_end() {
        let mut bytes = serialize(&[("index.ts", "x")], b"");
        // The first source pointer's length, after the name pointer.
        let at = HEADER_LEN + POINTER_LEN + 4;
        bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            parse(&bytes),
            Err(SourceMapError::PointerOutOfRange { index: 1 })
        );
    }

    #[test]
    fn refuses_a_frame_claiming_more_than_the_cap() {
        // A bare zstd frame header (RFC 8878 section 3.1.1.1): the magic
        // number, then a descriptor of 0xE0 (an 8-byte content size, single
        // segment), then the size. No blocks follow; the size is read from
        // the header alone, so the cap must trip before decompression starts.
        let mut forged = vec![0x28, 0xB5, 0x2F, 0xFD, 0xE0];
        forged.extend((1u64 << 40).to_le_bytes());
        let file = SourceFile {
            name: "index.ts",
            compressed: &forged,
        };
        assert_eq!(
            file.decompress(),
            Err(SourceMapError::TooLarge {
                name: "index.ts".to_string(),
                claimed: 1 << 40
            })
        );
    }
}
