//! Reading the library: what the window lists. All of it is read from the
//! index in memory, never from the blocks' bytes.

use std::collections::HashSet;

use opendraft_core::InsertUnits;

use super::{Library, TrashRecord};
use crate::blocks_payload::folder;
use crate::blocks_payload::BlockUuid;

/// One block as the window shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub id: BlockUuid,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    /// `""` for the top level of My blocks.
    pub folder: String,
    pub unit: InsertUnits,
    pub dynamic: bool,
    pub has_attributes: bool,
    pub favourite: bool,
    /// Names of the blocks it contains, at any depth, each once.
    pub components: Vec<String>,
}

impl Library {
    /// Every block the user can see, in folder then name order.
    pub fn blocks(&self) -> Vec<BlockInfo> {
        let mut blocks = self.index.blocks.clone();
        blocks.sort_by_cached_key(|b| (b.folder.to_lowercase(), b.name.to_lowercase()));
        blocks
    }

    pub fn block(&self, id: BlockUuid) -> Option<BlockInfo> {
        self.index.blocks.iter().find(|b| b.id == id).cloned()
    }

    /// The blocks directly in `folder` (`""` for the top level).
    pub fn blocks_in(&self, folder: &str) -> Vec<BlockInfo> {
        let mut blocks = self.blocks();
        blocks.retain(|b| folder::same(&b.folder, folder));
        blocks
    }

    /// Every folder: the ones the user made and the ones holding a block,
    /// with their ancestors, in name order, each as first written.
    pub fn folders(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut folders = Vec::new();
        let held = self.index.blocks.iter().map(|b| b.folder.as_str());
        for path in self.index.folders.iter().map(String::as_str).chain(held) {
            for f in folder::with_ancestors(path) {
                if seen.insert(f.to_lowercase()) {
                    folders.push(f);
                }
            }
        }
        folders.sort_by_cached_key(|f| f.to_lowercase());
        folders
    }

    /// The folders directly inside `folder`, as full paths.
    pub fn subfolders(&self, folder: &str) -> Vec<String> {
        let mut folders = self.folders();
        folders.retain(|f| folder::same(folder::parent(f), folder));
        folders
    }

    /// The favourites, in the order they were made.
    pub fn favourites(&self) -> Vec<BlockInfo> {
        self.index
            .favourites
            .iter()
            .filter_map(|id| self.block(*id))
            .collect()
    }

    /// The blocks last inserted, most recent first.
    pub fn recents(&self) -> Vec<BlockInfo> {
        self.index
            .recents
            .iter()
            .filter_map(|id| self.block(*id))
            .collect()
    }

    /// What is in the Trash, newest first.
    pub fn trash_items(&self) -> Vec<TrashRecord> {
        let mut items = self.index.trash.clone();
        items.sort_by(|a, b| b.deleted.cmp(&a.deleted).then(b.id.cmp(&a.id)));
        items
    }
}
