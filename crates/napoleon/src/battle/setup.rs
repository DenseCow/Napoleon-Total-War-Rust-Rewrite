//! Builds the starting battle: two armies of real unit types, read from the game database.
//!
//! Only unit *keys* (names) are written here. All their stats (men, melee attack, charge,
//! accuracy, reload skill, ammunition, the musket projectile,
//! defence, armour, morale, formation spacing) are read from the player's own install at run
//! time. If a key is missing (e.g. when running on the test fixture), the first unit of the
//! same category is used instead.
//!
//! - Deployment: each army stands in its deployment area of the battle map
//!   (`deployment_areas.xml`; the first setup = 1v1; alliance 0 = the player, alliance 1 = the
//!   enemy, PROVISIONAL), facing the way the area says.
//!   The units are laid out as the original's default deployment does (`groupformations.bin`
//!   templates, [`deploy_with_templates`]); if no template takes them, side by side [`UNIT_GAP_M`]
//!   apart in slot order, wrapping into further lines behind if the area is too narrow
//!   (PLACEHOLDER fallback).
//!   Without a map the armies use the fixed slice positions.
//! - Speeds: `battle_entities` walk/run (the man's entity, or the mount's when mounted).
//! - Formation: depth = `unit_stats_land` #43 (INFERRED: default ranks), spacing = #44/#45
//!   (close order).
//! - The model gets the map's ground (types + level-0 heights) and `unit_movement_modifiers`.

use std::sync::Arc;

use bevy::prelude::*;
use ntw_data::GameDatabase;
use ntw_formats::battle_spec::{BattleSpec, SpecUnit};
use ntw_formats::battle_terrain::{BattleMap, DeploymentArea, DeploymentSetup};
use ntw_formats::group_formation::{self, GroupUnit, PURPOSE_DEPLOYMENT, Role, Template};
use ntw_formats::db::{DbTable, DbValue, Schema};
use ntw_formats::pack::Vfs;
use ntw_formats::unit_model::BattleTables;
use ntw_sim::battle::attributes::shot_type_value;
use ntw_sim::battle::ground::{BattleGround, GridSpec, GroundTypeGrid, HeightGrid, MovementClass, speed_table};
use ntw_sim::battle::model::{Battle, LandUnit, Reinforcement};
use ntw_sim::battle::shooting::MissileWeapon;
use ntw_sim::battle::fatigue::FatigueEffects;


use super::{BattlePhase, BattleSim, UnitInfo};
use ntw_sim::battle::victory::VictoryRules;
use crate::data::GameData;

/// One slot in an army's order of battle.
struct Slot {
    /// Preferred unit key in the `units` table.
    key: &'static str,
    /// Fallback category ("infantry" / "cavalry") if the key is not in the database.
    category: &'static str,
    /// Starting x position in metres when there is no map (y is set per side).
    x: f32,
    /// `Some(star rating)`: this slot is the army general (a battle file `general` element).
    general: Option<i32>,
}

/// French army (side 0, the player). Real unit keys from the `units` table.
const FRANCE: [Slot; 5] = [
    Slot { key: "Inf_Line_French_Fusiliers", category: "infantry", x: -75.0, general: None },
    Slot { key: "Inf_Line_French_18th_Ligne", category: "infantry", x: -10.0, general: None },
    Slot { key: "Inf_Gren_French_Grenadiers", category: "infantry", x: 55.0, general: None },
    Slot { key: "Cav_Heavy_French_Cuirassiers", category: "cavalry", x: 130.0, general: None },
    // The army general, as Austerlitz_Battle.xml defines France's (unit type, `star_rating level`).
    Slot { key: "Gen_Late_Napoleon", category: "cavalry", x: 200.0, general: Some(5) },
];

/// Austrian army (side 1, the AI).
const AUSTRIA: [Slot; 5] = [
    Slot { key: "Inf_Line_Austrian_German_Fusiliers", category: "infantry", x: -75.0, general: None },
    Slot { key: "Inf_Line_Austrian_Hungarian_Fusiliers", category: "infantry", x: -10.0, general: None },
    Slot { key: "Inf_Gren_Austrian_German_Grenadiers", category: "infantry", x: 55.0, general: None },
    Slot { key: "Cav_Light_Austrian_1st_Hussars", category: "cavalry", x: 130.0, general: None },
    // The army general, as Austerlitz_Battle.xml defines Austria's.
    Slot { key: "Gen_Generals_Staff", category: "cavalry", x: 200.0, general: Some(6) },
];

/// Distance of each army's line from the centre of the field when there is no map, in metres.
const START_DISTANCE_M: f32 = 150.0;
/// PLACEHOLDER: gap between neighbouring units (and between lines) in a deployment area.
pub const UNIT_GAP_M: f32 = 10.0;
/// PLACEHOLDER ranks when `unit_stats_land` #43 is missing or 0.
const FALLBACK_RANKS: (u32, u32) = (4, 2);
/// RNG seed of the first battle (any value works; same seed = same battle, every time).
pub const FIRST_SEED: u32 = 1805;

/// What the setup reads from the install besides `GameData`: entity speeds, the map's ground
/// and deployment areas. Built once; R (restart) reuses it.
#[derive(Resource, Default)]
pub struct SetupData {
    /// `battle_personalities` + `battle_entities`.
    entities: Option<BattleTables>,
    /// The ground under the units (flat grassland without a map).
    ground: Arc<BattleGround>,
    /// The map's 1v1 deployment setup, if any.
    deployment: Option<DeploymentSetup>,
    /// The historical battle being fought (`battles` key + its specification), if any.
    pub battle: Option<(String, Arc<BattleSpec>)>,
    /// `groupformations.bin`: the templates of the default deployment (empty if unreadable).
    group_formations: Vec<Template>,
    /// Each side's researched technologies, from [`BattleStart::technologies`].
    pub technologies: Vec<Option<Vec<String>>>,
    /// The map's near buildings with their fire lines (garrisons).
    pub buildings: Vec<ntw_sim::battle::garrison::BattleBuilding>,
    /// The map preset's `max_weather_type_key` (`weather.xml`), if any.
    pub weather: Option<String>,
    /// A custom battle's armies ([`BattleStart::custom`]); empty otherwise.
    pub custom: Vec<ntw_script::ui::CustomArmy>,
}

impl SetupData {
    /// Reads everything from the install, the loaded battle map (`crate::terrain`) and the
    /// chosen battle's specification file.
    pub fn load(vfs: Option<&Vfs>, map: Option<&BattleMap>, start: Option<&BattleStart>) -> Self {
        let entities = vfs.and_then(|v| BattleTables::from_vfs(v).map_err(|e| warn!("battle_entities: {e}")).ok());
        let modifiers = vfs.map(movement_modifiers).unwrap_or_default();
        let ground = Arc::new(map.map_or_else(BattleGround::default, |m| ground_of(m, &modifiers)));
        let deployment = map.and_then(|m| m.one_v_one().cloned());
        let battle = match (vfs, start.and_then(|s| s.spec.as_deref().map(|p| (s, p)))) {
            (Some(vfs), Some((start, path))) => match vfs.read(path).map_err(|e| e.to_string()).and_then(|b| BattleSpec::parse(&b).map_err(|e| e.to_string())) {
                Ok(spec) => {
                    info!("Battle {}: {path}, {} alliances", start.key, spec.alliances.len());
                    Some((start.key.clone(), Arc::new(spec)))
                }
                Err(e) => {
                    warn!("Battle {}: {path} not read ({e}); using the test armies", start.key);
                    None
                }
            },
            _ => None,
        };
        let group_formations = vfs
            .and_then(|v| v.read("groupformations.bin").ok())
            .and_then(|b| group_formation::read_templates(&b).map_err(|e| warn!("{e}")).ok())
            .unwrap_or_default();
        let technologies = start.map(|s| s.technologies.clone()).unwrap_or_default();
        let radius = garrison_radius(entities.as_ref());
        let buildings = map.map(|m| battle_buildings(vfs, m, radius)).unwrap_or_default();
        let weather = map.and_then(|m| m.weather.as_ref()).map(|w| w.max_weather_type_key.clone()).filter(|k| !k.is_empty());
        let custom = start.map(|s| s.custom.clone()).unwrap_or_default();
        Self { entities, ground, deployment, battle, group_formations, technologies, buildings, weather, custom }
    }
}

/// Fallback soldier radius for the garrison slot spacing when `battle_entities` is not loaded.
const GARRISON_SOLDIER_RADIUS_M: f32 = 0.35;

/// The garrison slot radius `0x00E619D0`: the largest radius (record `+0x4C`) of the
/// `battle_entities` rows whose `+0x18` is 0. INFERRED: `+0x18` is the class enum with infantry 0, and
/// the radius is column 12 (men 0.35 m), so slots are 0.7 m apart (0.875 m on walls).
fn garrison_radius(entities: Option<&BattleTables>) -> f32 {
    entities
        .map(|t| t.entities().filter(|e| e.class == "infantry").map(|e| e.radius).fold(0.0f32, f32::max))
        .filter(|r| *r > 0.0)
        .unwrap_or(GARRISON_SOLDIER_RADIUS_M)
}

