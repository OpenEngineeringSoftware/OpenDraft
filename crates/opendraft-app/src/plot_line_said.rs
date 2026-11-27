//! What -PLOT says about a typed answer it cannot use, and its lists
//! (issue #138: said in English, translated where shown).

use opendraft_commands::{Editor, Wording};
use opendraft_ui::key;

pub(crate) const NOT_A_PAPER: &str = key!("\"%{answer}\" is not a paper size.");
pub(crate) const NOT_A_SCALE: &str = key!("\"%{answer}\" is not a scale; try 1:50, 2:1 or Fit.");
pub(crate) const NOT_AN_OFFSET: &str = key!("\"%{answer}\" is not an offset; type x,y.");
pub(crate) const NO_LAYOUT: &str = key!("There is no layout called \"%{answer}\".");
pub(crate) const NO_DEVICE: &str = key!("There is no output device called \"%{answer}\".");

/// Says `text` about the typed `answer` (its field `%{answer}`).
pub(crate) fn refuse(ed: &Editor, text: &str, answer: &str) {
    ed.say(Wording::new(text, &[("answer", answer)]));
}

pub(crate) fn say_names(ed: &Editor, what: &str, names: &[String]) {
    ed.say(Wording::new(what, &[("names", &names.join(", "))]));
}
