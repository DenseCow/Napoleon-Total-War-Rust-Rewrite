//! [`BattleSim`], the battle as a Bevy resource. Its own module so `info` stays private: the
//! only writer is [`BattleSim::add_unit`], which keeps it in `battle.units`' order.

use super::{BattlePhase, ScriptEvent, UnitInfo};
use bevy::prelude::*;
use ntw_sim::battle::model::{Battle, LandUnit};
use ntw_sim::battle::speed::BattleSpeed;
use ntw_sim::battle::victory::{Outcome, VictoryRules};

/// A unit that can still take orders: men left, active and not gone off the field.
pub fn on_field(u: &LandUnit) -> bool {
    u.men > 0 && u.active && !u.left_field
}

/// How a selection change combines with the current selection ([`BattleSim::select`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    /// The units become the selection (a plain click, a plain box).
    Replace,
    /// The units join the selection (a box with CTRL held).
    Add,
    /// A click with CTRL held: one unit joins the selection, or leaves it if it was selected. CTRL
    /// adds: "press and hold the Ctrl key to select multiple units" (advice
    /// `advice_levels_onscreen_text_1152775702`, CONFIRMED from the shipped text). PROVISIONAL: that
    /// a CTRL click on a selected unit deselects it (the card manager's rule is not traced).
    Toggle,
}

/// `side` when it is one of the two sides the battle UI and AI are built for (0 or 1).
fn one_of_two_sides(side: u8) -> Option<u8> {
    (side <= 1).then_some(side)
}

