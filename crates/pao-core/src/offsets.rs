use crate::byte_reader::LittleEndianByteReader;
use crate::layout_version::LayoutVersion;
use crate::string_pointer::StringPointer;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offsets {
    pub byte_count: u64,
    pub modules_ptr: StringPointer,
    pub entry_point_id: u32,
    pub compile_exec_argv_ptr: Option<StringPointer>, // 32-byte layouts only
    pub flags: Option<u32>,                           // V1_3_14 only
}

pub fn parse_offsets(bytes: &[u8], layout: LayoutVersion) -> Option<Offsets> {
    if bytes.len() != layout.offsets_size() {
        return None;
    }
    let byte_count = bytes.u64_at(0)?;
    let modules_ptr = bytes.pointer_at(8)?;
    let entry_point_id = bytes.u32_at(16)?;

    let compile_exec_argv_ptr = if layout.offsets_size() == 32 {
        Some(bytes.pointer_at(20)?)
    } else {
        None
    };
    let flags = if layout == LayoutVersion::V1_3_14 {
        Some(bytes.u32_at(28)?)
    } else {
        None
    };

    Some(Offsets {
        byte_count,
        modules_ptr,
        entry_point_id,
        compile_exec_argv_ptr,
        flags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 32 bytes immediately before the trailer in
    /// `1.4.4-bun-linux-x64-bytecode` from `tools/bun-matrix/build.sh`.
    /// `modules_ptr.length / 52` is 2: the entry module and the fixture's
    /// text import.
    const BUN_1_4_4_OFFSETS: [u8; 32] = [
        0x20, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // byte_count
        0x9f, 0x07, 0x00, 0x00, 0x68, 0x00, 0x00, 0x00, // modules_ptr
        0x00, 0x00, 0x00, 0x00, // entry_point_id
        0x1f, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // compile_exec_argv_ptr
        0xfc, 0x05, 0x00, 0x00, // flags
    ];

    #[test]
    fn parses_32_byte_offsets() {
        let offsets = parse_offsets(&BUN_1_4_4_OFFSETS, LayoutVersion::V1_3_14)
            .expect("32 bytes is the V1_3_14 size");
        assert_eq!(
            offsets,
            Offsets {
                byte_count: 2080,
                modules_ptr: StringPointer { offset: 1951, length: 104 },
                entry_point_id: 0,
                compile_exec_argv_ptr: Some(StringPointer { offset: 2079, length: 0 }),
                flags: Some(0x5fc),
            }
        );
        assert_eq!(offsets.modules_ptr.length as usize / LayoutVersion::V1_3_14.entry_size(), 2);
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(parse_offsets(&BUN_1_4_4_OFFSETS[..24], LayoutVersion::V1_3_14), None);
        assert_eq!(parse_offsets(&BUN_1_4_4_OFFSETS, LayoutVersion::V1_0), None);
    }

    /// The first 20 bytes are laid out identically in every version, so the
    /// 24-byte reading of the same bytes keeps those fields and drops the two
    /// that 24-byte layouts do not have. Bytes 20..24 are padding there.
    #[test]
    fn parses_24_byte_offsets() {
        let offsets = parse_offsets(&BUN_1_4_4_OFFSETS[..24], LayoutVersion::V1_0)
            .expect("24 bytes is the V1_0 size");
        assert_eq!(offsets.byte_count, 2080);
        assert_eq!(offsets.modules_ptr, StringPointer { offset: 1951, length: 104 });
        assert_eq!(offsets.entry_point_id, 0);
        assert_eq!(offsets.compile_exec_argv_ptr, None);
        assert_eq!(offsets.flags, None);
    }

    /// V1_2_23 has the 32-byte record but its last 4 bytes are padding on
    /// releases before 1.3.7, so `flags` is withheld rather than reported as 0.
    #[test]
    fn withholds_flags_before_v1_3_14() {
        let offsets = parse_offsets(&BUN_1_4_4_OFFSETS, LayoutVersion::V1_2_23)
            .expect("32 bytes is the V1_2_23 size");
        assert!(offsets.compile_exec_argv_ptr.is_some());
        assert_eq!(offsets.flags, None);
    }
}
