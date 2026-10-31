use crate::string_pointer::StringPointer;

/// Fixed-position little-endian reads.
pub trait LittleEndianByteReader {
    fn u8_at(&self, at: usize) -> Option<u8>;
    fn u32_at(&self, at: usize) -> Option<u32>;
    fn u64_at(&self, at: usize) -> Option<u64>;
    fn pointer_at(&self, at: usize) -> Option<StringPointer>;
}
impl LittleEndianByteReader for [u8] {
    fn u8_at(&self, at: usize) -> Option<u8> {
        self.get(at).copied()
    }
    fn u32_at(&self, at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            self.get(at..at.checked_add(4)?)?.try_into().ok()?,
        ))
    }
    fn u64_at(&self, at: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            self.get(at..at.checked_add(8)?)?.try_into().ok()?,
        ))
    }
    fn pointer_at(&self, at: usize) -> Option<StringPointer> {
        Some(StringPointer {
            offset: self.u32_at(at)?,
            length: self.u32_at(at.checked_add(4)?)?,
        })
    }
}
