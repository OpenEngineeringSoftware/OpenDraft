//! WBLOCK: write a block, or the selected objects made into one, to an
//! `.oblock` file of its own (see `opendraft_content::oblock`). The drawing is
//! not changed.

use std::path::PathBuf;

use opendraft_commands::{Answer, Cancelled, Editor, Options};
use opendraft_content::oblock::BlockFile;
use opendraft_ui::key;

use crate::library_add_cmd::choose_source;
use crate::oblock_doc::oblock_path;
use crate::state::AppState;

impl AppState {
    /// WBLOCK, started from the command table.
    pub fn wblock(&mut self) {
        let doc = &self.docs[self.active];
        let folder = self.folder_for_dialogs(doc.path.as_deref());
        self.start_command("WBLOCK", move |ed| wblock(ed, folder.clone()));
    }
}

/// The file the user named: a bare name goes in `folder` (beside the drawing,
/// else where the last file was), and a name with no extension gets
/// `.oblock`.
fn resolve(text: &str, folder: Option<&PathBuf>) -> Result<PathBuf, String> {
    let typed = PathBuf::from(text.trim().trim_matches('"'));
    let placed = match (typed.parent(), folder) {
        (Some(parent), Some(folder)) if parent.as_os_str().is_empty() => folder.join(&typed),
        _ => typed,
    };
    oblock_path(placed)
}

/// WBLOCK: the selected objects, or a block of the drawing chosen by name (or
/// the objects, with `Objects`), are written with the blocks they hold and the
/// layers, linetypes and text styles they use.
pub fn wblock(ed: &mut Editor, folder: Option<PathBuf>) -> Result<(), Cancelled> {
    let prompt = key!("Enter name of existing block to write to a file");
    let Some((source, block)) = choose_source(ed, prompt)? else {
        return Ok(());
    };
    let name = source
        .block(block)
        .map(|b| b.name.clone())
        .unwrap_or_default();
    let suggested = resolve(&name, folder.as_ref()).unwrap_or_default();
    let path = loop {
        let options =
            Options::new(key!("Enter file name")).default(suggested.to_string_lossy().into_owned());
        let text = match ed.ask_string(options)? {
            Answer::Value(t) | Answer::Keyword(t) => t,
            Answer::Enter => suggested.to_string_lossy().into_owned(),
        };
        let path = match resolve(&text, folder.as_ref()) {
            Ok(path) => path,
            Err(why) => {
                ed.say(why);
                continue;
            }
        };
        if !path.exists() || confirm_overwrite(ed, &path)? {
            break path;
        }
    };
    let (file, warnings) = match BlockFile::from_block(&source, block, &[]) {
        Ok(made) => made,
        Err(why) => {
            ed.say(format!("Block \"{name}\" cannot be written: {why}"));
            return Ok(());
        }
    };
    match file.save(&path) {
        Ok(()) => ed.say(format!(
            "Block \"{}\" written to {}.",
            file.name(),
            path.display()
        )),
        Err(why) => ed.say(format!("Could not write {}: {why}", path.display())),
    }
    for warning in warnings {
        ed.say(warning);
    }
    Ok(())
}

fn confirm_overwrite(ed: &mut Editor, path: &std::path::Path) -> Result<bool, Cancelled> {
    let message = format!("{} already exists. Replace it?", path.display());
    let options = Options::<String>::new(&message)
        .keyword("Yes", "Y")
        .keyword("No", "N")
        .default("No".to_owned());
    Ok(ed.ask_keyword(options)?.as_deref() == Some("Yes"))
}

#[cfg(test)]
#[path = "wblock_cmd_tests.rs"]
mod tests;
