//! The machinery that turns decoded DB rows into named Rust structs.
//!
//! Each table is described once, in [`crate::schemas`], with the `db_record!` macro.
//! From that one description the macro generates:
//! * the struct, with one named field per column, in file order;
//! * its [`Schema`] (column types and version guards), handed to
//!   [`ntw_formats::db::DbTable::read`];
//! * `from_row`, which moves the decoded values into the struct.
//!
//! Because the struct and the schema come from the same list, they can never get out of step.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ntw_formats::db::{DbTable, DbValue, FieldType, Schema};

use crate::DataError;

/// A Rust type that can hold one DB column.
///
/// | Rust type | column type |
/// |---|---|
/// | `String` | string |
/// | `Option<String>` | optional string (`None` = absent; the exe reads that as `""`) |
/// | `bool` | 1-byte bool |
/// | `i32` / `f32` | 4 bytes (the exe does not say which; the schema decides) |
/// | `u16` | 2 bytes |
pub trait Column: Sized {
    /// The wire type for this Rust type.
    const FIELD: FieldType;
    /// Takes the value out of a decoded cell, or `None` on a type mismatch.
    fn from_value(v: &DbValue) -> Option<Self>;
}

impl Column for String {
    const FIELD: FieldType = FieldType::Str;
    fn from_value(v: &DbValue) -> Option<Self> {
        match v {
            DbValue::Str(s) => Some(s.clone()),
            _ => None,
        }
    }
}

impl Column for Option<String> {
    const FIELD: FieldType = FieldType::OptStr;
    fn from_value(v: &DbValue) -> Option<Self> {
        match v {
            DbValue::OptStr(s) => Some(s.clone()),
            _ => None,
        }
    }
}

impl Column for bool {
    const FIELD: FieldType = FieldType::Bool;
    fn from_value(v: &DbValue) -> Option<Self> {
        v.as_bool()
    }
}

impl Column for i32 {
    const FIELD: FieldType = FieldType::I32;
    fn from_value(v: &DbValue) -> Option<Self> {
        v.as_i32()
    }
}

impl Column for f32 {
    const FIELD: FieldType = FieldType::F32;
    fn from_value(v: &DbValue) -> Option<Self> {
        v.as_f32()
    }
}

impl Column for u16 {
    const FIELD: FieldType = FieldType::U16;
    fn from_value(v: &DbValue) -> Option<Self> {
        v.as_u16()
    }
}

/// Reads the next cell of a row into a field. On failure, returns the field's name.
#[doc(hidden)]
pub fn take<T: Column>(cell: Option<&DbValue>, field: &'static str) -> Result<T, &'static str> {
    cell.and_then(T::from_value).ok_or(field)
}

/// A struct that mirrors one DB table's rows. Implemented by `db_record!`.
pub trait DbRecord: Sized {
    /// The table name, e.g. `"units"`. The file is `db\<TABLE>_tables\<TABLE>`.
    const TABLE: &'static str;
    /// The column types and version guards, in file order.
    fn schema() -> Schema;
    /// Builds a record from one decoded row. On a type mismatch, returns the field's name.
    fn from_row(row: &[DbValue]) -> Result<Self, &'static str>;
    /// The row's lookup key (usually column 0).
    fn key(&self) -> &str;
    /// The table's path inside the packs.
    fn path() -> String {
        format!("db/{0}_tables/{0}", Self::TABLE)
    }
}

/// All rows of one table plus a key → row index.
///
/// If several rows share a key, [`get`](Self::get) returns the **first** one (INFERRED; the
/// exe's duplicate handling is not documented). All rows stay in [`rows`](Self::rows).
///
/// A table never changes after it is built, so its [`id`](Self::id) names its rows: a row number
/// read from a table stays valid wherever that table's id is still seen.
#[derive(Debug, Clone)]
pub struct Table<T> {
    version: u32,
    rows: Vec<T>,
    index: HashMap<String, usize>,
    id: u64,
}

/// The next [`Table::id`] (0 is the empty default table's).
static NEXT_TABLE_ID: AtomicU64 = AtomicU64::new(1);

impl<T> Default for Table<T> {
    fn default() -> Self {
        Self { version: 0, rows: Vec::new(), index: HashMap::new(), id: 0 }
    }
}

/// Equal rows and version (the id names a build, not the contents).
impl<T: PartialEq> PartialEq for Table<T> {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version && self.rows == other.rows
    }
}

