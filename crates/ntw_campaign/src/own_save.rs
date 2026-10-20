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
//! [`FormatError::Newer`]. A field an old save lacks and no default can stand in for (data the
//! campaign's start holds) bumps the version too: the older save is converted from the campaign's
//! start ([`read_with_start`]), and read without it is an error ([`FormatError::NeedsStart`]).
//!
//! | version | change |
//! |---|---|
//! | 1 | the first |
//! | 2 | regions keep their unit resources (`Region::unit_resources`, `REGION/RESOURCES_ARRAY`); a version-1 save takes them from the campaign's start |
//! | 3 | regions keep their road and walls holders (`Region::road_holder`, `Region::fortification_holder`); an older save takes them from the campaign's start |
//! | 4 | construction items keep their kind (`ConstructionItem::kind`: build, repair, demolish) and their active byte (`ConstructionItem::active`, read active from an older save); an older save's repair, stored as a build of the standing level, is converted (`CampaignModel::convert_legacy_repairs`), and a version 1 or 2 save made mid-round gets back the construction step the turn start ran then (see [`read_with_start`]) |
//! | 5 | trade routes keep their capital port flag (`TradePath::capital_port`); an older save's routes that leave by sea from their exporter's capital get it (see [`read_with_start`]) |
//! | 6 | the diplomacy manager's treaty-break counts start at 1 (`TREATY_BREAKS_START`, the exe's preset) and the access-abuse counts are kept (`World::access_abuses`); an older save's treaty-break entries, counted from 0, get 1 added, and it has no access-abuse counts (none were kept, read 0) (see [`read_with_start`]) |
//! | 7 | trade routes keep their own accumulated value (`TradePath::accumulated`, +0x3C); `World::trade_accumulated` holds only the value of a pair's built route (a pair with no stored route). An older save's per-pair value of a pair with stored routes goes to its first route (see [`read_with_start`]) |
//! | 8 | the commerce raids (`World::commerce_raids`) and the trade network's course they need: each leg's spline ids (`TradeLeg::splines`) and each segment's curves (`TradeSegmentSight::curves`); an older save, with no raids, takes the course from the campaign's start (see [`read_with_start`]) |
//! | 9 | the running economics record's expense trackers: each faction's land and naval upkeep trackers (`World::upkeep_booked`) and each region's policing tracker (`PopulationState::policing_booked`), with the policing cost (`PopulationState::policing_cost`); an older save's trackers take what its running record holds (see [`read_with_start`]) |
//! | 10 | each faction's diplomacy records keep their order (`World::diplomacy_record_order`, the order the allies of a war are called in); an older save takes it from the campaign's start, or without one walks the records by faction id (see [`read_with_start`]) |
//! | 11 | the region manager's order (`World::region_order`, the order each faction's region list follows after a load); an older save takes it from the campaign's start, or without one walks the regions by id (see [`read_with_start`]) |
//!
//! **What is saved.** The whole [`CampaignModel`] except the fields marked `serde(skip)` in
//! `ntw_sim`: game data rebuilt on load (`rules` from the DB, `terrain` from the map), fields the
//! original does not save either (the trait / ancillary script RNGs, the open negotiation, a duel
//! wound, sabotage marks, agents' used actions, spy-network sight, the regions' gain order: its reload
//! rebuilds each faction's region list in the manager's order) and the ESF writer's own
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
pub const FORMAT_VERSION: u32 = 11;

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
    /// Legacy: the scripts' restricted units as saves written before `World::restricted_units` kept them
    /// (read only; [`read`] merges them into the model, which holds them now: each unit once, when not listed
    /// already). Written only when not empty, so a save from [`SaveDataRef`] and one from this struct stay identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restricted_units: Vec<String>,
}

