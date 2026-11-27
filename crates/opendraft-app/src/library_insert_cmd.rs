//! LIBINSERT: place a block of the user's library in the drawing, worded like
//! INSERT. Nothing is added to the drawing until the point is picked; then the
//! block, the blocks it holds, the layers and styles they use and the
//! reference are added in one undo step.

use std::collections::HashMap;
use std::sync::Arc;

use opendraft_commands::{
    Answer, Cancelled, Editor, InsertFromLibrary, LibraryInsertSink, NumberOptions, Options,
    PointOptions,
};
use opendraft_content::library::BlockInfo;
use opendraft_core::block::FieldText;
use opendraft_core::{
    BlockClash, BlockDef, BlockId, BlockNameClash, Document, InsertUnits, LibraryPlacement,
};
use opendraft_ui::key;

use crate::attribute_prompt::ask_values;
use crate::defaults::EditorDefaults;
use crate::insert_cmd::{get_factor, placed};

/// Most names offered as options at the name prompt; `?` lists them all.
const MAX_OFFERED: usize = 20;

/// The library block to insert, asked by name; `None` when the user ends the
/// command.
fn choose(
    ed: &Editor,
    blocks: &[BlockInfo],
    newest: Option<&str>,
) -> Result<Option<BlockInfo>, Cancelled> {
    loop {
        let mut options = Options::new(key!("Enter library block name"));
        if let Some(newest) = newest {
            options = options.default(newest.to_owned());
        }
        for b in blocks.iter().take(MAX_OFFERED) {
            options = options.keyword(b.name.clone(), b.name.clone());
        }
        let name = match crate::block_cmd::ask_block_name(ed, options)? {
            crate::block_cmd::NameAnswer::Done => return Ok(None),
            crate::block_cmd::NameAnswer::List => {
                for b in blocks {
                    let folder = if b.folder.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", b.folder)
                    };
                    ed.say(format!("  \"{}\"{folder}", b.name));
                }
                continue;
            }
            crate::block_cmd::NameAnswer::Name(name) => name,
        };
        match blocks.iter().find(|b| b.name.eq_ignore_ascii_case(&name)) {
            Some(found) => return Ok(Some(found.clone())),
            None => ed.say(format!("Block \"{name}\" is not in the library.")),
        }
    }
}

/// What to do with the blocks the drawing already has under these names, which
/// differ from the library's: one answer for all of them, or the one the user
/// asked to keep for this session. `Rename` is the default, so a drawing's
/// own block is never changed unless the user says so. A block the drawing has
/// that is the same as the library's is never asked about: see
/// [`settle_all`].
fn settle_clashes(
    ed: &Editor,
    clashes: &[BlockNameClash],
) -> Result<HashMap<BlockId, BlockClash>, Cancelled> {
    if clashes.is_empty() {
        return Ok(HashMap::new());
    }
    let defaults = ed.defaults();
    if let Some(how) = defaults.library.remembered_clash() {
        return Ok(clashes.iter().map(|c| (c.source, how)).collect());
    }
    let names = clashes
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let options = Options::worded(
        key!("This drawing already has: %{names}. Add as new, use the drawing's, or replace them?"),
        &[("names", &names)],
    )
    .keyword("New", "N")
    .keyword("Use", "U")
    .keyword("Replace", "R")
    .default("New".to_owned());
    let how = match ed.get_keyword(options)?.as_str() {
        "Use" => BlockClash::UseExisting,
        "Replace" => BlockClash::Replace,
        _ => BlockClash::Rename,
    };
    let keep = Options::<String>::new(key!("Use this answer for the rest of this session?"))
        .keyword("Yes", "Y")
        .keyword("No", "N")
        .default("No".to_owned());
    if ed.ask_keyword(keep)?.as_deref() == Some("Yes") {
        defaults.library.remember_clash(Some(how));
        ed.say(key!("That answer is kept until OpenDraft is closed."));
    }
    Ok(clashes.iter().map(|c| (c.source, how)).collect())
}

/// How every block of the source that the drawing already has is settled:
/// the ones that are the same (objects, base point and unit) use the drawing's
/// without asking, and the others are asked about.
fn settle_all(
    ed: &Editor,
    source: &Document,
    clashes: Vec<BlockNameClash>,
) -> Result<HashMap<BlockId, BlockClash>, Cancelled> {
    let source = source.clone();
    let (same, different): (Vec<_>, Vec<_>) = ed.read(move |doc| {
        clashes
            .into_iter()
            .partition(|c| match (source.block(c.source), doc.block(c.existing)) {
                (Some(mine), Some(theirs)) => mine.same_as(&source, theirs, doc),
                _ => false,
            })
    })?;
    let mut choices: HashMap<BlockId, BlockClash> = same
        .iter()
        .map(|c| (c.source, BlockClash::UseExisting))
        .collect();
    choices.extend(settle_clashes(ed, &different)?);
    Ok(choices)
}