/// The soldier slots of a building model: each intact fire line holds `max(1, floor(length / (2 × radius)))`
/// men (`0x006AA630`; the spacing is `2 × radius`, `2.5 × radius` for walls, `0x00E60E80`; CONFIRMED rule,
/// radius from `battle_entities`, see [`garrison_radius`]). The building's capacity is these slots capped by
/// its type's `+0x6C` (not found).
fn garrison_slots(r: &ntw_formats::models_building::ModelBuilding, radius: f32) -> u32 {
    let spacing = 2.0 * radius;
    r.intact_fire_lines()
        .map(|l| {
            let d = [l.end[0] - l.start[0], l.end[1] - l.start[1], l.end[2] - l.start[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            ((len / spacing).floor() as u32).max(1)
        })
        .sum()
}

/// The map's near buildings (the scripts' `battle:buildings()` order) with the number of intact
/// fire lines of their model (`models_building`; a building without any cannot be garrisoned,
/// INFERRED, see `ntw_sim::battle::garrison`). Wall buildings (`+0x231`) are the `battlefield_buildings`
/// rows of category `fort` (the building setup `0x00688DD0` sets `+0x230`/`+0x231` for category 8,
/// and the category enum `0x00E4E9A0` makes `fort` 8; CONFIRMED, the record link INFERRED).
fn battle_buildings(vfs: Option<&Vfs>, map: &BattleMap, radius: f32) -> Vec<ntw_sim::battle::garrison::BattleBuilding> {
    let mut lines = std::collections::HashMap::new();
    if let Some(vfs) = vfs
        && let Some(path) = vfs.list("db/models_building_tables").into_iter().next().map(str::to_owned)
    {
        match vfs.read(&path).map_err(|e| e.to_string()).and_then(|b| ntw_formats::models_building::read(&b).map_err(|e| e.to_string())) {
            Ok(rows) => {
                for r in rows {
                    lines.insert(r.key.to_ascii_lowercase(), garrison_slots(&r, radius));
                }
            }
            Err(e) => warn!("models_building: {e}; no building can be garrisoned"),
        }
    }
    let mut forts = std::collections::HashSet::new();
    if let Some(vfs) = vfs
        && let Some(path) = vfs.list("db/battlefield_buildings_tables").into_iter().next().map(str::to_owned)
        && let Ok(bytes) = vfs.read(&path)
        && let Ok(t) = DbTable::read(&bytes, &Schema::from_codes("s,s,s,s,i,o,o,i").expect("valid schema"))
    {
        for r in &t.rows {
            if let (Some(DbValue::Str(k)), Some(DbValue::Str(c))) = (r.first(), r.get(1))
                && c == "fort"
            {
                forts.insert(k.to_ascii_lowercase());
            }
        }
    }
    map.buildings_near
        .iter()
        .map(|b| ntw_sim::battle::garrison::BattleBuilding {
            key: b.key.clone(),
            position: b.position,
            fire_lines: lines.get(&b.key.to_ascii_lowercase()).copied().unwrap_or(0),
            wall: forts.contains(&b.key.to_ascii_lowercase()),
            occupant: None,
        })
        .collect()
}

/// The battle to fight: a `battles` DB key and its specification file (VFS path). Insert before
/// entering `GameMode::Battle` (the front end's StartBattle, or `--battle-key <KEY>`). Without it,
/// `--battle` fights the test slice.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct BattleStart {
    /// `battles` key, e.g. `NHB_Arcole`.
    pub key: String,
    /// The record's specification `.xml` (None for preset-only battles: no armies in the file).
    pub spec: Option<String>,
    /// The terrain preset (lower case, as `--battle-map` takes it).
    pub map: Option<String>,
    /// Each side's researched technologies (a campaign battle: [`researched_technologies`] of the
    /// side's faction); empty or `None` = no technology state, every ability and shot is available.
    pub technologies: Vec<Option<Vec<String>>>,
    /// A custom battle's armies (the front end's Play Battle pages); empty for other battles.
    pub custom: Vec<ntw_script::ui::CustomArmy>,
}

impl BattleStart {
    /// Looks a battle up in the `battles` table (`ntw_script::ui::frontend::read_battles`).
    pub fn from_battles_table(key: &str) -> Option<Self> {
        let source = ntw_script::ScriptSource::from_install(crate::config::game_data_dir()).ok()?;
        let rec = ntw_script::ui::frontend::read_battles(&source).into_iter().find(|b| b.key.eq_ignore_ascii_case(key))?;
        let spec = rec.spec.to_ascii_lowercase().ends_with(".xml").then(|| rec.spec.clone());
        let map = ntw_script::ui::frontend::battle_terrain(&source, &rec);
        Some(Self { key: rec.key, spec, map, technologies: Vec::new(), custom: Vec::new() })
    }
}

/// `unit_movement_modifiers` rows (name, 4 floats); schema `s,f,f,f,f` (worker2, 28 rows).
fn movement_modifiers(vfs: &Vfs) -> Vec<(String, [f32; 4])> {
    let path = "db/unit_movement_modifiers_tables/unit_movement_modifiers";
    let Ok(bytes) = vfs.read(path) else {
        warn!("{path} missing: ground types do not change speed");
        return Vec::new();
    };
    let schema = Schema::from_codes("s,f,f,f,f").expect("valid schema");
    match DbTable::read(&bytes, &schema) {
        Ok(t) => t
            .rows
            .iter()
            .map(|r| {
                let f = |i: usize| r.get(i).and_then(DbValue::as_f32).unwrap_or(1.0);
                (r.first().and_then(DbValue::as_str).unwrap_or_default().to_owned(), [f(1), f(2), f(3), f(4)])
            })
            .collect(),
        Err(e) => {
            warn!("{path}: {e:?}");
            Vec::new()
        }
    }
}

/// The model's view of a battle map: ground types and level-0 heights.
pub fn ground_of(map: &BattleMap, modifiers: &[(String, [f32; 4])]) -> BattleGround {
    let (w, h) = (map.definition.base_terrain_width, map.definition.base_terrain_height);
    let types = map.ground_types.as_ref().filter(|_| w > 0.0 && h > 0.0).map(|g| GroundTypeGrid {
        spec: GridSpec { cols: g.width, rows: g.height, width: w, height: h },
        cells: g.cells.clone(),
    });
    let heights = map.ground().map(|hf| HeightGrid {
        spec: GridSpec {
            cols: hf.width,
            rows: hf.height,
            width: hf.settings.world_width,
            height: hf.settings.world_height,
        },
        heights: (0..hf.height as i64).flat_map(|r| (0..hf.width as i64).map(move |c| hf.sample_m(c, r))).collect(),
    });
    BattleGround { types, heights, speed_modifiers: speed_table(modifiers.iter().map(|(n, v)| (n.as_str(), *v))) }
}

/// Startup system: reads the setup data and creates the `BattleSim` resource.
pub fn start_battle(mut commands: Commands, data: Res<GameData>, start: Option<Res<BattleStart>>) {
    let vfs = Vfs::open_install(crate::config::game_data_dir()).ok();
    let map = crate::terrain::current_map();
    let setup = SetupData::load(vfs.as_ref(), map.as_deref(), start.as_deref());
    commands.insert_resource(build_battle(&data.db, &setup, FIRST_SEED));
    commands.insert_resource(setup);
}

/// One unit before it is placed: its model unit (position not yet set) and display info.
struct Built {
    unit: LandUnit,
    info: UnitInfo,
}

/// Creates a fresh battle from the database with the given RNG seed: the chosen historical
/// battle's armies, or the test slice when there is none (or it is a naval battle, PLACEHOLDER:
/// no ships yet).
pub fn build_battle(db: &GameDatabase, setup: &SetupData, seed: u32) -> BattleSim {
    // All three rule tables come from the game data: `_kv_morale`, `_kv_fatigue`, `_kv_rules`.
    let mut battle = Battle::with_rules(seed, db.kv_morale, db.kv_fatigue, db.kv_rules_sim);
    // Column 6 (+0x20) of `unit_stats_land_experience_bonuses` in file order: the per-tick fatigue
    // bonus of a unit's experience level (CONFIRMED read in 0x00670F40, BATTLE_FIDELITY.md §58 (3)).
    battle.experience_fatigue = db.experience_fatigue_bonuses();
    battle.ground = Arc::clone(&setup.ground);
    apply_weather(&mut battle, db, setup.weather.as_deref());
    let mut sim = BattleSim::new(battle, seed);
    match &setup.battle {
        _ if !setup.custom.is_empty() => custom_armies(db, setup, &mut sim),
        Some((key, spec)) if !spec.is_naval() => historical_armies(db, setup, key, spec, &mut sim),
        Some((key, _)) => {
            warn!("Battle {key} is a naval battle: ships are not done yet (PLACEHOLDER: the test armies fight on its map)");
            test_armies(db, setup, &mut sim);
        }
        None => test_armies(db, setup, &mut sim),
    }
    // The deployment stage comes first unless the battle file skips it (`skip_deployment`) or a
    // test harness asks (`--skip-deployment`, or the FPS log).
    let skip = setup.battle.as_ref().is_some_and(|(_, s)| s.skip_deployment)
        || std::env::args().any(|a| a == "--skip-deployment")
        || std::env::var_os("NAPOLEON_FPS_LOG").is_some();
    sim.battle.buildings = setup.buildings.clone();
    // The skirmish default of the unit constructor (`abilities::skirmish_default`, CONFIRMED).
    for i in 0..sim.battle.units.len() {
        sim.battle.units[i].skirmish = sim.battle.skirmish_default(i);
    }
    sim.phase = if skip { BattlePhase::Conflict } else { BattlePhase::Deployment };
    // The battle file's own wind, in world axes and read as metres per second (see
    // `BattleSim::wind`). Only the flag cloth reads it.
    if let Some((x, y)) = setup.battle.as_ref().and_then(|(_, s)| s.prevailing_wind) {
        sim.wind = [x, 0.0, -y];
        info!("Battle wind: ({x:.2}, {y:.2}) m/s -> world {:?}", sim.wind);
    }
    sim
}

/// A `units` key from a battle file, matched case-insensitively against the database (the
/// files' spelling sometimes differs in case; INFERRED to be accepted by the exe).
fn resolve_spec_key(db: &GameDatabase, key: &str) -> Option<String> {
    if db.unit_stats(key).is_some() {
        return Some(key.to_owned());
    }
    db.units.rows().iter().find(|u| u.key.eq_ignore_ascii_case(key) && db.unit_stats(&u.key).is_some()).map(|u| u.key.clone())
}

/// The armies of a historical battle file: every alliance is a side (alliance index = model
/// side), every `army` of it is placed as the file says (position, orientation, frontage, men).
/// Reinforcement armies start off the field and arrive in turn (see below).
fn historical_armies(db: &GameDatabase, setup: &SetupData, key: &str, spec: &BattleSpec, sim: &mut BattleSim) {
    let player = spec.player_army().unwrap_or((0, 0));
    let mut next_id = 1;
    for (ai, alliance) in spec.alliances.iter().enumerate() {
        let side = ai as u8;
        if let (Some(name), Some(army)) = (sim.side_names.get_mut(ai), alliance.armies.first()) {
            *name = db.faction(&army.faction).map_or_else(|| army.faction.clone(), |f| f.screen_name.clone());
            sim.side_factions[ai] = army.faction.clone();
        }
        for (ri, army) in alliance.armies.iter().enumerate() {
            let controllable = (ai, ri) == player;
            if controllable {
                sim.camera_start = army.camera_start.zip(army.camera_target);
                sim.deployment_area = army.deployment_area;
            }
            for su in &army.units {
                let Some(unit_key) = resolve_spec_key(db, &su.unit_type) else {
                    warn!("{key}: unit type {} is not in the database; skipped", su.unit_type);
                    continue;
                };
                let (mut unit, mut info) = make_unit(db, setup, &unit_key, next_id, side);
                apply_spec_unit(db, &unit_key, su, &mut unit, &mut info);
                unit.army_index = u8::try_from(ri).unwrap_or(u8::MAX);
                info.faction = army.faction.clone();
                info.controllable = controllable;
                info.army = (ai, ri);
                sim.add_unit(unit, info);
                next_id += 1;
            }
        }
    }
    // Reinforcement armies (BATTLE_FIDELITY.md §13): their units start off the field and arrive one
    // at a time at the playable-area edge in the `approach_angle` direction (degrees, CONFIRMED;
    // 0 = +y like the file orientations and the edge point are PROVISIONAL). `manually_deployed`
    // units wait for the battle script's `deploy_reinforcement(true)` (CONFIRMED flag).
    //
    // Order, and it matters: this is a **second pass over every alliance**, so the built order is
    // all alliances' units first and only then all alliances' reinforcement units — *not*
    // army-then-reinforcements per alliance. `historical_armies` is the only place that order is
    // defined; anything that walks a battle file to match the built units must flatten the same way
    // (`battle_file_experience_reaches_every_unit` does).
    let (dimension, centre) = spec.playable_area.unwrap_or((2000.0, (0.0, 0.0)));
    // The playable area bounds the skirmish evade (battle +0x84..+0x90; INFERRED: `dimension` is the
    // square's full side, as the entry points above assume).
    if let Some((d, c)) = spec.playable_area {
        sim.battle.playable_area = Some([c.0 - 0.5 * d, c.1 - 0.5 * d, c.0 + 0.5 * d, c.1 + 0.5 * d]);
    }
    for (ai, alliance) in spec.alliances.iter().enumerate() {
        let side = ai as u8;
        for (ri, army) in alliance.reinforcements.iter().enumerate() {
            let army_index = u8::try_from(alliance.armies.len() + ri).unwrap_or(u8::MAX);
            let a = army.approach_angle.unwrap_or(0.0).to_radians();
            let entry = (centre.0 + 0.5 * dimension * a.sin(), centre.1 + 0.5 * dimension * a.cos());
            let facing = (-a.cos()).atan2(-a.sin());
            sim.battle.reinforcement_entries.push(((side, army_index), entry, facing));
            sim.battle.reinforcement_target = centre;
            for su in &army.units {
                let Some(unit_key) = resolve_spec_key(db, &su.unit_type) else {
                    warn!("{key}: reinforcement unit type {} is not in the database; skipped", su.unit_type);
                    continue;
                };
                let (mut unit, mut info) = make_unit(db, setup, &unit_key, next_id, side);
                apply_spec_unit(db, &unit_key, su, &mut unit, &mut info);
                unit.army_index = army_index;
                unit.active = false;
                unit.position = entry;
                unit.reinforcement = if su.manually_deployed { Reinforcement::Held } else { Reinforcement::Waiting };
                info.faction = army.faction.clone();
                info.controllable = false;
                info.army = (ai, alliance.armies.len() + ri);
                sim.add_unit(unit, info);
                next_id += 1;
            }
        }
    }
    sim.battle_key = Some(key.to_owned());
    sim.battle_script = spec.description.battle_script.clone();
    sim.victory = VictoryRules {
        time_limit_s: spec.description.duration,
        timeout_winner: spec.description.timeout_winner.and_then(|w| u8::try_from(w).ok()),
    };
    info!(
        "Battle {key}: {} units ({} the player's), {} vs {}",
        sim.info.len(),
        sim.info.iter().filter(|i| i.controllable).count(),
        sim.side_names[0],
        sim.side_names[1]
    );
}

/// What a battle file says about one unit: men (`num_soldiers`), place, facing, frontage
/// (drawn width; ranks = men / files, INFERRED), experience and general.
fn apply_spec_unit(db: &GameDatabase, key: &str, su: &SpecUnit, unit: &mut LandUnit, info: &mut UnitInfo) {
    if let Some(n) = su.num_soldiers.filter(|&n| n > 0) {
        // The file's number is the card's men as is: the parser 0x0050CAE0 hands num_soldiers to the card
        // builder 0x00513440, which stores it at card +0xC8 and +0xCC with no unit-size scaling (CONFIRMED).
        (unit.men, unit.max_men) = (n, n);
    }
    unit.position = su.position;
    // Battle-file orientation: 0 = +y, π/2 = +x (install test `armies_face_each_other`).
    unit.facing = std::f32::consts::FRAC_PI_2 - su.orientation;
    if let (Some(width), Some(stats)) = (su.width.filter(|w| *w > 0.0), db.unit_stats(key)) {
        let files = (width / stats.spacing_file_close.max(0.1)).round().max(1.0);
        let ranks = (unit.men.max(1) as f32 / files).ceil().max(1.0);
        info.ranks = ranks as u32;
        info.size_m = Vec2::new(width, (ranks * stats.spacing_rank_close).max(2.0));
        (unit.formation_width, unit.formation_depth) = (info.size_m.x, info.size_m.y);
        // The front rank: the men who fire with the default drill (`shooting::volley_plan`).
        unit.formation_files = files as u32;
    }
    info.experience = su.experience.unwrap_or(0);
    // Byte `unit+0xD48` is the experience level (CONFIRMED: exposed as "Experience" by `0x005ABF40`
    // and `0x005CD340`; read by the waver/rout timers `0x0053E4D0`/`0x0053A720`). The battle file's
    // `unit_experience level` lands here (parsed by `0x0050CAE0` into the card).
    unit.experience = su.experience.unwrap_or(0).min(255) as u8;
    info.general = su.general.as_ref().map(|g| g.name.clone());
    // The army general: a `general` element; rank = `star_rating level` (the exe does not read
    // `experience` there, CONFIRMED 0x0050CAE0).
    unit.general_rank = su.general.as_ref().map(|g| g.star_rating.unwrap_or(0) as i32);
    // The card's capability block (special abilities and shot types; CONFIRMED enums).
    unit.capabilities = ntw_sim::battle::attributes::UnitCapabilities::from_names(
        su.special_abilities.iter().map(String::as_str),
        su.shot_types.iter().map(String::as_str),
    );
    info.category = su.category.clone();
    info.script_name = su.script_name.clone();
}

/// The fixed France vs Austria test armies (the `--battle` slice without a battle key).
fn test_armies(db: &GameDatabase, setup: &SetupData, sim: &mut BattleSim) {
    let armies: [(&[Slot; 5], f32, f32); 2] = [
        // (order of battle, y position, facing angle). Facing 0 = +x, π/2 = +y ("up").
        (&FRANCE, -START_DISTANCE_M, std::f32::consts::FRAC_PI_2),
        (&AUSTRIA, START_DISTANCE_M, -std::f32::consts::FRAC_PI_2),
    ];
    sim.side_names = ["France".to_string(), "Austria".to_string()];
    sim.side_factions = ["france".to_string(), "austria".to_string()];
    sim.deployment_area = setup.deployment.as_ref().and_then(|d| d.alliances.first()).and_then(|a| a.areas.first()).cloned();
    let mut next_id = 1;
    for (side, (slots, y, facing)) in armies.into_iter().enumerate() {
        let mut built = Vec::new();
        // Test harness: `NAPOLEON_BATTLE_REPEAT=n` repeats each order of battle n times (for
        // measuring frame rates with bigger armies).
        let repeat = std::env::var("NAPOLEON_BATTLE_REPEAT").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).max(1);
        for (copy, slot) in (0..repeat).flat_map(|c| slots.iter().map(move |s| (c, s))) {
            let Some(key) = resolve_key(db, slot.key, slot.category) else {
                warn!("No unit available for slot {}", slot.key);
                continue;
            };
            let (mut unit, mut unit_info) = make_unit(db, setup, &key, next_id, side as u8);
            unit.position = (slot.x + copy as f32 * 300.0, y);
            unit.general_rank = slot.general;
            if slot.general.is_some() {
                unit_info.general = Some(unit_info.name.clone());
            }
            unit.facing = facing;
            unit_info.faction = sim.side_factions[side].clone();
            unit_info.controllable = side == 0;
            unit_info.army = (side, 0);
            built.push(Built { unit, info: unit_info });
            next_id += 1;
        }
        let area = setup.deployment.as_ref().and_then(|d| d.alliances.get(side)).and_then(|a| a.areas.first());
        if let Some(area) = area {
            let sizes: Vec<Vec2> = built.iter().map(|b| b.info.size_m).collect();
            let classes: Vec<&str> = built.iter().map(|b| b.unit.unit_class.as_str()).collect();
            // unit_stats_land #88: guerrilla deployment (the exe's unit `+0x1C5`).
            let guerrilla: Vec<bool> = built.iter().map(|b| db.unit_stats(&b.info.key).is_some_and(|s| s.unknown_238)).collect();
            // The original's default deployment (group formation templates); our side-by-side
            // line only if no template lays the army out.
            let placed = match deploy_with_templates(area, &setup.group_formations, &sim.side_factions[side], &classes, &sizes, &guerrilla) {
                Some((template, placed)) => {
                    info!("Default deployment of {}: \"{template}\"", sim.side_names[side]);
                    placed
                }
                None => deploy_in_area(area, &sizes),
            };
            for (b, (pos, facing)) in built.iter_mut().zip(placed) {
                b.unit.position = pos;
                b.unit.facing = facing;
            }
        }
        for b in built {
            sim.add_unit(b.unit, b.info);
        }
    }
    // `BattleSim::add_unit` keeps `info` in the model's (id) order.
}

