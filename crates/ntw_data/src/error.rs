//! The error type for loading game data.

use std::fmt;

use ntw_formats::db::DbError;
use ntw_formats::pack::PackError;

/// Everything that can go wrong while loading the game database.
#[derive(Debug)]
pub enum DataError {
    /// A pack could not be opened or a file could not be read from it.
    Pack(PackError),
    /// A DB table's bytes do not match its schema (short read, leftover bytes, ...).
    Db {
        /// The table name, e.g. `"units"`.
        table: &'static str,
        /// What the binary reader reported.
        error: DbError,
    },
    /// A decoded value had the wrong type for a struct field. This is an internal
    /// bug (the struct and its schema disagree), not a data problem.
    BadColumn {
        /// The table name.
        table: &'static str,
        /// 0-based row number.
        row: usize,
        /// The struct field that could not be filled.
        field: &'static str,
    },
    /// A key-value table lacks a key the game needs.
    MissingKvKey {
        /// The table, e.g. `"_kv_morale"`.
        table: &'static str,
        /// The missing key.
        key: &'static str,
    },
    /// One of the two projectile effect tables (`projectiles_explosions`, `projectile_impacts`)
    /// would not read: a row key from `projectiles` is missing, a row's shape is not the shipped
    /// one, or the row count does not match the header.
    ProjectileFx {
        /// The table, e.g. `"projectiles_explosions"`.
        table: &'static str,
        /// What the reader reported.
        error: ntw_formats::projectile_fx::ProjectileFxError,
    },
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(e) => write!(f, "{e}"),
            Self::Db { table, error } => write!(f, "DB table {table}: {error}"),
            Self::BadColumn { table, row, field } => {
                write!(f, "DB table {table}, row {row}: field {field} has the wrong type")
            }
            Self::MissingKvKey { table, key } => write!(f, "{table} has no key {key:?}"),
            Self::ProjectileFx { table, error } => write!(f, "{table}: {error}"),
        }
    }
}

impl std::error::Error for DataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pack(e) => Some(e),
            Self::Db { error, .. } => Some(error),
            Self::ProjectileFx { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl From<PackError> for DataError {
    fn from(e: PackError) -> Self {
        Self::Pack(e)
    }
}
