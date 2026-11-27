//! What the window shows in one frame, read from the library and the drawing
//! once, so drawing the window never touches either.

use opendraft_content::library::{fold, BlockInfo, Library, Query, TrashRecord};
use opendraft_core::BlockId;

use super::{Env, Place, Selected, Window};

/// A block of the open drawing.
pub struct DrawingBlock {
    pub id: BlockId,
    pub name: String,
    pub references: usize,
    /// The library has a block of this name already.
    pub in_library: bool,
    pub unit: opendraft_core::InsertUnits,
    pub dynamic: bool,
    pub has_attributes: bool,
}

#[derive(Default)]
pub struct Snapshot {
    /// Why the library cannot be shown (no folder, damaged files).
    pub problem: Option<String>,
    /// Every folder of My blocks.
    pub folders: Vec<String>,
    /// The folders directly in the place shown (My blocks only).
    pub subfolders: Vec<String>,
    /// The blocks to list: those of the place, or the search's.
    pub shown: Vec<BlockInfo>,
    pub trash: Vec<TrashRecord>,
    pub drawing: Vec<DrawingBlock>,
    pub favourites: usize,
    pub recents: usize,
    pub selected: Option<BlockInfo>,
    /// Tells pictures of the library's blocks whether they are stale.
    pub stamp: u64,
    /// What is listed is the whole library's search or filter result (only
    /// in My blocks; the other places are narrowed in place).
    pub searching: bool,
    /// The search words or a filter narrow what is listed, wherever it is.
    pub narrowed: bool,
    /// Blocks in the Trash, before any search narrows the list.
    pub trash_total: usize,
    /// The units the library's blocks are in.
    pub units: Vec<opendraft_core::InsertUnits>,
}

/// The search's words, folded for matching.
fn words_of(text: &str) -> Vec<String> {
    fold(text).split_whitespace().map(str::to_owned).collect()
}

/// Whether `name` has every one of `words` (all of them when there are none).
fn has_words(name: &str, words: &[String]) -> bool {
    let name = fold(name);
    words.iter().all(|w| name.contains(w.as_str()))
}

/// `list` kept to the blocks the search and filters let through.
fn narrow(lib: &Library, list: Vec<BlockInfo>, narrowed: bool, query: &Query) -> Vec<BlockInfo> {
    if !narrowed {
        return list;
    }
    let hits: Vec<_> = lib.search(query).into_iter().map(|b| b.id).collect();
    list.into_iter().filter(|b| hits.contains(&b.id)).collect()
}

impl Snapshot {
    pub fn take(win: &Window, env: &Env) -> Snapshot {
        let narrowed = !win.search.trim().is_empty() || win.filter.active();
        let mut snap = Snapshot {
            // My blocks searches the whole library; the other places only
            // narrow what they hold.
            searching: narrowed && matches!(win.place, Place::Mine(_)),
            narrowed,
            drawing: Vec::new(),
            ..Snapshot::default()
        };
        let place = win.place.clone();
        let selected = win.selected.clone();
        let query = win.filter.query(&win.search);
        let searching = snap.searching;
        let words = words_of(&win.search);
        let read = env.library.with(|lib| {
            let mut s = Snapshot::default();
            s.folders = lib.folders();
            s.trash = lib.trash_items();
            s.favourites = lib.favourites().len();
            s.recents = lib.recents().len();
            s.shown = if searching {
                lib.search(&query)
            } else {
                match &place {
                    Place::Mine(folder) => {
                        s.subfolders = lib.subfolders(folder);
                        lib.blocks_in(folder)
                    }
                    Place::Favourites => narrow(lib, lib.favourites(), narrowed, &query),
                    Place::Recent => narrow(lib, lib.recents(), narrowed, &query),
                    Place::Drawing | Place::Trash => Vec::new(),
                }
            };
            if let Selected::Block(id) = selected {
                s.selected = lib.block(id);
            }
            let all = lib.blocks();
            let names: Vec<String> = all.iter().map(|b| b.name.clone()).collect();
            for b in &all {
                if !s.units.iter().any(|u| u.code() == b.unit.code()) {
                    s.units.push(b.unit);
                }
            }
            (s, names)
        });
        match read {
            Ok((read, names)) => {
                snap.units = read.units;
                snap.folders = read.folders;
                snap.subfolders = read.subfolders;
                snap.shown = read.shown;
                snap.trash_total = read.trash.len();
                snap.trash = read
                    .trash
                    .into_iter()
                    .filter(|t| has_words(&t.name, &words))
                    .collect();
                snap.favourites = read.favourites;
                snap.recents = read.recents;
                snap.selected = read.selected;
                snap.drawing = env
                    .document
                    .blocks()
                    .iter()
                    .filter(|b| has_words(&b.name, &words))
                    .map(|b| DrawingBlock {
                        id: b.id,
                        name: b.name.clone(),
                        references: env.document.block_references(b.id).len(),
                        in_library: names.iter().any(|n| n.eq_ignore_ascii_case(&b.name)),
                        unit: b.unit,
                        dynamic: b.is_dynamic(),
                        has_attributes: b.entities.iter().any(|e| {
                            matches!(e.entity, opendraft_core::Entity::AttributeDefinition(_))
                        }),
                    })
                    .collect();
            }
            Err(error) => snap.problem = Some(error.to_string()),
        }
        snap.stamp = env.library.stamp();
        snap
    }

    /// "12 blocks" under the contents.
    pub fn count_text(&self, win: &Window) -> String {
        let count = match (&win.place, self.searching) {
            (_, true) => self.shown.len(),
            (Place::Mine(_), _) => self.shown.len() + self.subfolders.len(),
            (Place::Favourites | Place::Recent, _) => self.shown.len(),
            (Place::Drawing, _) => self.drawing.len(),
            (Place::Trash, _) => self.trash.len(),
        };
        if count == 1 {
            "1 item".to_owned()
        } else {
            format!("{count} items")
        }
    }
}
