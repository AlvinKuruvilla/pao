//! Decompression of a single zstd frame whose size the frame records, with a
//! cap on that size.
//!
//! Two things in an executable are zstd frames: the original sources in a
//! source map, which Bun compresses itself (`docs/format.md` section 6.1),
//! and `.zst` files an app embeds as assets. Both carry a decompressed size
//! read from the executable and checked against nothing, so it is capped
//! before anything is allocated.

use std::fmt;

/// The largest decompressed size accepted. Arbitrary: 256 MiB is far above
/// any source file or text asset seen, and low enough that a forged frame
/// size cannot exhaust memory. Raising it only matters for generated files.
pub const MAX_DECOMPRESSED_LEN: u64 = 256 << 20;

/// The four bytes every zstd frame starts with (RFC 8878 section 3.1.1).
pub const MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Why a frame did not decompress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// The bytes do not start with a zstd frame header.
    NotZstd,
    /// The frame does not record its decompressed size. Bun's source map
    /// writer always records it.
    UnknownSize,
    /// The frame claims more than [`MAX_DECOMPRESSED_LEN`] bytes.
    TooLarge { claimed: u64 },
    /// zstd rejected the frame, or it decompressed to a size other than the
    /// one it claims.
    Corrupt(String),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::NotZstd => write!(f, "not a zstd frame"),
            FrameError::UnknownSize => write!(f, "the zstd frame does not record its size"),
            FrameError::TooLarge { claimed } => write!(
                f,
                "the zstd frame claims {claimed} bytes, over the {MAX_DECOMPRESSED_LEN}-byte limit"
            ),
            FrameError::Corrupt(message) => write!(f, "the zstd frame is corrupt: {message}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// The bytes `frame` decompresses to.
pub fn decompress(frame: &[u8]) -> Result<Vec<u8>, FrameError> {
    if !frame.starts_with(&MAGIC) {
        return Err(FrameError::NotZstd);
    }
    let claimed = zstd::zstd_safe::get_frame_content_size(frame)
        .map_err(|_| FrameError::NotZstd)?
        .ok_or(FrameError::UnknownSize)?;
    if claimed > MAX_DECOMPRESSED_LEN {
        return Err(FrameError::TooLarge { claimed });
    }
    // `bulk::decompress` fails rather than writing past `claimed` bytes, so
    // the cap holds even if the frame lies about its size.
    let bytes = zstd::bulk::decompress(frame, claimed as usize)
        .map_err(|error| FrameError::Corrupt(error.to_string()))?;
    if bytes.len() as u64 != claimed {
        return Err(FrameError::Corrupt(format!(
            "decompressed to {} bytes, frame claims {claimed}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let frame = zstd::bulk::compress(b"export const a = 1;\n", 1).expect("compresses");
        assert_eq!(decompress(&frame), Ok(b"export const a = 1;\n".to_vec()));
    }

    #[test]
    fn refuses_a_frame_claiming_more_than_the_cap() {
        // A bare frame header (RFC 8878 section 3.1.1.1): the magic number,
        // then a descriptor of 0xE0 (an 8-byte content size, single segment),
        // then the size. No blocks follow; the size is read from the header
        // alone, so the cap must trip before decompression starts.
        let mut forged = MAGIC.to_vec();
        forged.push(0xE0);
        forged.extend((1u64 << 40).to_le_bytes());
        assert_eq!(
            decompress(&forged),
            Err(FrameError::TooLarge { claimed: 1 << 40 })
        );
    }

    #[test]
    fn refuses_bytes_that_are_not_zstd() {
        assert_eq!(decompress(b"# README\n"), Err(FrameError::NotZstd));
    }
}
