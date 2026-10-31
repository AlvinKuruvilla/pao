use std::ops::Range;

/// A slice of the payload, as `(offset, length)`. The offset counts from the
/// payload base, not from the start of the file; use
/// [`Payload::get`](crate::payload::Payload::get) to read the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringPointer {
    pub offset: u32,
    pub length: u32,
}

impl StringPointer {
    /// `offset .. offset + length`, or `None` if that overflows `usize`.
    pub fn range(self) -> Option<Range<usize>> {
        let start = self.offset as usize;
        Some(start..start.checked_add(self.length as usize)?)
    }
}