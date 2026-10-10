//! # ntw_campaign: start positions and saves → campaign model
//!
//! Napoleon keeps a whole campaign in one ESF file (Creative Assembly's binary tree format,
//! read by [`ntw_formats::esf`]):
//! * a **start position** `data\campaigns\<campaign>\startpos.esf`, root record
//!   `CAMPAIGN_STARTPOS`. This is the state on turn 1.
//! * a **save game** `%APPDATA%\The Creative Assembly\Napoleon\save_games\*.save`, root record
//!   `CAMPAIGN_SAVE_GAME`. Same tree, but without the front-end block
//!   `CAMPAIGN_PREOPEN_MAP_INFO`, and with newer record versions (W3 §4).
//!
//! This crate reads either file and builds the simulation's
//! [`CampaignModel`](ntw_sim::campaign::CampaignModel): factions, diplomacy, treasury, regions,
//! settlements, buildings, characters, armies, navies, units, the calendar and the RNG state.
//!
//! ```no_run
//! use ntw_data::GameDatabase;
//! let data = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
//! let db = GameDatabase::from_install(data).unwrap();
//! let loaded = ntw_campaign::read_file(
//!     format!(r"{data}\campaigns\eur_napoleon\startpos.esf"),
//!     &db,
//! ).unwrap();
//! let world = &loaded.model.world;
//! println!("{} factions, {} regions", world.factions.len(), world.regions.len());
//! for w in &loaded.warnings { println!("warning: {w}"); }
//! ```
//!
//! ## What is read (see `analysis/worker3/STARTPOS_LAYOUT.md` for every field position)
//! | ESF | model | evidence |
//! |---|---|---|
//! | `CAMPAIGN_MODEL/RandSeed` u32 | `rng.state` | CONFIRMED location; INFERRED that it is the live LCG state |
//! | `CAMPAIGN_MODEL/CAMPAIGN_CALENDAR` | `calendar` | CONFIRMED (W3 §3.1) |
//! | `WORLD/FACTION_ARRAY` + `REBEL_FACTION` | `world.factions` | CONFIRMED |
//! | `FACTION_ECONOMICS` #1 | `Faction::treasury` | CONFIRMED values, INFERRED meaning |
//! | `DIPLOMACY_RELATIONSHIP` #0, #4 | `Faction::diplomacy` | CONFIRMED |
//! | `CHARACTER_ARRAY` | `world.characters` | CONFIRMED structure |
//! | `ARMY_ARRAY` (`ARMY` / `NAVY`) | `world.forces` | CONFIRMED structure |
//! | `REGION_MANAGER/REGIONS_ARRAY` | `world.regions` | CONFIRMED structure, owner cross-checked |
//!
//! Everything else in the file (traits, ancillaries, technologies, population classes, AI state,
//! trade, pathfinding, scripting state, ...) is not representable in the model yet and is
//! skipped without error. The layout document lists those blocks as UNKNOWN/TODO.
//!
//! ## Errors
//! Bad input never panics. A file that cannot become a campaign gives a [`LoadError`]; a small
//! oddity (e.g. a unit key the database does not know, which mods can cause) gives a
//! [`LoadWarning`] and loading continues.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ai_keys;
pub mod cai;
pub mod cai_world;
pub mod trade;
pub mod victory;
pub mod regiments;
pub mod charnames;
pub mod header_map;
pub mod names;
pub mod own_save;
mod details;
mod error;
mod fields;
mod obstacles;
mod rules;
pub mod map_display;
pub mod source;
pub mod shroud;
pub mod grid_load_check;
pub mod grid_obstacle;
pub mod pathing;
pub mod save;
pub mod missions;
pub mod save_check;
pub mod script_values;
mod world;

use std::path::Path;

use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::calendar::{Calendar, Date};
use ntw_sim::campaign::{CampaignModel, FactionId, TurnState};
use ntw_sim::rng::CaRng;

pub use error::{LoadError, LoadWarning};
pub use rules::rules_from_db;


use fields::{array, child, date, rec_at, str_at, u32_at};

