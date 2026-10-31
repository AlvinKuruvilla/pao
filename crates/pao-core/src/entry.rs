use crate::byte_reader::LittleEndianByteReader;
use crate::layout_version::LayoutVersion;
use crate::string_pointer::StringPointer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleEntry {
    pub name: StringPointer,
    pub contents: StringPointer,
    pub sourcemap: StringPointer,
    pub bytecode: Option<StringPointer>,    // all but V1_0
    pub module_info: Option<StringPointer>, // V1_3_14
    pub bytecode_origin_path: Option<StringPointer>, // V1_3_14
    pub encoding: Option<u8>,               // all but V1_0
    pub loader: u8,
    pub module_format: Option<u8>, // all but V1_0
    pub side: Option<u8>,          // V1_2_23, V1_3_14
}

/// `bytes` is exactly `layout.entry_size()` long.
pub fn parse_entry(bytes: &[u8], layout: LayoutVersion) -> Option<ModuleEntry> {
    if bytes.len() != layout.entry_size() {
        return None;
    }
    let n = layout.pointer_count();
    // Pointer i sits at byte 8 * i; a closure because it is called per index.
    let ptr = |i: usize| bytes.pointer_at(8 * i);
    let pointers: Vec<StringPointer> = (0..n).map(ptr).collect::<Option<_>>()?;

    // The single-byte fields follow the last pointer, except in V1_0, where
    // Zig placed the lone `loader` byte straight after the three pointers and
    // the other fields do not exist.
    let (encoding, loader, module_format, side) = if layout == LayoutVersion::V1_0 {
        (None, bytes.u8_at(24)?, None, None)
    } else {
        let b = 8 * n;
        // V1_1_30 leaves the fourth byte as padding; `side` arrived in 1.2.23.
        let side = if matches!(layout, LayoutVersion::V1_2_23 | LayoutVersion::V1_3_14) {
            Some(bytes.u8_at(b + 3)?)
        } else {
            None
        };
        (
            Some(bytes.u8_at(b)?),
            bytes.u8_at(b + 1)?,
            Some(bytes.u8_at(b + 2)?),
            side,
        )
    };

    // Every layout has at least three pointers, so indexing 0..3 cannot panic;
    // `get` yields `None` for the pointers a layout lacks.
    Some(ModuleEntry {
        name: pointers[0],
        contents: pointers[1],
        sourcemap: pointers[2],
        bytecode: pointers.get(3).copied(),
        module_info: pointers.get(4).copied(),
        bytecode_origin_path: pointers.get(5).copied(),
        encoding,
        loader,
        module_format,
        side,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each vector is the first module entry of a fixture binary built by
    // tools/bun-matrix/build.sh for linux-x64, written in
    // 8-byte groups so the pointer slots line up with the format table. The
    // `--bytecode` builds are used where the layout has a bytecode pointer, so
    // that slot holds a non-empty value and a misplaced read would show.
    // `module_format` 2 is CommonJS: Bun only emits bytecode for CJS output.

    fn hex(groups: &str) -> Vec<u8> {
        let digits: String = groups.split_whitespace().collect();
        (0..digits.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).expect("test vectors are valid hex"))
            .collect()
    }

    fn sp(offset: u32, length: u32) -> StringPointer {
        StringPointer { offset, length }
    }

    /// Bun 1.1.20, plain build. The loader byte sits after three pointers.
    #[test]
    fn parses_v1_0() {
        let bytes = hex("0000000027000000 2700000064010000 0000000000000000 0100000000000000");
        assert_eq!(
            parse_entry(&bytes, LayoutVersion::V1_0),
            Some(ModuleEntry {
                name: sp(0, 39),
                contents: sp(39, 356),
                sourcemap: sp(0, 0),
                bytecode: None,
                module_info: None,
                bytecode_origin_path: None,
                encoding: None,
                loader: 1,
                module_format: None,
                side: None,
            })
        );
    }

    /// Bun 1.2.0 with `--bytecode`. Byte 35 is padding, so `side` is absent.
    #[test]
    fn parses_v1_1_30() {
        let bytes =
            hex("880e000029000000 b20e0000b5010000 0000000000000000 00000000880e0000 01010200");
        assert_eq!(
            parse_entry(&bytes, LayoutVersion::V1_1_30),
            Some(ModuleEntry {
                name: sp(3720, 41),
                contents: sp(3762, 437),
                sourcemap: sp(0, 0),
                bytecode: Some(sp(0, 3720)),
                module_info: None,
                bytecode_origin_path: None,
                encoding: Some(1),
                loader: 1,
                module_format: Some(2),
                side: None,
            })
        );
    }

    /// Bun 1.3.7 with `--bytecode`. Same size as V1_1_30; byte 35 is `side`.
    #[test]
    fn parses_v1_2_23() {
        let bytes =
            hex("180f000029000000 420f0000b5010000 0000000000000000 78000000a00e0000 01010200");
        assert_eq!(
            parse_entry(&bytes, LayoutVersion::V1_2_23),
            Some(ModuleEntry {
                name: sp(3864, 41),
                contents: sp(3906, 437),
                sourcemap: sp(0, 0),
                bytecode: Some(sp(120, 3744)),
                module_info: None,
                bytecode_origin_path: None,
                encoding: Some(1),
                loader: 1,
                module_format: Some(2),
                side: Some(0),
            })
        );
    }

    /// Bun 1.4.4 with `--bytecode`. Six pointers, then four single bytes.
    #[test]
    fn parses_v1_3_14() {
        let bytes = hex(
            "2b07000029000000 4c050000c2010000 0000000000000000 78000000b0020000 \
             0000000000000000 5507000029000000 01010200",
        );
        assert_eq!(
            parse_entry(&bytes, LayoutVersion::V1_3_14),
            Some(ModuleEntry {
                name: sp(1835, 41),
                contents: sp(1356, 450),
                sourcemap: sp(0, 0),
                bytecode: Some(sp(120, 688)),
                module_info: Some(sp(0, 0)),
                bytecode_origin_path: Some(sp(1877, 41)),
                encoding: Some(1),
                loader: 1,
                module_format: Some(2),
                side: Some(0),
            })
        );
    }

    /// A wrong-length slice is the only input `parse_entry` rejects; whether
    /// the pointers make sense is the scanner's decision, not this function's.
    #[test]
    fn rejects_only_wrong_length() {
        assert_eq!(parse_entry(&[0; 36], LayoutVersion::V1_3_14), None);
        assert_eq!(parse_entry(&[0; 52], LayoutVersion::V1_0), None);
        for layout in LayoutVersion::LAYOUT_VERSIONS {
            let garbage = vec![0xff; layout.entry_size()];
            assert!(
                parse_entry(&garbage, layout).is_some(),
                "{layout:?} rejected a correctly sized slice"
            );
        }
    }
}
