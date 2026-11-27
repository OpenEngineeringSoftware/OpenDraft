//! LIBADD: copy a block of the drawing, or the selected objects made into one,
//! into the user's block library (the Content Manager's local half).

use opendraft_commands::{Answer, Cancelled, Editor, Options};
use opendraft_content::library::{CaptureRequest, LibraryError, NameTaken};
use opendraft_core::{BlockDef, BlockId, Document, EntityId, Point};
use opendraft_ui::key;

use crate::block_cmd::{ask_block_name, list_blocks, NameAnswer};
use crate::content_library::now;
use crate::defaults::EditorDefaults;

/// A drawing that holds the block to copy, and the block in it.
pub(crate) type Source = (Document, BlockId);

/// What was chosen to add, or `None` when the user ended the command.
/// `prompt` asks which block, as the command words it.
pub(crate) fn choose_source(
    ed: &mut Editor,
    prompt: &'static str,
) -> Result<Option<Source>, Cancelled> {
    let selected = ed.pickset().to_vec();
    if !selected.is_empty() {
        return from_objects(ed, selected);
    }
    let names: Vec<String> =
        ed.read(|doc| doc.blocks().iter().map(|b| b.name.clone()).collect())?;
    loop {
        let mut options = Options::new(prompt).keyword("Objects", "O");
        for name in names.iter().take(20) {
            options = options.keyword(name.clone(), name.clone());
        }
        let name = match ask_block_name(ed, options)? {
            NameAnswer::Done => return Ok(None),
            NameAnswer::List => {
                for line in ed.read(list_blocks)? {
                    ed.say(line);
                }
                continue;
            }
            NameAnswer::Name(name) => name,
        };
        if name == "Objects" {
            let chosen = ed.get_selection(key!("Select objects"))?;
            return from_objects(ed, chosen);
        }
        let lookup = name.clone();
        let found = ed.read(move |doc| {
            let id = doc.block_named(&lookup)?.id;
            Some((doc.extract(&[]), id))
        })?;
        match found {
            Some(source) => return Ok(Some(source)),
            None => ed.say(format!("Block \"{name}\" not found.")),
        }
    }
}

/// The objects made into a block in a scratch copy of the drawing's tables,
/// so the drawing itself is not touched.
fn from_objects(ed: &mut Editor, chosen: Vec<EntityId>) -> Result<Option<Source>, Cancelled> {
    if chosen.is_empty() {
        ed.say("No objects selected.");
        return Ok(None);
    }
    let name = ed.get_string(Options::new(key!("Enter block name")))?;
    let name = name.trim().to_owned();
    if let Some(why) = opendraft_core::block::block_name_problem(&name) {
        ed.say(why);
        return Ok(None);
    }
    let base = ed.get_point(key!("Specify insertion base point"))?;
    let made = ed.read(move |doc| scratch_block(doc, &chosen, &name, base))?;
    match made {
        Ok(source) => Ok(Some(source)),
        Err(why) => {
            ed.say(why);
            Ok(None)
        }
    }
}

fn scratch_block(
    doc: &Document,
    chosen: &[EntityId],
    name: &str,
    base: Point,
) -> Result<Source, String> {
    let mut scratch = doc.extract(chosen);
    let id = scratch.allocate_block_id();
    let mut def = BlockDef::from_objects(id, name, base, scratch.objects());
    def.unit = doc.units().insert;
    let change = scratch.redefined_blocks(def).map_err(str::to_owned)?;
    scratch.set_blocks(change.blocks);
    Ok((scratch, id))
}

/// How the user wants a taken name settled.
fn ask_taken(ed: &Editor, name: &str) -> Result<Option<NameTaken>, Cancelled> {
    let options = Options::worded(
        key!("The library already has a block called \"%{name}\". Keep both or replace it?"),
        &[("name", name)],
    )
    .keyword("Both", "B")
    .keyword("Replace", "R")
    .keyword("Cancel", "C")
    .default("Both".to_owned());
    Ok(match ed.get_keyword(options)?.as_str() {
        "Both" => Some(NameTaken::KeepBoth),
        "Replace" => Some(NameTaken::Replace),
        _ => None,
    })
}

/// LIBADD: add a block to the library. The selected objects, or a block of
/// the drawing chosen by name (or the objects, with `Objects`), are copied
/// with the blocks they hold and the layers, linetypes and text styles they
/// use. The drawing is not changed.
pub fn library_add(ed: &mut Editor) -> Result<(), Cancelled> {
    let defaults = ed.defaults();
    let prompt = key!("Enter name of block to add to the library");
    let Some((source, block)) = choose_source(ed, prompt)? else {
        return Ok(());
    };
    let default_name = source
        .block(block)
        .map(|b| b.name.clone())
        .unwrap_or_default();
    let name = match ed
        .ask_string(Options::new(key!("Enter name in the library")).default(default_name.clone()))?
    {
        Answer::Value(text) if !text.trim().is_empty() => text.trim().to_owned(),
        _ => default_name,
    };
    let mut request = CaptureRequest {
        name: Some(name),
        ..CaptureRequest::default()
    };
    loop {
        let done = defaults
            .library
            .with(|lib| lib.capture(&source, block, &request, now()));
        for notice in defaults.library.take_notices() {
            ed.say(notice);
        }
        let result = match done {
            Ok(result) => result,
            Err(error) => {
                ed.say(error.to_string());
                return Ok(());
            }
        };
        match result {
            Ok(captured) => {
                ed.say(format!("Block \"{}\" added to the library.", captured.name));
                if captured.replaced {
                    ed.say("The block it replaced is in the library's Trash.");
                }
                for warning in captured.warnings {
                    ed.say(warning);
                }
                return Ok(());
            }
            Err(LibraryError::NameTaken(taken)) => match ask_taken(ed, &taken)? {
                Some(how) => request.if_name_taken = how,
                None => return Ok(()),
            },
            Err(other) => {
                ed.say(other.to_string());
                return Ok(());
            }
        }
    }
}
