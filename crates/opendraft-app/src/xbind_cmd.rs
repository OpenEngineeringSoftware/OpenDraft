//! `XBIND`: make individual xref-dependent symbols ordinary host records,
//! keeping the external references themselves.

use opendraft_commands::{Cancelled, Editor, Options, Wording};
use opendraft_core::{Document, XbindSymbol};
use opendraft_ui::key;

use crate::xref_apply;

/// One dependent symbol XBIND can offer by name.
struct Choice {
    label: String,
    symbol: XbindSymbol,
}

/// `-XREF Bind`: make whole references ordinary blocks.
pub(crate) fn bind_all(ed: &mut Editor) -> Result<(), Cancelled> {
    let blocks = crate::xref_cmd::chosen(ed, key!("Enter xref name(s) to bind:"))?;
    if blocks.is_empty() {
        ed.say(key!("No such external reference."));
        return Ok(());
    }
    let count = blocks.len();
    crate::xref_apply::bind(ed, blocks, crate::xref_defaults::defaults_of(ed).bind_type)?;
    ed.say(Wording::count(
        count,
        key!("Bound %{count} external reference."),
        key!("Bound %{count} external references."),
    ));
    Ok(())
}

/// The command.
pub(crate) fn xbind(ed: &mut Editor) -> Result<(), Cancelled> {
    let type_options =
        Options::<String>::new(key!("Enter symbol type [Block/Dimstyle/LAyer/LType/Text]"))
            .keyword("Block", "B")
            .keyword("Dimstyle", "D")
            .keyword("LAyer", "A")
            .keyword("LType", "L")
            .keyword("Text", "T")
            .default("Block".to_owned());
    let label = ed.get_keyword(type_options)?;
    let kind = match label.as_str() {
        "Dimstyle" => 'd',
        "LAyer" => 'a',
        "LType" => 'l',
        "Text" => 't',
        _ => 'b',
    };
    let prompt = match kind {
        'd' => {
            ed.say(key!(
                "Binding dimension styles is not built yet; an xref carries its dimension styles inside the snapshot."
            ));
            return Ok(());
        }
        'a' => key!("Enter dependent layer name(s):"),
        'l' => key!("Enter dependent linetype name(s):"),
        't' => key!("Enter dependent text style name(s):"),
        _ => key!("Enter dependent block name(s):"),
    };
    let choices = ed.read(move |doc| choices(doc, kind))?;
    if choices.is_empty() {
        ed.say(key!("There are no dependent symbols of that kind."));
        return Ok(());
    }
    let typed = ed.get_string(Options::<String>::new(prompt))?;
    let symbols = chosen(&typed, &choices);
    if symbols.is_empty() {
        ed.say(key!("No such dependent symbol."));
        return Ok(());
    }
    let count = symbols.len();
    xref_apply::xbind(ed, symbols, crate::xref_defaults::defaults_of(ed).bind_type)?;
    ed.say(Wording::count(
        count,
        key!("Bound %{count} dependent symbol."),
        key!("Bound %{count} dependent symbols."),
    ));
    Ok(())
}

fn choices(doc: &Document, kind: char) -> Vec<Choice> {
    let mut out = Vec::new();
    for xref in doc.xrefs() {
        let Some(root) = doc.block(xref.block) else {
            continue;
        };
        let prefix = format!("{}|", root.name);
        match kind {
            'a' => {
                for bound in &xref.layers {
                    out.push(Choice {
                        label: format!("{}{}", prefix, bound.source),
                        symbol: XbindSymbol::Layer {
                            xref: xref.block,
                            layer: bound.layer,
                        },
                    });
                }
            }
            'l' => {
                for record in doc.linetypes().records() {
                    if record.name.starts_with(&prefix) {
                        out.push(Choice {
                            label: record.name.clone(),
                            symbol: XbindSymbol::Linetype {
                                xref: xref.block,
                                linetype: record.id,
                            },
                        });
                    }
                }
            }
            't' => {
                for style in doc.text_styles() {
                    if style.name.starts_with(&prefix) {
                        out.push(Choice {
                            label: style.name.clone(),
                            symbol: XbindSymbol::TextStyle {
                                xref: xref.block,
                                style: style.id,
                            },
                        });
                    }
                }
            }
            _ => {
                for block in &xref.blocks {
                    out.push(Choice {
                        label: format!("{}{}", prefix, block.name),
                        symbol: XbindSymbol::Block {
                            xref: xref.block,
                            block: block.id,
                        },
                    });
                }
            }
        }
    }
    out
}

/// The symbols named, or every one for `*`; a bare source name matches every
/// reference that has it.
fn chosen(typed: &str, choices: &[Choice]) -> Vec<XbindSymbol> {
    let typed = typed.trim();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut take = |choice: &Choice| {
        if seen.insert(choice.symbol) {
            out.push(choice.symbol);
        }
    };
    if typed == "*" {
        for choice in choices {
            take(choice);
        }
        return out;
    }
    for name in typed.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        for choice in choices {
            let bare = choice
                .label
                .split_once('|')
                .map_or(choice.label.as_str(), |(_, rest)| rest);
            if choice.label.eq_ignore_ascii_case(name) || bare.eq_ignore_ascii_case(name) {
                take(choice);
            }
        }
    }
    out
}
