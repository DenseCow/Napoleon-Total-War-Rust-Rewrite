//! [`BattleSim`], the battle as a Bevy resource. Its own module so `info` stays private: the
//! only writer is [`BattleSim::add_unit`], which keeps it in `battle.units`' order.

use super::{BattlePhase, ScriptEvent, UnitInfo};
use bevy::prelude::*;
use ntw_sim::battle::model::{Battle, LandUnit};
use ntw_sim::battle::speed::BattleSpeed;
use ntw_sim::battle::victory::{Outcome, VictoryRules};

/// The battle, as a Bevy resource: the authoritative `ntw_sim` model plus display info.
#[derive(Resource)]
pub struct BattleSim {
    /// The model. Only `tick_battle` advances it; `input` only sets orders.
    pub battle: Battle,
    /// Current battle speed (CONFIRMED set and cycle order).
    pub speed: BattleSpeed,
    /// Speed to return to when un-pausing.
    pub speed_before_pause: BattleSpeed,
    /// Display info for every unit, in the same order as `battle.units` (private: [`Self::add_unit`] is the only writer).
    info: Vec<UnitInfo>,
    /// Names of the two sides, e.g. ["France", "Austria"].
    pub side_names: [String; 2],
    /// RNG seed this battle started from (shown in the HUD; R restarts with seed + 1).
    pub seed: u32,
    /// The currently selected player (side 0) unit id, if any.
    pub selected: Option<u32>,
    /// Faction key of each side's first army (e.g. `france`), for uniforms and music.
    pub side_factions: [String; 2],
    /// The `battles` key of the historical battle being fought (None = the test slice).
    pub battle_key: Option<String>,
    /// The player's army's `camera_start_position` and `camera_target_position` from the battle
    /// file, as (x, height, y) map coordinates.
    pub camera_start: Option<([f32; 3], [f32; 3])>,
    /// The player's deployment area (map metres), if the battle has one.
    pub deployment_area: Option<ntw_formats::battle_terrain::DeploymentArea>,
    /// Deployment, conflict or finished.
    pub phase: BattlePhase,
    /// The battle file's time limit and timeout winner (none for the test slice).
    pub victory: VictoryRules,
    /// How the battle ended (Ongoing until then).
    pub outcome: Outcome,
    /// The player chose to keep fighting after the result.
    pub continued: bool,
    /// `battle_description/battle_script` of the battle file (e.g. `Arcole_Battle`).
    pub battle_script: Option<String>,
    /// The player's commands and camera inputs since the last script tick, for the battle script's
    /// command and input handlers (see `scripts`).
    pub script_events: Vec<ScriptEvent>,
    /// The battle file's `weather/prevailing_wind` as a world-space wind velocity in m/s
    /// (battle `(x, y)` -> world `(x, 0, -y)`). Used by the flag cloth (`flag`). **INFERRED**: the
    /// shipped vectors are `0`, `(0, 5)`, `(0, 9)` and `(10, 0)`, which read as metres per second,
    /// but the exe normalises the vector separately for the wind audio levels, so its own unit is
    /// not the metre. PROVISIONAL target: the exe-side wind speed the verlet item is fed.
    pub wind: [f32; 3],
    /// A number no other `BattleSim` of this run has: a restart (R) or a new battle gets a new one,
    /// even with the same unit ids, so the views of one battle are never reused for another.
    pub build: u32,
}

/// The next [`BattleSim::build`] number.
pub(super) fn next_build() -> u32 {
    static BUILDS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl BattleSim {
    /// A battle with no units yet, at normal speed (units come in through [`Self::add_unit`], which
    /// keeps their display info in step, so a battle that already has some is refused).
    pub fn new(battle: Battle, seed: u32) -> Self {
        assert!(battle.units.is_empty(), "BattleSim::new takes a battle with no units; add them with BattleSim::add_unit");
        Self {
            battle,
            speed: BattleSpeed::Normal,
            speed_before_pause: BattleSpeed::Normal,
            info: Vec::new(),
            side_names: [String::new(), String::new()],
            seed,
            selected: None,
            side_factions: [String::new(), String::new()],
            battle_key: None,
            camera_start: None,
            deployment_area: None,
            phase: BattlePhase::Conflict,
            victory: VictoryRules::default(),
            outcome: Outcome::Ongoing,
            continued: false,
            battle_script: None,
            script_events: Vec::new(),
            wind: [0.0; 3],
            build: next_build(),
        }
    }

    /// Display info of every unit, in `battle.units` order.
    pub fn infos(&self) -> &[UnitInfo] {
        &self.info
    }

    /// Display info of a unit by id (`info` is sorted by id: a binary search).
    pub fn info_of(&self, id: u32) -> Option<&UnitInfo> {
        self.info.binary_search_by_key(&id, |i| i.id).ok().map(|i| &self.info[i])
    }

    /// Adds a unit and its display info: `info` goes to the index the model's
    /// [`Battle::add_unit`] gives the unit (sorted by id), so `info` keeps `battle.units`' order
    /// whatever order units are added in (a reinforcement with a lower id included).
    pub fn add_unit(&mut self, unit: LandUnit, info: UnitInfo) {
        debug_assert_eq!(unit.id, info.id);
        // `info` is only ever written here, so it is always as long as `battle.units`: a bare
        // `sim.battle.add_unit` would put it out of step, and the index below would be wrong.
        assert_eq!(self.info.len(), self.battle.units.len(), "a unit was added to the model without its info");
        let at = self.battle.units.partition_point(|u| u.id < unit.id);
        self.battle.add_unit(unit);
        self.info.insert(at, info);
    }

    /// Display info of unit `id`, expected at `slot` (its index in `battle.units`; [`add_unit`]
    /// keeps `info` in that order): O(1) while that slot holds it, else found by id.
    ///
    /// [`add_unit`]: Self::add_unit
    pub fn info_at(&self, slot: usize, id: u32) -> Option<&UnitInfo> {
        self.info.get(slot).filter(|i| i.id == id).or_else(|| self.info_of(id))
    }

    /// Every model unit with its display info, paired by id (a unit with no info is skipped).
    pub fn units_with_info(&self) -> impl Iterator<Item = (&LandUnit, &UnitInfo)> {
        self.battle.units.iter().enumerate().filter_map(|(i, u)| Some((u, self.info_at(i, u.id)?)))
    }

    /// Model unit `id` (units are sorted by id: a binary search).
    pub fn unit(&self, id: u32) -> Option<&LandUnit> {
        self.battle.unit_index(id).map(|i| &self.battle.units[i])
    }

    /// Model unit `id` with its display info.
    pub fn unit_with_info(&self, id: u32) -> Option<(&LandUnit, &UnitInfo)> {
        let i = self.battle.unit_index(id)?;
        Some((&self.battle.units[i], self.info_at(i, id)?))
    }
}