/// The same body as [`SaveData`] with the model borrowed, for writing without cloning it (both
/// serialise to identical text, so [`read_parts`] reads what [`write`] wrote from either).
#[derive(Debug, Serialize)]
pub struct SaveDataRef<'a> {
    /// See [`SaveData::human`].
    pub human: &'a str,
    /// See [`SaveData::model`].
    pub model: &'a CampaignModel,
    /// See [`SaveData::rebel_faction`].
    pub rebel_faction: Option<FactionId>,
    /// See [`SaveData::script_values`].
    pub script_values: Vec<ScriptSaveValue>,
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
    /// The save is from an older format version that needs the campaign's start to convert
    /// ([`read_with_start`]), and the start was not given or could not be used.
    NeedsStart {
        /// The save's format version.
        found: u32,
        /// Why the conversion could not be made.
        reason: String,
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
            Self::NeedsStart { found, reason } => {
                write!(f, "the save has format version {found}, which is converted from its campaign's start: {reason}")
            }
            Self::Decode(e) => write!(f, "the save is damaged: {e}"),
            Self::Encode(e) => write!(f, "cannot encode the save: {e}"),
        }
    }
}

impl std::error::Error for FormatError {}

/// The header of a save of `model` made by `human` now: the loaded campaign's facts (`base`) with
/// the turn, year and date of the model, `season_name` as its season (the calendar season's
/// on-screen name, which the caller looks up: `0x00888320` stores what `0x008ED110` returns, CONFIRMED,
/// UI_FIDELITY.md §5.1), the human marked as the player, the territory
/// pictures of `pictures` rendered for the human's regions (as the original's header shows them,
/// [`crate::header_map`]) and `timestamp` (unix seconds). Portrait and flag are kept from `base`, as
/// the ESF writer keeps them. Every theatre of `base` is kept (the campaign's theatres,
/// [`CampaignInfo::theatres`]); one whose pictures did not load (logged where they are loaded) is
/// saved without a picture (0 x 0).
pub fn save_header(base: &CampaignInfo, model: &CampaignModel, human: &str, season_name: &str, timestamp: u32, pictures: &[crate::header_map::TheatrePictures]) -> CampaignInfo {
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
    info.header.season_name = season_name.to_owned();
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
pub fn write<D: Serialize>(info: &CampaignInfo, data: &D) -> Result<Vec<u8>, FormatError> {
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

/// The header and body sections of a save and its format version, after the version check.
fn sections(bytes: &[u8]) -> Result<(u32, &[u8], &[u8]), FormatError> {
    let rest = bytes.strip_prefix(MAGIC.as_slice()).ok_or(FormatError::Truncated)?;
    let word = |b: &[u8]| -> Option<u32> { Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?)) };
    let version = word(rest).ok_or(FormatError::Truncated)?;
    if version > FORMAT_VERSION {
        return Err(FormatError::Newer { found: version });
    }
    let len = word(&rest[4..]).ok_or(FormatError::Truncated)? as usize;
    let rest = &rest[8..];
    if rest.len() < len {
        return Err(FormatError::Truncated);
    }
    let (header, body) = rest.split_at(len);
    Ok((version, header, body))
}

/// Reads only the header (for the Load Game page). The header has not changed between versions.
pub fn read_info(bytes: &[u8]) -> Result<CampaignInfo, FormatError> {
    decode(sections(bytes)?.1)
}

/// The header and the body exactly as stored (the model without rules), with the save's format
/// version. A body older than [`FORMAT_VERSION`] is not converted: it lacks what [`read_with_start`]
/// adds (a version-1 save's regions have no unit resources here).
pub fn read_stored(bytes: &[u8]) -> Result<(u32, CampaignInfo, SaveData), FormatError> {
    let (version, header, body) = sections(bytes)?;
    Ok((version, decode(header)?, decode(body)?))
}

/// The header and the body of a save of the current [`FORMAT_VERSION`], exactly as stored (the
/// model without rules). An older save is [`FormatError::NeedsStart`]: read it with
/// [`read_with_start`].
pub fn read_parts(bytes: &[u8]) -> Result<(CampaignInfo, SaveData), FormatError> {
    match read_stored(bytes)? {
        (FORMAT_VERSION, info, data) => Ok((info, data)),
        (found, ..) => Err(FormatError::NeedsStart { found, reason: "convert it with read_with_start".into() }),
    }
}

