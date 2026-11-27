//! Importing blocks from a drawing file the user opened (`.odraft`, DXF or
//! DWG; the existing readers do the reading): the user ticks the blocks they
//! want, and they become personal blocks in a folder (specification 10.4).

use opendraft_core::{BlockId, Document};

use rusqlite::Transaction;

use super::capture::{add_file, NameTaken};
use super::error::LibraryError;
use super::Library;
use crate::oblock::BlockFile;

/// A block of the file that can be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub id: BlockId,
    pub name: String,
    /// Objects in it.
    pub objects: usize,
    /// Blocks it contains, which come with it.
    pub contains: usize,
}

/// What an import did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub added: Vec<String>,
    /// Blocks not imported, with why.
    pub skipped: Vec<(String, String)>,
    /// What did not carry over exactly, worded for the user.
    pub warnings: Vec<String>,
}

/// The blocks of `doc` worth listing: not the anonymous ones a drawing makes
/// for itself (`*U1`, `*D3`, `*Model_Space`...), nor a block with a name a
/// library cannot hold.
pub fn importable(doc: &Document) -> Vec<Candidate> {
    doc.blocks()
        .iter()
        .filter(|b| !b.name.starts_with('*'))
        .map(|b| {
            let mut inner = std::collections::HashSet::new();
            let mut stack: Vec<_> = b.nested_blocks().collect();
            while let Some(id) = stack.pop() {
                if inner.insert(id) {
                    if let Some(d) = doc.block(id) {
                        stack.extend(d.nested_blocks());
                    }
                }
            }
            Candidate {
                id: b.id,
                name: b.name.clone(),
                objects: b.entities.len(),
                contains: inner.len(),
            }
        })
        .collect()
}

impl Library {
    /// Copies `blocks` of `source` into `folder`, as one change: either all
    /// that could be added are, or (when the library cannot be written)
    /// none. A block that cannot be added is reported, and the rest go on.
    pub fn import(
        &mut self,
        source: &Document,
        blocks: &[BlockId],
        folder: &str,
        if_name_taken: NameTaken,
        now: u64,
    ) -> Result<ImportReport, LibraryError> {
        self.edit(true, |tx, _| {
            Ok(import_all(tx, source, blocks, folder, if_name_taken, now))
        })
    }
}

fn import_all(
    tx: &Transaction,
    source: &Document,
    blocks: &[BlockId],
    folder: &str,
    if_taken: NameTaken,
    now: u64,
) -> ImportReport {
    let mut report = ImportReport::default();
    for id in blocks {
        let name = source
            .block(*id)
            .map_or_else(|| "(missing)".to_owned(), |b| b.name.clone());
        // A block that fails halfway leaves nothing behind.
        let added = BlockFile::from_block(source, *id, &[])
            .map_err(LibraryError::from)
            .and_then(|(file, warnings)| {
                tx.execute_batch("SAVEPOINT block")?;
                match add_file(tx, file, folder, if_taken, now) {
                    Ok(added) => {
                        tx.execute_batch("RELEASE block")?;
                        Ok((added, warnings))
                    }
                    Err(error) => {
                        let _ = tx.execute_batch("ROLLBACK TO block; RELEASE block");
                        Err(error)
                    }
                }
            });
        match added {
            Ok((done, warnings)) => {
                report.added.push(done.name);
                report.warnings.extend(warnings);
            }
            Err(error) => {
                let why = match error {
                    LibraryError::NameTaken(_) => "the library has a block of that name".to_owned(),
                    other => other.to_string(),
                };
                report.skipped.push((name, why));
            }
        }
    }
    report.warnings.sort();
    report.warnings.dedup();
    report
}

#[cfg(test)]
mod tests {
    use opendraft_core::Document;

    use super::*;
    use crate::library::tests::{names, open, NOW};
    use crate::test_support::{add_block, drawing_with};

    /// A drawing with blocks "nut", "bolt" (holding the nut), "*U1" and "plate".
    fn file() -> (Document, Vec<BlockId>) {
        let mut doc = Document::default();
        let nut = add_block(&mut doc, "nut", "Fixings", 1.0, &[]);
        let bolt = add_block(&mut doc, "bolt", "Fixings", 2.0, &[&nut]);
        add_block(&mut doc, "*U1", "Fixings", 3.0, &[]);
        let plate = add_block(&mut doc, "plate", "Plates", 4.0, &[]);
        (doc, vec![nut.id, bolt.id, plate.id])
    }

    #[test]
    fn anonymous_blocks_are_not_listed_and_what_a_block_holds_is_counted() {
        let (doc, _) = file();
        let list = importable(&doc);
        let listed: Vec<_> = list.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(listed, ["nut", "bolt", "plate"]);
        let bolt = list.iter().find(|c| c.name == "bolt").expect("bolt");
        assert_eq!((bolt.objects, bolt.contains), (2, 1));
    }

    #[test]
    fn ticked_blocks_become_personal_blocks_in_the_folder_in_one_write() {
        let (_dir, mut lib) = open();
        let (doc, ids) = file();
        let report = lib
            .import(&doc, &ids[1..], "From file/Parts", NameTaken::Cancel, NOW)
            .expect("imports");
        assert_eq!(report.added, ["bolt", "plate"]);
        assert!(report.skipped.is_empty());
        assert_eq!(names(&lib.blocks()), ["bolt", "plate"]);
        assert!(lib.blocks().iter().all(|b| b.folder == "From file/Parts"));
        assert_eq!(lib.blocks()[0].components, ["nut"]);
        // One write: the revision rose once.
        assert_eq!(lib.revision(), 1);
        assert_eq!(super::super::tests::reopen(&lib).blocks().len(), 2);
    }

    #[test]
    fn a_taken_name_is_skipped_kept_both_or_replaced_as_asked() {
        let (_dir, mut lib) = open();
        let (own, id) = drawing_with("plate", 9.0);
        lib.capture(&own, id, &Default::default(), NOW)
            .expect("captures");
        let (doc, ids) = file();

        let skipped = lib
            .import(&doc, &ids[2..], "", NameTaken::Cancel, NOW)
            .expect("imports");
        assert_eq!(skipped.skipped.len(), 1);
        assert_eq!(skipped.skipped[0].0, "plate");
        assert_eq!(lib.blocks().len(), 1);

        let both = lib
            .import(&doc, &ids[2..], "", NameTaken::KeepBoth, NOW)
            .expect("imports");
        assert_eq!(both.added, ["plate_2"]);

        let replaced = lib
            .import(&doc, &ids[2..], "", NameTaken::Replace, NOW)
            .expect("imports");
        assert_eq!(replaced.added, ["plate"]);
        assert_eq!(lib.trash_items().len(), 1);
    }

    #[test]
    fn a_bad_folder_skips_every_block_and_leaves_the_library_alone() {
        let (_dir, mut lib) = open();
        let (doc, ids) = file();
        let report = lib
            .import(&doc, &ids, "a:b", NameTaken::Cancel, NOW)
            .expect("the call itself works");
        assert_eq!(report.added.len(), 0);
        assert_eq!(report.skipped.len(), 3);
        assert!(lib.blocks().is_empty());
    }

    #[test]
    fn a_block_that_is_not_in_the_file_is_reported() {
        let (_dir, mut lib) = open();
        let (doc, _) = file();
        let report = lib
            .import(&doc, &[BlockId(999)], "", NameTaken::Cancel, NOW)
            .expect("imports");
        assert_eq!(report.skipped.len(), 1);
    }
}