/// The armies of a custom battle (the front end's StartBattle): every alliance is a side, every
/// army of it is deployed in its deployment area of the map's 1v1 setup like the test armies
/// (default deployment templates, else side by side). Experience comes from the army page;
/// the general's unit carries the army's general. PROVISIONAL: the men are the units' full
/// numbers (the unit size option is not applied yet), and a 2v2 army uses its alliance's
/// area of the same index (or the first one).
fn custom_armies(db: &GameDatabase, setup: &SetupData, sim: &mut BattleSim) {
    let mut next_id = 1;
    let mut army_counts = [0usize; 2];
    for army in &setup.custom {
        let side = (army.alliance as usize).min(1);
        let ri = army_counts[side];
        army_counts[side] += 1;
        if ri == 0 {
            sim.side_names[side] = db.faction(&army.faction).map_or_else(|| army.faction.clone(), |f| f.screen_name.clone());
            sim.side_factions[side] = army.faction.clone();
        }
        let area = setup.deployment.as_ref().and_then(|d| d.alliances.get(side)).and_then(|a| a.areas.get(ri).or_else(|| a.areas.first()));
        if army.human && sim.deployment_area.is_none() {
            sim.deployment_area = area.cloned();
        }
        let mut built = Vec::new();
        for (key, xp) in &army.units {
            let Some(unit_key) = resolve_spec_key(db, key) else {
                warn!("Custom battle: unit {key} is not a land unit in the database; skipped");
                continue;
            };
            let (mut unit, mut info) = make_unit(db, setup, &unit_key, next_id, side as u8);
            info.experience = *xp;
            // Byte `unit+0xD48` (CONFIRMED experience level for the waver/rout timers).
            unit.experience = (*xp).min(255) as u8;
            if unit.unit_class == "general" {
                unit.general_rank = Some(0);
                info.general = Some(info.name.clone());
            }
            unit.army_index = u8::try_from(ri).unwrap_or(u8::MAX);
            info.faction = army.faction.clone();
            info.controllable = army.human;
            info.army = (side, ri);
            built.push(Built { unit, info });
            next_id += 1;
        }
        let (y, facing) = if side == 0 {
            (-START_DISTANCE_M, std::f32::consts::FRAC_PI_2)
        } else {
            (START_DISTANCE_M, -std::f32::consts::FRAC_PI_2)
        };
        let placed = match area {
            Some(area) => {
                let sizes: Vec<Vec2> = built.iter().map(|b| b.info.size_m).collect();
                let classes: Vec<&str> = built.iter().map(|b| b.unit.unit_class.as_str()).collect();
                let guerrilla: Vec<bool> = built.iter().map(|b| db.unit_stats(&b.info.key).is_some_and(|s| s.unknown_238)).collect();
                match deploy_with_templates(area, &setup.group_formations, &army.faction, &classes, &sizes, &guerrilla) {
                    Some((template, placed)) => {
                        info!("Default deployment of {}: \"{template}\"", army.faction);
                        placed
                    }
                    None => deploy_in_area(area, &sizes),
                }
            }
            // No map: a line across the field, 40 m per unit.
            None => (0..built.len()).map(|i| ((i as f32 * 40.0 - built.len() as f32 * 20.0, y), facing)).collect(),
        };
        for (b, (pos, facing)) in built.iter_mut().zip(placed) {
            b.unit.position = pos;
            b.unit.facing = facing;
        }
        for b in built {
            sim.add_unit(b.unit, b.info);
        }
    }
    info!(
        "Custom battle: {} units ({} the player's), {} vs {}",
        sim.info.len(),
        sim.info.iter().filter(|i| i.controllable).count(),
        sim.side_names[0],
        sim.side_names[1]
    );
}