/// Reads a whole save of the current [`FORMAT_VERSION`]. The rules are rebuilt from `db` (they are
/// game data, not saved); the map's movement grid and region links come from the campaign's map as
/// for any load. An older save that needs its campaign's start to convert is
/// [`FormatError::NeedsStart`]; [`crate::source::open`] reads it with [`read_with_start`].
pub fn read(bytes: &[u8], db: &GameDatabase) -> Result<LoadedCampaign, FormatError> {
    read_with_start(bytes, db, || Err("no start given (open the save through source::open)".into()))
}

/// [`read`], converting a save older than [`FORMAT_VERSION`] from `start`, the campaign at its start
/// (its source's [`new_campaign`](crate::source::CampaignSource::new_campaign) model), called only
/// for a save older than version 11:
/// * version 1 → 2: every region takes the unit resources (`RESOURCES_ARRAY`) of the start's region
///   of the same key. Nothing in the campaign changes a region's resources, so the start's are the
///   ones the save would hold. A region the start does not have fails the load: never a region left
///   without its resources (which would drop every unit needing one from its recruitable list and
///   refund its queued ones at the turn start).
/// * version 2 → 3: every region takes the road and walls holders of the start's region of the same
///   key, kept only while that faction still holds one of the save region's slots. A holder changes
///   only when its faction loses the region (`CampaignModel::change_region_owner` clears it with every
///   slot that faction holds), so this is exact whenever the start's holder also held a slot there
///   (or was the start's owner, kept while it still owns the region); any other holder is kept and logged.
/// * version 3 → 4, with no start: a repair stored as a build of the standing building's level becomes a
///   repair item (`CampaignModel::convert_legacy_repairs`, once the rules are attached). A version 1 or 2
///   save also predates construction at the round end: its factions stepped construction at their own turn
///   start, so those up to the one whose turn it is (the save is made in it) have run this round's step
///   already, and the round end would run it again. Each item such a faction's turn start stepped (a build
///   in a slot its region's owner holds, or a repair; never the rebels', which no longer step) gets that
///   turn back, once. A version 3 save written by a build of the hour before the round-end change has the
///   same extra step but cannot be told from a later one, and is read as it is.
/// * version 4 → 5, with no start: each trade route whose first hop is a sea hop from its exporter's capital
///   is a capital port route (`TradePath::capital_port`), as every route of the original's saves shows
///   (INFERRED for a capital that moved after the route was made).
/// * version 5 → 6, with no start: every stored treaty-break count (diplomacy manager +0x1C) gets 1 added: older
///   builds counted it from 0 where the exe presets 1 (`0x00AE8DD0` / `0x00AE8E10`), and a faction with no
///   entry already reads the preset. The access-abuse counts (+0x18) were not kept (`World::access_abuses`
///   reads 0, the exe's preset, as older builds charged every abuse at count 0); the other two counters
///   already started at the exe's 0.
/// * version 6 → 7, with no start: an older save kept one accumulated trade value per (exporter, importer)
///   pair (`World::trade_accumulated`), the sum of its routes' (+0x3C, `0x00B05CC0`). A pair with stored routes
///   gets it on its first route (`TradePath::accumulated`): the pair's total is kept, its split over the routes
///   was not; a pair without one keeps it as its built route's value.
/// * version 7 → 8: an older save holds no commerce raids (none could be made) and lacks the static course of
///   the trade network: each leg takes the spline ids of the start's leg between the same nodes
///   (`TradeLeg::splines`) and each trade segment the curves of the start's segment of the same index
///   (`TradeSegmentSight::curves`). Without a usable start the load goes on with a warning and no segment can
///   be raided.
/// * version 8 → 9, with no start: an older save has no expense trackers; it paid the upkeep it wrote into the
///   running record's categories 19 / 20 at each round end, which become the faction's upkeep trackers, and each
///   region's policing tracker starts at 0, so the running record is rebuilt as a load leaves it
///   (`ntw_sim::campaign::economy::rebuild_running_record`: category 21 the regions' sum, 0; the categories no
///   tracker keeps, 0). The next round end's first pass books each region's policing cost.
/// * version 9 → 10: an older save lacks the order of the diplomacy records (`World::diplomacy_record_order`); it
///   takes the start's (a record made since sits after them, as the exe pushes one at the end, `0x00B5F330`). Without a
///   usable start the load goes on with a warning and the records are walked by faction id.
/// * version 10 → 11: an older save lacks the region manager's order (`World::region_order`); it takes the
///   start's, which is exact: nothing in play changes the manager's order and the exe's writer saves it as it is
///   (`0x00A52FE0`). Without a usable start the load goes on with a warning and the regions are walked by id.
pub fn read_with_start(bytes: &[u8], db: &GameDatabase, start: impl FnOnce() -> Result<CampaignModel, String>) -> Result<LoadedCampaign, FormatError> {
    let (version, info, data) = read_stored(bytes)?;
    let mut model = data.model;
    // Only the steps before version 11 need the start (built here, once, before the steps).
    let start = (version < 11).then(start);
    if version < 3 {
        let fail = |reason: String| FormatError::NeedsStart { found: version, reason };
        let start = match &start {
            Some(Ok(s)) => s,
            Some(Err(e)) => return Err(fail(e.clone())),
            None => return Err(fail("no start".into())),
        };
        convert_from_start(version, &mut model, start).map_err(fail)?;
        give_back_turn_start_construction(&mut model);
        copy_trade_course(&mut model, start);
    }
    crate::attach_rules(&mut model, db, &info.campaign_key);
    // Held trait levels: kept from the save, set from the points for a save from before it kept them.
    ntw_sim::campaign::characters::load_trait_levels(&mut model);
    // The legacy list was merged into a set before the model kept the exe's list, so a unit it names is
    // added once and only when not listed already.
    for key in data.restricted_units {
        if !model.world.restricted_units.contains(&key) {
            model.world.restricted_units.push(key);
        }
    }
    let units = std::mem::take(&mut model.world.restricted_units);
    model.world.restricted_units = crate::known_restricted_units(&model, units, "the own save's restricted units");
    if version < 4 {
        let n = model.convert_legacy_repairs();
        if n > 0 {
            log::info!("Own save format version {version}: {n} repairs stored as builds of the standing level converted");
        }
    }
    if version < 5 {
        let n = derive_capital_port_routes(&mut model);
        log::info!("Own save format version {version}: {n} trade routes leaving by sea from their exporter's capital marked as capital port routes");
    }
    if version < 6 {
        let n = migrate_treaty_breaks(&mut model);
        log::info!("Own save format version {version}: {n} treaty-break counts counted from 0 moved to the preset start of 1");
    }
    if version < 7 {
        let n = migrate_trade_accumulated(&mut model);
        log::info!("Own save format version {version}: {n} per-pair accumulated trade values moved to the pair's first route");
    }
    // (A version 1 or 2 save took the course with its conversion above.)
    if (3..8).contains(&version) {
        match &start {
            Some(Ok(start)) => copy_trade_course(&mut model, start),
            Some(Err(e)) => log::warn!("Own save format version {version}: the trade network's course was not restored (no campaign start: {e}); no trade segment can be raided"),
            None => {}
        }
    }
    if version < 9 {
        let n = migrate_expense_trackers(&mut model);
        log::info!("Own save format version {version}: the upkeep trackers of {n} factions set from their running records");
    }
    if version < 10 {
        match &start {
            Some(Ok(start)) => {
                model.world.diplomacy_record_order.clone_from(&start.world.diplomacy_record_order);
                log::info!("Own save format version {version}: the order of {} factions' diplomacy records taken from the campaign's start", start.world.diplomacy_record_order.len());
            }
            Some(Err(e)) => log::warn!("Own save format version {version}: the diplomacy records' order was not restored (no campaign start: {e}); they are walked by faction id"),
            None => {}
        }
    }
    if version < 11 {
        match &start {
            Some(Ok(start)) => {
                model.world.region_order.clone_from(&start.world.region_order);
                log::info!("Own save format version {version}: the order of {} regions taken from the campaign's start", start.world.region_order.len());
            }
            Some(Err(e)) => log::warn!("Own save format version {version}: the regions' order was not restored (no campaign start: {e}); they are walked by id"),
            None => {}
        }
    }
    // Each running record as the exe's load leaves one (`0x00B95B90`, as for the original's save in
    // `crate::read_esf`): the tracked categories booked again by their trackers, the script's
    // `other_income_mod` entries dropped (the scripts add them again).
    for f in model.world.factions.keys().copied().collect::<Vec<_>>() {
        ntw_sim::campaign::economy::rebuild_running_record(&mut model, f);
    }
    Ok(LoadedCampaign {
        model,
        info,
        rebel_faction: data.rebel_faction,
        warnings: Vec::new(),
        script_values: data.script_values,
    })
}

