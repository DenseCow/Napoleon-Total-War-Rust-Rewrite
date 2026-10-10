//! Errors and warnings produced while loading a start position or save.
//!
//! * A [`LoadError`] means the file could not be turned into a campaign at all: it is not an ESF
//!   file, it is the wrong kind of ESF file, or a record that every campaign needs is missing or
//!   has an unexpected layout. Loading stops.
//! * A [`LoadWarning`] means one piece of data was odd but the rest is fine (for example a unit key
//!   that the game database does not know, which can happen with mods). Loading continues and the
//!   warning is listed in [`LoadedCampaign::warnings`](crate::LoadedCampaign::warnings).

use std::fmt;
use std::path::PathBuf;

use ntw_formats::esf::EsfError;

/// Everything that can make loading fail. Bad input never panics; it ends up here.
#[derive(Debug)]
pub enum LoadError {
    /// The file could not be read from disk.
    Io {
        /// The file we tried to read.
        path: PathBuf,
        /// What the operating system reported.
        error: std::io::Error,
    },
    /// The bytes are not a valid ESF file (bad magic, truncated, corrupt offsets, ...).
    Esf(EsfError),
    /// The ESF root record has the wrong name, e.g. a save was passed to `load_startpos`.
    WrongRoot {
        /// The root name(s) we accept here.
        expected: &'static str,
        /// The root name found in the file.
        found: String,
    },
    /// A record that every campaign needs is not there.
    MissingRecord {
        /// Where we looked, e.g. `CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD`.
        path: String,
    },
    /// A positional field is missing or has another type than the layout says.
    BadField {
        /// The record holding the field.
        path: String,
        /// The child index inside that record (`#n` in `analysis/worker3/STARTPOS_LAYOUT.md`).
        index: usize,
        /// The type we expected, e.g. `"i32"`.
        expected: &'static str,
        /// The type found, or `None` if the record has fewer children.
        found: Option<&'static str>,
    },
    /// One of our own saves ([`crate::own_save`]) could not be read.
    OwnSave(crate::own_save::FormatError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "cannot read {}: {error}", path.display()),
            Self::Esf(e) => write!(f, "not a valid ESF file: {e}"),
            Self::WrongRoot { expected, found } => {
                write!(f, "ESF root record is {found:?}, expected {expected}")
            }
            Self::MissingRecord { path } => write!(f, "missing record {path}"),
            Self::OwnSave(e) => write!(f, "NapoleonRust save: {e}"),
            Self::BadField {
                path,
                index,
                expected,
                found: Some(found),
            } => {
                write!(
                    f,
                    "{path} child #{index}: expected {expected}, found {found}"
                )
            }
            Self::BadField {
                path,
                index,
                expected,
                found: None,
            } => {
                write!(
                    f,
                    "{path} child #{index}: expected {expected}, but the record ends earlier"
                )
            }
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error, .. } => Some(error),
            Self::Esf(e) => Some(e),
            Self::OwnSave(e) => Some(e),
            _ => None,
        }
    }
}

impl From<EsfError> for LoadError {
    fn from(e: EsfError) -> Self {
        Self::Esf(e)
    }
}

