//! What the library remembers of the Trash, and the limits on what it keeps.

/// The most recently inserted blocks that are remembered.
pub const MAX_RECENTS: usize = 20;
/// Days a deleted block stays in the Trash.
pub const TRASH_DAYS: u64 = 30;

/// A deleted block in the Trash. The date and the folder it was in are kept
/// beside the block's bytes, not in them (specification 5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashRecord {
    pub id: u64,
    pub uuid: crate::blocks_payload::BlockUuid,
    pub name: String,
    pub folder: String,
    /// Seconds since 1970.
    pub deleted: u64,
}