/// The version 7 → 8 step of [`read_with_start`]: copies the trade network's static course from the campaign's
/// start (each leg's spline ids by its node pair, each segment's curves by its index); logs how many.
fn copy_trade_course(model: &mut CampaignModel, start: &CampaignModel) {
    let key = |a: u32, b: u32| (a.min(b), a.max(b));
    let splines: std::collections::BTreeMap<(u32, u32), &Vec<u32>> = start.world.trade_network.iter().map(|l| (key(l.from, l.to), &l.splines)).collect();
    let w = &mut model.world;
    let mut legs = 0;
    for l in &mut w.trade_network {
        if let Some(s) = splines.get(&key(l.from, l.to)) {
            l.splines.clone_from(s);
            legs += 1;
        }
    }
    let mut segments = 0;
    for (s, from) in w.trade_sight.iter_mut().zip(&start.world.trade_sight) {
        s.curves.clone_from(&from.curves);
        segments += 1;
    }
    log::info!("Own save format version < 8: the course of {legs} of {} trade legs and {segments} of {} trade segments taken from the campaign's start", w.trade_network.len(), w.trade_sight.len());
}

/// The version 8 → 9 step of [`read_with_start`]: each faction's upkeep trackers take its running record's
/// categories 19 / 20 (the load then rebuilds the record from the trackers); returns how many factions have a
/// record.
fn migrate_expense_trackers(model: &mut CampaignModel) -> usize {
    let records: Vec<(ntw_sim::campaign::FactionId, [i32; 2])> =
        model.world.economy_history.iter().filter_map(|(&f, h)| h.last().map(|r| (f, [r[19], r[20]]))).collect();
    for &(f, booked) in &records {
        model.world.upkeep_booked.insert(f, booked);
    }
    records.len()
}