/// Root record name of a start position (CONFIRMED, all 8 shipped campaigns).
pub const STARTPOS_ROOT: &str = "CAMPAIGN_STARTPOS";
/// Root record name of a save game (CONFIRMED, W3 §4).
pub const SAVE_ROOT: &str = "CAMPAIGN_SAVE_GAME";

/// Which kind of file was loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileKind {
    /// `startpos.esf` (root `CAMPAIGN_STARTPOS`).
    Startpos,
    /// A `.save` file (root `CAMPAIGN_SAVE_GAME`).
    Save,
}

/// `SAVE_GAME_HEADER` (v1/v2): what the load-game screen shows. Present in both file kinds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SaveHeader {
    /// #0 the player's faction key (the default faction in a startpos).
    pub faction_key: String,
    /// #1 portrait image path.
    pub portrait: String,
    /// #2 1-based turn number (= `turns_elapsed + 1`, CONFIRMED W3 §3.1).
    pub turn_number: u32,
    /// #3 year.
    pub year: u32,
    /// #4 season name, e.g. "Winter".
    pub season_name: String,
    /// #5 flag image path.
    pub flag_path: String,
    /// #6 `DATE`. Missing in header version 1 (ita / mp_ita startpos, CONFIRMED W3 §2.2).
    pub date: Option<Date>,
    /// #7 `MAPS[]`: the region-ownership pictures the load-game page shows, one per theatre.
    pub maps: Vec<HeaderMap>,
}

/// One `SAVE_GAME_HEADER/MAPS` item (v0): {utf16 theatre key, u32 width, u32 height, i32 row pitch
/// in bytes, u32[] pixels}. CONFIRMED in the original's saves: pitch = width x 4 and width x
/// height pixels (`europe_main` 605 x 300, `spain_main` 337 x 300); pixels 0xAARRGGBB, rows
/// top-down (INFERRED from the colours). The front end's `GetExtendedSaveGameInfo` hands them to
/// the load-game page as `Maps[theatre]` (`0x008982C0`, CONFIRMED).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct HeaderMap {
    /// Theatre key (`campaign_map_playable_areas` area), e.g. `europe_main`.
    pub theatre: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row pitch in bytes (width x 4 in every save).
    pub pitch: i32,
    /// Pixels, 0xAARRGGBB.
    pub pixels: Vec<u32>,
}

impl HeaderMap {
    /// The pixels as RGBA8, rows as stored (any pitch padding skipped).
    pub fn rgba(&self) -> Vec<u8> {
        let stride = (self.pitch.max(0) as usize / 4).max(self.width as usize);
        let mut out = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let p = self.pixels.get(y * stride + x).copied().unwrap_or(0);
                out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
            }
        }
        out
    }
}

/// One `CAMPAIGN_SETUP/CAMPAIGN_PLAYERS_SETUP/PLAYERS_ARRAY` entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlayerSetup {
    /// #2 faction key.
    pub faction_key: String,
    /// #3 bool. INFERRED: true for the human player's faction (W3 §3.2).
    pub is_human: bool,
    /// #4 bool. INFERRED: true for factions the player may choose (W3 §3.2).
    pub is_playable: bool,
}

/// Facts about the file that are not part of the simulation state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CampaignInfo {
    /// Start position or save.
    pub kind: FileKind,
    /// ESF header timestamp (unix seconds; time of saving for saves).
    pub timestamp: u32,
    /// `BUILD` #0 (build id string).
    pub build_id: String,
    /// `BUILD` #1 (version string).
    pub build_version: String,
    /// `SAVE_GAME_HEADER`.
    pub header: SaveHeader,
    /// `CAMPAIGN_SETUP` #0, e.g. `eur_napoleon`.
    pub campaign_key: String,
    /// `CAMPAIGN_MAP_DATA` #2, e.g. `nap_europe`.
    pub map_key: String,
    /// `CAMPAIGN_PLAYERS_SETUP` entries, in file order.
    pub players: Vec<PlayerSetup>,
}