impl<T: DbRecord> Table<T> {
    /// Decodes a table file. Fails unless every row parses and the data ends exactly after the last row.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DataError> {
        let raw = DbTable::read(bytes, &T::schema()).map_err(|error| DataError::Db { table: T::TABLE, error })?;
        let rows = raw
            .rows
            .iter()
            .enumerate()
            .map(|(row, cells)| {
                T::from_row(cells).map_err(|field| DataError::BadColumn { table: T::TABLE, row, field })
            })
            .collect::<Result<Vec<T>, DataError>>()?;
        Ok(Self::from_rows(raw.version, rows))
    }

    /// Builds a table from records made in code (used by the test fixture).
    pub fn from_rows(version: u32, rows: Vec<T>) -> Self {
        let mut index = HashMap::with_capacity(rows.len());
        for (i, r) in rows.iter().enumerate() {
            index.entry(r.key().to_owned()).or_insert(i);
        }
        Self { version, rows, index, id: NEXT_TABLE_ID.fetch_add(1, Ordering::Relaxed) }
    }

    /// The row with this key (exact, case-sensitive match).
    pub fn get(&self, key: &str) -> Option<&T> {
        self.index.get(key).map(|&i| &self.rows[i])
    }

    /// [`get`](Self::get) with the row's number in [`rows`](Self::rows).
    pub fn get_row(&self, key: &str) -> Option<(usize, &T)> {
        self.index.get(key).map(|&i| (i, &self.rows[i]))
    }

    /// Names this table's rows: every table built gets a new id (a clone keeps it, as it has the
    /// same rows), so a cache of row numbers checks it to see that the table was not replaced.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// All rows in file order.
    pub fn rows(&self) -> &[T] {
        &self.rows
    }

    /// Iterates over all rows in file order.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.rows.iter()
    }

    /// Number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True if the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The table version from the file header (selects which versioned columns exist).
    pub fn version(&self) -> u32 {
        self.version
    }
}

impl<'a, T> IntoIterator for &'a Table<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.iter()
    }
}

/// Picks the `IfAbsent` rule for a versioned column: `copy` means "copy the previous column".
#[doc(hidden)]
#[macro_export]
macro_rules! __db_absent {
    () => {
        ::ntw_formats::db::IfAbsent::Default
    };
    (copy) => {
        ::ntw_formats::db::IfAbsent::CopyPrevious
    };
}

