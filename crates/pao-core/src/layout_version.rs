/// Pao's supported Bun versions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutVersion {
    /// Bun 1.0.0 – 1.1.20: 24-byte Offsets, 32-byte entries.
    V1_0,
    /// Bun 1.1.30 – 1.2.12: 24-byte Offsets, 36-byte entries.
    V1_1_30,
    /// Bun 1.2.23 – 1.3.7: 32-byte Offsets, 36-byte entries.
    V1_2_23,
    /// Bun 1.3.14 – 1.4.4: 32-byte Offsets, 52-byte entries.
    V1_3_14,
}
impl LayoutVersion {
    pub const LAYOUT_VERSIONS: [Self; 4] =
        [Self::V1_0, Self::V1_1_30, Self::V1_2_23, Self::V1_3_14];
    /// Bytes of the Offsets record immediately before the trailer.
    pub const fn offsets_size(self) -> usize {
        match self {
            LayoutVersion::V1_0 | LayoutVersion::V1_1_30 => 24,
            LayoutVersion::V1_2_23 | LayoutVersion::V1_3_14 => 32,
        }
    }

    /// Bytes per module-table entry.
    pub const fn entry_size(self) -> usize {
        match self {
            LayoutVersion::V1_0 => 32,
            LayoutVersion::V1_1_30 | LayoutVersion::V1_2_23 => 36,
            LayoutVersion::V1_3_14 => 52,
        }
    }
    pub const fn pointer_count(self) -> usize {
        match self {
            LayoutVersion::V1_0 => 3,
            LayoutVersion::V1_1_30 | LayoutVersion::V1_2_23 => 4,
            LayoutVersion::V1_3_14 => 6,
        }
    }
}
