use std::fmt;

use crate::blocks_payload::folder::FolderError;
use crate::blocks_payload::PayloadError;
use crate::envelope::EnvelopeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryError {
    /// Reading or writing a file failed.
    Io(String),
    /// A library file is damaged and no good copy was found. Nothing is
    /// overwritten.
    Corrupt(String),
    /// No such block, folder or Trash item.
    NotFound,
    /// A block of this name is already in the library.
    NameTaken(String),
    InvalidName(String),
    Folder(FolderError),
    /// The block is part of others; delete or change them first. Holds their
    /// names.
    InUse(Vec<String>),
    /// The drawing's rules refused it (a block that holds itself, say).
    Refused(String),
    Package(String),
    /// The file is a library of another version, or not a library: it is
    /// left exactly as it is.
    Unsupported(String),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Io(m) => write!(f, "The library could not be read or written: {m}"),
            LibraryError::Corrupt(m) => write!(f, "The library is damaged: {m}"),
            LibraryError::NotFound => write!(f, "That is no longer in the library."),
            LibraryError::NameTaken(n) => write!(f, "The library already has a block called {n}."),
            LibraryError::InvalidName(m)
            | LibraryError::Refused(m)
            | LibraryError::Package(m)
            | LibraryError::Unsupported(m) => {
                write!(f, "{m}")
            }
            LibraryError::Folder(e) => write!(f, "{e}"),
            LibraryError::InUse(parents) => {
                write!(f, "It is part of {}.", parents.join(", "))
            }
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<FolderError> for LibraryError {
    fn from(e: FolderError) -> Self {
        LibraryError::Folder(e)
    }
}

impl From<rusqlite::Error> for LibraryError {
    fn from(e: rusqlite::Error) -> Self {
        LibraryError::Io(e.to_string())
    }
}

impl From<crate::oblock::OblockError> for LibraryError {
    fn from(e: crate::oblock::OblockError) -> Self {
        match e {
            crate::oblock::OblockError::NoSuchBlock => LibraryError::NotFound,
            other => LibraryError::Package(other.to_string()),
        }
    }
}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        LibraryError::Io(e.to_string())
    }
}

impl From<PayloadError> for LibraryError {
    fn from(e: PayloadError) -> Self {
        LibraryError::Package(e.to_string())
    }
}

impl From<EnvelopeError> for LibraryError {
    fn from(e: EnvelopeError) -> Self {
        LibraryError::Package(e.to_string())
    }
}