/// True if a map point lies inside a deployment area (a rectangle `width` across and `height` deep
/// around its centre, turned by its orientation; INFERRED reading of the area fields).
pub fn area_contains(area: &DeploymentArea, p: (f32, f32)) -> bool {
    let (fx, fy) = area.facing_vector();
    let (dx, dy) = (p.0 - area.centre.0, p.1 - area.centre.1);
    let along = dx * fx + dy * fy;
    let across = dx * fy - dy * fx;
    (area.width <= 0.0 || across.abs() <= area.width / 2.0) && (area.height <= 0.0 || along.abs() <= area.height / 2.0)
}

/// A unit's map position and battle facing.
type Placed = ((f32, f32), f32);

/// The role a unit class counts as in the templates' composition test (INFERRED from the
/// class names; the exe tests the unit's category, `group_formation::Role`).
fn role_of(class: &str) -> Role {
    let c = class.to_ascii_lowercase();
    if c.starts_with("artillery") {
        Role::Artillery
    } else if c.starts_with("cavalry") || c == "dragoons" || c == "elephants" || c == "general" {
        Role::Cavalry
    } else {
        Role::Infantry
    }
}

/// The original's default deployment (`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §5.2): the
/// army's group formation template is chosen and laid out (`group_formation::{choose, assign,
/// layout}`, CONFIRMED rules), then placed facing the area's direction with the template origin
/// on the area's centre, pulled back in 0.5 m steps from 1 m until every unit stands inside the
/// area, at most `area depth / 2 − group depth` (`0x005DFC40`; the start, step and limit are
/// CONFIRMED structure, the area field read as the half depth INFERRED); if no pull-back fits
/// them all, the one with the most units inside. `None` when no template takes the units.
/// Returns the template's name and each unit's position and facing.
pub fn deploy_with_templates(
    area: &DeploymentArea,
    templates: &[Template],
    faction: &str,
    classes: &[&str],
    sizes: &[Vec2],
    guerrilla: &[bool],
) -> Option<(String, Vec<Placed>)> {
    if templates.is_empty() || sizes.is_empty() {
        return None;
    }
    let units: Vec<GroupUnit> = classes
        .iter()
        .zip(sizes)
        .map(|(c, s)| GroupUnit { class: group_formation::class_id(c), role: role_of(c), width: s.x, depth: s.y })
        .collect();
    // The template is chosen for the whole army; the guerrilla-deployment units (unit_stats_land
    // #88, unit `+0x1C5`) are laid out with it as a second group placed 25 m ahead (CONFIRMED,
    // `0x005BA640`).
    let t = &templates[group_formation::choose(templates, &units, faction, PURPOSE_DEPLOYMENT)];
    let mut out = vec![((0.0, 0.0), area.sim_facing()); units.len()];
    for (ahead, second) in [(0.0f32, false), (25.0, true)] {
        let members: Vec<usize> = (0..units.len()).filter(|&i| guerrilla.get(i).copied().unwrap_or(false) == second).collect();
        if members.is_empty() {
            continue;
        }
        let group: Vec<GroupUnit> = members.iter().map(|&i| units[i]).collect();
        let (_, assignment) = group_formation::assign(t, &group)?;
        let (local, bounds) = group_formation::layout(t, &group, &assignment);
        for (k, p) in place_group(area, &local, &bounds, ahead).into_iter().enumerate() {
            out[members[k]].0 = p;
        }
    }
    Some((t.name.clone(), out))
}