/// The version 6 → 7 step of [`read_with_start`]: moves the per-pair accumulated trade value of each pair
/// with stored routes onto its first route; returns how many.
fn migrate_trade_accumulated(model: &mut CampaignModel) -> usize {
    let w = &mut model.world;
    let mut n = 0;
    w.trade_accumulated.retain(|pair, value| {
        let Some(first) = w.trade_paths.get_mut(pair).and_then(|ps| ps.first_mut()) else { return true };
        first.accumulated = first.accumulated.saturating_add(*value);
        n += 1;
        false
    });
    n
}

/// The version 5 → 6 step of [`read_with_start`]: adds the preset start [`TREATY_BREAKS_START`](ntw_sim::campaign::TREATY_BREAKS_START)
/// to every stored treaty-break count, which older builds counted from 0 (wrapping, as the exe's u32 does); returns how many.
fn migrate_treaty_breaks(model: &mut CampaignModel) -> usize {
    for n in model.world.treaty_breaks.values_mut() {
        *n = n.wrapping_add(ntw_sim::campaign::TREATY_BREAKS_START);
    }
    model.world.treaty_breaks.len()
}

/// The version 4 → 5 step of [`read_with_start`]: marks the capital port routes; returns how many.
fn derive_capital_port_routes(model: &mut CampaignModel) -> usize {
    let capitals: std::collections::BTreeMap<FactionId, ntw_sim::campaign::RegionId> =
        model.world.faction_details.iter().filter_map(|(f, d)| d.capital.map(|c| (*f, c))).collect();
    let mut n = 0;
    for ((exporter, _), paths) in model.world.trade_paths.iter_mut() {
        for p in paths {
            p.capital_port = p.waypoints.first().is_some_and(|w| w.sea && capitals.get(exporter) == Some(&w.region));
            n += usize::from(p.capital_port);
        }
    }
    n
}