/// Everything a load produces.
#[derive(Debug, Clone)]
pub struct LoadedCampaign {
    /// The simulation state.
    pub model: CampaignModel,
    /// Header and setup facts.
    pub info: CampaignInfo,
    /// The id of `WORLD/REBEL_FACTION/FACTION`. The rebel faction is included in
    /// `model.world.factions` (with its stored key, which is empty) so that rebel armies in saves
    /// have an owner.
    pub rebel_faction: Option<FactionId>,
    /// Non-fatal oddities, in the order they were found.
    pub warnings: Vec<LoadWarning>,
    /// The scripts' `save_value` slots (`EPISODIC_RESTRICTIONS/LUA[]`), in order; empty for a
    /// start position. Give them to the script host before `LoadingGame` fires.
    pub script_values: Vec<script_values::ScriptSaveValue>,
    /// The scripts' restricted units (`EPISODIC_RESTRICTIONS/UNIT_RESTRICTIONS`); empty for a
    /// start position. Give them to the script state before the campaign UI is created. The
    /// restricted building levels of the same record are already in
    /// `model.world.restricted_buildings`.
    pub restricted_units: Vec<String>,
}

/// Loads a start position from bytes (root must be `CAMPAIGN_STARTPOS`).
pub fn load_startpos(bytes: &[u8], db: &GameDatabase) -> Result<CampaignModel, LoadError> {
    Ok(read_expecting(bytes, db, Some(FileKind::Startpos))?.model)
}

/// Loads a start position from a file on disk (read-only).
pub fn load_startpos_file(
    path: impl AsRef<Path>,
    db: &GameDatabase,
) -> Result<CampaignModel, LoadError> {
    load_startpos(&read_bytes(path.as_ref())?, db)
}

/// Loads a save game from bytes (root must be `CAMPAIGN_SAVE_GAME`).
pub fn load_save(bytes: &[u8], db: &GameDatabase) -> Result<CampaignModel, LoadError> {
    Ok(read_expecting(bytes, db, Some(FileKind::Save))?.model)
}

/// Loads a save game from a file on disk (read-only).
pub fn load_save_file(
    path: impl AsRef<Path>,
    db: &GameDatabase,
) -> Result<CampaignModel, LoadError> {
    load_save(&read_bytes(path.as_ref())?, db)
}

/// Loads a start position, an original save or one of our own saves ([`own_save`]) from bytes and
/// returns the model together with header facts and warnings.
pub fn read(bytes: &[u8], db: &GameDatabase) -> Result<LoadedCampaign, LoadError> {
    if own_save::is_own_save(bytes) {
        return Ok(own_save::read(bytes, db)?);
    }
    read_expecting(bytes, db, None)
}

/// Like [`read`], from a file on disk (read-only).
pub fn read_file(path: impl AsRef<Path>, db: &GameDatabase) -> Result<LoadedCampaign, LoadError> {
    read(&read_bytes(path.as_ref())?, db)
}

/// Loads either kind from an already-parsed ESF file.
pub fn read_esf(esf: &EsfFile, db: &GameDatabase) -> Result<LoadedCampaign, LoadError> {
    let kind = match esf.root.name.as_str() {
        STARTPOS_ROOT => FileKind::Startpos,
        SAVE_ROOT => FileKind::Save,
        other => {
            return Err(LoadError::WrongRoot {
                expected: "CAMPAIGN_STARTPOS or CAMPAIGN_SAVE_GAME",
                found: other.to_string(),
            });
        }
    };
    build(esf, kind, db)
}

