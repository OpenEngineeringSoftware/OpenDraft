//! External references: reading a source drawing, planning its snapshot, and
//! the saved-path rules.
//!
//! Nothing here knows about the interface. The app asks for a [`XrefPlan`]
//! and applies it through commands; this module only reads files and builds
//! core values.

mod error;
mod path;
mod plan;
mod remap;
mod source;

pub use error::{XrefLoadError, XrefPlanError};
pub use path::{classify, format_saved_path, resolve, stamp, XrefPathError};
pub use plan::plan_xref;
pub use source::{read_source, XrefLimits, XrefSource};
