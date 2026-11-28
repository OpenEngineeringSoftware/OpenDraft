//! Why an external reference could not be read or planned.

use std::fmt;
use std::io;

use opendraft_core::BlockId;
use opendraft_core::XrefProblem;

use crate::FileError;

/// Why a source drawing was not turned into a snapshot.
#[derive(Debug)]
pub enum XrefLoadError {
    Cancelled,
    UnsupportedSource(String),
    Io(io::Error),
    File(FileError),
    TooLarge { what: &'static str, limit: u64 },
    TooMany { what: &'static str, limit: u64 },
    InvalidSource(String),
    Plan(XrefPlanError),
}

impl fmt::Display for XrefLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "cancelled"),
            Self::UnsupportedSource(path) => {
                write!(f, "\"{path}\" is not a drawing this build can read")
            }
            Self::Io(e) => write!(f, "{e}"),
            Self::File(e) => write!(f, "{e}"),
            Self::TooLarge { what, limit } => {
                write!(f, "the source has more than {limit} {what}")
            }
            Self::TooMany { what, limit } => {
                write!(f, "the source has more than {limit} {what}")
            }
            Self::InvalidSource(why) => write!(f, "the source drawing is not usable: {why}"),
            Self::Plan(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for XrefLoadError {}

impl From<io::Error> for XrefLoadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<FileError> for XrefLoadError {
    fn from(error: FileError) -> Self {
        match error {
            FileError::Cancelled => Self::Cancelled,
            other => Self::File(other),
        }
    }
}

/// Why a source drawing cannot become a snapshot of the host.
#[derive(Debug, PartialEq)]
pub enum XrefPlanError {
    Problem(XrefProblem),
    SourceInvalid(String),
    TooMany {
        what: &'static str,
        limit: u64,
    },
    Cancelled,
    SelfReference,
    /// A name or path the plan cannot use, worded for the user.
    Message(String),
    /// The host already has a block or layer the snapshot would need.
    NameTaken(String),
    /// Nothing could be attached: it would be empty and the caller asked
    /// that not be accepted.
    EmptySource,
    /// The source could not be walked (a cycle or too deep).
    BrokenSource(String),
    /// A block id that the request named is not an external reference.
    NotAnXref(BlockId),
}

impl fmt::Display for XrefPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Problem(problem) => write!(f, "{problem}"),
            Self::SourceInvalid(why) => write!(f, "the source drawing is not usable: {why}"),
            Self::TooMany { what, limit } => {
                write!(f, "the source has more than {limit} {what}")
            }
            Self::Cancelled => write!(f, "cancelled"),
            Self::SelfReference => write!(f, "a drawing cannot reference itself"),
            Self::Message(why) => write!(f, "{why}"),
            Self::NameTaken(name) => {
                write!(
                    f,
                    "a block or layer called \"{name}\" is already in the drawing"
                )
            }
            Self::EmptySource => write!(f, "the source drawing is empty"),
            Self::BrokenSource(why) => write!(f, "the source drawing cannot be followed: {why}"),
            Self::NotAnXref(block) => {
                write!(f, "block {} is not an external reference", block.0)
            }
        }
    }
}

impl std::error::Error for XrefPlanError {}

impl From<XrefProblem> for XrefPlanError {
    fn from(problem: XrefProblem) -> Self {
        Self::Problem(problem)
    }
}