/// The [`read_with_start`] steps from `version` to 3, from `start` (the campaign at
/// its start); an error names what the start lacks.
fn convert_from_start(version: u32, model: &mut CampaignModel, start: &CampaignModel) -> Result<(), String> {
    let by_key: std::collections::HashMap<&str, &ntw_sim::campaign::Region> = start.world.regions.values().map(|r| (r.key.as_str(), r)).collect();
    let mut unheld = 0;
    for r in model.world.regions.values_mut() {
        let s = by_key.get(r.key.as_str()).ok_or_else(|| format!("the campaign's start has no region {}", r.key))?;
        if version < 2 {
            r.unit_resources.clone_from(&s.unit_resources);
        }
        // version < 3 here (the caller converts only older saves).
        let holds_slot = |slots: &[ntw_sim::campaign::RegionSlot], h: FactionId| slots.iter().any(|slot| slot.holder == Some(h));
        for (start_holder, holder) in [(s.road_holder, &mut r.road_holder), (s.fortification_holder, &mut r.fortification_holder)] {
            *holder = start_holder.filter(|&h| {
                if holds_slot(&s.slots, h) {
                    // Cleared together with that faction's slots.
                    holds_slot(&r.slots, h)
                } else if h == s.owner {
                    // Cleared when the start's owner loses the region; while it owns it, kept or not
                    // reads the same (held by the owner).
                    r.owner == h
                } else {
                    // Cleared only if that faction took the region and lost it since: unknown here.
                    unheld += 1;
                    true
                }
            });
        }
    }
    let regions = model.world.regions.len();
    if version < 2 {
        log::info!("Own save format version {version}: unit resources of {regions} regions taken from the campaign's start");
    }
    log::info!("Own save format version {version}: road and walls holders of {regions} regions taken from the campaign's start");
    if unheld > 0 {
        log::warn!("Own save format version {version}: {unheld} road or walls holders of the campaign's start hold no other slot of their region and do not own it; kept as at the start");
    }
    Ok(())
}

/// The version 1 / 2 → 3 construction step (see [`read_with_start`]): every item the turn starts already run
/// this round stepped gets one turn back. Runs before the legacy repairs are converted, so a repair is
/// still a build of the standing level here.
fn give_back_turn_start_construction(model: &mut CampaignModel) {
    let Some(current) = model.turn.current else { return };
    let order = model.world.factions_in_turn_order();
    let Some(at) = order.iter().position(|&f| f == current) else { return };
    let stepped: Vec<FactionId> = order[..=at].iter().copied().filter(|&f| !model.is_rebel_faction(f)).collect();
    let mut n = 0;
    for reg in model.world.regions.values_mut().filter(|r| stepped.contains(&r.owner)) {
        let steps: Vec<bool> = reg
            .construction
            .iter()
            .map(|i| reg.slot_held(i.slot) || reg.building_at(i.slot).is_some_and(|b| b.level_key == i.level_key))
            .collect();
        for (item, step) in reg.construction.iter_mut().zip(steps) {
            if step {
                item.turns_remaining += 1;
                n += 1;
            }
        }
    }
    if n > 0 {
        log::info!("Own save format version below 3: {n} construction items stepped at their faction's turn start this round get that turn back");
    }
}

impl From<FormatError> for LoadError {
    fn from(e: FormatError) -> Self {
        LoadError::OwnSave(e)
    }
}

#[cfg(test)]
pub(crate) mod tests;
