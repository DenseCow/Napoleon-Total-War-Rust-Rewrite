//! NapoleonRust's own campaign save format: the campaign model serialised as it is, with no ESF
//! tree behind it, so any campaign can be saved whatever source it came from (DESIGN.md §3.5.1,
//! `analysis/modding/OWN_SAVE_FORMAT.md`). Saves in the original's `.save` format are out of scope
//! (user decision); [`crate::save`] still writes that format for tools and tests.
//!
//! Layout (all integers little-endian):
//! | bytes | content |
//! |---|---|
//! | 16 | [`MAGIC`] |
//! | 4 | format version ([`FORMAT_VERSION`]) |
//! | 4 | `n`: length of the header section |
//! | n | header: deflate (zlib) of the RON text of [`CampaignInfo`] (what the Load Game page shows) |
//! | rest | body: deflate (zlib) of the RON text of [`SaveData`] (the model and the scripts' slots) |
//!
//! The header comes first and alone so the Load Game page reads it without the body.
//!
//! **Versions and migration.** RON names every field, so a field added to the model with
//! `#[serde(default)]` loads from an older save without a version change. A change an old save
//! cannot express that way (a renamed or reshaped field) bumps [`FORMAT_VERSION`] and adds a step to
//! [`read`] that converts the older text before it is decoded. A save from a newer version gives
//! [`FormatError::Newer`].
//!
//! **What is saved.** The whole [`CampaignModel`] except the fields marked `serde(skip)` in
//! `ntw_sim`: game data rebuilt on load (`rules` from the DB, `terrain` from the map), fields the
//! original does not save either (the trait / ancillary script RNGs, the open negotiation, a duel
//! wound, sabotage marks, agents' used actions, spy-network sight) and the ESF writer's own
//! bookkeeping (`recruitment_sources`). No count is capped: every collection is written as long as
//! it is.

use std::io::{Read, Write};

use ntw_data::GameDatabase;
use ntw_sim::campaign::{CampaignModel, FactionId};
use serde::{Deserialize, Serialize};

use crate::script_values::ScriptSaveValue;
use crate::{CampaignInfo, LoadError, LoadedCampaign};

/// The first bytes of every save in this format.
pub const MAGIC: &[u8; 16] = b"NAPOLEONRUST\0SAV";
/// The format version this build writes and the newest it reads.
pub const FORMAT_VERSION: u32 = 1;

/// Everything a save holds besides its header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    /// The human player's faction key.
    pub human: String,
    /// The campaign state.
    pub model: CampaignModel,
    /// The rebel faction's id, if the campaign has one.
    pub rebel_faction: Option<FactionId>,
    /// The scripts' `save_value` slots, in order (the values `load_value` gives back).
    pub script_values: Vec<ScriptSaveValue>,
    /// The scripts' restricted units (the restricted building levels are in the model).
    pub restricted_units: Vec<String>,
}

/// Why a save in this format could not be read or written.
#[derive(Debug)]
pub enum FormatError {
    /// The bytes end before a section does.
    Truncated,
    /// The save was written by a newer NapoleonRust.
    Newer {
        /// The save's format version.
        found: u32,
    },
    /// A section is not valid compressed RON for its type.
    Decode(String),
    /// The data could not be encoded (a value RON cannot represent).
    Encode(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "the save is truncated"),
            Self::Newer { found } => write!(f, "the save has format version {found}; this NapoleonRust reads up to {FORMAT_VERSION}"),
            Self::Decode(e) => write!(f, "the save is damaged: {e}"),
            Self::Encode(e) => write!(f, "cannot encode the save: {e}"),
        }
    }
}

impl std::error::Error for FormatError {}

