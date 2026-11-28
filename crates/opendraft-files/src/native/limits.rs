//! How many things a file may hold. A file is capped in bytes already, but a
//! few bytes per entry turn into far more memory once each entry is built, so
//! the long lists are cut off while they are read: the reader stops at the
//! entry past the limit instead of collecting them all first. The limits are
//! far above any real drawing.

use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{Deserialize, Deserializer, Error, MapAccess, SeqAccess, Visitor};

/// Objects in the drawing.
pub(super) const OBJECTS: usize = 5_000_000;
/// Rows of a table: layers, styles, layouts and blocks.
pub(super) const TABLE_ROWS: usize = 100_000;
/// Objects in one block.
pub(super) const BLOCK_OBJECTS: usize = 1_000_000;
/// Points, bulges, widths and edges of one shape.
pub(super) const VERTICES: usize = 1_000_000;
/// Runs, fields, children and references in the text of one attribute.
pub(super) const FIELD_ITEMS: usize = 4096;
/// Parameters, actions and values of one dynamic block or insert.
pub(super) const SCHEMA_ITEMS: usize = 4096;
/// Entries of one parameter's list of allowed values.
pub(super) const VALUE_LIST: usize = 65_536;
/// Bytes of data a field has cached.
pub(super) const CACHED_BYTES: usize = 1024 * 1024;

struct Bounded<T> {
    max: usize,
    what: &'static str,
    entries: PhantomData<T>,
}

impl<'de, T: Deserialize<'de>> Visitor<'de> for Bounded<T> {
    type Value = Vec<T>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a list of at most {} {}", self.max, self.what)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
        // A claimed length is not believed past a modest start.
        let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1024));
        while let Some(entry) = seq.next_element()? {
            if out.len() >= self.max {
                return Err(A::Error::custom(format!(
                    "the file has more than {} {}",
                    self.max, self.what
                )));
            }
            out.push(entry);
        }
        Ok(out)
    }
}

fn bounded<'de, D, T>(d: D, max: usize, what: &'static str) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    d.deserialize_seq(Bounded {
        max,
        what,
        entries: PhantomData,
    })
}

pub(super) fn objects<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    bounded(d, OBJECTS, "objects")
}

pub(super) fn table_rows<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    bounded(d, TABLE_ROWS, "table entries")
}

pub(super) fn block_objects<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    bounded(d, BLOCK_OBJECTS, "objects in a block")
}

pub(super) fn schema_items<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    bounded(d, SCHEMA_ITEMS, "parameters, actions or values")
}

pub(super) fn value_list<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    bounded(d, VALUE_LIST, "allowed values")
}

pub(super) fn vertices<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    bounded(d, VERTICES, "points in a shape")
}

pub(super) fn field_items<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    bounded(d, FIELD_ITEMS, "parts of a field")
}

pub(super) fn cached_bytes<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    bounded(d, CACHED_BYTES, "bytes of cached field data")
}

/// A map of at most [`TABLE_ROWS`] entries, naming counts (a report's skipped
/// kinds).
struct BoundedMap {
    max: usize,
    what: &'static str,
}

impl<'de> Visitor<'de> for BoundedMap {
    type Value = BTreeMap<String, u64>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a map of at most {} {}", self.max, self.what)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut out = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<String, u64>()? {
            if out.len() >= self.max {
                return Err(A::Error::custom(format!(
                    "the file has more than {} {}",
                    self.max, self.what
                )));
            }
            out.insert(key, value);
        }
        Ok(out)
    }
}

pub(super) fn map_rows<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, u64>, D::Error> {
    d.deserialize_map(BoundedMap {
        max: TABLE_ROWS,
        what: "counted kinds",
    })
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct Holder {
        #[serde(deserialize_with = "tiny")]
        list: Vec<u32>,
    }

    fn tiny<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u32>, D::Error> {
        bounded(d, 3, "numbers")
    }

    fn parse(text: &str) -> Result<Vec<u32>, String> {
        serde_json::from_str::<Holder>(text)
            .map(|h| h.list)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn a_list_up_to_the_limit_is_read_whole() {
        assert_eq!(parse(r#"{"list":[]}"#), Ok(vec![]));
        assert_eq!(parse(r#"{"list":[1,2,3]}"#), Ok(vec![1, 2, 3]));
    }

    #[test]
    fn a_list_past_the_limit_is_refused_with_what_there_were_too_many_of() {
        let why = parse(r#"{"list":[1,2,3,4]}"#).unwrap_err();
        assert!(why.contains("more than 3 numbers"), "{why}");
    }

    #[test]
    fn the_reader_stops_at_the_first_entry_past_the_limit() {
        // The fifth entry is not a number, but the fourth already ended the
        // read: the error is about the count, not about the entry.
        let why = parse(r#"{"list":[1,2,3,4,"x"]}"#).unwrap_err();
        assert!(why.contains("more than 3 numbers"), "{why}");
    }
}