/// Places a laid-out group in an area (`0x005DFC40`): template origin `ahead` metres in front of
/// the area centre, pulled back from 1 m in 0.5 m steps until every unit is inside, at most
/// `area depth / 2 − group depth`; else the pull-back with the most units inside.
fn place_group(area: &DeploymentArea, local: &[(f32, f32)], bounds: &group_formation::Rect, ahead: f32) -> Vec<(f32, f32)> {
    let (fx, fy) = area.facing_vector();
    let (forward, right) = (Vec2::new(fx, fy), Vec2::new(fy, -fx));
    let centre = Vec2::new(area.centre.0, area.centre.1) + forward * ahead;
    let at = |pull: f32| -> Vec<(f32, f32)> {
        local.iter().map(|&(x, y)| centre + right * x + forward * (y - pull)).map(|p| (p.x, p.y)).collect()
    };
    let limit = if area.height > 0.0 { area.height / 2.0 - bounds.depth() } else { 0.0 };
    let (mut pull, mut best) = (1.0f32, (0usize, 1.0f32));
    loop {
        let inside = at(pull).iter().filter(|&&p| area_contains(area, p)).count();
        if inside == local.len() {
            best = (inside, pull);
            break;
        }
        if inside > best.0 {
            best = (inside, pull);
        }
        if pull * pull >= limit * limit || pull > 10_000.0 {
            break;
        }
        pull += 0.5;
    }
    at(best.1)
}

/// Positions and facings for units of the given drawn sizes (width across, depth along the
/// facing) in a deployment area: side by side across the area, centred, in order; a unit that
/// would cross the area's side starts a new line behind (PLACEHOLDER layout; used only when no
/// group formation template takes the army, see [`deploy_with_templates`]).
pub fn deploy_in_area(area: &DeploymentArea, sizes: &[Vec2]) -> Vec<((f32, f32), f32)> {
    let (fx, fy) = area.facing_vector();
    let forward = Vec2::new(fx, fy);
    // To the right of the facing direction (clockwise by 90° seen from above).
    let right = Vec2::new(fy, -fx);
    let centre = Vec2::new(area.centre.0, area.centre.1);
    let max_width = if area.width > 0.0 { area.width } else { f32::MAX };
    // Split into lines.
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (i, s) in sizes.iter().enumerate() {
        let need = if lines.last().unwrap().is_empty() { s.x } else { used + UNIT_GAP_M + s.x };
        if need > max_width && !lines.last().unwrap().is_empty() {
            lines.push(vec![i]);
            used = s.x;
        } else {
            lines.last_mut().unwrap().push(i);
            used = need;
        }
    }
    let mut out = vec![((0.0, 0.0), area.sim_facing()); sizes.len()];
    let mut back = 0.0;
    for line in &lines {
        let width: f32 = line.iter().map(|&i| sizes[i].x).sum::<f32>() + UNIT_GAP_M * (line.len().saturating_sub(1)) as f32;
        let depth = line.iter().map(|&i| sizes[i].y).fold(0.0, f32::max);
        let mut along = -width / 2.0;
        for &i in line {
            let p = centre + right * (along + sizes[i].x / 2.0) - forward * (back + depth / 2.0);
            out[i].0 = (p.x, p.y);
            along += sizes[i].x + UNIT_GAP_M;
        }
        back += depth + UNIT_GAP_M;
    }
    out
}

/// Returns `preferred` if the database has stats for it, else the first unit of `category`.
fn resolve_key(db: &GameDatabase, preferred: &str, category: &str) -> Option<String> {
    if db.unit_stats(preferred).is_some() {
        return Some(preferred.to_string());
    }
    db.units
        .rows()
        .iter()
        .find(|u| u.category == category && db.unit_stats(&u.key).is_some())
        .map(|u| u.key.clone())
}

/// The researched technologies of a campaign faction, for [`BattleStart::technologies`]: its
/// `FACTION_TECHNOLOGY_MANAGER` entries in state [`TECH_RESEARCHED`] (read-only from the campaign
/// model, `FactionDetails::technologies`; EFFECTS_FIDELITY.md §3). `None` if the faction is unknown
/// or its details were not loaded.
///
/// [`TECH_RESEARCHED`]: ntw_sim::campaign::effects::TECH_RESEARCHED
///
/// Not called yet: the campaign does not launch real-time battles (`campaign::play` HOOK); the
/// launch fills [`BattleStart::technologies`] with this for each side.
#[allow(dead_code)]
pub fn researched_technologies(model: &ntw_sim::campaign::world::CampaignModel, faction_key: &str) -> Option<Vec<String>> {
    let id = model.faction_by_key(faction_key)?.id;
    let details = model.world.faction_details.get(&id)?;
    Some(
        details
            .technologies
            .iter()
            .filter(|(_, state)| *state == ntw_sim::campaign::effects::TECH_RESEARCHED)
            .map(|(k, _)| k.clone())
            .collect(),
    )
}

/// The unit's missile weapon from the real data, or `None` if it has no projectile or no ammunition.
///
/// - Infantry and cavalry: `unit_stats_land.projectile` → `projectiles` row.
/// - Artillery: the first projectile its `gun_type` can fire (`gun_type_to_projectiles`, in the
///   exe's shot type order, `GameDatabase::gun_projectiles`).
///   The first is loaded; the others are the unit's `shot_options` ([`shot_options`]), which
///   `change_shot_type` switches to.
pub fn missile_weapon(db: &GameDatabase, stats: &ntw_data::UnitStatsLand) -> Option<MissileWeapon> {
    if stats.ammunition <= 0 {
        return None;
    }
    let projectile = db.primary_projectile(stats)?;
    Some(weapon_of(stats, projectile))
}

/// The other shots an artillery unit can load: `(shot type, weapon)` for every projectile of its
/// gun after the first (shot type = `projectiles.shot_type` in the exe's shot type enum).
///
/// Technologies: a shot type that a technology enables (`effect_bonus_value_shot_type_junctions`,
/// e.g. canister) is offered only when `researched` enables it; `None` (no campaign technology
/// state yet, PROVISIONAL) offers every shot.
pub fn shot_options(db: &GameDatabase, stats: &ntw_data::UnitStatsLand, researched: Option<&[String]>) -> Vec<(u8, MissileWeapon)> {
    if stats.ammunition <= 0 || db.unit_projectile(stats).is_some() {
        return Vec::new();
    }
    let enabled = researched.map(|t| db.technology_unlocks(t).1);
    let allowed = |s: &str| match &enabled {
        Some(on) => !db.shot_type_needs_technology(s) || on.iter().any(|x| x.eq_ignore_ascii_case(s)),
        None => true,
    };
    db.gun_projectiles(stats).into_iter().skip(1).filter(|p| allowed(&p.shot_type)).filter_map(|p| Some((shot_type_value(&p.shot_type)?, weapon_of(stats, p)))).collect()
}

fn weapon_of(stats: &ntw_data::UnitStatsLand, projectile: &ntw_data::Projectile) -> MissileWeapon {
    MissileWeapon {
        // INFERRED: the weapon range of W1 §12.6 is the projectile's effective range (col 11).
        range: projectile.effective_range,
        accuracy: stats.accuracy as f32, // unit_stats_land col 26
        reload_skill: stats.reload_skill, // col 27
        reload_time_s: projectile.reload_time, // projectiles col 24
        damage: projectile.damage, // projectiles col 17
        projectiles_per_shot: projectile.projectiles_per_shot.max(1) as u32, // col 8
        is_artillery: stats.is_artillery, // col 23
        guns: stats.num_guns.max(0) as u32, // col 3
        ballistics: ballistics_of(projectile),
    }
}