/// The battle, as a Bevy resource: the authoritative `ntw_sim` model plus display info.
#[derive(Resource)]
pub struct BattleSim {
    /// The model. Only `tick_battle` advances it; `input` only sets orders.
    pub battle: Battle,
    /// Current battle speed (CONFIRMED set and cycle order); [`Self::set_speed`] is its only writer.
    speed: BattleSpeed,
    /// A time request was granted since the HUD last told root.lua (see [`Self::set_speed`]).
    speed_changed: bool,
    /// Speed to return to when un-pausing.
    pub speed_before_pause: BattleSpeed,
    /// Display info for every unit, in the same order as `battle.units` (private: [`Self::add_unit`] is the only writer).
    info: Vec<UnitInfo>,
    /// Names of the two sides, e.g. ["France", "Austria"].
    pub side_names: [String; 2],
    /// RNG seed this battle started from (shown in the HUD; the test key F8 restarts with seed + 1).
    pub seed: u32,
    /// The player's selected units, in the order they were selected (the original keeps the
    /// selection as lists that every `BCQ_MULTIPLE_SELECTION_ORDER_*` executor walks, e.g.
    /// `0x006D1280`; CONFIRMED). Changed only through [`Self::select`].
    selection: Vec<u32>,
    /// Faction key of each side's first army (e.g. `france`), for uniforms and music.
    pub side_factions: [String; 2],
    /// The player's alliance (the side of the player's army: a historical battle's playable army, a
    /// custom battle's human one; 0 by default), always 0 or 1: the setup keeps the player on one of
    /// the two sides the battle UI and AI are built for, the original's two. The original keeps it at the battle's `+0x281A4`
    /// and compares an army's alliance (`+0x44` → `+0x10`) with it, e.g. for the mouse-over marker
    /// (`0x00789BD0`).
    player_side: u8,
    /// The `battles` key of the historical battle being fought (None = the test slice).
    pub battle_key: Option<String>,
    /// The player's army's `camera_start_position` and `camera_target_position` from the battle
    /// file, as (x, height, y) map coordinates.
    pub camera_start: Option<([f32; 3], [f32; 3])>,
    /// The player's deployment area (map metres), if the battle has one.
    pub deployment_area: Option<ntw_formats::battle_terrain::DeploymentArea>,
    /// The battle was decided (or a harness decided it) and the player has not chosen to continue.
    /// Private: [`Self::finish`] and [`Self::continue_battle`] are its writers; [`Self::phase`]
    /// reads it.
    finished: bool,
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
    /// A number no other `BattleSim` of this run has: a restart (the test key F8) or a new battle gets a new one,
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
            speed_changed: false,
            speed_before_pause: BattleSpeed::Normal,
            info: Vec::new(),
            side_names: [String::new(), String::new()],
            seed,
            selection: Vec::new(),
            side_factions: [String::new(), String::new()],
            player_side: 0,
            battle_key: None,
            camera_start: None,
            deployment_area: None,
            finished: false,
            victory: VictoryRules::default(),
            outcome: Outcome::Ongoing,
            continued: false,
            battle_script: None,
            script_events: Vec::new(),
            wind: [0.0; 3],
            build: next_build(),
        }
    }

    /// Deployment, conflict or finished. Deployment is the model's own flag
    /// ([`Battle::deploying`]), the one source the special-ability gate also reads.
    pub fn phase(&self) -> BattlePhase {
        if self.finished {
            BattlePhase::Finished
        } else if self.battle.deploying {
            BattlePhase::Deployment
        } else {
            BattlePhase::Conflict
        }
    }

    /// Ends the deployment phase (`0x00551BA0`, [`Battle::end_deployment`]: the selected deployable
    /// defences are built); nothing once it is over.
    pub fn end_deployment(&mut self) {
        if !self.battle.deploying {
            return;
        }
        self.battle.end_deployment();
        if !self.battle.defences.is_empty() {
            info!("Battle: {} deployable defences built", self.battle.defences.len());
        }
    }

    /// The battle is decided: `outcome` is kept and the results are shown.
    pub fn finish(&mut self, outcome: Outcome) {
        self.outcome = outcome;
        self.finished = true;
    }

    /// The player keeps fighting after the result (`PostBattleDismissContinueBattle`): the battle
    /// runs again, with no victory check from then on (a battle a harness decided during
    /// deployment ends its deployment here).
    pub fn continue_battle(&mut self) {
        self.continued = true;
        self.finished = false;
        self.end_deployment();
    }

    /// The current battle speed.
    pub fn speed(&self) -> BattleSpeed {
        self.speed
    }

    /// Grants a time request: sets the speed and notes it for the HUD, which tells root.lua's
    /// `TimeRequestChanged` once per granted request (CONFIRMED 0x005E79A0; never at battle start,
    /// and also when the speed asked for is the current one).
    pub fn set_speed(&mut self, speed: BattleSpeed) {
        self.speed = speed;
        self.speed_changed = true;
    }

    /// Whether a time request was granted since the last call (and clears it), for the HUD's
    /// `TimeRequestChanged` call.
    pub fn take_speed_change(&mut self) -> bool {
        std::mem::take(&mut self.speed_changed)
    }

    /// Display info of every unit, in `battle.units` order.
    pub fn infos(&self) -> &[UnitInfo] {
        &self.info
    }

    /// Display info of a unit by id (`info` is sorted by id: a binary search).
    pub fn info_of(&self, id: u32) -> Option<&UnitInfo> {
        self.info.binary_search_by_key(&id, |i| i.id).ok().map(|i| &self.info[i])
    }

    /// The selected units, first selected first.
    pub fn selection(&self) -> &[u32] {
        &self.selection
    }

    /// The player's alliance, 0 or 1 (see the field).
    pub fn player_side(&self) -> u8 {
        self.player_side
    }

    /// Sets the player's alliance: one of the two sides the battle UI and AI are built for (the
    /// setup keeps a modded battle's player there, `BattleSpec::player_army_in`). A side past the
    /// second is a setup bug (no setup path gives one): a debug build stops; the release branch is
    /// only defensive, it logs and keeps the first side.
    pub fn set_player_side(&mut self, side: u8) {
        debug_assert!(side <= 1, "player side {side}: the setup keeps the player on the two sides");
        self.player_side = one_of_two_sides(side).unwrap_or_else(|| {
            error!("Battle: player side {side} is not one of the two sides; the player stays on side 0");
            0
        });
    }

    /// Whether the player's alliance ([`Self::player_side()`]) won: `None` while the battle is not
    /// decided, `Some(false)` for a loss and also for a draw.
    pub fn player_won(&self) -> Option<bool> {
        self.outcome.is_over().then(|| self.outcome.winner() == Some(self.player_side))
    }

    /// The other of the two sides, the one the battle AI plays (the original fights two).
    /// PROVISIONAL: the alliances past the second (a modded battle) get no AI.
    pub fn enemy_side(&self) -> u8 {
        1 - self.player_side
    }

    /// Whether unit `id` is selected.
    pub fn is_selected(&self, id: u32) -> bool {
        self.selection.contains(&id)
    }

    /// Changes the selection (the one writer). Units the player does not control and units no
    /// longer on the field are dropped; each unit is kept once.
    pub fn select(&mut self, ids: &[u32], mode: SelectMode) {
        match mode {
            SelectMode::Replace => self.selection.clear(),
            SelectMode::Add => {}
            SelectMode::Toggle => {
                if let [id] = ids
                    && let Some(at) = self.selection.iter().position(|s| s == id)
                {
                    self.selection.remove(at);
                    return;
                }
            }
        }
        for &id in ids {
            let on_field = self.battle.unit_index(id).is_some_and(|i| on_field(&self.battle.units[i]));
            if on_field && self.info_of(id).is_some_and(|i| i.controllable) && !self.selection.contains(&id) {
                self.selection.push(id);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::battle::fatigue::KvFatigue;
    use ntw_sim::battle::morale::KvMorale;

    /// Units 1–3 are the player's, unit 4 the enemy's.
    fn sim() -> BattleSim {
        let mut sim = BattleSim::new(Battle::new(1, KvMorale::default(), KvFatigue::default()), 1);
        for id in 1..=4 {
            let side = u8::from(id == 4);
            sim.add_unit(LandUnit::new(id, side, 100, (id as f32 * 30.0, 0.0)), UnitInfo { id, controllable: side == 0, ..UnitInfo::default() });
        }
        sim
    }

    /// The player's side decides who won and which side the AI plays: the player on the second
    /// alliance wins when that side does.
    #[test]
    fn the_players_side_decides_the_win_and_the_ais_side() {
        let mut sim = sim();
        assert_eq!((sim.player_won(), sim.enemy_side()), (None, 1));
        sim.outcome = ntw_sim::battle::victory::Outcome::Won { side: 1 };
        assert_eq!(sim.player_won(), Some(false));
        sim.set_player_side(1);
        assert_eq!((sim.player_won(), sim.enemy_side()), (Some(true), 0));
        sim.outcome = ntw_sim::battle::victory::Outcome::Draw;
        assert_eq!(sim.player_won(), Some(false), "a draw is not won");
    }

    /// A side past the two is a setup bug: a debug build stops on it.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "the setup keeps the player on the two sides")]
    fn a_player_side_past_the_two_is_a_setup_bug() {
        sim().set_player_side(2);
    }

    /// The release branch's check: only 0 and 1 are sides (tested in every build).
    #[test]
    fn only_the_first_two_sides_are_player_sides() {
        assert_eq!([0, 1, 2, 255].map(one_of_two_sides), [Some(0), Some(1), None, None]);
    }

    /// A release build keeps the player on the first side for a side past the two.
    #[test]
    #[cfg(not(debug_assertions))]
    fn a_release_build_keeps_a_bad_player_side_on_the_first() {
        let mut sim = sim();
        sim.set_player_side(2);
        assert_eq!((sim.player_side(), sim.enemy_side()), (0, 1));
    }

    /// A plain click or box replaces the selection; a CTRL box adds; a CTRL click toggles one unit;
    /// the enemy's units, units gone from the field and repeats are never selected.
    #[test]
    fn selection_modes() {
        let mut sim = sim();
        sim.select(&[2, 4, 2], SelectMode::Replace);
        assert_eq!(sim.selection(), [2]);
        sim.select(&[3, 1], SelectMode::Add);
        assert_eq!(sim.selection(), [2, 3, 1]);
        sim.select(&[3], SelectMode::Toggle);
        assert_eq!(sim.selection(), [2, 1], "a CTRL click on a selected unit deselects it");
        sim.select(&[3], SelectMode::Toggle);
        assert_eq!(sim.selection(), [2, 1, 3]);
        assert!(sim.is_selected(3) && !sim.is_selected(4));
        sim.select(&[], SelectMode::Replace);
        assert!(sim.selection().is_empty());
        sim.battle.units[0].left_field = true;
        sim.battle.units[1].men = 0;
        sim.select(&[1, 2, 3], SelectMode::Replace);
        assert_eq!(sim.selection(), [3], "units gone from the field are not selected");
    }
}
