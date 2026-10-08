//! Small helpers for reading positional ESF fields with good error messages.
//!
//! ESF values have no names (see `ntw_formats::esf`), so the loader reads "child #n of record X".
//! These helpers check the type of child #n and turn a mismatch into a
//! [`LoadError::BadField`] instead of a panic.

use ntw_formats::esf::{EsfNode, EsfRecord, EsfRecordArray};

use crate::LoadError;

/// The type name of the child at `index`, if there is one.
fn found(r: &EsfRecord, index: usize) -> Option<&'static str> {
    r.get(index).map(EsfNode::type_name)
}

fn bad(r: &EsfRecord, path: &str, index: usize, expected: &'static str) -> LoadError {
    LoadError::BadField {
        path: path.to_string(),
        index,
        expected,
        found: found(r, index),
    }
}

/// Child `index` as an `i32`.
pub(crate) fn i32_at(r: &EsfRecord, index: usize, path: &str) -> Result<i32, LoadError> {
    r.get_i32(index).ok_or_else(|| bad(r, path, index, "i32"))
}

/// Child `index` as a `u32`.
pub(crate) fn u32_at(r: &EsfRecord, index: usize, path: &str) -> Result<u32, LoadError> {
    r.get_u32(index).ok_or_else(|| bad(r, path, index, "u32"))
}

/// Child `index` as a string (UTF-16 or ASCII).
pub(crate) fn str_at<'a>(r: &'a EsfRecord, index: usize, path: &str) -> Result<&'a str, LoadError> {
    r.get_str(index)
        .ok_or_else(|| bad(r, path, index, "string"))
}

/// Child `index` as a record.
pub(crate) fn rec_at<'a>(
    r: &'a EsfRecord,
    index: usize,
    path: &str,
) -> Result<&'a EsfRecord, LoadError> {
    r.get(index)
        .and_then(EsfNode::as_record)
        .ok_or_else(|| bad(r, path, index, "record"))
}

/// The first child record called `name`.
pub(crate) fn child<'a>(
    r: &'a EsfRecord,
    name: &str,
    path: &str,
) -> Result<&'a EsfRecord, LoadError> {
    r.child(name).ok_or_else(|| LoadError::MissingRecord {
        path: format!("{path}/{name}"),
    })
}

/// The first child record array called `name`.
pub(crate) fn array<'a>(
    r: &'a EsfRecord,
    name: &str,
    path: &str,
) -> Result<&'a EsfRecordArray, LoadError> {
    r.record_array(name)
        .ok_or_else(|| LoadError::MissingRecord {
            path: format!("{path}/{name}[]"),
        })
}

/// The `n`-th plain value (not a record or record array) of `r`, for records whose optional
/// child records shift the child indices (e.g. `FACTION`).
pub(crate) fn value(r: &EsfRecord, n: usize) -> Option<&EsfNode> {
    r.values().nth(n)
}

/// Reads a `DATE` record: `{u32 year, u32 season, u32 month, u32 half}` (W3 §3.1, CONFIRMED).
pub(crate) fn date(r: &EsfRecord, path: &str) -> Result<ntw_sim::calendar::Date, LoadError> {
    Ok(ntw_sim::calendar::Date {
        year: u32_at(r, 0, path)?,
        season: u32_at(r, 1, path)?,
        month: u32_at(r, 2, path)?,
        half: u32_at(r, 3, path)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatches_become_errors() {
        let mut r = EsfRecord::new("R", 0);
        r.children.push(EsfNode::U32(7));
        assert_eq!(u32_at(&r, 0, "R").unwrap(), 7);
        let e = i32_at(&r, 0, "R").unwrap_err();
        assert_eq!(e.to_string(), "R child #0: expected i32, found u32");
        let e = str_at(&r, 3, "R").unwrap_err();
        assert!(matches!(
            e,
            LoadError::BadField {
                index: 3,
                found: None,
                ..
            }
        ));
        assert!(matches!(
            child(&r, "X", "R"),
            Err(LoadError::MissingRecord { .. })
        ));
    }
}