/// The fatigue-effect multipliers per fatigue level of a unit category (`fatigue_effects`, key
/// `threshold;category`; INFERRED: the category is `units.category`, whose values are exactly the
/// table's land categories). Levels without a row keep `FatigueEffects::NONE`.
pub fn fatigue_effects_of(db: &GameDatabase, category: &str) -> [FatigueEffects; 6] {
    let mut out = [FatigueEffects::NONE; 6];
    for r in db.fatigue_effects.iter().filter(|r| r.category.eq_ignore_ascii_case(category)) {
        let level = match r.threshold.as_str() {
            "threshold_tired" => 3,
            "threshold_very_tired" => 4,
            "threshold_exhausted" => 5,
            _ => continue,
        };
        out[level] = FatigueEffects { speed: 1.0 + r.speed, charge: 1.0 + r.charge, control: 1.0 + r.control, attack: 1.0 + r.attack };
    }
    out
}

/// The battle's weather from the map preset's `weather.xml` `max_weather_type_key` (a
/// `battle_weather_types` key): its intensity (visibility) and kind (rain or snow for the reload
/// and fatigue rules). INFERRED: the preset key caps the weather; every shipped preset says `dry`,
/// so every preset battle is dry whatever the original's pick inside the cap.
fn apply_weather(battle: &mut Battle, db: &GameDatabase, key: Option<&str>) {
    let Some(w) = key.and_then(|k| db.battle_weather_types.iter().find(|w| w.key.eq_ignore_ascii_case(k))) else { return };
    battle.weather_intensity = w.intensity.max(0) as u32;
    battle.weather = match w.kind {
        0 => ntw_sim::battle::fatigue::Weather::Rain,
        1 => ntw_sim::battle::fatigue::Weather::Snow,
        _ => ntw_sim::battle::fatigue::Weather::Clear,
    };
    info!("Battle weather {}: intensity {}, {:?}", w.key, w.intensity, battle.weather);
}

/// The projectile's ballistic columns (10, 13, 14, 15) for the chance-to-hit angle and bonus terms.
pub fn ballistics_of(p: &ntw_data::Projectile) -> ntw_sim::battle::missile::Ballistics {
    use ntw_sim::battle::missile::{Ballistics, Trajectory};
    Ballistics {
        trajectory: Trajectory::from_name(&p.trajectory_class).unwrap_or_default(),
        muzzle_velocity: p.muzzle_velocity,
        max_elevation_deg: p.max_elevation,
        accuracy_modifier: p.accuracy_modifier,
    }
}