/// The header of a save of `model` made by `human` now: the loaded campaign's facts (`base`) with
/// the turn, year, date and season name of the model, the human marked as the player, the territory
/// pictures of `pictures` rendered for the human's regions (as the original's header shows them,
/// [`crate::header_map`]) and `timestamp` (unix seconds). Portrait and flag are kept from `base`, as
/// the ESF writer keeps them. Every theatre of `base` is kept (the campaign's theatres,
/// [`CampaignInfo::theatres`]); one whose pictures did not load (logged where they are loaded) is
/// saved without a picture (0 x 0).
pub fn save_header(base: &CampaignInfo, model: &CampaignModel, human: &str, timestamp: u32, pictures: &[crate::header_map::TheatrePictures]) -> CampaignInfo {
    let cal = &model.calendar;
    let owned = crate::header_map::owned_regions(model, human);
    let maps = base
        .theatres()
        .map(|theatre| match pictures.iter().find(|p| p.theatre() == theatre) {
            Some(p) => crate::HeaderMap {
                theatre: theatre.to_owned(),
                width: p.size().0,
                height: p.size().1,
                pitch: (p.size().0 * 4) as i32,
                pixels: p.render(&owned, None),
            },
            None => crate::HeaderMap { theatre: theatre.to_owned(), ..Default::default() },
        })
        .collect();
    let mut info = base.clone();
    info.kind = crate::FileKind::Save;
    info.timestamp = timestamp;
    info.build_id = "NapoleonRust".to_owned();
    info.build_version = env!("CARGO_PKG_VERSION").to_owned();
    info.header.faction_key = human.to_owned();
    info.header.turn_number = cal.turn_number();
    info.header.year = cal.date.year;
    info.header.date = Some(cal.date);
    if let Some(season) = cal.date.season_header_name() {
        info.header.season_name = season.to_owned();
    }
    info.header.maps = maps;
    for p in &mut info.players {
        p.is_human = p.faction_key == human;
    }
    info
}

/// True when `bytes` start with this format's magic.
pub fn is_own_save(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, FormatError> {
    let text = ron::to_string(value).map_err(|e| FormatError::Encode(e.to_string()))?;
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(text.as_bytes()).and_then(|_| z.finish()).map_err(|e| FormatError::Encode(e.to_string()))
}

fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, FormatError> {
    let mut text = String::new();
    flate2::read::ZlibDecoder::new(bytes).read_to_string(&mut text).map_err(|e| FormatError::Decode(e.to_string()))?;
    ron::from_str(&text).map_err(|e| FormatError::Decode(e.to_string()))
}

/// Writes a save: `info` is the header (the caller has filled it for this save: turn, date,
/// territory pictures, timestamp), `data` the body.
pub fn write(info: &CampaignInfo, data: &SaveData) -> Result<Vec<u8>, FormatError> {
    let header = encode(info)?;
    let body = encode(data)?;
    let len = u32::try_from(header.len()).map_err(|_| FormatError::Encode(format!("header of {} bytes", header.len())))?;
    let mut out = Vec::with_capacity(24 + header.len() + body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&body);
    Ok(out)
}

/// The header and body sections of a save, after the version check.
fn sections(bytes: &[u8]) -> Result<(&[u8], &[u8]), FormatError> {
    let rest = bytes.strip_prefix(MAGIC.as_slice()).ok_or(FormatError::Truncated)?;
    let word = |b: &[u8]| -> Option<u32> { Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?)) };
    let version = word(rest).ok_or(FormatError::Truncated)?;
    if version > FORMAT_VERSION {
        return Err(FormatError::Newer { found: version });
    }
    // Version 1 is the first: older versions to convert do not exist yet.
    let len = word(&rest[4..]).ok_or(FormatError::Truncated)? as usize;
    let rest = &rest[8..];
    if rest.len() < len {
        return Err(FormatError::Truncated);
    }
    Ok(rest.split_at(len))
}

/// Reads only the header (for the Load Game page).
pub fn read_info(bytes: &[u8]) -> Result<CampaignInfo, FormatError> {
    decode(sections(bytes)?.0)
}

/// The header and the body exactly as stored (the model without rules).
pub fn read_parts(bytes: &[u8]) -> Result<(CampaignInfo, SaveData), FormatError> {
    let (header, body) = sections(bytes)?;
    Ok((decode(header)?, decode(body)?))
}

/// Reads a whole save. The rules are rebuilt from `db` (they are game data, not saved); the map's
/// movement grid and region links come from the campaign's map as for any load.
pub fn read(bytes: &[u8], db: &GameDatabase) -> Result<LoadedCampaign, FormatError> {
    let (info, data) = read_parts(bytes)?;
    let mut model = data.model;
    crate::attach_rules(&mut model, db, &info.campaign_key);
    Ok(LoadedCampaign {
        model,
        info,
        rebel_faction: data.rebel_faction,
        warnings: Vec::new(),
        script_values: data.script_values,
        restricted_units: data.restricted_units,
    })
}

impl From<FormatError> for LoadError {
    fn from(e: FormatError) -> Self {
        LoadError::OwnSave(e)
    }
}

#[cfg(test)]
pub(crate) mod tests;