/// Defines a record struct together with its schema. See the module docs.
///
/// ```ignore
/// db_record! {
///     /// Docs for the struct.
///     pub struct Example in "example", key = key {
///         /// Docs for the column.
///         key: String,
///         count: i32,
///         newer_column: bool => since 2,       // present only when version >= 2
///         copied_column: i32 => since 1 copy,  // absent: copy the previous column
///     }
/// }
/// ```
macro_rules! db_record {
    (
        $(#[$sm:meta])*
        pub struct $name:ident in $table:literal, key = $key:ident {
            $(
                $(#[$fm:meta])*
                $f:ident : $t:ty $( => since $v:literal $( $copy:ident )? )?
            ),* $(,)?
        }
    ) => {
        $(#[$sm])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $name {
            $( $(#[$fm])* pub $f: $t, )*
        }

        impl $crate::record::DbRecord for $name {
            const TABLE: &'static str = $table;

            fn schema() -> ::ntw_formats::db::Schema {
                ::ntw_formats::db::Schema {
                    fields: vec![$(
                        ::ntw_formats::db::FieldDef {
                            ty: <$t as $crate::record::Column>::FIELD,
                            min_version: 0 $( + $v )?,
                            if_absent: $crate::__db_absent!($( $( $copy )? )?),
                        }
                    ),*],
                }
            }

            fn from_row(row: &[::ntw_formats::db::DbValue]) -> Result<Self, &'static str> {
                let mut cells = row.iter();
                Ok(Self {
                    $( $f: $crate::record::take::<$t>(cells.next(), stringify!($f))?, )*
                })
            }

            fn key(&self) -> &str {
                &self.$key
            }
        }
    };
}
pub(crate) use db_record;

#[cfg(test)]
mod tests {
    use super::*;

    db_record! {
        /// A made-up table used only to test the macro.
        pub struct Sample in "sample", key = name {
            name: String,
            note: Option<String>,
            flag: bool,
            count: i32,
            count_copy: i32 => since 1 copy,
            ratio: f32 => since 2,
            small: u16,
        }
    }

    fn table_bytes(version: u32, body: &[u8], rows: u32) -> Vec<u8> {
        let mut b = vec![0xFC, 0xFD, 0xFE, 0xFF];
        b.extend_from_slice(&version.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&rows.to_le_bytes());
        b.extend_from_slice(body);
        b
    }

    #[test]
    fn schema_matches_fields() {
        let s = Sample::schema();
        assert_eq!(s.fields.len(), 7);
        assert_eq!(s.fields[4].min_version, 1);
        assert_eq!(s.fields[4].if_absent, ntw_formats::db::IfAbsent::CopyPrevious);
        assert_eq!(s.fields[5].min_version, 2);
        assert_eq!(s.fields[5].ty, FieldType::F32);
        assert_eq!(Sample::path(), "db/sample_tables/sample");
    }

    #[test]
    fn decodes_rows_by_version() {
        // v0 row: "ab", absent note, true, 5, (copy) , (default), 9
        let body = [2, 0, b'a', 0, b'b', 0, 0, 1, 5, 0, 0, 0, 9, 0];
        let t = Table::<Sample>::from_bytes(&table_bytes(0, &body, 1)).unwrap();
        let r = t.get("ab").unwrap();
        assert_eq!((r.note.clone(), r.flag, r.count, r.count_copy, r.ratio, r.small), (None, true, 5, 5, 0.0, 9));
        assert_eq!(t.version(), 0);

        // v2 row with every column present.
        let mut body = vec![1, 0, b'x', 0, 1, 1, 0, b'n', 0, 0];
        body.extend_from_slice(&1i32.to_le_bytes());
        body.extend_from_slice(&2i32.to_le_bytes());
        body.extend_from_slice(&0.5f32.to_le_bytes());
        body.extend_from_slice(&3u16.to_le_bytes());
        let t = Table::<Sample>::from_bytes(&table_bytes(2, &body, 1)).unwrap();
        let r = &t.rows()[0];
        assert_eq!((r.note.as_deref(), r.count_copy, r.ratio, r.small), (Some("n"), 2, 0.5, 3));
    }

    #[test]
    fn leftover_bytes_are_an_error() {
        let body = [2, 0, b'a', 0, b'b', 0, 0, 1, 5, 0, 0, 0, 9, 0, 0xEE];
        assert!(matches!(
            Table::<Sample>::from_bytes(&table_bytes(0, &body, 1)),
            Err(DataError::Db { table: "sample", .. })
        ));
    }

    #[test]
    fn first_duplicate_key_wins() {
        let a = Sample { name: "k".into(), count: 1, ..Default::default() };
        let b = Sample { name: "k".into(), count: 2, ..Default::default() };
        let t = Table::from_rows(0, vec![a, b]);
        assert_eq!((t.get("k").unwrap().count, t.len()), (1, 2));
        assert!(t.get("K").is_none());
    }

    /// Every built table gets its own id (a row-number cache checks it), a clone keeps it, and
    /// equality ignores it (the id names a build, not the contents).
    #[test]
    fn each_build_has_its_own_id_and_a_clone_keeps_it() {
        let row = || Sample { name: "k".into(), count: 1, ..Default::default() };
        let (a, b) = (Table::from_rows(0, vec![row()]), Table::from_rows(0, vec![row()]));
        assert_ne!(a.id(), b.id());
        assert_ne!(a.id(), Table::<Sample>::default().id());
        assert_eq!(a.clone().id(), a.id());
        assert_eq!(a, b);
        assert_eq!(a.get_row("k").map(|(i, r)| (i, r.count)), Some((0, 1)));
    }
}