/// Builds one model unit (position not set) and its display info from its database rows.
fn make_unit(db: &GameDatabase, setup: &SetupData, key: &str, id: u32, side: u8) -> (LandUnit, UnitInfo) {
    let entities = setup.entities.as_ref();
    // The side's researched technologies (campaign battles), read-only from the campaign model.
    let researched = setup.technologies.get(side as usize).and_then(|t| t.as_deref());
    let record = db.unit(key).expect("resolve_key checked this unit exists");
    let stats = db.unit_stats(key).expect("resolve_key checked the stats exist");
    let mounted = stats.num_mounts > 0 && stats.mount_entity.is_some();
    let is_cavalry = record.category == "cavalry" || mounted;

    let mut u = LandUnit::new(id, side, stats.num_men.max(0) as u32, (0.0, 0.0));
    // Real stats (unit_stats_land; column numbers per analysis/worker1/DB_BUILDERS.md):
    u.melee_attack = stats.melee_attack; // col 34
    u.charge_bonus = stats.charge_bonus; // col 35
    u.melee_defence = stats.melee_defence; // col 36
    u.armour = stats.armour; // col 11
    // CONFLICT NOTE: W1 §12.9 inferred col 36 = shield and col 37 = defence. ntw_data (W4), with
    // W2's data reading, has col 36 = defence (values like 6 for fusiliers) and col 37 always 0.
    // We follow the data evidence: no shields in this era, so shield = col 37 (= 0).
    u.shield = stats.unknown_188;
    u.morale_stat = stats.morale; // col 42
    u.attributes = GameDatabase::unit_attributes(stats);
    // The card's capability block from the DB (battle files replace it with their own list).
    u.capabilities = db.unit_capabilities_with(key, &record.unit_class, researched);
    u.is_cavalry = is_cavalry;
    u.unit_class = record.unit_class.clone();
    u.unit_category = record.category.clone();
    u.fatigue_effects = fatigue_effects_of(db, &record.category);
    // The `unit_movement_modifiers` column (`0x006543D0`, see `MovementClass`).
    u.movement_class = if stats.is_artillery {
        match record.unit_class.as_str() {
            "artillery_foot" => MovementClass::FootArtillery,
            "artillery_horse" => MovementClass::HorseArtillery,
            _ => MovementClass::Fixed,
        }
    } else if matches!(record.category.as_str(), "cavalry" | "cavalry_camels" | "dragoons") || is_cavalry {
        MovementClass::Mounted
    } else {
        MovementClass::Infantry
    };
    // Missile weapon (muskets): unit_stats_land accuracy/reload/ammunition + its projectiles row.
    if let Some(weapon) = missile_weapon(db, stats) {
        u.missile = Some(weapon);
        u.shot_options = shot_options(db, stats, researched);
        u.shot_type = db.primary_projectile(stats).and_then(|p| shot_type_value(&p.shot_type));
        u.ammunition = stats.ammunition.max(0) as u32; // col 31
    }
    // Speeds (m/s) from `battle_entities` (col 3 walk, col 4 run; INFERRED columns, see
    // CAVALRY.md §4): the mount's entity when mounted, else the man's.
    let entity = if mounted { stats.mount_entity.as_deref().unwrap_or_default() } else { stats.man_entity.as_str() };
    match entities.and_then(|t| t.entity(entity)).filter(|e| e.walk_speed > 0.0 && e.run_speed > 0.0) {
        Some(e) => (u.walk_speed, u.run_speed) = (e.walk_speed, e.run_speed),
        // PLACEHOLDER speeds when the entity is unknown (e.g. on the test fixture).
        None => (u.walk_speed, u.run_speed) = if is_cavalry { (3.0, 9.0) } else { (1.5, 4.0) },
    }

    // Drawn size from the real formation depth and spacing (close order).
    let ranks = if stats.default_ranks > 0 {
        stats.default_ranks as u32
    } else if is_cavalry {
        FALLBACK_RANKS.1
    } else {
        FALLBACK_RANKS.0
    };
    let files = (stats.num_men.max(1) as f32 / ranks as f32).ceil();
    let size_m = Vec2::new(
        (files * stats.spacing_file_close).max(5.0),
        (ranks as f32 * stats.spacing_rank_close).max(2.0),
    );
    // The model's formation size (skirmish strips, defence pieces): the drawn block.
    (u.formation_width, u.formation_depth) = (size_m.x, size_m.y);
    // The front rank: the men who fire with the default drill (`shooting::volley_plan`).
    u.formation_files = files as u32;
    let info = UnitInfo { id, name: record.dev_name.clone(), size_m, key: key.to_owned(), ranks, category: record.category.clone(), ..Default::default() };
    (u, info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::battle::model::BattleResult;

    /// The real France vs Austria slice, run headless (no window) from the player's install.
    /// Skipped (passes) when the game is not installed. Checks that the musket units really
    /// shoot with real data and that the battle still ends.
    #[test]
    fn real_slice_units_shoot() {
        let dir = crate::config::game_data_dir();
        let Ok(db) = GameDatabase::from_install(&dir) else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let mut sim = build_battle(&db, &SetupData::default(), FIRST_SEED);
        let shooters = sim.battle.units.iter().filter(|u| u.missile.is_some()).count();
        assert_eq!(shooters, 6, "three musket units per side");
        let (mut volleys, mut volley_kills) = (0, 0);
        let mut ticks = 0;
        while sim.battle.battle_result() == BattleResult::Ongoing && ticks < 30_000 {
            sim.battle.step();
            volleys += sim.battle.volleys.len();
            volley_kills += sim.battle.volleys.iter().map(|v| v.kills).sum::<u32>();
            ticks += 1;
        }
        eprintln!(
            "{ticks} ticks, {volleys} volleys, {volley_kills} killed by fire, result {:?}",
            sim.battle.battle_result()
        );
        assert!(volleys > 0 && volley_kills > 0);
        assert_ne!(sim.battle.battle_result(), BattleResult::Ongoing);
    }

    /// **BATTLE_FIDELITY.md §58 (3):** the battle file's `unit_experience level` reaches
    /// `LandUnit::experience` (the byte `unit+0xD48`) unit for unit, over every installed land
    /// battle — and the shipped data really uses the whole 0..9 range, so the field is load-bearing
    /// and not a dead one. Skipped when the game is not installed.
    #[test]
    fn battle_file_experience_reaches_every_unit() {
        let dir = crate::config::game_data_dir();
        let (Ok(db), Ok(vfs), Ok(source)) =
            (GameDatabase::from_install(&dir), Vfs::open_install(&dir), ntw_script::ScriptSource::from_install(&dir))
        else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let mut battles = 0;
        let mut widest: (usize, String) = (0, String::new());
        let mut max_level = 0u32;
        for rec in ntw_script::ui::frontend::read_battles(&source) {
            if !rec.spec.to_ascii_lowercase().ends_with(".xml") || vfs.read(&rec.spec).is_err() {
                continue;
            }
            let start = BattleStart { key: rec.key.clone(), spec: Some(rec.spec.clone()), map: None, technologies: Vec::new(), custom: Vec::new() };
            let setup = SetupData::load(Some(&vfs), None, Some(&start));
            let Some((_, spec)) = setup.battle.clone() else { continue };
            if spec.is_naval() {
                continue;
            }
            let sim = build_battle(&db, &setup, FIRST_SEED);
            // The same order `historical_armies` walks: every alliance's units first, then every
            // alliance's reinforcement units (two separate loops, not per alliance). It skips a
            // unit whose type is not in the database, which `historical_battles_build_with_all_units`
            // shows does not happen for these battles.
            let mut want: Vec<u32> =
                spec.alliances.iter().flat_map(|a| &a.armies).flat_map(|a| &a.units).map(|u| u.experience.unwrap_or(0)).collect();
            want.extend(
                spec.alliances.iter().flat_map(|a| &a.reinforcements).flat_map(|a| &a.units).map(|u| u.experience.unwrap_or(0)),
            );
            assert_eq!(sim.battle.units.len(), want.len(), "{}: some unit types were skipped", rec.key);
            let mut levels: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
            for (u, &want_xp) in sim.battle.units.iter().zip(&want) {
                assert_eq!(u32::from(u.experience), want_xp, "{}: unit {} ({}) experience", rec.key, u.id, sim.info_of(u.id).map_or("", |i| i.key.as_str()));
                assert_eq!(sim.info_of(u.id).map_or(0, |i| i.experience), want_xp, "{}: the unit card's display value", rec.key);
                levels.insert(want_xp);
            }
            eprintln!("{:24} {} units, {} distinct experience levels {levels:?}", rec.key, want.len(), levels.len());
            if levels.len() > widest.0 {
                widest = (levels.len(), rec.key.clone());
            }
            max_level = max_level.max(levels.iter().copied().max().unwrap_or(0));
            battles += 1;
        }
        assert!(battles >= 10, "only {battles} land battles");
        // One battle must exercise most of the ten chevron rows, and the top row must appear:
        // otherwise the wiring is never checked above rank 0, which is where a stand-in would hide.
        assert!(widest.0 >= 5, "the widest battle ({}) has only {} distinct levels", widest.1, widest.0);
        assert!(max_level >= 9, "the highest experience level in the data is {max_level}, never 9");
    }

    /// **BATTLE_FIDELITY.md §58 (3):** the level the battle file hands a unit really moves the
    /// numbers the exe moves with it — the `unit_stats_land_experience_bonuses` fatigue term
    /// (`0x00670F40`) and the waver / rout timers (`0x0053E4D0` / `0x0053A720`) — read from the
    /// real install, so the check cannot pass on invented values. Skipped when not installed.
    #[test]
    fn the_experience_level_moves_fatigue_and_the_morale_timers() {
        use ntw_sim::battle::{fatigue, morale};
        let dir = crate::config::game_data_dir();
        let Ok(db) = GameDatabase::from_install(&dir) else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let rows = db.experience_fatigue_bonuses();
        assert_eq!(rows.len(), 10, "one fatigue row per chevron level 0..9");
        // Level 0 is the baseline and no level gives a veteran a *positive* fatigue bonus (the term
        // makes veterans tire more slowly: `0x00670F40` adds the row to every soldier's fatigue).
        assert_eq!(fatigue::experience_bonuses(&rows, 0), 0);
        assert!(rows[9] <= rows[0], "level 9 ({}) is not better than level 0 ({})", rows[9], rows[0]);
        assert!(fatigue::experience_bonuses(&rows, 9) < 0, "level 9 must tire more slowly than level 0");
        // The timers: a veteran wavers for longer and routs for less long (BATTLE_FIDELITY.md §2.1).
        for level in 0..=9u8 {
            assert_eq!(morale::waver_timeout(&db.kv_morale, level), db.kv_morale.waver_base_timeout + level as i32 * 5);
            assert_eq!(morale::rout_timeout(&db.kv_morale, level), (db.kv_morale.broken_finish_base_timeout - level as i32 * 20).max(0));
        }
        assert!(morale::waver_timeout(&db.kv_morale, 9) > morale::waver_timeout(&db.kv_morale, 0));
        assert!(morale::rout_timeout(&db.kv_morale, 9) < morale::rout_timeout(&db.kv_morale, 0));
        eprintln!(
            "experience fatigue rows {rows:?}; waver {} -> {}, rout {} -> {}",
            morale::waver_timeout(&db.kv_morale, 0),
            morale::waver_timeout(&db.kv_morale, 9),
            morale::rout_timeout(&db.kv_morale, 0),
            morale::rout_timeout(&db.kv_morale, 9)
        );
    }

    /// A campaign battle's technologies: France's researched technologies in the Europe start
    /// position (read-only), and what they leave of the line fusiliers' abilities and the foot
    /// artillery's shots. Skipped when the game is not installed.
    #[test]
    fn campaign_technologies_gate_abilities_and_shots() {
        let dir = crate::config::game_data_dir();
        let Ok(db) = GameDatabase::from_install(&dir) else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let model = ntw_campaign::load_startpos_file(dir.join("campaigns").join("eur_napoleon").join("startpos.esf"), &db).unwrap();
        let techs = researched_technologies(&model, "france").expect("france");
        let unlocks = db.technology_unlocks(&techs);
        eprintln!("france researched {techs:?}; unlocks {unlocks:?}");
        let caps = db.unit_capabilities_with("Inf_Line_French_Fusiliers", "infantry_line", Some(&techs));
        let all = db.unit_capabilities("Inf_Line_French_Fusiliers", "infantry_line");
        assert!(caps.abilities & !all.abilities == 0, "a subset of the ungated list");
        for (name, v) in ntw_sim::battle::attributes::ABILITY_NAMES {
            if all.has_ability(v) {
                let gated = db.ability_needs_technology(name);
                assert_eq!(caps.has_ability(v), !gated || unlocks.0.iter().any(|a| a == name), "{name}");
            }
        }
        // France starts without `military1_fire_and_advance`: square formation yes, fire and advance no.
        use ntw_sim::battle::attributes::ability;
        assert!(caps.has_ability(ability::SQUARE_FORMATION) && !caps.has_ability(ability::FIRE_AND_ADVANCE));
        assert!(researched_technologies(&model, "no_such_faction").is_none());
    }

    /// Every projectile's `shot_type` is a name of the exe's shot type enum, and artillery units
    /// get their gun's other shots for `change_shot_type`. Skipped when the game is not installed.
    #[test]
    fn artillery_shot_options_from_the_data() {
        let dir = crate::config::game_data_dir();
        let Ok(db) = GameDatabase::from_install(&dir) else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let unknown: Vec<&str> =
            db.projectiles.iter().map(|p| p.shot_type.as_str()).filter(|s| shot_type_value(s).is_none()).collect();
        assert!(unknown.is_empty(), "shot types not in the enum: {unknown:?}");
        let with_options = db.unit_stats_land.iter().filter(|s| !shot_options(&db, s, None).is_empty()).count();
        eprintln!("{with_options} units can change shot type");
        assert!(with_options > 0);
    }

    /// Every land battle of the `battles` table with a specification file builds with all of its
    /// (non-reinforcement) units found in the database, and the player commands some of them.
    /// Skipped when the game is not installed.
    #[test]
    fn historical_battles_build_with_all_units() {
        let dir = crate::config::game_data_dir();
        let (Ok(db), Ok(vfs), Ok(source)) =
            (GameDatabase::from_install(&dir), Vfs::open_install(&dir), ntw_script::ScriptSource::from_install(&dir))
        else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let mut built = 0;
        for rec in ntw_script::ui::frontend::read_battles(&source) {
            // Records whose file is not installed (DLC, or `Austerlitz_3v3`) are locked in the menu.
            if !rec.spec.to_ascii_lowercase().ends_with(".xml") || vfs.read(&rec.spec).is_err() {
                continue;
            }
            let start = BattleStart { key: rec.key.clone(), spec: Some(rec.spec.clone()), map: None, technologies: Vec::new(), custom: Vec::new() };
            let setup = SetupData::load(Some(&vfs), None, Some(&start));
            let Some((_, spec)) = setup.battle.clone() else { panic!("{}: {} not read", rec.key, rec.spec) };
            if spec.is_naval() {
                continue;
            }
            let sim = build_battle(&db, &setup, FIRST_SEED);
            // Armies and reinforcement armies (the latter start off the field).
            let expected: usize = spec.alliances.iter().flat_map(|a| a.armies.iter().chain(&a.reinforcements)).map(|a| a.units.len()).sum();
            eprintln!("{:24} {} units, {} the player's", rec.key, sim.info.len(), sim.info.iter().filter(|i| i.controllable).count());
            assert_eq!(sim.info.len(), expected, "{}: some unit types are missing", rec.key);
            assert!(sim.info.iter().any(|i| i.controllable), "{}", rec.key);
            assert_eq!(sim.battle_key.as_deref(), Some(rec.key.as_str()));
            built += 1;
        }
        assert!(built >= 10, "only {built} historical land battles");
    }

    /// A custom battle (the front end's default Austria vs Denmark setup on `nap_mp_amazon`, as
    /// `ui_run single_player,sp_battle,button_classic_battle,button_host,button_ok` requests it):
    /// every unit is built with its experience, the human army is the player's, each side's
    /// faction names it, the general commands, and every unit stands in its side's deployment
    /// area. Skipped when the game is not installed.
    #[test]
    fn custom_battle_armies_deploy_in_their_areas() {
        use ntw_script::ui::CustomArmy;
        let dir = crate::config::game_data_dir();
        let (Ok(db), Ok(vfs)) = (GameDatabase::from_install(&dir), Vfs::open_install(&dir)) else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let map = BattleMap::load(&vfs, "nap_mp_amazon").expect("nap_mp_amazon");
        let units = |keys: &[(&str, u32)]| keys.iter().map(|(k, x)| ((*k).to_owned(), *x)).collect::<Vec<_>>();
        let custom = vec![
            CustomArmy {
                alliance: 0,
                army: 0,
                faction: "austria".into(),
                human: true,
                units: units(&[
                    ("Gen_Generals_Staff", 1),
                    ("Cav_Light_Austrian_Hungarian_Hussars", 0),
                    ("Art_Foot_Austrian_6_lber", 0),
                    ("Inf_Line_Austrian_German_Fusiliers", 3),
                    ("Inf_Militia_Austrian_Landwehr", 0),
                ]),
            },
            CustomArmy {
                alliance: 1,
                army: 0,
                faction: "denmark".into(),
                human: false,
                units: units(&[
                    ("Gen_Generals_Staff", 0),
                    ("Cav_Light_Light_Dragoons", 0),
                    ("Art_Horse_6_lber", 0),
                    ("Inf_Line_Line_Infantry", 0),
                    ("Inf_Skirm_Norwegian_Ski_Troops", 1),
                ]),
            },
        ];
        let start = BattleStart { key: "NAP_MP_Amazon".into(), spec: None, map: Some("nap_mp_amazon".into()), technologies: Vec::new(), custom };
        let setup = SetupData::load(Some(&vfs), Some(&map), Some(&start));
        let sim = build_battle(&db, &setup, FIRST_SEED);
        assert_eq!(sim.info.len(), 10);
        assert_eq!(sim.side_factions, ["austria".to_string(), "denmark".to_string()]);
        assert!(sim.battle_key.is_none(), "no battle file");
        let xp: Vec<u32> = sim.info.iter().map(|i| i.experience).collect();
        assert_eq!(xp, [1, 0, 0, 3, 0, 0, 0, 0, 0, 1]);
        for (u, i) in sim.battle.units.iter().zip(&sim.info) {
            assert_eq!(i.controllable, u.side == 0, "{}", i.key);
            assert_eq!(u.general_rank.is_some(), i.key == "Gen_Generals_Staff", "{}", i.key);
            let area = &setup.deployment.as_ref().unwrap().alliances[u.side as usize].areas[0];
            assert!(area_contains(area, u.position), "{} at {:?} outside its area", i.key, u.position);
        }
        assert!(sim.deployment_area.is_some(), "the player's area is shown");
    }

    fn area(orientation: f32, width: f32) -> DeploymentArea {
        DeploymentArea { id: 0, centre: (100.0, -50.0), width, height: 200.0, orientation }
    }

    #[test]
    fn template_deployment_turns_with_the_area_and_stays_inside() {
        use ntw_formats::group_formation::{Element, Placement, UNLIMITED};
        let el = |id, placement, class, spacing| Element {
            id,
            placement,
            priority: 1.0,
            arrangement: 0,
            spacing,
            min_units: 0,
            max_units: UNLIMITED,
            classes: vec![(group_formation::class_id(class), 1.0)],
        };
        let t = Template {
            name: "line".into(),
            priority: 1.0,
            purposes: PURPOSE_DEPLOYMENT,
            min_percent: [0; 3],
            factions: Vec::new(),
            elements: vec![
                el(0, Placement::Block { pos: (0.0, 0.0) }, "infantry_line", 2.0),
                el(1, Placement::Relative { anchor: 0, offset: (0.0, -10.0) }, "general", 2.0),
            ],
            min_units: 0,
            max_units: u32::MAX,
        };
        let sizes = [Vec2::new(40.0, 6.0), Vec2::new(40.0, 6.0), Vec2::new(10.0, 6.0)];
        let classes = ["infantry_line", "infantry_line", "general"];
        // Facing +x (orientation π/2): the line runs along y, the general is behind (smaller x).
        let a = area(std::f32::consts::FRAC_PI_2, 1000.0);
        let (name, placed) = deploy_with_templates(&a, std::slice::from_ref(&t), "france", &classes, &sizes, &[]).unwrap();
        assert_eq!(name, "line");
        assert!(placed.iter().all(|(p, _)| area_contains(&a, *p)));
        assert!((placed[0].0.1 - placed[1].0.1).abs() > 40.0, "line runs across the facing");
        assert!(placed[2].0.0 < placed[0].0.0 - 10.0, "general behind the line");
        assert!(placed.iter().all(|(_, f)| (*f - a.sim_facing()).abs() < 1e-6));
        // Everything already fits, so the group is pulled back the first 1 m only.
        assert!((placed[0].0.0 - (100.0 - 1.0)).abs() < 1e-3, "{:?}", placed[0]);
        assert!(deploy_with_templates(&a, &[], "france", &classes, &sizes, &[]).is_none());
        // A guerrilla-deployment unit forms a second group 25 m ahead (then pulled back 1 m).
        let (_, g) = deploy_with_templates(&a, &[t], "france", &["infantry_line", "infantry_line"], &sizes[..2], &[false, true]).unwrap();
        assert!((g[1].0.0 - (100.0 + 25.0 - 1.0)).abs() < 1e-3, "{:?}", g[1]);
        assert!((g[0].0.0 - (100.0 - 1.0)).abs() < 1e-3, "{:?}", g[0]);
    }

    #[test]
    fn deployment_line_faces_the_area_direction() {
        // Orientation π/2 = facing +x; "right" of that is −y.
        let sizes = [Vec2::new(40.0, 6.0), Vec2::new(60.0, 6.0)];
        let d = deploy_in_area(&area(std::f32::consts::FRAC_PI_2, 1000.0), &sizes);
        // Line width 40 + 10 + 60 = 110, centred: first unit centre at along = -55 + 20 = -35.
        let ((x0, y0), f0) = d[0];
        let ((x1, y1), _) = d[1];
        assert!(f0.abs() < 1e-5, "sim facing 0 = +x");
        assert!((y0 - (-50.0 + 35.0)).abs() < 1e-3 && (y1 - (-50.0 - 25.0)).abs() < 1e-3, "{y0} {y1}");
        // Both in the first line, half a depth behind the centre (−x).
        assert!((x0 - 97.0).abs() < 1e-3 && (x1 - 97.0).abs() < 1e-3);
    }

    #[test]
    fn area_contains_respects_orientation() {
        // Facing +x: 1000 m across (y), 200 m deep (x), centred at (100, -50).
        let a = area(std::f32::consts::FRAC_PI_2, 1000.0);
        assert!(area_contains(&a, (100.0, 400.0)));
        assert!(area_contains(&a, (190.0, -50.0)));
        assert!(!area_contains(&a, (250.0, -50.0)));
        assert!(!area_contains(&a, (100.0, 500.0)));
    }

    #[test]
    fn deployment_wraps_into_lines_behind() {
        let sizes = [Vec2::new(80.0, 6.0); 3];
        // 100 m wide area: one unit per line; facing +y (orientation 0).
        let d = deploy_in_area(&area(0.0, 100.0), &sizes);
        let ys: Vec<f32> = d.iter().map(|p| p.0.1).collect();
        assert!((ys[0] - (-53.0)).abs() < 1e-3);
        assert!((ys[1] - (-53.0 - 16.0)).abs() < 1e-3, "{ys:?}");
        assert!(d.iter().all(|p| (p.0.0 - 100.0).abs() < 1e-3));
    }
}