/// Something odd that did not stop loading. The affected item was kept or skipped as described.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWarning {
    /// A unit key is not in the `units` DB table. The unit is still loaded.
    UnknownUnitKey {
        /// The force holding the unit (raw id).
        force: u32,
        /// The unknown key.
        key: String,
    },
    /// A region key is not in the `regions` DB table. The region is still loaded.
    UnknownRegionKey(String),
    /// A faction key is not in the `factions` DB table. The faction is still loaded.
    UnknownFactionKey(String),
    /// A building level key is not in the `building_levels` DB table. The building is still loaded.
    UnknownBuildingKey {
        /// The region holding the building.
        region: String,
        /// The unknown key.
        key: String,
    },
    /// A character type string that `CharacterKind` does not know. The character is skipped.
    UnknownCharacterType {
        /// Raw character id.
        id: i32,
        /// The type string.
        kind: String,
    },
    /// A government record name that `GovernmentType` does not know. The faction gets the
    /// PLACEHOLDER [`GovernmentType::AbsoluteMonarchy`](ntw_sim::campaign::GovernmentType).
    UnknownGovernment {
        /// The faction key.
        faction: String,
        /// The `GOV_IMP` item's record name (empty if there was none).
        name: String,
    },
    /// A diplomatic stance string that `Stance` does not know. That relationship is skipped.
    UnknownStance {
        /// The faction key.
        faction: String,
        /// The stance string.
        stance: String,
    },
    /// A diplomacy relationship points at a faction id that does not exist. It is skipped.
    DanglingDiplomacy {
        /// The faction key that stores the relationship.
        faction: String,
        /// The target faction id.
        target: i32,
    },
    /// A region's owner id is not a loaded faction. The region is still loaded with that id.
    DanglingRegionOwner {
        /// The region key.
        region: String,
        /// The owner id as stored.
        owner: i32,
    },
    /// A force's commander id is not a loaded character. The force is loaded without commander.
    DanglingCommander {
        /// Raw force id.
        force: u32,
        /// Raw commander id.
        commander: i32,
    },
    /// A recruitment queue item whose #0 is missing, not an integer, or 0 (never a real id). The
    /// item is kept with a new id (`World::alloc_id`), so the queue and the save keep it.
    RecruitmentItemWithoutId {
        /// The region key.
        region: String,
        /// The queued unit key.
        unit: String,
    },
    /// A recruitment queue item whose id an earlier item (file order) already has. Recruitment
    /// ids are unique across the world, as `CancelRecruitment` names an item by id alone; the later
    /// item is kept with a new id (`World::alloc_id`).
    DuplicateRecruitmentItemId {
        /// The region key.
        region: String,
        /// The queued unit key.
        unit: String,
        /// The repeated raw id.
        id: i32,
    },
    /// A recruitment queue item without its turns (#3) or unit key (#6). It is skipped.
    UnreadableRecruitmentItem {
        /// The region key.
        region: String,
    },
    /// A recruitment manager (the region's or a port slot's) without its `REGION_RECRUITMENT_ITEM_ARRAY`:
    /// read as an empty queue (the save adds the array only when a new item is written to that manager).
    RecruitmentManagerWithoutItems {
        /// The region key.
        region: String,
        /// `None`: the region's own manager; `Some(i)`: the manager of `REGION_SLOT_ARRAY` item `i` (a port).
        port_slot: Option<usize>,
    },
    /// A `REGION` without its own `REGION_RECRUITMENT_MANAGER` (#27 in every region of the shipped
    /// files): its own queue is read as empty (the save adds the manager only when a new item is
    /// written to it).
    RegionWithoutRecruitmentManager {
        /// The region key.
        region: String,
    },
    /// Two objects of the same kind share an id. The later one is skipped.
    DuplicateId {
        /// `"faction"`, `"region"`, `"fort"`, `"character"` or `"force"`.
        kind: &'static str,
        /// The repeated raw id.
        id: i64,
    },
}

impl fmt::Display for LoadWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownUnitKey { force, key } => {
                write!(
                    f,
                    "force {force}: unit key {key:?} is not in the units table"
                )
            }
            Self::UnknownRegionKey(k) => write!(f, "region key {k:?} is not in the regions table"),
            Self::UnknownFactionKey(k) => {
                write!(f, "faction key {k:?} is not in the factions table")
            }
            Self::UnknownBuildingKey { region, key } => {
                write!(
                    f,
                    "region {region}: building {key:?} is not in building_levels"
                )
            }
            Self::UnknownCharacterType { id, kind } => {
                write!(f, "character {id}: unknown type {kind:?} (skipped)")
            }
            Self::UnknownGovernment { faction, name } => {
                write!(f, "faction {faction}: unknown government {name:?}")
            }
            Self::UnknownStance { faction, stance } => {
                write!(f, "faction {faction}: unknown stance {stance:?} (skipped)")
            }
            Self::DanglingDiplomacy { faction, target } => {
                write!(
                    f,
                    "faction {faction}: relationship with unknown faction id {target} (skipped)"
                )
            }
            Self::DanglingRegionOwner { region, owner } => {
                write!(
                    f,
                    "region {region}: owner id {owner} is not a loaded faction"
                )
            }
            Self::DanglingCommander { force, commander } => {
                write!(
                    f,
                    "force {force}: commander {commander} is not a loaded character"
                )
            }
            Self::DuplicateId { kind, id } => write!(f, "duplicate {kind} id {id} (skipped)"),
            Self::RecruitmentItemWithoutId { region, unit } => {
                write!(f, "region {region}: recruitment item {unit:?} has no usable id: missing, not an integer, or 0 (kept with a new id)")
            }
            Self::DuplicateRecruitmentItemId { region, unit, id } => {
                write!(f, "region {region}: recruitment item {unit:?} repeats id {id} (kept with a new id)")
            }
            Self::UnreadableRecruitmentItem { region } => {
                write!(f, "region {region}: recruitment item without turns or unit key (skipped)")
            }
            Self::RecruitmentManagerWithoutItems { region, port_slot } => {
                let manager = port_slot.map_or_else(|| "its own recruitment manager".to_owned(), |i| format!("the recruitment manager of port slot {i}"));
                write!(f, "region {region}: {manager} has no item array (read as an empty queue)")
            }
            Self::RegionWithoutRecruitmentManager { region } => {
                write!(f, "region {region}: no recruitment manager of its own (read as an empty queue)")
            }
        }
    }
}