/// LIBINSERT: a library block name, then the insertion point, as INSERT. A
/// block of another unit is scaled to the drawing's. A name the drawing
/// already has is settled first (add as new, use the drawing's, or replace its
/// definition, which every reference follows). One undo step.
pub fn library_insert(ed: &mut Editor) -> Result<(), Cancelled> {
    let defaults = ed.defaults();
    let listed = defaults
        .library
        .with(|lib| (lib.blocks(), lib.recents().first().map(|b| b.name.clone())));
    for notice in defaults.library.take_notices() {
        ed.say(notice);
    }
    let (blocks, newest) = match listed {
        Ok(listed) => listed,
        Err(error) => {
            ed.say(error.to_string());
            return Ok(());
        }
    };
    if blocks.is_empty() {
        ed.say("The library has no blocks. Add one with LIBADD first.");
        return Ok(());
    }
    // The Content Manager names the block when it starts the command.
    let picked = defaults
        .library
        .take_pick()
        .and_then(|id| blocks.iter().find(|b| b.id == id).cloned());
    let chosen = match picked {
        Some(block) => block,
        None => match choose(ed, &blocks, newest.as_deref())? {
            Some(block) => block,
            None => return Ok(()),
        },
    };
    let id = chosen.id;
    let found = defaults.library.with(|lib| {
        let (doc, block) = lib.source(id)?;
        Some((doc.clone(), block, doc.block(block)?.clone()))
    });
    let Ok(Some((source, root, def))) = found else {
        ed.say("That block is no longer in the library.");
        return Ok(());
    };
    let clashes = {
        let source = source.clone();
        ed.read(move |doc| doc.block_clashes(&source, root).unwrap_or_default())?
    };
    let choices = settle_all(ed, &source, clashes)?;

    let from = def.unit;
    let units = ed.read(move |doc| InsertUnits::ratio(from, doc.units().insert))?;
    let (at, scale, rotation) = ask_placement(ed, &def, units)?;
    let values = ask_values(ed, &def)?
        .into_iter()
        .map(|r| r.value)
        .collect::<Vec<FieldText>>();
    let placement = LibraryPlacement {
        insertion: at,
        scale_x: scale.0 * units,
        scale_y: scale.1 * units,
        rotation,
        values: Some(values),
    };

    let sink = LibraryInsertSink::default();
    let action = InsertFromLibrary::new(source, root, choices, placement).reporting(sink.clone());
    let guard = sink.clone();
    let done = ed.transaction("Insert from library", move |tx| {
        tx.perform(action);
        let refused = guard.lock().map_or(None, |r| r.refused);
        match refused {
            None => Ok(()),
            Some(_) => Err(Cancelled),
        }
    });
    let report = sink.lock().map(|r| r.clone()).unwrap_or_default();
    if let Some(why) = report.refused {
        ed.say(format!("Nothing was inserted: {why}"));
        return Ok(());
    }
    done?;
    if report.block_name != chosen.name {
        ed.say(format!(
            "Block added to this drawing as \"{}\".",
            report.block_name
        ));
    }
    if report.rebound > 0 {
        ed.say(format!(
            "The block's definition was replaced; {} followed it.",
            crate::text::plural(report.rebound, "reference")
        ));
    }
    for warning in &report.warnings {
        ed.say(warning.to_string());
    }
    // Recent is a courtesy: a failure to note it is not worth saying.
    let _ = defaults.library.with(|lib| lib.touch_recent(id));
    Ok(())
}

/// The insertion point, with Scale and Rotate as options before it and a
/// ghost of the block that follows the cursor.
fn ask_placement(
    ed: &Editor,
    block: &Arc<BlockDef>,
    units: f64,
) -> Result<(opendraft_core::Point, (f64, f64), f64), Cancelled> {
    let mut scale = (1.0, 1.0);
    // Level with the coordinate system's X axis unless turned.
    let mut rotation = ed.read(|doc: &Document| doc.ccs().angle)?;
    loop {
        let (shown, s, r) = (block.clone(), (scale.0 * units, scale.1 * units), rotation);
        let options = PointOptions::new(key!("Specify insertion point"))
            .keyword("Scale", "S")
            .keyword("Rotate", "R")
            .ghost(move |at| {
                let defaults = shown.new_references(|d| d.default_value.clone());
                vec![placed(&shown, at, s, r, defaults)]
            });
        match ed.ask_point(options)? {
            Answer::Value(at) => return Ok((at, scale, rotation)),
            Answer::Keyword(k) if k == "Scale" => {
                let f = get_factor(ed, key!("Specify scale factor for XY axes"), scale.0)?;
                scale = (f, f);
            }
            Answer::Keyword(_) => {
                let options = NumberOptions::new(key!("Specify rotation angle")).default(rotation);
                rotation = ed.get_angle(options)?;
            }
            Answer::Enter => {}
        }
    }
}
