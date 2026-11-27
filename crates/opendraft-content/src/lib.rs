//! The Content Manager's data side: the `.odpack` envelope, the
//! `opendraft.blocks` payload and the local library, with the hub client to
//! follow. No egui.

pub mod blocks_payload;
pub mod envelope;
pub mod library;
pub mod oblock;

#[cfg(test)]
mod test_support;
