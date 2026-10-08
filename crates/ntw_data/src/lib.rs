//! # ntw_data
//!
//! Typed Rust structs for the original game's database tables, loaded from the
//! player's **own install** (read-only) through [`ntw_formats`].
//!
//! The raw reader in `ntw_formats::db` turns a table into rows of loose values. This crate
//! knows each table's real layout ([`schemas`]) and gives every column a name, so game code
//! can write `stats.melee_attack` instead of `row[34]`.
//!
//! ```no_run
//! use ntw_data::GameDatabase;
//! let db = GameDatabase::from_install(r"C:\...\Napoleon Total War\data").unwrap();
//! let unit = db.land_unit("Inf_Line_Austrian_German_Fusiliers").unwrap();
//! println!("{} men, accuracy {}", unit.stats.num_men, unit.stats.accuracy);
//! ```
//!
//! Without an install, use [`GameDatabase::test_fixture`]. It holds made-up placeholder
//! numbers, never game data.
//!
//! | module | contents |
//! |---|---|
//! | [`schemas`] | `UnitRecord`, `UnitStatsLand`, `Projectile`, `FactionRecord`, `RegionRecord`, `BuildingLevel`, `Technology`, ... |
//! | [`kv`] | `_kv_rules` / `_kv_morale` / `_kv_fatigue`, with the exe's float-to-int truncation |
//! | [`record`] | the `Table<T>` container and the machinery behind the schemas |
//! | [`GameDatabase`] | everything loaded, with key lookups and foreign-key helpers |
//! | [`debugger`] | the addresses in the 0-G promotion probe, and a checker for the script that uses them |

mod database;
pub mod debugger;
mod error;
pub mod kv;
pub mod record;
pub mod campaign;
pub mod effects;
pub mod characters;
pub mod schemas;
pub mod weather;

pub use database::{DataSource, GameDatabase, LandUnitView};
pub use error::DataError;
pub use record::{DbRecord, Table};
pub use schemas::*;
pub use campaign::CampaignTables;