/// Reads only the facts the load-game screen shows (header, campaign, map, players) from a
/// start position, an original save or one of our own saves, without building the campaign model
/// (no database needed).
pub fn read_info(bytes: &[u8]) -> Result<CampaignInfo, LoadError> {
    if own_save::is_own_save(bytes) {
        return Ok(own_save::read_info(bytes)?);
    }
    let esf = EsfFile::from_bytes(bytes)?;
    let root = &esf.root;
    let kind = match root.name.as_str() {
        STARTPOS_ROOT => FileKind::Startpos,
        SAVE_ROOT => FileKind::Save,
        other => {
            return Err(LoadError::WrongRoot { expected: "CAMPAIGN_STARTPOS or CAMPAIGN_SAVE_GAME", found: other.to_string() });
        }
    };
    let rp = root.name.as_str();
    let bpath = format!("{rp}/BUILD");
    let build_rec = child(root, "BUILD", rp)?;
    let header = read_header(child(root, "SAVE_GAME_HEADER", rp)?, &format!("{rp}/SAVE_GAME_HEADER"))?;
    let env_path = format!("{rp}/CAMPAIGN_ENV");
    let env = child(root, "CAMPAIGN_ENV", rp)?;
    let setup_path = format!("{env_path}/CAMPAIGN_SETUP");
    let setup = child(env, "CAMPAIGN_SETUP", &env_path)?;
    let mpath = format!("{env_path}/CAMPAIGN_MODEL");
    let model_rec = child(env, "CAMPAIGN_MODEL", &env_path)?;
    let map_data = child(model_rec, "CAMPAIGN_MAP_DATA", &mpath)?;
    Ok(CampaignInfo {
        kind,
        timestamp: esf.header.timestamp,
        build_id: str_at(build_rec, 0, &bpath)?.to_string(),
        build_version: str_at(build_rec, 1, &bpath)?.to_string(),
        header,
        campaign_key: str_at(setup, 0, &setup_path)?.to_string(),
        map_key: str_at(map_data, 2, &format!("{mpath}/CAMPAIGN_MAP_DATA"))?.to_string(),
        players: read_players(setup, &setup_path)?,
    })
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, LoadError> {
    std::fs::read(path).map_err(|error| LoadError::Io {
        path: path.to_path_buf(),
        error,
    })
}

fn read_expecting(
    bytes: &[u8],
    db: &GameDatabase,
    want: Option<FileKind>,
) -> Result<LoadedCampaign, LoadError> {
    let esf = EsfFile::from_bytes(bytes)?;
    let expected = match want {
        Some(FileKind::Startpos) => Some(STARTPOS_ROOT),
        Some(FileKind::Save) => Some(SAVE_ROOT),
        None => None,
    };
    if let Some(expected) = expected
        && esf.root.name != expected
    {
        return Err(LoadError::WrongRoot {
            expected,
            found: esf.root.name.clone(),
        });
    }
    read_esf(&esf, db)
}

fn build(esf: &EsfFile, kind: FileKind, db: &GameDatabase) -> Result<LoadedCampaign, LoadError> {
    let root = &esf.root;
    let rp = root.name.as_str();

    let bpath = format!("{rp}/BUILD");
    let build_rec = child(root, "BUILD", rp)?;
    let header = read_header(
        child(root, "SAVE_GAME_HEADER", rp)?,
        &format!("{rp}/SAVE_GAME_HEADER"),
    )?;

    let env_path = format!("{rp}/CAMPAIGN_ENV");
    let env = child(root, "CAMPAIGN_ENV", rp)?;
    let setup_path = format!("{env_path}/CAMPAIGN_SETUP");
    let setup = child(env, "CAMPAIGN_SETUP", &env_path)?;
    let players = read_players(setup, &setup_path)?;

    let mpath = format!("{env_path}/CAMPAIGN_MODEL");
    let model_rec = child(env, "CAMPAIGN_MODEL", &env_path)?;
    let map_data = child(model_rec, "CAMPAIGN_MAP_DATA", &mpath)?;
    let map_key = str_at(map_data, 2, &format!("{mpath}/CAMPAIGN_MAP_DATA"))?;
    let seed = u32_at(
        child(model_rec, "RandSeed", &mpath)?,
        0,
        &format!("{mpath}/RandSeed"),
    )?;
    let calendar = read_calendar(
        child(model_rec, "CAMPAIGN_CALENDAR", &mpath)?,
        &format!("{mpath}/CAMPAIGN_CALENDAR"),
    )?;

    let mut check = world::Checker {
        db,
        warnings: Vec::new(),
    };
    let wpath = format!("{mpath}/WORLD");
    let mut loaded = world::load_world(child(model_rec, "WORLD", &mpath)?, &wpath, &mut check)?;
    // New objects get ids above every integer of the file (one global id map in the original:
    // SAVE_COMPAT.md §3).
    loaded.world.next_id = first_free_id(root);
    world::assign_missing_recruitment_ids(&mut loaded.world);
    world::link_recruitment_sources(&mut loaded.world, loaded.recruitment_sources);

    let campaign_key = str_at(setup, 0, &setup_path)?.to_string();
    let mut model = CampaignModel::new(calendar, CaRng::new(seed), loaded.world);
    attach_rules(&mut model, db, &campaign_key);
    // Movement maximums include the commanders' force factor (CAMPAIGN_FIDELITY.md §Action points).
    model.refresh_movement_maximums();
    // The routes: accumulated values (SAVE_COMPAT.md §12), paths, commodity volumes and prices
    // (CAMPAIGN_FIDELITY.md §Trade).
    model.world.trade_accumulated = trade::read_accumulated(esf, &model);
    (model.world.trade_paths, model.world.commodity_prices) = trade::read_paths(esf, &model);
    model.world.domestic_trade = trade::read_domestic(esf, &model);
    trade::read_network(esf, &mut model);
    // Sight: the grid, the shrouds and the regions' sight shapes (CHARACTERS_FIDELITY.md §10).
    shroud::read(esf, &mut model);
    // The campaign AI's manager / personality keys and region base values (AI_RESEARCH.md §2.3, §4).
    ai_keys::fill(root, &mut model);
    names::fill_allocators(root, &mut model);
    // The historical characters already made and the two episodic force-success switches
    // (CHARACTERS_FIDELITY.md §7, §8).
    (model.world.historical_created, model.world.force_success_for_human) = details::model_extras(model_rec);
    // The scripts' restricted building levels (EPISODIC_RESTRICTIONS): the scripts set them only
    // on a new game (InitialiseCampaign at UICreated), so a save carries them.
    let restrictions = script_values::read_restrictions(esf);
    model.world.restricted_buildings = restrictions.buildings.into_iter().collect();
    if kind == FileKind::Save {
        // A save is made inside the human player's turn (INFERRED); the header names that faction.
        if let Some(f) = model.faction_by_key(&header.faction_key).map(|f| f.id) {
            model.turn = TurnState::in_turn_of(f, vec![f]);
        }
    }
    Ok(LoadedCampaign {
        model,
        info: CampaignInfo {
            kind,
            timestamp: esf.header.timestamp,
            build_id: str_at(build_rec, 0, &bpath)?.to_string(),
            build_version: str_at(build_rec, 1, &bpath)?.to_string(),
            header,
            campaign_key,
            map_key: map_key.to_string(),
            players,
        },
        rebel_faction: loaded.rebel_faction,
        warnings: check.warnings,
        script_values: script_values::read_script_values(esf),
        restricted_units: restrictions.units,
    })
}

/// Gives the model the campaign's rules from the DB (game data, not state: rebuilt on every load,
/// whatever the model came from).
pub fn attach_rules(model: &mut CampaignModel, db: &GameDatabase, campaign_key: &str) {
    let mut rules = rules_from_db(db, campaign_key);
    rules::fill_ship_guns(&mut rules, db, &model.world);
    model.rules = std::sync::Arc::new(rules);
}

/// The first multiple of 8 above every integer value (and integer array element) of the tree
/// that is below `0x7fff_0000`, so ids made from it collide with no id the file holds (ids live
/// in one global map in the original, SAVE_COMPAT.md §3) and stay positive as i32.
pub fn first_free_id(root: &EsfRecord) -> u32 {
    fn scan(nodes: &[EsfNode], top: &mut u32) {
        fn see(top: &mut u32, v: u64) {
            if v < 0x7fff_0000 {
                *top = (*top).max(v as u32);
            }
        }
        for n in nodes {
            match n {
                EsfNode::Record(r) => scan(&r.children, top),
                EsfNode::RecordArray(a) => a.items.iter().for_each(|i| scan(i, top)),
                EsfNode::U32Array(a) => a.iter().for_each(|&v| see(top, u64::from(v))),
                EsfNode::I32Array(a) => a.iter().filter(|&&v| v > 0).for_each(|&v| see(top, v as u64)),
                other => {
                    if let Some(v) = other.as_int().filter(|&v| v > 0) {
                        see(top, v as u64);
                    }
                }
            }
        }
    }
    let mut top = 0;
    scan(&root.children, &mut top);
    (top / 8 + 1) * 8
}

/// `SAVE_GAME_HEADER` v1/v2 (see [`SaveHeader`]).
fn read_header(r: &EsfRecord, path: &str) -> Result<SaveHeader, LoadError> {
    let date = match r.child("DATE") {
        Some(d) => Some(date(d, &format!("{path}/DATE"))?),
        None => None,
    };
    Ok(SaveHeader {
        faction_key: str_at(r, 0, path)?.to_string(),
        portrait: str_at(r, 1, path)?.to_string(),
        turn_number: u32_at(r, 2, path)?,
        year: u32_at(r, 3, path)?,
        season_name: str_at(r, 4, path)?.to_string(),
        flag_path: str_at(r, 5, path)?.to_string(),
        date,
        maps: r
            .record_array("MAPS")
            .into_iter()
            .flat_map(|a| a.items.iter())
            .map(|it| HeaderMap {
                theatre: it.first().and_then(EsfNode::as_str).unwrap_or_default().to_string(),
                width: it.get(1).and_then(EsfNode::as_u32).unwrap_or(0),
                height: it.get(2).and_then(EsfNode::as_u32).unwrap_or(0),
                pitch: it.get(3).and_then(EsfNode::as_i32).unwrap_or(0),
                pixels: it.get(4).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default(),
            })
            .collect(),
    })
}

/// `CAMPAIGN_CALENDAR` v2: #0 u32 turns per year, #1 u32 turn in year, #2 `DATE`,
/// #3 u32 turns elapsed (CONFIRMED, W3 §3.1). Copied exactly as stored.
fn read_calendar(r: &EsfRecord, path: &str) -> Result<Calendar, LoadError> {
    Ok(Calendar {
        turns_per_year: u32_at(r, 0, path)?,
        turn_in_year: u32_at(r, 1, path)?,
        date: date(rec_at(r, 2, path)?, &format!("{path}/DATE"))?,
        turns_elapsed: u32_at(r, 3, path)?,
    })
}

/// `CAMPAIGN_SETUP/CAMPAIGN_PLAYERS_SETUP/PLAYERS_ARRAY[]/CAMPAIGN_PLAYER_SETUP` v3:
/// #2 utf16 faction key, #3 bool, #4 bool (meanings INFERRED, W3 §3.2).
fn read_players(setup: &EsfRecord, path: &str) -> Result<Vec<PlayerSetup>, LoadError> {
    let ppath = format!("{path}/CAMPAIGN_PLAYERS_SETUP");
    let players = child(setup, "CAMPAIGN_PLAYERS_SETUP", path)?;
    let mut out = Vec::new();
    for (i, p) in array(players, "PLAYERS_ARRAY", &ppath)?
        .records()
        .enumerate()
    {
        let ip = format!("{ppath}/PLAYERS_ARRAY[{i}]/CAMPAIGN_PLAYER_SETUP");
        out.push(PlayerSetup {
            faction_key: str_at(p, 2, &ip)?.to_string(),
            is_human: p.get_bool(3).unwrap_or(false),
            is_playable: p.get_bool(4).unwrap_or(false),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;

impl LoadedCampaign {
    /// Makes `faction_key` the (only) human faction of a start position that has not started yet,
    /// and returns false if no faction has that key. The front end calls this with the faction the
    /// player picked; the `--campaign` harness uses the startpos header's default faction.
    pub fn set_human(&mut self, faction_key: &str) -> bool {
        let Some(id) = self.model.faction_by_key(faction_key).map(|f| f.id) else { return false };
        if !self.model.turn.started {
            self.model.turn.humans = vec![id];
            // Only the human keeps a shroud (INFERRED: the start positions give one to every
            // playable faction, the vanilla saves of a new campaign keep only the human's;
            // CHARACTERS_FIDELITY.md §10).
            self.model.world.shrouds.retain(|f, _| *f == id);
        }
        true
    }
}
