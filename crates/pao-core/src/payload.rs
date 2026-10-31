use crate::string_pointer::StringPointer;

/// The bytes from the payload base up to the `Offsets` record, `byte_count`
/// long. Every [`StringPointer`] is relative to its start.
///
/// Only the scanner constructs one, after it has located the base. Holding
/// the payload in its own type means the whole file, also a `&[u8]`, cannot be
/// passed where the payload is meant: pointers into it would silently read
/// the wrong bytes.
#[derive(Clone, Copy)]
pub struct Payload<'a>(&'a [u8]);

// The derived impl would print every byte of the payload.
impl std::fmt::Debug for Payload<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Payload")
            .field("len", &self.0.len())
            .finish()
    }
}

impl<'a> Payload<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// The bytes `ptr` names, or `None` if it runs past the payload.
    pub fn get(&self, ptr: StringPointer) -> Option<&'a [u8]> {
        self.0.get(ptr.range()?)
    }
}
