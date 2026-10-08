//! # Battle AI v1
//!
//! Controls one or more sides (alliances) of an [`ntw_sim::battle::model::Battle`]. It **reads**
//! the model and gives orders **only** through the model's public order functions
//! (`order_move`, `order_fire`, `order_halt`, `order_fire_at_will`, `order_attack_unit`,
//! `order_end_charge`, `order_hold_position`). It uses no randomness, no clock and no hash maps,
//! so the same battle gives the same orders on every machine (replays and lockstep multiplayer).
//!
//! ## Structure (mirrors the original, `analysis/ai/AI_RESEARCH.md` §3)
//! The original's battle AI (`EmpireBattle\Source\AI\BattleAI.cpp`, `HighLevelPlanner.cpp`,
//! `MeleeManager\MeleeAnalysers\*`, CONFIRMED source paths) has, per alliance:
//! - a **high-level planner** that splits the army into unit groups ("ug:" names, CONFIRMED) and
//!   gives each group a **tactic** with its own state machine. Tactic names and FSM state names are
//!   CONFIRMED from debug strings, e.g. `AI_TACTIC_ATTACK_BATTLEGROUP` with
//!   {Change Formation, Reform, Move to Form-up, Move to Target, Outflank},
//!   `AI_TACTIC_DEFEND_ABSTRACT` with {Change Formation, Change Unit Formations, Reform, Defend
//!   Line}, `AI_TACTIC_STOP_AND_SHOOT` (tracks its "efficiency" = kills per tick and its
//!   gradient), `AI_TACTIC_OUTFLANK` {Approaching, Outflanking};
//! - a **melee manager** whose analysers turn enemy units into MELEE / MISSILE / RETREAT
//!   objectives, allocated to our units ([`melee_manager`]).
//!
//! Here, v1 uses two groups per alliance: the **line** (infantry and artillery) running
//! ATTACK_BATTLEGROUP or DEFEND_LINE, with STOP_AND_SHOOT for its musket units, and the **cavalry
//! wings** running OUTFLANK. The melee manager overrides a group's order for any unit it assigns.
//!
//! ## What is PROVISIONAL
//! Every distance, timer and threshold named `*_PROVISIONAL`-style in [`AiParams`] is our own
//! stand-in, because the original's values have not been found yet (the battle AI has **no DB
//! table**, CONFIRMED by the table list; its numbers live in the exe). The formation shape is
//! ours too (the original reads `GroupFormations.bin`, not decoded yet).

pub mod auction;
pub mod classes;
pub mod geom;
pub mod melee_manager;
pub mod outflank;
pub mod phase;
pub mod plan;
pub mod rating;

use std::collections::BTreeMap;

use ntw_sim::battle::model::{Battle, LandUnit, MELEE_CONTACT_RANGE};

use geom::{P, add, centroid, dist, dot, norm_or, right_of, rotate, scale, sub};
use melee_manager::{Assignments, Candidate, ObjectiveKind, allocate};
use rating::{AttackSide, attack_side, balance, engaged, melee_base_priority, melee_potential};

/// Tuning numbers. Every default is PROVISIONAL unless its doc says CONFIRMED.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiParams {
    /// Ticks between two "thinks" of one alliance (1 s). CONFIRMED: the alliance AI update
    /// (`0x007B25A0`) runs its planner steps when its update counter `% 10 == 1`, and it runs once per
    /// battle tick (caller chain `0x0057DF40` → `0x00580640` → ... → `0x005832C0`, AI_RESEARCH §3.3).
    pub think_interval_ticks: u32,
    /// Routing enemies farther than this from our army are not chased (PROVISIONAL: stands in for
    /// the map edge routers leave by).
    pub pursuit_radius: f32,
    /// CONFIRMED count (`0x007DB360`: `counter % 30 == 0`): melee objectives are re-allocated every
    /// 30 alliance-AI updates (= ticks, CONFIRMED as above).
    pub melee_replan_ticks: u32,
    /// CONFIRMED (`0x00791FD0`, `< 2500` squared metres): a melee attacker this close is
    /// force-assigned to its target.
    pub force_melee_distance: f32,
    /// CONFIRMED number (`0x007D05F0`, `5625` = 75 m squared) used as the infantry melee reach.
    /// Its exact role in the original is INFERRED.
    pub infantry_melee_reach: f32,
    /// How far cavalry looks for melee targets while outflanking. PROVISIONAL.
    pub cavalry_search_radius: f32,
    /// How far cavalry looks for melee targets while waiting on the wings. PROVISIONAL.
    pub cavalry_reserve_radius: f32,
    /// How far assaulting infantry goes after a target. PROVISIONAL.
    pub assault_reach: f32,
    /// How far a shaken unit falls back. PROVISIONAL.
    pub fall_back_distance: f32,
    /// Charge when this close (cavalry). PROVISIONAL.
    pub cavalry_charge_distance: f32,
    /// Charge when this close (infantry). PROVISIONAL.
    pub infantry_charge_distance: f32,
    /// Ticks after contact before the charge flag is dropped. PROVISIONAL (the original's charge
    /// timer is morale component `[0x1A]`, length UNKNOWN).
    pub charge_hold_ticks: u32,
    /// Minimum melee potential (0..1) for a cavalry charge. PROVISIONAL.
    pub min_melee_potential: f32,
    /// A slot counts as reached within this distance. PROVISIONAL.
    pub slot_tolerance: f32,
    /// "Formed=%u%%": CONFIRMED threshold 60 % (`0x00797100`: formed when `tactic+0x7E0 >= 60`).
    /// Our measure of it (units holding their place in the line's shape) is PROVISIONAL.
    pub formed_fraction: f32,
    /// Give up waiting for the formation after this many ticks. PROVISIONAL.
    pub form_timeout_ticks: u32,
    /// Musket units stop at this fraction of their range (same as the model's own advance stop).
    pub shoot_range_fraction: f32,
    /// CONFIRMED (`0x007CE370`, `< 400` squared): ADVANCING_TOWARDS_LINE ends when the group is
    /// within 20 m of the firing line's midpoint.
    pub shoot_line_arrive_distance: f32,
    /// How close a STOP_AND_SHOOT unit goes to its place on the line before it stops. PROVISIONAL.
    pub line_move_tolerance: f32,
    /// CONFIRMED (`0x0085BDC0`): the creep line puts the shortest-ranged shooter this many metres
    /// inside its range.
    pub creep_inside_range: f32,
    /// CONFIRMED (`0x00796DF0`): the hold is not efficient once the latest kill sample is more than
    /// 200 ticks old.
    pub efficiency_stale_ticks: u32,
    /// CONFIRMED (`0x007D9190`): the baseline sample moves up to a new kill sample when it is more
    /// than 100 ticks older.
    pub efficiency_baseline_ticks: u32,
    /// CONFIRMED (`0x00796DF0`): efficient while the mean kills per unit grow by at least 0.01 per
    /// tick between the two samples.
    pub efficiency_min_rate: f32,
    /// STOP_AND_SHOOT's planner score (`0x007B1BD0`) needs the group's virtual `+0x1C` to be at
    /// least 70 (CONFIRMED value; INFERRED to be the formed percentage).
    pub stop_and_shoot_min_formed: f32,
    /// CONFIRMED (`0x007B5C00`, `formed percent > 49`): a defending line below this formed
    /// fraction goes back to REFORM.
    pub defend_reform_below: f32,
    /// The withdraw test's own flags (`0x0076A230`: every army's `+0xD4→+0xCC` and `+0x2D4 == 0`,
    /// both UNKNOWN). PROVISIONAL: false (the AI never withdraws; the model has no map edge).
    pub withdraw_allowed: bool,
    /// An enemy this close on a flank or rear makes a unit turn to face it. PROVISIONAL.
    pub flank_threat_distance: f32,
    /// Gap between units in the line, metres. PROVISIONAL.
    pub line_gap: f32,
    /// Distance from the enemy line to the form-up point, beyond the longest musket range. PROVISIONAL.
    pub form_up_extra_distance: f32,
    /// CONFIRMED number (`0x007A5730`: `160.0 < tactic+0x80C` and `+0x808`): the line only goes
    /// back to a form-up point while the target is farther than this (INFERRED: metres to the target).
    pub form_up_min_distance: f32,
    /// CONFIRMED number (`0x007B5640`: `tactic+0x80C <= 140.0`): within this distance of the
    /// target an unformed or badly facing line reforms (INFERRED meaning as above).
    pub reform_distance: f32,
    /// CONFIRMED number (`0x007B5AF0` / `0x007B5640`: `|Δangle| < 0x2AAC` of 0x10000 = 60°): the
    /// line faces its target well enough within this angle (degrees).
    pub facing_tolerance_deg: f32,
    /// Clearance an outflank point keeps from every enemy unit. PROVISIONAL (the original's point
    /// tests `0x007A6510` / `0x007A61F0` are not decoded).
    pub outflank_clearance: f32,
}

impl Default for AiParams {
    fn default() -> Self {
        AiParams {
            think_interval_ticks: 10,
            melee_replan_ticks: 30,
            pursuit_radius: 300.0,
            force_melee_distance: 50.0,
            infantry_melee_reach: 75.0,
            cavalry_search_radius: 400.0,
            cavalry_reserve_radius: 150.0,
            assault_reach: 200.0,
            fall_back_distance: 60.0,
            cavalry_charge_distance: 50.0,
            infantry_charge_distance: 30.0,
            charge_hold_ticks: 30,
            min_melee_potential: 0.55,
            slot_tolerance: 8.0,
            formed_fraction: 0.6,
            form_timeout_ticks: 300,
            shoot_range_fraction: 0.9,
            shoot_line_arrive_distance: 20.0,
            line_move_tolerance: 1.0,
            creep_inside_range: 3.0,
            efficiency_stale_ticks: 200,
            efficiency_baseline_ticks: 100,
            efficiency_min_rate: 0.01,
            stop_and_shoot_min_formed: 0.7,
            defend_reform_below: 0.5,
            withdraw_allowed: false,
            flank_threat_distance: 80.0,
            line_gap: 12.0,
            form_up_extra_distance: 60.0,
            form_up_min_distance: 160.0,
            reform_distance: 140.0,
            facing_tolerance_deg: 60.0,
            outflank_clearance: 100.0,
        }
    }
}

/// The tactic of the line group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTactic {
    /// `AI_TACTIC_ATTACK_BATTLEGROUP` (CONFIRMED name).
    Attack(AttackState),
    /// `AI_TACTIC_DEFEND_LINE` / `AI_TACTIC_DEFEND_ABSTRACT` (CONFIRMED names).
    Defend(DefendState),
    /// `AI_TACTIC_STOP_AND_SHOOT` (CONFIRMED name and FSM) for the line's muskets.
    StopAndShoot(ShootState),
}

/// `AI_TACTIC_ATTACK_BATTLEGROUP` FSM states with the original's numbers (CONFIRMED, `0x00765540`
/// reads the state at tactic `+0x828`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum AttackState {
    /// 0 "Change Formation" (not used by v1).
    ChangeFormation = 0,
    /// 2 "Reform".
    Reform = 2,
    /// 3 "Move to Form-up".
    MoveToFormUp = 3,
    /// 4 "Move to Target".
    MoveToTarget = 4,
    /// 5 "Outflank" (v1 runs outflanking in the cavalry group instead).
    Outflank = 5,
}

/// `AI_TACTIC_DEFEND_ABSTRACT` FSM states (CONFIRMED, `0x007659C0` reads tactic `+0xAC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum DefendState {
    /// 0 "Change Formation" (not used by v1).
    ChangeFormation = 0,
    /// 1 "Change Unit Formations" (not used by v1).
    ChangeUnitFormations = 1,
    /// 2 "Reform".
    Reform = 2,
    /// 3 "Defend Line".
    DefendLine = 3,
}

/// `AI_TACTIC_OUTFLANK` states (CONFIRMED strings "Approaching" / "Outflanking").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutflankState {
    /// Waiting on the wings of the line.
    Approaching,
    /// Riding round the enemy flanks.
    Outflanking,
}

/// `AI_TACTIC_STOP_AND_SHOOT` FSM states (CONFIRMED names and transitions from the state
/// descriptors, AI_RESEARCH §3.2; vtable `0x01344CA4`, transition slots `+0x78..+0x84`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShootState {
    /// ADVANCING_TOWARDS_LINE (`+0x78`): move to the firing line.
    AdvancingTowardsLine,
    /// FORM_ON_LINE (`+0x7C`): form up on the firing line.
    FormOnLine,
    /// CREEP_FORWARD (`+0x80`): close in so the shortest-ranged shooter is 3 m inside range.
    CreepForward,
    /// HOLD_THE_LINE (`+0x84`): stand and fire while the kills keep coming.
    HoldTheLine,
}

/// `STOP AND SHOOT EFFICIENCY ( kills,tick ) : Current ( %u, %u ) ::: Previous ( %u , %u ) :::
/// Gradient ( %f )` (CONFIRMED log format, `0x00766E30`). The tactic's `+0x9C` sample pair
/// `{kills0, tick0, kills1, tick1}` (CONFIRMED layout): a sample is the group's mean kill count per
/// unit (integer) and the battle tick (`0x00762E20`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Efficiency {
    /// (mean kills, tick) of the baseline ("Previous").
    pub previous: (u32, u32),
    /// (mean kills, tick) of the latest rise ("Current").
    pub current: (u32, u32),
}

impl Efficiency {
    /// HOLD_THE_LINE entry (`0x00795940`): both samples set to now.
    pub fn reset(&mut self, kills: u32, tick: u32) {
        self.previous = (kills, tick);
        self.current = (kills, tick);
    }

    /// The sampler run on every tactic update (`0x007D9190`, CONFIRMED): only a rise of the mean
    /// kills is recorded; the baseline moves up with it once it is more than `baseline_ticks` old.
    pub fn sample(&mut self, kills: u32, tick: u32, baseline_ticks: u32) {
        if kills > self.current.0 {
            self.current = (kills, tick);
            if self.previous.1.wrapping_add(baseline_ticks) < tick {
                self.previous = (kills, tick);
            }
        }
    }

    /// `0x00796DF0` (CONFIRMED): not efficient once the latest rise is more than `stale_ticks`
    /// old; otherwise efficient while the rate between the two samples is at least `min_rate`
    /// (and always when both are the same tick).
    pub fn efficient(&self, now: u32, stale_ticks: u32, min_rate: f32) -> bool {
        if now.wrapping_sub(self.current.1) > stale_ticks {
            return false;
        }
        if self.current.1 == self.previous.1 {
            return true;
        }
        let dk = self.current.0.wrapping_sub(self.previous.0) as f32;
        let dt = self.current.1.wrapping_sub(self.previous.1) as f32;
        dk / dt >= min_rate
    }

    /// The display's gradient (kills per tick between the samples).
    pub fn gradient(&self) -> f32 {
        let dt = self.current.1.wrapping_sub(self.previous.1);
        if dt == 0 { 0.0 } else { self.current.0.wrapping_sub(self.previous.0) as f32 / dt as f32 }
    }
}

/// What one of our units is doing (for display and tests).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnitTask {
    /// Following its group slot.
    Slot(P),
    /// Holding still to shoot (stop and shoot).
    StopAndShoot,
    /// Turning to face a threat on its flank / rear.
    FaceThreat(u32),
    /// A melee manager objective.
    Objective(Candidate),
}

/// The AI of one alliance.
#[derive(Debug, Clone, PartialEq)]
pub struct AllianceAi {
    /// The side it controls.
    pub side: u8,
    /// Line group tactic and FSM state.
    pub line: LineTactic,
    /// Tick the current line state started.
    pub line_since: u32,
    /// Cavalry group state.
    pub cavalry: OutflankState,
    /// Line anchor (centre of the front) and facing direction (towards the enemy).
    pub anchor: P,
    /// Unit vector towards the enemy.
    pub dir: P,
    /// Stop-and-shoot efficiency.
    pub efficiency: Efficiency,
    /// The line group's encounter phase (`phase`, CONFIRMED rules), updated every think.
    pub phase: phase::Phase,
    /// Each tactic's ten pre-drawn random values (`+0x18..+0x3C`, read by `0x007CD510` /
    /// `0x007CD530`), in the battlegroup's tactic order (ATTACK_BATTLEGROUP, OUTFLANK,
    /// DOUBLE_ENVELOPMENT, STOP_AND_SHOOT, LIMBERED_ARTILLERY, GENERAL_SUPPORT). CONFIRMED
    /// (`0x0070AED0`): every tactic draws its ten at construction, `x = x × 0x343FD + 0x269EC3`,
    /// `x >> 16`, from an LCG held at `+0x50` of an object INFERRED to be the battle's RNG.
    /// PROVISIONAL: drawn from a copy of the battle RNG (ours does not advance it).
    pub seeds: Option<[[u32; 10]; 6]>,
    /// True once stop-and-shoot has ended and the line goes in with the bayonet.
    pub assault: bool,
    /// Latest melee-manager assignments.
    pub assignments: Assignments,
    /// Latest task per unit.
    pub tasks: BTreeMap<u32, UnitTask>,
    /// Tick each unit entered melee (for ending the charge).
    pub melee_since: BTreeMap<u32, u32>,
    /// Where each falling-back unit rallies (fixed when it starts to fall back).
    pub rally: BTreeMap<u32, P>,
    /// Latest balance (own / (own + enemy) strength).
    pub balance: f32,
    /// Tick an enemy last came within shooting distance (stalemate check).
    pub last_contact_tick: u32,
    /// `tactic+0x823` (CONFIRMED flag): the outflank decision (`0x007A5C80`) is taken once per
    /// attack.
    pub outflank_checked: bool,
    /// The point the line outflanks to (state 5).
    pub outflank_point: Option<P>,
    /// Tick of the latest melee re-allocation.
    pub last_melee_plan: Option<u32>,
    /// Tick of the latest think, if any.
    pub last_think: Option<u32>,
    /// STOP_AND_SHOOT: the firing line's midpoint and facing (tactic `+0xAC..+0xC0`).
    pub shoot_line: Option<(P, P)>,
    /// DEFEND_ABSTRACT: the units of the line when it last reformed (a unit leaving it makes the
    /// line reform, `0x0076D870`).
    pub line_members: Vec<u32>,
    /// Our battle alliance's `+0x70` (INFERRED: the battle's defender). The battle model has no
    /// attacker / defender yet: PROVISIONAL default false (set it with [`BattleAi::set_defender`]).
    pub defender: bool,
    /// The high-level plan vote's state (`plan`, CONFIRMED rules).
    pub plan: plan::PlanState,
    /// The alliance mode (`+0x61C`) of the latest plan.
    pub mode: plan::Mode,
    /// Strength blocks (ours, enemy's) at the first update (`+0x574..` / `+0x5AC..`).
    pub initial_blocks: Option<(plan::Block, plan::Block)>,
    /// Tick of the first update (the update counter `+0x634` counts from it).
    pub start_tick: Option<u32>,
    /// The battlegroup's tactics and the units they own (the auction, `auction`).
    pub tactics: Vec<auction::Tactic>,
    /// Units on a wing tactic (OUTFLANK / DOUBLE_ENVELOPMENT) and their side (+1 right, −1 left).
    pub wing_side: BTreeMap<u32, f32>,
    /// The side the active OUTFLANK took.
    pub outflank_sign: Option<f32>,
    /// The point the active OUTFLANK took (`0x007AE2C0`, tactic `+0x94/+0x98`).
    pub outflank_target: Option<P>,
}

/// The battle AI: one [`AllianceAi`] per controlled side.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleAi {
    /// Tuning numbers.
    pub params: AiParams,
    /// Controlled alliances, in side order.
    pub alliances: Vec<AllianceAi>,
}

impl BattleAi {
    /// An AI controlling the given sides with default parameters.
    pub fn new(sides: &[u8]) -> Self {
        Self::with_params(sides, AiParams::default())
    }

    /// An AI controlling the given sides.
    pub fn with_params(sides: &[u8], params: AiParams) -> Self {
        let mut sides: Vec<u8> = sides.to_vec();
        sides.sort_unstable();
        sides.dedup();
        BattleAi {
            params,
            alliances: sides
                .into_iter()
                .map(|side| AllianceAi {
                    side,
                    line: LineTactic::Attack(AttackState::Reform),
                    line_since: 0,
                    cavalry: OutflankState::Approaching,
                    anchor: (0.0, 0.0),
                    dir: (0.0, 1.0),
                    efficiency: Efficiency::default(),
                    phase: phase::Phase::None,
                    seeds: None,
                    assault: false,
                    assignments: Assignments::new(),
                    tasks: BTreeMap::new(),
                    melee_since: BTreeMap::new(),
                    rally: BTreeMap::new(),
                    balance: 0.5,
                    last_contact_tick: 0,
                    last_think: None,
                    outflank_checked: false,
                    outflank_point: None,
                    last_melee_plan: None,
                    shoot_line: None,
                    line_members: Vec::new(),
                    defender: false,
                    plan: plan::PlanState::default(),
                    mode: plan::Mode::None,
                    initial_blocks: None,
                    start_tick: None,
                    tactics: auction::new_tactics(),
                    wing_side: BTreeMap::new(),
                    outflank_sign: None,
                    outflank_target: None,
                })
                .collect(),
        }
    }

    /// Call once per model tick, **before** `battle.step()`. Each alliance thinks every
    /// `think_interval_ticks` (its first think happens on the first call: deployment).
    pub fn update(&mut self, battle: &mut Battle) {
        let params = self.params;
        for ai in &mut self.alliances {
            let due = ai
                .last_think
                .is_none_or(|t| battle.tick.wrapping_sub(t) >= params.think_interval_ticks);
            if due {
                ai.think(battle, &params);
                ai.last_think = Some(battle.tick);
            }
        }
    }

    /// Marks `side` as the battle's defender or not: the battle alliance's `+0x70` (CONFIRMED round 6:
    /// set by the alliance setup `0x005055E0` to "this alliance's index == battle setup `+0x88`",
    /// and the script condition `BattleAllianceIsAttacker` (`0x0052A280`, "the attacker from the
    /// campaign map") is true for the alliance without it when the other has it; `+0x88 == −1`:
    /// no defender). Campaign battles set the defending side; historical battles have no defender
    /// field (INFERRED `+0x88 = −1`), so nobody holds.
    pub fn set_defender(&mut self, side: u8, defender: bool) {
        for a in self.alliances.iter_mut().filter(|a| a.side == side) {
            a.defender = defender;
        }
    }

    /// The AI of `side`, if controlled.
    pub fn alliance(&self, side: u8) -> Option<&AllianceAi> {
        self.alliances.iter().find(|a| a.side == side)
    }
}

/// Unit roles used by the planner. INFERRED from the original's "Category:" / "Class:" debug
/// strings; v1 only needs these three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// Foot units (line, light, grenadiers, ...).
    Infantry,
    /// Mounted units.
    Cavalry,
    /// Guns.
    Artillery,
}

/// The role of a model unit.
pub fn role(u: &LandUnit) -> Role {
    if u.missile.is_some_and(|w| w.is_artillery) {
        Role::Artillery
    } else if u.is_cavalry {
        Role::Cavalry
    } else {
        Role::Infantry
    }
}

/// True if the AI can give the unit orders: on the field, with men, not routing.
fn controllable(u: &LandUnit) -> bool {
    u.active && u.men > 0 && !u.morale.is_routing_or_shattered() && !u.script_controlled
}

/// PROVISIONAL frontage of a unit in metres (the drawn formation is not part of the model).
fn frontage(u: &LandUnit) -> f32 {
    match role(u) {
        Role::Infantry => (u.max_men as f32 / 4.0).max(10.0),
        Role::Cavalry => (u.max_men as f32 / 2.0 * 1.6).max(10.0),
        Role::Artillery => 25.0,
    }
}

impl AllianceAi {
    fn own<'a>(&self, b: &'a Battle) -> impl Iterator<Item = &'a LandUnit> + 'a {
        let side = self.side;
        b.units.iter().filter(move |u| u.side == side)
    }

    fn enemies<'a>(&self, b: &'a Battle) -> impl Iterator<Item = &'a LandUnit> + 'a {
        let side = self.side;
        b.units.iter().filter(move |u| u.side != side && u.men > 0 && u.active)
    }

    /// Human-readable state, in the style of the original's debug lines.
    pub fn status(&self) -> String {
        let line = match self.line {
            LineTactic::Attack(s) => format!("AI_TACTIC_ATTACK_BATTLEGROUP::FSM={}", attack_name(s)),
            LineTactic::Defend(s) => format!("AI_TACTIC_DEFEND_ABSTRACT::FSM={}", defend_name(s)),
            LineTactic::StopAndShoot(s) => format!(
                "AI_TACTIC_STOP_AND_SHOOT::FSM={}::EFFICIENCY ( kills,tick ) : Current ( {}, {} ) ::: Previous ( {} , {} ) ::: Gradient ( {:.4} )",
                shoot_name(s),
                self.efficiency.current.0,
                self.efficiency.current.1,
                self.efficiency.previous.0,
                self.efficiency.previous.1,
                self.efficiency.gradient()
            ),
        };
        let cav = match self.cavalry {
            OutflankState::Approaching => "Approaching",
            OutflankState::Outflanking => "Outflanking",
        };
        format!(
            "Alliance {}::PLAN={}::MODE={:?}::{line}::AI_TACTIC_OUTFLANK::{cav}::AUCTION={}{}::balance={:.2}",
            self.side,
            self.plan.plan,
            self.mode,
            self.tactics.iter().filter(|t| t.active).map(|t| format!("{:?}:{}", t.kind, t.units.len())).collect::<Vec<_>>().join(","),
            if self.assault { "::ASSAULT" } else { "" },
            self.balance
        )
    }

    fn set_line(&mut self, t: LineTactic, tick: u32) {
        if self.line != t {
            // A new attack (from a defence) may outflank again (`+0x823` belongs to the tactic).
            if matches!(self.line, LineTactic::Defend(_)) && matches!(t, LineTactic::Attack(_)) {
                self.outflank_checked = false;
                self.outflank_point = None;
            }
            self.line = t;
            self.line_since = tick;
        }
    }

    /// A strength block (`0x007CE410`, CONFIRMED fields) of our side (`ours`) or of the enemy.
    /// The tracker holds the side's active units (`+0x1E4`, INFERRED "on the field"); units with
    /// the flag `+0x278D` (INFERRED routing) count half in the missile and total sums (and, as
    /// INFERRED for `0x0079C1E0`, in the melee sum).
    fn block(&self, b: &Battle, ours: bool) -> plan::Block {
        let (mut men, mut melee, mut missile, mut inf, mut mounted, mut total) = (0i64, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for u in b.units.iter().filter(|u| u.active && (u.side == self.side) == ours) {
            let half = if u.morale.is_routing_or_shattered() { 0.5 } else { 1.0 };
            let (me, mi) = (rating::melee_rating(u), rating::missile_rating(u));
            men += u.men as i64;
            melee += me * half;
            missile += mi * half;
            total += (me + mi) * half;
            let (cat, _) = classes::codes(u);
            if cat == classes::category::INFANTRY {
                inf += me + mi;
            }
            if classes::is_mounted(cat) {
                mounted += me + mi;
            }
        }
        plan::Block {
            men: men as i32,
            melee: melee.round() as i32,
            missile: missile.round() as i32,
            infantry: inf.round() as i32,
            mounted: mounted.round() as i32,
            total: total.round() as i32,
        }
    }

    /// The high-level plan and the alliance mode (§3.1c). Defence modes (3 defend, 9 withdraw)
    /// put the line on DEFEND_ABSTRACT, the attack modes on ATTACK_BATTLEGROUP; plan 3 and the
    /// search mode keep the line as it is. PROVISIONAL: an attacking line already in melee is not
    /// pulled back to defend (the original swaps the objectives; how it frees engaged units is
    /// not decoded), and withdraw is played as defend (no map edge to leave by).
    fn run_plan(&mut self, b: &Battle, p: &AiParams, first: bool, tick: u32) {
        use plan::Mode;
        let Some((oi, ei)) = self.initial_blocks else { return };
        let ours = self.block(b, true);
        let enemy = self.block(b, false);
        let our_units: Vec<(u8, bool, bool)> = self
            .own(b)
            .filter(|u| u.active)
            .map(|u| {
                let (_, class) = classes::codes(u);
                (class, class == classes::class::GENERAL, u.can_shoot())
            })
            .collect();
        let their_units: Vec<(bool, bool)> =
            self.enemies(b).filter(|u| u.active).map(|u| (classes::is_mounted(classes::codes(u).0), u.can_shoot())).collect();
        let flags = plan::Flags {
            defender: self.defender,
            withdraw_allowed: p.withdraw_allowed,
            // PROVISIONAL: only our own defender flag is known here.
            composition_defends: plan::composition_defends(self.defender, &our_units, &their_units),
            ..plan::Flags::default()
        };
        let updates = tick.wrapping_sub(self.start_tick.unwrap_or(tick)).wrapping_add(1);
        let chosen = plan::vote(&mut self.plan, &flags, (&oi, &ei), (&ours, &enemy), updates);
        self.mode = plan::mode_for(chosen, self.mode, first);
        let defend = matches!(self.mode, Mode::Defend | Mode::Withdraw);
        let attack = matches!(self.mode, Mode::Attack | Mode::AttackTarget | Mode::Assault | Mode::Mode11 | Mode::Mode13);
        if first {
            self.line = if defend { LineTactic::Defend(DefendState::Reform) } else { LineTactic::Attack(AttackState::Reform) };
            self.line_since = tick;
        } else if defend && !matches!(self.line, LineTactic::Defend(_)) {
            let line_in_melee = self.own(b).any(|u| controllable(u) && role(u) != Role::Cavalry && u.in_melee);
            if !line_in_melee {
                self.assault = false;
                self.shoot_line = None;
                self.set_line(LineTactic::Defend(DefendState::Reform), tick);
            }
        } else if attack && matches!(self.line, LineTactic::Defend(_)) {
            self.set_line(LineTactic::Attack(AttackState::MoveToFormUp), tick);
        }
    }

    fn think(&mut self, b: &mut Battle, p: &AiParams) {
        let tick = b.tick;
        let first = self.last_think.is_none();
        let own_pos: Vec<P> = self.own(b).filter(|u| controllable(u)).map(|u| u.position).collect();
        let Some(own_c) = centroid(own_pos.iter().copied()) else { return };
        let enemy_pos: Vec<P> = self
            .enemies(b)
            .filter(|u| !u.morale.is_routing_or_shattered())
            .map(|u| u.position)
            .collect();
        let Some(enemy_c) = centroid(enemy_pos.iter().copied()) else {
            // No fighting enemy left: stand.
            return;
        };
        self.balance = balance(b, self.side);

        if first {
            // Deployment: every unit holds its ground unless ordered, and fires at will.
            let ids: Vec<u32> = self.own(b).map(|u| u.id).collect();
            for id in ids {
                b.order_hold_position(id, true);
                b.order_fire_at_will(id, true);
            }
            self.start_tick = Some(tick);
            self.initial_blocks = Some((self.block(b, true), self.block(b, false)));
            self.line_since = tick;
            self.last_contact_tick = tick;
            self.anchor = own_c;
        }
        // The high-level plan (`0x007D7020` → `0x007B1FB0`, CONFIRMED vote) every 10th update and
        // while deploying; our think runs every 10 ticks. Its mode picks attack or defence.
        self.run_plan(b, p, first, tick);

        // Facing: from our line towards the enemy centre. Keep the last direction if degenerate.
        self.dir = norm_or(sub(enemy_c, own_c), self.dir);
        let line_ids: Vec<u32> = self
            .own(b)
            .filter(|u| controllable(u) && role(u) != Role::Cavalry && !self.wing_side.contains_key(&u.id))
            .map(|u| u.id)
            .collect();
        let max_range = self
            .own(b)
            .filter(|u| controllable(u) && role(u) == Role::Infantry && u.can_shoot())
            .map(|u| u.missile_range(b.kv_rules.fire_on_walls_range_modifier))
            .fold(0.0f32, f32::max);
        let nearest_enemy_dist = self
            .own(b)
            .filter(|u| controllable(u))
            .flat_map(|u| enemy_pos.iter().map(move |e| dist(u.position, *e)))
            .fold(f32::INFINITY, f32::min);
        if nearest_enemy_dist <= max_range.max(MELEE_CONTACT_RANGE) * 1.5 {
            self.last_contact_tick = tick;
        }

        self.update_efficiency(b, p, &line_ids);
        self.update_line_fsm(b, p, own_c, enemy_c, max_range, &line_ids);
        if let (LineTactic::StopAndShoot(_), Some((_, facing))) = (self.line, self.shoot_line) {
            // The firing line keeps the facing it was given (the entries order the group's facing).
            self.dir = facing;
        }
        self.update_cavalry(b, p);

        // Slots for the line group and the cavalry wings.
        let slots = self.slots(b, p, enemy_c);
        // The melee manager.
        // `0x007DB360` (CONFIRMED): the melee manager clears and re-allocates every objective on
        // every 30th update of the alliance AI; in between, units keep their objectives.
        // PROVISIONAL: an objective whose target is gone (dead, routed, out of range for a
        // missile objective) or whose unit broke, or an engaged unit without an objective,
        // triggers an early re-plan (how the original handles invalid objectives between its
        // 30-update re-plans is UNKNOWN).
        let stale = self.assignments.iter().any(|(id, c)| {
            let Some(ui) = b.unit_index(*id) else { return true };
            let u = &b.units[ui];
            if u.is_out_of_fight() {
                return true;
            }
            match c.target.and_then(|t| b.unit_index(t)) {
                None => c.kind != ObjectiveKind::Retreat,
                Some(ti) => {
                    let t = &b.units[ti];
                    t.is_out_of_fight()
                        || (c.kind == ObjectiveKind::Missile
                            && dist(u.position, t.position) > u.missile_range(b.kv_rules.fire_on_walls_range_modifier))
                }
            }
        }) || self.own(b).any(|u| controllable(u) && !self.assignments.contains_key(&u.id) && engaged(b, u));
        if stale || self.last_melee_plan.is_none_or(|t| tick.wrapping_sub(t) >= p.melee_replan_ticks) {
            let candidates = self.candidates(b, p);
            self.assignments = allocate(&candidates);
            self.last_melee_plan = Some(tick);
        }
        self.issue_orders(b, p, &slots);
    }

    /// The STOP_AND_SHOOT group's sample (`0x00762E20`, CONFIRMED): the mean of its units' kill
    /// counters (integer division) and the tick.
    fn kill_sample(&self, b: &Battle, line_ids: &[u32]) -> u32 {
        let kills: Vec<u32> = line_ids
            .iter()
            .filter_map(|id| b.unit_index(*id))
            .filter(|i| role(&b.units[*i]) == Role::Infantry)
            .map(|i| b.units[i].kills)
            .collect();
        if kills.is_empty() { 0 } else { kills.iter().sum::<u32>() / kills.len() as u32 }
    }

    /// The sampler of the STOP_AND_SHOOT update (`0x007D3F00` → `0x007D9190`, CONFIRMED: it runs
    /// on every update of the tactic, before the FSM step).
    fn update_efficiency(&mut self, b: &Battle, p: &AiParams, line_ids: &[u32]) {
        if matches!(self.line, LineTactic::StopAndShoot(_)) {
            let k = self.kill_sample(b, line_ids);
            self.efficiency.sample(k, b.tick, p.efficiency_baseline_ticks);
        }
    }

    /// The STOP_AND_SHOOT group's shooters: line infantry that can fire.
    fn shooters<'a>(&self, b: &'a Battle, line_ids: &'a [u32]) -> impl Iterator<Item = &'a LandUnit> + 'a {
        line_ids
            .iter()
            .filter_map(|id| b.unit_index(*id))
            .map(|i| &b.units[i])
            .filter(|u| role(u) == Role::Infantry && u.can_shoot())
    }

    /// The creep line (`0x0085BDC0`, CONFIRMED): when the target is farther than the group's
    /// **shortest** missile range (`0x0085BCE0`), the line moves `distance − range + 3 m` towards
    /// it, keeping the group's frontage. Returns (midpoint, facing).
    fn creep_line(&self, b: &Battle, p: &AiParams, line_ids: &[u32], line_c: P, target: P) -> (P, P) {
        let m = b.kv_rules.fire_on_walls_range_modifier;
        let range = self.shooters(b, line_ids).map(|u| u.missile_range(m)).fold(f32::INFINITY, f32::min);
        let range = if range.is_finite() { range } else { 0.0 };
        let d = dist(line_c, target);
        let dir = norm_or(sub(target, line_c), self.dir);
        let advance = if range < d { d - range + p.creep_inside_range } else { 0.0 };
        (add(line_c, scale(dir, advance)), dir)
    }

    /// The firing line of ADVANCING_TOWARDS_LINE (`0x0085B530`): when our mean missile range beats
    /// the target's (`0x0085BC40`), the line stands at our **longest** range from the target,
    /// facing it (CONFIRMED). Otherwise the original picks from a list of candidate points (not
    /// decoded); with no candidates it uses the creep line, which is what we use there
    /// (PROVISIONAL).
    fn firing_line(&self, b: &Battle, p: &AiParams, line_ids: &[u32], line_c: P, target: P) -> (P, P) {
        let m = b.kv_rules.fire_on_walls_range_modifier;
        let ours: Vec<f32> = self.shooters(b, line_ids).map(|u| u.missile_range(m)).collect();
        let mean = |v: &[f32]| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
        let theirs: Vec<f32> = self
            .enemies(b)
            .filter(|e| e.missile.is_some() && !e.is_cavalry)
            .map(|e| if e.can_shoot() { e.missile_range(m) } else { 0.0 })
            .collect();
        if !ours.is_empty() && mean(&theirs) < mean(&ours) {
            let longest = ours.iter().copied().fold(0.0f32, f32::max);
            let back = norm_or(sub(line_c, target), scale(self.dir, -1.0));
            (add(target, scale(back, longest)), scale(back, -1.0))
        } else {
            self.creep_line(b, p, line_ids, line_c, target)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_line_fsm(&mut self, b: &Battle, p: &AiParams, own_c: P, enemy_c: P, max_range: f32, line_ids: &[u32]) {
        let tick = b.tick;
        let age = tick.wrapping_sub(self.line_since);
        let formed = self.formed_fraction(b, p, enemy_c, line_ids);
        // ATTACK_BATTLEGROUP transitions (the original's FSM: per-state update thunks at
        // 0x007029E0..0x00702CB0 test conditions and return a transition; CONFIRMED structure and
        // thresholds, see AI_RESEARCH §3.2). "Formed" here is the line's shape (`cohesion`), the
        // target distance is from the line's centre to the enemy centre (INFERRED `+0x80C`).
        let line_c = centroid(line_ids.iter().filter_map(|id| b.unit_index(*id)).map(|i| b.units[i].position)).unwrap_or(own_c);
        let d_target = dist(line_c, enemy_c);
        // The encounter phase (`0x0076C0B0`, CONFIRMED rules): engaged = in melee contact,
        // shooting = not engaged and has a target in range this tick (`aiming`, INFERRED for
        // `0x0055AFE0`); the distance is from the line's centre to the enemy centre (INFERRED
        // for the battlegroup's virtual `+0x0C`).
        {
            let units: Vec<&LandUnit> = line_ids.iter().filter_map(|id| b.unit_index(*id)).map(|i| &b.units[i]).collect();
            let n = units.len() as u32;
            let eng = units.iter().filter(|u| rating::engaged(b, u)).count() as u32;
            let shoot = units.iter().filter(|u| !rating::engaged(b, u) && u.can_shoot() && u.aiming).count() as u32;
            self.phase = phase::phase(n, eng, shoot, Some(d_target * d_target), self.phase);
        }
        let is_formed = self.cohesion(b, p, enemy_c, line_ids) >= p.formed_fraction;
        let facing_ok = self.line_facing_ok(b, p, line_ids);
        // `0x0055AD60` (INFERRED "engaged"): an engaged group neither reforms nor outflanks.
        let engaged = self.assault || line_ids.iter().filter_map(|id| b.unit_index(*id)).any(|i| b.units[i].in_melee);
        let needs_reform = !engaged && d_target <= p.reform_distance && !(is_formed && facing_ok); // 0x007B5640 → Reform
        let back_to_form_up = !engaged && !is_formed && d_target > p.form_up_min_distance; // 0x007A5730 → Move to Form-up
        match self.line {
            LineTactic::Attack(AttackState::Reform) | LineTactic::Attack(AttackState::ChangeFormation) => {
                // Form the line where the army stands (Reform entry `0x007B5910`: formation at the
                // group's position, facing the target).
                if age == 0 {
                    self.anchor = own_c;
                }
                if (is_formed && facing_ok && formed >= p.formed_fraction) || age >= p.form_timeout_ticks {
                    // `0x007B5AF0` (formed and facing within 60°) → Move to Target. The timeout is
                    // PROVISIONAL (the original also leaves after a re-entry counter `+0x818 > 3`).
                    self.set_line(LineTactic::Attack(AttackState::MoveToTarget), tick);
                } else if back_to_form_up && formed >= p.formed_fraction {
                    self.set_line(LineTactic::Attack(AttackState::MoveToFormUp), tick);
                }
            }
            LineTactic::Attack(AttackState::MoveToFormUp) => {
                let stand = max_range + p.form_up_extra_distance;
                let current = dist(self.anchor, enemy_c);
                if current > stand {
                    self.anchor = sub(enemy_c, scale(self.dir, stand));
                }
                // `0x007A5A80`: formed and at the form-up point → Move to Target.
                if (is_formed && formed >= p.formed_fraction) || age >= p.form_timeout_ticks {
                    self.set_line(LineTactic::Attack(AttackState::MoveToTarget), tick);
                } else if needs_reform {
                    self.set_line(LineTactic::Attack(AttackState::Reform), tick);
                } else if self.consider_outflank(b, p, line_c, enemy_c, line_ids, engaged) {
                    self.set_line(LineTactic::Attack(AttackState::Outflank), tick);
                }
            }
            LineTactic::Attack(AttackState::MoveToTarget) => {
                // Advance until the muskets are in range (stop and shoot), or into contact.
                let stand = if max_range > 0.0 && !self.assault {
                    max_range * p.shoot_range_fraction * 0.9
                } else {
                    0.0
                };
                self.anchor = sub(enemy_c, scale(self.dir, stand));
                if !self.assault && !self.wing_tactic_active() && self.stop_and_shoot_scores(b, p, line_ids, d_target, formed) {
                    // The planner's tactic auction hands the line to STOP_AND_SHOOT (its score
                    // 164 beats ATTACK_BATTLEGROUP's 1, `0x00751800`).
                    let (mid, facing) = self.firing_line(b, p, line_ids, line_c, enemy_c);
                    self.shoot_line = Some((mid, facing));
                    self.anchor = mid;
                    self.set_line(LineTactic::StopAndShoot(ShootState::AdvancingTowardsLine), tick);
                } else if needs_reform {
                    self.set_line(LineTactic::Attack(AttackState::Reform), tick);
                } else if back_to_form_up {
                    self.set_line(LineTactic::Attack(AttackState::MoveToFormUp), tick);
                } else if self.consider_outflank(b, p, line_c, enemy_c, line_ids, engaged) {
                    self.set_line(LineTactic::Attack(AttackState::Outflank), tick);
                }
            }
            LineTactic::Attack(AttackState::Outflank) => {
                // State 5: the line marches to the outflank point; `0x007A60A0` → Move to Target
                // once there (or when the formation is gone, `Formed = 0`).
                let point = self.outflank_point.unwrap_or(enemy_c);
                self.anchor = point;
                let arrived = dist(line_c, point) <= p.slot_tolerance * 2.0 + 1.0;
                if arrived || self.cohesion(b, p, enemy_c, line_ids) <= 0.0 || age >= p.form_timeout_ticks {
                    self.set_line(LineTactic::Attack(AttackState::MoveToTarget), tick);
                }
            }
            LineTactic::StopAndShoot(state) => {
                // AI_TACTIC_STOP_AND_SHOOT (CONFIRMED transitions, AI_RESEARCH §3.2). Conditions
                // that wait for the group's moves are only tested from the second update in a
                // state, after the entry's orders went out (the original's entry orders the move and
                // the next update tests it).
                let (mid, _) = self.shoot_line.unwrap_or((line_c, self.dir));
                let all_stopped = age > 0
                    && line_ids
                        .iter()
                        .filter_map(|id| b.unit_index(*id))
                        .all(|i| b.units[i].destination.is_none());
                if !self.stop_and_shoot_keeps(b, line_ids) {
                    // The planner drops STOP_AND_SHOOT when its keep test (`0x0075ECA0`) fails;
                    // the auction then gives the units to ATTACK_BATTLEGROUP: in with the bayonet.
                    self.begin_assault(tick);
                    return;
                }
                match state {
                    ShootState::AdvancingTowardsLine => {
                        // `0x007CE370`: within 20 m of the line's midpoint → FORM_ON_LINE.
                        if dist(line_c, mid) < p.shoot_line_arrive_distance {
                            self.set_line(LineTactic::StopAndShoot(ShootState::FormOnLine), tick);
                        }
                    }
                    ShootState::FormOnLine | ShootState::CreepForward => {
                        // `0x00791440`: every unit has finished moving → HOLD_THE_LINE.
                        if all_stopped {
                            self.set_line(LineTactic::StopAndShoot(ShootState::HoldTheLine), tick);
                            // Entry `0x00794A50`: both samples set to now.
                            let k = self.kill_sample(b, line_ids);
                            self.efficiency.reset(k, tick);
                        }
                    }
                    ShootState::HoldTheLine => {
                        // `0x0079A720`: not efficient → CREEP_FORWARD.
                        if !self.efficiency.efficient(tick, p.efficiency_stale_ticks, p.efficiency_min_rate) {
                            // The assault comes only from the planner's keep test above
                            // (CONFIRMED structure); an inefficient line creeps forward.
                            let (cmid, facing) = self.creep_line(b, p, line_ids, line_c, enemy_c);
                            self.shoot_line = Some((cmid, facing));
                            self.anchor = cmid;
                            self.set_line(LineTactic::StopAndShoot(ShootState::CreepForward), tick);
                        }
                    }
                }
            }
            LineTactic::Defend(DefendState::ChangeFormation) => {
                // `0x00791390`: the group's formation is ready → REFORM (ours is always ready).
                self.set_line(LineTactic::Defend(DefendState::Reform), tick);
            }
            LineTactic::Defend(DefendState::ChangeUnitFormations) => {
                // `0x00447880` returns 1: REFORM on the next update.
                if age > 0 {
                    self.set_line(LineTactic::Defend(DefendState::Reform), tick);
                }
            }
            LineTactic::Defend(DefendState::Reform) => {
                // Entry `0x007B5A40` orders the formation; the first one forms where the army
                // stands (PROVISIONAL position; later reforms keep the line where it was).
                if age == 0 && self.line_members.is_empty() {
                    self.anchor = own_c;
                }
                if age == 0 {
                    self.line_members = line_ids.to_vec();
                }
                // `0x00797150`: ready and formed percent == 100 → DEFEND_LINE (CONFIRMED). The
                // timeout is PROVISIONAL (our units can be held off their slots by the melee
                // manager, which the original's percentage may not count).
                if formed >= 1.0 || age >= p.form_timeout_ticks {
                    self.set_line(LineTactic::Defend(DefendState::DefendLine), tick);
                }
            }
            LineTactic::Defend(DefendState::DefendLine) => {
                // `0x007B5C00` (CONFIRMED): back to REFORM when the group changed (`0x0076D870`: a
                // unit left it) or the formed percent fell below 50.
                let changed = self.line_members.iter().any(|id| !line_ids.contains(id));
                if changed || formed < p.defend_reform_below {
                    self.set_line(LineTactic::Defend(DefendState::Reform), tick);
                    // Entry `0x007B5A40`: the re-formed group is the units left.
                    self.line_members = line_ids.to_vec();
                }
            }
        }
    }

    /// Missile and melee strength of our line and of the enemy (`0x0079C940` = Σ missile ratings
    /// `+0xBEC`, CONFIRMED; `0x0079C1E0` INFERRED Σ melee ratings `+0xBE8`; units with the UNKNOWN
    /// flag `+0x278D` count half there, left out). The enemy side is every enemy unit
    /// (PROVISIONAL: the original uses the target battlegroup).
    fn shoot_strengths(&self, b: &Battle, line_ids: &[u32]) -> (f32, f32, f32, f32) {
        let own: Vec<&LandUnit> = line_ids.iter().filter_map(|id| b.unit_index(*id)).map(|i| &b.units[i]).collect();
        let our_missile: f32 = own.iter().map(|u| rating::missile_rating(u)).sum();
        let our_melee: f32 = own.iter().map(|u| rating::melee_rating(u)).sum();
        let their_missile: f32 = self.enemies(b).map(rating::missile_rating).sum();
        let their_melee: f32 = self.enemies(b).map(rating::melee_rating).sum();
        (our_missile.round(), our_melee.round(), their_missile.round(), their_melee.round())
    }

    /// STOP_AND_SHOOT's planner score `0x007B1BD0` is 164 (else 0) when (CONFIRMED tests): the
    /// group has no artillery, at least one shooter, the target is within some shooter's missile
    /// range, the group's `+0x1C` value is at least 70 (INFERRED formed percent), and the missile
    /// strengths pass `theirs < 2 × ours` with ours > 0 (INFERRED orientation: the first sum is
    /// the target's). Also required there and not modelled: encounter phase CONTACT (5) and no
    /// target unit passing `0x0055AD30`.
    fn stop_and_shoot_scores(&self, b: &Battle, p: &AiParams, line_ids: &[u32], d_target: f32, formed: f32) -> bool {
        let m = b.kv_rules.fire_on_walls_range_modifier;
        let artillery = line_ids.iter().filter_map(|id| b.unit_index(*id)).any(|i| role(&b.units[i]) == Role::Artillery);
        let in_range = self.shooters(b, line_ids).any(|u| d_target <= u.missile_range(m));
        if self.phase != phase::Phase::Contact || artillery || !in_range || formed < p.stop_and_shoot_min_formed {
            return false;
        }
        let (our_missile, _, their_missile, _) = self.shoot_strengths(b, line_ids);
        our_missile > 0.0 && their_missile < 2.0 * our_missile
    }

    /// STOP_AND_SHOOT's keep test `0x0075ECA0` (CONFIRMED tests): the group keeps the tactic while
    /// it has a shooter, no shooter is engaged in melee (`0x0054EE90`, INFERRED "engaged"), and
    /// `their missile ≤ 2 × our missile` and `our melee ≤ 4 × their melee` (INFERRED orientation,
    /// as in the score). Also required there and not modelled: phase not GENERAL MELEE (7), the
    /// target battlegroup's `+0x8C != 100`, no shooter with the target flag `+0x278D`.
    fn stop_and_shoot_keeps(&self, b: &Battle, line_ids: &[u32]) -> bool {
        if self.phase == phase::Phase::GeneralMelee {
            return false;
        }
        if self.shooters(b, line_ids).next().is_none() || self.shooters(b, line_ids).any(|u| rating::engaged(b, u)) {
            return false;
        }
        let (our_missile, our_melee, their_missile, their_melee) = self.shoot_strengths(b, line_ids);
        their_missile <= 2.0 * our_missile && our_melee <= 4.0 * their_melee
    }

    /// The line goes in with the bayonet: ATTACK_BATTLEGROUP MOVE_TO_TARGET with the assault flag
    /// (the planner gives the units back to ATTACK_BATTLEGROUP when STOP_AND_SHOOT's keep test
    /// fails).
    fn begin_assault(&mut self, tick: u32) {
        self.assault = true;
        self.shoot_line = None;
        self.set_line(LineTactic::Attack(AttackState::MoveToTarget), tick);
    }

    /// The wing tactics through the battlegroup's tactic auction (`auction`, `0x00751800`,
    /// CONFIRMED): OUTFLANK, DOUBLE_ENVELOPMENT and STOP_AND_SHOOT compete by score; the winners
    /// claim units (CONFIRMED counts and preferences) and keep them while their keep tests hold;
    /// ATTACK_BATTLEGROUP (the default) has the rest. See [`WingBidder`] for the scores. The
    /// seeds are each tactic's own ten draws (see [`AllianceAi::seeds`]).
    fn update_cavalry(&mut self, b: &Battle, _p: &AiParams) {
        let seeds = *self.seeds.get_or_insert_with(|| {
            let mut r = b.rng;
            let mut s = [[0u32; 10]; 6];
            for t in s.iter_mut() {
                for v in t.iter_mut() {
                    *v = r.next16();
                }
            }
            s
        });
        let units: Vec<u32> = self.own(b).filter(|u| controllable(u)).map(|u| u.id).collect();
        if let LineTactic::Defend(_) = self.line {
            // DEFEND_ABSTRACT battlegroups were not decoded with their tactic set: the round-3
            // PROVISIONAL rule (the cavalry rides out on both wings when we are at least even and
            // the enemy is close) stands in, and the attack tactics are released.
            for t in &mut self.tactics {
                t.active = false;
                t.units.clear();
            }
            self.wing_side.clear();
            let go = self.balance >= 0.5 && b.tick.wrapping_sub(self.last_contact_tick) < 50;
            if go {
                let right = right_of(self.dir);
                let own_c = centroid(self.own(b).filter(|u| controllable(u)).map(|u| u.position)).unwrap_or(self.anchor);
                for u in self.own(b).filter(|u| controllable(u) && role(u) == Role::Cavalry) {
                    let side = if dot(sub(u.position, own_c), right) >= 0.0 { 1.0 } else { -1.0 };
                    self.wing_side.insert(u.id, side);
                }
            }
            self.cavalry = if self.wing_side.is_empty() { OutflankState::Approaching } else { OutflankState::Outflanking };
            return;
        }
        // STOP_AND_SHOOT claims no units; its active flag mirrors our line FSM (which runs its
        // CONFIRMED score / keep tests) so that the exclusions apply.
        let sas_active = matches!(self.line, LineTactic::StopAndShoot(_));
        let mut tactics = std::mem::take(&mut self.tactics);
        if let Some(t) = tactics.iter_mut().find(|t| t.kind == auction::Kind::StopAndShoot) {
            t.active = sas_active;
        }
        let newly = {
            let mut bidder = WingBidder::new(self, b, seeds);
            let newly = auction::run(&mut tactics, &units, auction::Kind::AttackBattlegroup, &mut bidder);
            // OUTFLANK goes to the side chosen at its claim (`0x007AE2C0`); DOUBLE_ENVELOPMENT
            // splits its units by the side each stands on (`0x007D0870`, INFERRED).
            let mut sides = BTreeMap::new();
            for t in &tactics {
                for &id in &t.units {
                    match t.kind {
                        auction::Kind::Outflank => {
                            sides.insert(id, self.outflank_sign.unwrap_or(bidder.outflank_side));
                        }
                        auction::Kind::DoubleEnvelopment => {
                            sides.insert(id, bidder.side_of(id));
                        }
                        _ => {}
                    }
                }
            }
            if newly.contains(&auction::Kind::Outflank) {
                self.outflank_sign = Some(bidder.outflank_side);
                self.outflank_target = bidder.outflank_point;
            }
            self.wing_side = sides;
            newly
        };
        if !tactics.iter().any(|t| t.kind == auction::Kind::Outflank && t.active) {
            self.outflank_sign = None;
            self.outflank_target = None;
        }
        let _ = newly;
        self.tactics = tactics;
        self.cavalry = if self.wing_side.is_empty() { OutflankState::Approaching } else { OutflankState::Outflanking };
    }

    /// OUTFLANK or DOUBLE_ENVELOPMENT is active (they exclude STOP_AND_SHOOT, `0x0075DD00`).
    fn wing_tactic_active(&self) -> bool {
        self.tactics
            .iter()
            .any(|t| t.active && matches!(t.kind, auction::Kind::Outflank | auction::Kind::DoubleEnvelopment))
    }

    /// The line's shape kept: the fraction of `ids` whose offset from the line's centre matches
    /// their slot's offset from the slots' centre within twice `slot_tolerance` (our stand-in for
    /// the original's "Formed=%u%%", PROVISIONAL measure). Unlike [`Self::formed_fraction`] it does
    /// not require the line to have arrived.
    fn cohesion(&self, b: &Battle, p: &AiParams, enemy_c: P, ids: &[u32]) -> f32 {
        let slots = self.slots(b, p, enemy_c);
        let pairs: Vec<(P, P)> = ids
            .iter()
            .filter_map(|id| Some((b.units[b.unit_index(*id)?].position, *slots.get(id)?)))
            .collect();
        if pairs.is_empty() {
            return 1.0;
        }
        let uc = centroid(pairs.iter().map(|x| x.0)).expect("not empty");
        let sc = centroid(pairs.iter().map(|x| x.1)).expect("not empty");
        let n = pairs.iter().filter(|(u, s)| dist(sub(*u, uc), sub(*s, sc)) <= p.slot_tolerance * 2.0).count();
        n as f32 / pairs.len() as f32
    }

    /// The line faces the target within `facing_tolerance_deg` (CONFIRMED 60°): the mean facing of
    /// its units against the direction to the enemy.
    fn line_facing_ok(&self, b: &Battle, p: &AiParams, ids: &[u32]) -> bool {
        let f = ids
            .iter()
            .filter_map(|id| b.unit_index(*id))
            .fold((0.0, 0.0), |a, i| add(a, (b.units[i].facing.cos(), b.units[i].facing.sin())));
        if geom::len(f) < 1e-6 {
            return true;
        }
        dot(norm_or(f, self.dir), self.dir) >= p.facing_tolerance_deg.to_radians().cos()
    }

    /// `0x007A5C80` (CONFIRMED structure): once per attack, a line that is not stronger
    /// (own / (own + enemy) rating ≤ 0.5) and has fewer than half its units engaged looks for an
    /// outflank point: at 1.1 × the distance to the target, turned 5°, 10°, ... 85° off the
    /// direction to it, one side then the other; the first valid point wins. Validity is
    /// PROVISIONAL (`outflank_clearance` from every enemy); the original tries a random side first
    /// (`0x007ADD80() < 51`), we try the side with fewer enemies first (PROVISIONAL, no RNG).
    #[allow(clippy::too_many_arguments)]
    fn consider_outflank(&mut self, b: &Battle, p: &AiParams, line_c: P, enemy_c: P, ids: &[u32], engaged: bool) -> bool {
        if self.outflank_checked || engaged {
            return false;
        }
        self.outflank_checked = true;
        let in_melee = ids.iter().filter_map(|id| b.unit_index(*id)).filter(|i| b.units[*i].in_melee).count();
        if in_melee * 2 >= ids.len().max(1) || self.balance > 0.5 {
            return false;
        }
        let to = sub(enemy_c, line_c);
        let d = geom::len(to) * 1.1;
        let dir = norm_or(to, self.dir);
        let right = right_of(dir);
        let enemies: Vec<P> = self.enemies(b).map(|u| u.position).collect();
        let on_right = enemies.iter().filter(|e| dot(sub(**e, line_c), right) > 0.0).count();
        // `rotate` turns counter-clockwise, i.e. to the left of `dir` for a positive angle.
        let sides: [f32; 2] = if on_right * 2 > enemies.len() { [1.0, -1.0] } else { [-1.0, 1.0] };
        let mut a = 5.0f32;
        while a <= 85.0 {
            for s in sides {
                let pt = add(line_c, scale(rotate(dir, s * a.to_radians()), d));
                if enemies.iter().all(|e| dist(*e, pt) >= p.outflank_clearance) {
                    self.outflank_point = Some(pt);
                    return true;
                }
            }
            a += 5.0;
        }
        false
    }

    /// Fraction of `ids` within the slot tolerance of their line slot.
    fn formed_fraction(&self, b: &Battle, p: &AiParams, enemy_c: P, ids: &[u32]) -> f32 {
        if ids.is_empty() {
            return 1.0;
        }
        let slots = self.slots(b, p, enemy_c);
        let n = ids
            .iter()
            .filter(|id| {
                let Some(i) = b.unit_index(**id) else { return true };
                slots.get(id).is_none_or(|s| dist(b.units[i].position, *s) <= p.slot_tolerance)
            })
            .count();
        n as f32 / ids.len() as f32
    }

    /// Target slot of every controllable unit (PROVISIONAL formation): infantry in one line
    /// centred on the anchor, artillery 40 m behind it, cavalry on the wings (alternating right,
    /// left) or, when outflanking, beyond the enemy's flanks.
    fn slots(&self, b: &Battle, p: &AiParams, enemy_c: P) -> BTreeMap<u32, P> {
        let mut out = BTreeMap::new();
        let right = right_of(self.dir);
        let on_wing = |u: &LandUnit| self.wing_side.contains_key(&u.id);
        let mut inf: Vec<&LandUnit> = self.own(b).filter(|u| controllable(u) && role(u) == Role::Infantry && !on_wing(u)).collect();
        let mut art: Vec<&LandUnit> = self.own(b).filter(|u| controllable(u) && role(u) == Role::Artillery && !on_wing(u)).collect();
        let mut cav: Vec<&LandUnit> = self.own(b).filter(|u| controllable(u) && role(u) == Role::Cavalry && !on_wing(u)).collect();
        let wings: Vec<&LandUnit> = self.own(b).filter(|u| controllable(u) && on_wing(u)).collect();
        // Order each group left to right as the units stand now, so nobody crosses the line to
        // reach its slot (stable sort: ties keep id order).
        let lateral = |u: &&LandUnit| u.position.0 * right.0 + u.position.1 * right.1;
        inf.sort_by(|a, c| lateral(a).total_cmp(&lateral(c)));
        art.sort_by(|a, c| lateral(a).total_cmp(&lateral(c)));
        // Cavalry: rightmost first (it takes the right wing), then leftmost, alternating.
        cav.sort_by(|a, c| lateral(c).total_cmp(&lateral(a)));
        let cav: Vec<&LandUnit> = {
            let (mut lo, mut hi) = (0usize, cav.len());
            let mut v = Vec::with_capacity(cav.len());
            for k in 0..cav.len() {
                if k % 2 == 0 { v.push(cav[lo]); lo += 1; } else { hi -= 1; v.push(cav[hi]); }
            }
            v
        };
        let width: f32 = inf.iter().map(|u| frontage(u)).sum::<f32>()
            + p.line_gap * inf.len().saturating_sub(1) as f32;
        let mut x = -width / 2.0;
        for u in &inf {
            let w = frontage(u);
            out.insert(u.id, add(self.anchor, scale(right, x + w / 2.0)));
            x += w + p.line_gap;
        }
        let back = sub(self.anchor, scale(self.dir, 40.0));
        let aw = art.len() as f32 * 30.0;
        for (k, u) in art.iter().enumerate() {
            out.insert(u.id, add(back, scale(right, -aw / 2.0 + 15.0 + k as f32 * 30.0)));
        }
        // Enemy line half-width (spread of enemy units across our facing).
        let enemy_half = self
            .enemies(b)
            .map(|e| {
                let d = sub(e.position, enemy_c);
                (d.0 * right.0 + d.1 * right.1).abs() + frontage(e) / 2.0
            })
            .fold(0.0f32, f32::max);
        let (mut r_off, mut l_off) = (width / 2.0 + 30.0, width / 2.0 + 30.0);
        // Cavalry held by ATTACK_BATTLEGROUP waits on the wings of the line.
        for (k, u) in cav.iter().enumerate() {
            let w = frontage(u);
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            let off = if sign > 0.0 { &mut r_off } else { &mut l_off };
            out.insert(u.id, add(self.anchor, scale(right, sign * (*off + w / 2.0))));
            *off += w + p.line_gap;
        }
        // Units of OUTFLANK / DOUBLE_ENVELOPMENT go beyond the enemy flank on their side, level
        // with the enemy line (PROVISIONAL geometry; the sub-move objects `0x0070B800` are not
        // decoded).
        let (mut kr, mut kl) = (0.0f32, 0.0f32);
        for u in &wings {
            let w = frontage(u);
            let sign = *self.wing_side.get(&u.id).unwrap_or(&1.0);
            let k = if sign > 0.0 { &mut kr } else { &mut kl };
            // OUTFLANK's units head for its CONFIRMED point (`0x007AE2C0`), spread outwards from
            // it (PROVISIONAL spacing); DOUBLE_ENVELOPMENT keeps the stand-in geometry.
            let slot = match self.outflank_target {
                Some(pt) if self.outflank_sign == Some(sign) => add(pt, scale(right, sign * *k * (w + p.line_gap))),
                _ => add(enemy_c, scale(right, sign * (enemy_half + 60.0 + *k * (w + p.line_gap)))),
            };
            out.insert(u.id, slot);
            *k += 1.0;
        }
        out
    }

    /// The melee manager's analysers: candidate objectives per controllable unit.
    fn candidates(&self, b: &Battle, p: &AiParams) -> BTreeMap<u32, Vec<Candidate>> {
        let mut out: BTreeMap<u32, Vec<Candidate>> = BTreeMap::new();
        let m = b.kv_rules.fire_on_walls_range_modifier;
        // Valid targets (CONFIRMED `0x0055CBD0`: active and with soldiers). Routing units are still
        // valid targets in the original's check; the AI simply values them by their remaining men.
        // PROVISIONAL pursuit limit: our battlefield has no edge for routers to leave by, so a
        // routing enemy farther than `pursuit_radius` from our army is no longer chased.
        let own_c = centroid(self.own(b).filter(|u| controllable(u)).map(|u| u.position)).unwrap_or(self.anchor);
        let targets: Vec<&LandUnit> = self
            .enemies(b)
            .filter(|t| !t.morale.is_routing_or_shattered() || dist(t.position, own_c) <= p.pursuit_radius)
            .collect();
        // PROVISIONAL: shaken units only fall back while some steady unit holds the line; when the
        // whole army is shaken there is nobody to hide behind, so they keep fighting.
        let steady = self.own(b).filter(|u| controllable(u) && !rating::should_fall_back(u)).count();
        let line_outflanking = self.line == LineTactic::Attack(AttackState::Outflank);
        for u in self.own(b).filter(|u| controllable(u)) {
            let mut list = Vec::new();
            let r = role(u);
            // The unit's AI state `+0xB34` as far as we can tell it (CONFIRMED: OUTFLANK sets 1,
            // `0x007C8820`; HOLD_THE_LINE sets 4 for shooters, `0x007C8A40`); the rest stay 0.
            let unit_state = if line_outflanking && r != Role::Cavalry { 1 } else { 0 };
            // RETREAT (type 4): shaken units out of contact fall back.
            if steady > 0 && rating::should_fall_back(u) && !engaged(b, u) {
                list.push(Candidate {
                    kind: ObjectiveKind::Retreat,
                    target: None,
                    priority: f32::MAX,
                    meets_minimum: true,
                    forced: true,
                    potential: 0.0,
                });
                out.insert(u.id, list);
                continue;
            }
            let range = u.missile_range(m);
            // MISSILE (type 3): targets in range. Priority = the missile sub-objective virtual
            // `+0x14` (`0x007D1510`, CONFIRMED parts): base² × shape(potential, 1, 0.3), ×2 for
            // the target it already fires at, × the distance/turn factor, and the melee/missile
            // potential ratio test (`0x006B0500`), the class factor `0x007D2CF0`
            // (`classes::missile_class_factor`). Left out (UNKNOWN or not in the model):
            // buildings, the formation-front test.
            let enemy_within_160 = targets.iter().any(|t| dist(u.position, t.position) <= 160.0);
            if u.can_shoot() {
                for t in &targets {
                    let d = dist(u.position, t.position);
                    if d > range || t.morale.is_routing_or_shattered() {
                        continue; // is_viable_target: out of range (CONFIRMED)
                    }
                    let pot = rating::missile_potential(u, t);
                    let base = rating::missile_base_priority(b, t);
                    let mut pr = base * base * rating::priority_shape(pot, 1.0, 0.3);
                    let near_target =
                        b.units.iter().filter(|o| o.id != t.id && o.active && o.men > 0 && dist(o.position, t.position) <= 160.0).count();
                    pr *= classes::missile_class_factor(u, t, near_target);
                    if u.fire_target == Some(t.id) {
                        pr *= 2.0;
                    }
                    pr *= rating::missile_distance_factor(d, rating::turn_angle(u.position, u.facing, t.position));
                    let mp = melee_potential(u, t);
                    let ratio = if mp != 0.0 && pot != 0.0 {
                        mp / (pot + if mp < 0.3 && pot > 0.05 { 0.25 } else { 0.0 })
                    } else {
                        0.0
                    };
                    if unit_state == 1 && ratio > 2.0 {
                        continue;
                    }
                    if unit_state != 1 && ratio > 3.0 {
                        pr *= 0.25;
                    }
                    list.push(Candidate {
                        kind: ObjectiveKind::Missile,
                        target: Some(t.id),
                        priority: pr,
                        meets_minimum: rating::meets_minimum(pr, rating::missile_rating(u), unit_state),
                        forced: false,
                        potential: pot,
                    });
                }
            }
            // MELEE (type 0).
            let melee_allowed = match r {
                Role::Cavalry => true,
                Role::Infantry => self.assault || !u.can_shoot(),
                Role::Artillery => false,
            };
            let shooter = u.can_shoot() && r != Role::Artillery;
            let own_shooters: Vec<&LandUnit> =
                self.own(b).filter(|o| o.id != u.id && controllable(o) && o.can_shoot()).collect();
            for t in &targets {
                if t.morale.is_routing_or_shattered() && r != Role::Cavalry {
                    continue;
                }
                let d = dist(u.position, t.position);
                let forced_by_distance = r != Role::Artillery && d <= p.force_melee_distance && (melee_allowed || engaged(b, t));
                let reach = match r {
                    Role::Cavalry => {
                        if self.wing_side.contains_key(&u.id) { p.cavalry_search_radius } else { p.cavalry_reserve_radius }
                    }
                    Role::Infantry => {
                        if self.assault { p.assault_reach } else { p.infantry_melee_reach }
                    }
                    Role::Artillery => 0.0,
                };
                if !(melee_allowed && d <= reach) && !forced_by_distance {
                    continue;
                }
                // `0x007D0D10` (CONFIRMED): a shooter that is not engaged only melees targets
                // within its own missile range (INFERRED: both range reads are the attacker's).
                if shooter && !engaged(b, u) && d > range {
                    continue;
                }
                let pot = melee_potential(u, t);
                let mut base = melee_base_priority(t);
                if t.morale.is_routing_or_shattered() {
                    base *= 0.25; // PROVISIONAL: the ×0.25 "target flag" read as routing
                }
                // The melee sub-objective virtual `+0x14` (`0x007D0D10`, CONFIRMED parts).
                let mut pr = base * base * rating::priority_shape(pot, 1.7, 0.3);
                // Class matchup `0x007D23E0` with the CONFIRMED class/category enums
                // (`classes::melee_class_factor`).
                pr *= classes::melee_class_factor(u, t, enemy_within_160);
                // Shooters weigh melee against shooting (CONFIRMED rule on the two ratings).
                if shooter {
                    let (tm, tl) = (rating::missile_rating(t), rating::melee_rating(t));
                    if tm == 0.0 {
                        pr *= 0.1;
                    } else if tl == 0.0 {
                        pr *= 2.0;
                    } else {
                        let ratio = rating::missile_rating(u) / tm;
                        if ratio <= rating::melee_rating(u) / tl {
                            pr *= 0.5;
                        } else {
                            pr *= (1.0 / ratio).min(0.5);
                        }
                    }
                }
                pr *= rating::melee_distance_factor(d, rating::turn_angle(u.position, u.facing, t.position), unit_state == 1);
                // `0x00796910` (CONFIRMED structure): ×0.5 when another of our shooters already has
                // the target in range in front of it.
                if pr > 0.0
                    && own_shooters.iter().any(|o| {
                        dist(o.position, t.position) < o.missile_range(m)
                            && dot((o.facing.cos(), o.facing.sin()), sub(t.position, o.position)) >= 0.0
                    })
                {
                    pr *= 0.5;
                }
                if shooter {
                    // Far away, the target facing us (within 45°) and not in melee: ×0.001;
                    // otherwise ×2 (CONFIRMED thresholds: 1.5 × range², 45°).
                    let facing_us = rating::turn_angle(t.position, t.facing, u.position) < std::f32::consts::FRAC_PI_4;
                    if d * d > 1.5 * range * range && facing_us && !t.in_melee {
                        pr *= 0.001;
                    } else {
                        pr *= 2.0;
                    }
                }
                if pr <= 0.0 && !forced_by_distance {
                    continue;
                }
                // The minimum test (`0x0079C730`) plus our PROVISIONAL cavalry potential floor.
                let meets = rating::meets_minimum(pr, rating::melee_rating(u), unit_state)
                    && (r != Role::Cavalry || pot >= p.min_melee_potential);
                list.push(Candidate {
                    kind: ObjectiveKind::Melee,
                    target: Some(t.id),
                    priority: pr,
                    meets_minimum: meets,
                    forced: forced_by_distance && meets,
                    potential: pot,
                });
            }
            if !list.is_empty() {
                out.insert(u.id, list);
            }
        }
        out
    }

    fn issue_orders(&mut self, b: &mut Battle, p: &AiParams, slots: &BTreeMap<u32, P>) {
        let tick = b.tick;
        let ids: Vec<u32> = self.own(b).filter(|u| controllable(u)).map(|u| u.id).collect();
        self.tasks.clear();
        // STOP_AND_SHOOT: while the line moves (advancing, forming, creeping) its infantry follows
        // the group's move instead of standing to shoot; in HOLD_THE_LINE it stands and fires.
        let line_moving = matches!(self.line, LineTactic::StopAndShoot(s) if s != ShootState::HoldTheLine);
        let line_holding = self.line == LineTactic::StopAndShoot(ShootState::HoldTheLine);
        for id in ids {
            let Some(i) = b.unit_index(id) else { continue };
            let u = b.units[i].clone();
            // Charge bookkeeping.
            if u.in_melee {
                self.melee_since.entry(id).or_insert(tick);
            } else {
                self.melee_since.remove(&id);
            }
            if self.assignments.get(&id).is_none_or(|c| c.kind != ObjectiveKind::Retreat) {
                self.rally.remove(&id);
            }
            let follows_line = line_moving && role(&u) == Role::Infantry;
            if let Some(c) = self
                .assignments
                .get(&id)
                .copied()
                .filter(|c| !(follows_line && c.kind == ObjectiveKind::Missile))
            {
                self.tasks.insert(id, UnitTask::Objective(c));
                match c.kind {
                    ObjectiveKind::Retreat => {
                        let dir = self.dir;
                        let rally = *self
                            .rally
                            .entry(id)
                            .or_insert_with(|| sub(u.position, scale(dir, p.fall_back_distance)));
                        if dist(u.position, rally) > p.slot_tolerance * 0.5 {
                            if u.destination.is_none_or(|d| dist(d, rally) > 1.0) {
                                b.order_move(id, rally);
                            }
                        } else if u.destination.is_some() {
                            b.order_halt(id);
                        }
                        b.order_end_charge(id);
                    }
                    ObjectiveKind::Missile => {
                        let t = c.target.expect("missile objectives have a target");
                        if u.fire_target != Some(t) {
                            b.order_fire(id, t);
                        }
                    }
                    ObjectiveKind::Melee => {
                        let t = c.target.expect("melee objectives have a target");
                        let Some(ti) = b.unit_index(t) else { continue };
                        let d = dist(u.position, b.units[ti].position);
                        let charge_d = if u.is_cavalry { p.cavalry_charge_distance } else { p.infantry_charge_distance };
                        let in_melee_long = self
                            .melee_since
                            .get(&id)
                            .is_some_and(|s| tick.wrapping_sub(*s) >= p.charge_hold_ticks);
                        let charge = d <= charge_d && !in_melee_long;
                        b.order_attack_unit(id, t, MELEE_CONTACT_RANGE * 0.5, charge);
                    }
                }
                continue;
            }
            if u.charging {
                b.order_end_charge(id);
            }
            // Flank / rear threat: turn to face it (PROVISIONAL reaction).
            if role(&u) != Role::Cavalry && !u.in_melee
                && let Some(threat) = self.flank_threat(b, &u, p) {
                    let tp = b.units[b.unit_index(threat).expect("threat exists")].position;
                    let to = norm_or(sub(tp, u.position), self.dir);
                    b.order_move(id, add(u.position, scale(to, 0.3)));
                    self.tasks.insert(id, UnitTask::FaceThreat(threat));
                    continue;
                }
            // Stop and shoot: in HOLD_THE_LINE the line infantry stands (and fires at will); in the
            // other tactics muskets with an enemy in range stay put and fire at will.
            if u.can_shoot() && !self.assault && !follows_line {
                let range = u.missile_range(b.kv_rules.fire_on_walls_range_modifier) * p.shoot_range_fraction;
                let in_range = (line_holding && role(&u) == Role::Infantry)
                    || self.enemies(b).any(|e| !e.morale.is_routing_or_shattered() && dist(u.position, e.position) <= range);
                if in_range {
                    if u.destination.is_some() || u.fire_target.is_some() {
                        b.order_halt(id);
                    }
                    self.tasks.insert(id, UnitTask::StopAndShoot);
                    continue;
                }
            }
            if let Some(slot) = slots.get(&id).copied() {
                self.tasks.insert(id, UnitTask::Slot(slot));
                // STOP_AND_SHOOT moves the line by a few metres at a time (creep: 3 m inside range),
                // so its infantry goes all the way to the slot (PROVISIONAL tolerance).
                let tolerance = if follows_line { p.line_move_tolerance } else { p.slot_tolerance * 0.5 };
                if dist(u.position, slot) > tolerance {
                    if u.destination.is_none_or(|d| dist(d, slot) > 1.0) {
                        b.order_move(id, slot);
                    }
                } else if u.destination.is_some() {
                    b.order_halt(id);
                }
            }
        }
    }

    /// The nearest fighting enemy within the threat distance that is on `u`'s flank or rear.
    fn flank_threat(&self, b: &Battle, u: &LandUnit, p: &AiParams) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for e in self.enemies(b) {
            if e.morale.is_routing_or_shattered() {
                continue;
            }
            let d = dist(u.position, e.position);
            if d > p.flank_threat_distance {
                continue;
            }
            if attack_side(u.position, u.facing, e.position) == AttackSide::Front {
                continue;
            }
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((e.id, d));
            }
        }
        best.map(|(id, _)| id)
    }
}

/// The wing tactics' virtuals for the auction (scores, keep tests, filters, claims), computed
/// from the battle at the start of the think. The target battlegroup is the enemy army
/// (PROVISIONAL), the outflank points are always found (`0x0078E3E0` not decoded, PROVISIONAL)
/// and the AI plans on flat ground (no target high-ground flag `+0x99`).
struct WingBidder {
    phase: phase::Phase,
    /// OUTFLANK's and DOUBLE_ENVELOPMENT's own seeds ([`AllianceAi::seeds`] rows 1 and 2).
    of_seeds: [u32; 10],
    de_seeds: [u32; 10],
    /// Our / the target's total strength (`0x007CCD60`).
    ours: i32,
    theirs: i32,
    own_count: usize,
    enemy_count: usize,
    /// % of men of class 8 (cavalry_missile) and of mounted men, ours and the target's
    /// (`+0x7C`/`+0x88`, `+0x78`/`+0x84`; men stand in for the UNKNOWN `+0x204`).
    our_class8: i32,
    their_class8: i32,
    our_mounted: i32,
    their_mounted: i32,
    /// Units the OUTFLANK / DOUBLE_ENVELOPMENT filters pass (`0x007D34A0` / `0x007D33B0`).
    eligible: BTreeMap<u32, f32>,
    /// The side OUTFLANK takes if it claims now (+1 right of our facing, −1 left).
    outflank_side: f32,
    /// The OUTFLANK point (`0x007AE2C0`), if any side has one.
    outflank_point: Option<P>,
    /// Both sides have a point (DOUBLE_ENVELOPMENT needs that).
    both_sides: bool,
}

impl WingBidder {
    fn new(ai: &AllianceAi, b: &Battle, seeds: [[u32; 10]; 6]) -> Self {
        let own: Vec<&LandUnit> = ai.own(b).filter(|u| controllable(u)).collect();
        let enemies: Vec<&LandUnit> = ai.enemies(b).filter(|u| u.active && u.men > 0).collect();
        let strength = |v: &[&LandUnit]| {
            v.iter()
                .map(|u| {
                    let half = if u.morale.is_routing_or_shattered() { 0.5 } else { 1.0 };
                    (rating::melee_rating(u) + rating::missile_rating(u)) * half
                })
                .sum::<f32>()
                .round() as i32
        };
        let pct = |v: &[&LandUnit], f: &dyn Fn(&LandUnit) -> bool| {
            let men: u32 = v.iter().map(|u| u.men).sum();
            (v.iter().filter(|u| f(u)).map(|u| u.men).sum::<u32>() * 100).checked_div(men).unwrap_or(0) as i32
        };
        let class8 = |u: &LandUnit| classes::codes(u).1 == 8;
        let mounted = |u: &LandUnit| classes::is_mounted(classes::codes(u).0);
        let right = right_of(ai.dir);
        let own_c = centroid(own.iter().map(|u| u.position)).unwrap_or(ai.anchor);
        let enemy_c = centroid(enemies.iter().map(|u| u.position)).unwrap_or(ai.anchor);
        let eligible = own
            .iter()
            .filter(|u| {
                let (cat, class) = classes::codes(u);
                (classes::is_mounted(cat) || cat == classes::category::ELEPHANTS || class == 0x11 || class == 0x16)
                    && class != classes::class::GENERAL
            })
            .map(|u| (u.id, if dot(sub(u.position, own_c), right) >= 0.0 { 1.0 } else { -1.0 }))
            .collect();
        // `0x007AE2C0` / `0x0078E3E0` (CONFIRMED, [`outflank`]): the outflank points at ±90° off
        // the target's facing, 200 m in CLOSE APPROACH, else 100 m; with both, the cheaper one
        // (distance from our group less 5 × the capped height drop) wins. PROVISIONAL inputs: the
        // target rectangle = the enemy army (mean facing, unit spread), the battle area = the
        // height grid, the terrain test = inside that area.
        let target = enemy_rect(&enemies, enemy_c, ai.dir);
        let area = b.ground.heights.as_ref().map(|h| outflank::Area {
            min: (-h.spec.width / 2.0, -h.spec.height / 2.0),
            max: (h.spec.width / 2.0, h.spec.height / 2.0),
        });
        let valid = |q: P, _: P, _: f32| area.as_ref().is_none_or(|a| a.contains(q));
        let height = |q: P| b.ground.height(q.0, q.1);
        let d = outflank::distance(ai.phase == phase::Phase::CloseApproach);
        let chosen = outflank::choose_side(&target, d, own_c, area.as_ref(), &valid, &height);
        let side_of = |q: P| if dot(sub(q, enemy_c), right) >= 0.0 { 1.0 } else { -1.0 };
        let outflank_point = chosen.map(|(_, q)| q);
        let outflank_side = outflank_point.map_or(1.0, side_of);
        let both_sides = outflank::point_test(&target, d, -outflank::SIDE_ANGLE, area.as_ref(), &valid).is_some()
            && outflank::point_test(&target, d, outflank::SIDE_ANGLE, area.as_ref(), &valid).is_some();
        WingBidder {
            phase: ai.phase,
            of_seeds: seeds[1],
            de_seeds: seeds[2],
            ours: strength(&own),
            theirs: strength(&enemies),
            own_count: own.len(),
            enemy_count: enemies.len(),
            our_class8: pct(&own, &class8),
            their_class8: pct(&enemies, &class8),
            our_mounted: pct(&own, &mounted),
            their_mounted: pct(&enemies, &mounted),
            eligible,
            outflank_side,
            outflank_point,
            both_sides,
        }
    }

    /// The side a unit stands on (`0x007D0870`: same side of the group's axis as the point).
    fn side_of(&self, id: u32) -> f32 {
        *self.eligible.get(&id).unwrap_or(&1.0)
    }

    /// The common weight terms: by strength and unit counts, and the class-8 term (+5 when the
    /// target has more than 30 % class-8 men, +7 when also more than ours).
    fn weight(&self, base: i32, weaker_more: i32, weaker_fewer: i32) -> i32 {
        let counts = if self.ours < self.theirs {
            if self.enemy_count < self.own_count { weaker_more } else { weaker_fewer }
        } else if self.enemy_count < self.own_count {
            4
        } else {
            0
        };
        let class8 = if self.their_class8 > 30 {
            if self.our_class8 < self.their_class8 { 7 } else { 5 }
        } else {
            0
        };
        base + counts + class8
    }
}

impl auction::Bidder for WingBidder {
    fn phase(&self) -> u8 {
        self.phase as u8
    }

    fn filter(&self, kind: auction::Kind, unit: u32) -> u8 {
        use auction::Kind;
        match kind {
            Kind::AttackBattlegroup | Kind::StopAndShoot => 1,
            Kind::LimberedArtillery => 0,
            Kind::Outflank => match self.eligible.get(&unit) {
                Some(&s) if s == self.outflank_side => 2,
                Some(_) => 1,
                None => 0,
            },
            Kind::DoubleEnvelopment => {
                if self.eligible.contains_key(&unit) { 2 } else { 0 }
            }
        }
    }

    /// Scores (CONFIRMED): ATTACK_BATTLEGROUP 1 (`0x007B1700`); OUTFLANK `100 + seed2 % 65`
    /// (`0x007B1980`) in CLOSE APPROACH (`seed0 % 4 != 0`) or CONTACT, with more than two free
    /// units, a target of more than two units, ours ≥ 0.6 × theirs and a positive weight (3, or 8
    /// on high ground; weaker: ±8 by unit counts; stronger with more units +4; class-8 +5/+7);
    /// DOUBLE_ENVELOPMENT `80 + seed1 % 65` (`0x007B1720`) in CONTACT, a target of more than four
    /// units, `theirs × 4 / 5 ≤ ours` and a positive weight (1, or 6 on high ground; weaker: ±6;
    /// stronger with more units +4; class-8 +5/+7; +6 when our mounted % beats 1.5 × theirs).
    /// STOP_AND_SHOOT is scored by the line FSM, LIMBERED_ARTILLERY is not modelled.
    fn score(&mut self, kind: auction::Kind, free: &[u32]) -> f32 {
        use auction::Kind;
        use phase::Phase;
        match kind {
            Kind::AttackBattlegroup => {
                if free.is_empty() { 0.0 } else { 1.0 }
            }
            Kind::Outflank => {
                let ok = matches!(self.phase, Phase::CloseApproach | Phase::Contact)
                    && free.len() > 2
                    && (self.phase != Phase::CloseApproach || !self.of_seeds[0].is_multiple_of(4))
                    && self.enemy_count > 2
                    && self.theirs as f32 * 0.6 <= self.ours as f32
                    && self.weight(3, 8, -8) > 0
                    && self.outflank_point.is_some();
                if ok { 100.0 + (self.of_seeds[2] % 65) as f32 } else { 0.0 }
            }
            Kind::DoubleEnvelopment => {
                let mounted = if self.our_mounted > self.their_mounted * 3 / 2 { 6 } else { 0 };
                let ok = self.phase == Phase::Contact
                    && self.enemy_count > 4
                    && self.theirs * 4 / 5 <= self.ours
                    && self.weight(1, 6, -6) + mounted > 0
                    && self.both_sides;
                if ok { 80.0 + (self.de_seeds[1] % 65) as f32 } else { 0.0 }
            }
            Kind::StopAndShoot | Kind::LimberedArtillery => 0.0,
        }
    }

    /// Keep tests: ATTACK_BATTLEGROUP while it has units (`0x0075EA40`); OUTFLANK and
    /// DOUBLE_ENVELOPMENT (`0x0075EC50`) in phases CLOSE APPROACH .. GENERAL MELEE while a
    /// sub-move still runs (PROVISIONAL: while one of their units still fights); STOP_AND_SHOOT
    /// by the line FSM.
    fn keeps(&mut self, kind: auction::Kind, owned: &std::collections::BTreeSet<u32>) -> bool {
        use auction::Kind;
        match kind {
            Kind::Outflank | Kind::DoubleEnvelopment => {
                self.phase >= phase::Phase::CloseApproach && !owned.is_empty()
            }
            Kind::StopAndShoot => true,
            _ => !owned.is_empty(),
        }
    }

    fn claim(&mut self, kind: auction::Kind, active: bool, free: &[u32]) -> auction::Claim {
        use auction::{Claim, Kind};
        match kind {
            Kind::AttackBattlegroup | Kind::LimberedArtillery => Claim::All,
            Kind::StopAndShoot => Claim::None,
            _ if active => Claim::None,
            Kind::Outflank => auction::outflank_claim(free.len(), self.of_seeds[0]),
            Kind::DoubleEnvelopment => {
                let eligible = free.iter().filter(|u| self.eligible.contains_key(u)).count();
                auction::double_envelopment_claim(free.len(), eligible, self.de_seeds[0])
            }
        }
    }
}

fn attack_name(s: AttackState) -> &'static str {
    match s {
        AttackState::ChangeFormation => "Change Formation",
        AttackState::Reform => "Reform",
        AttackState::MoveToFormUp => "Move to Form-up",
        AttackState::MoveToTarget => "Move to Target",
        AttackState::Outflank => "Outflank",
    }
}

fn defend_name(s: DefendState) -> &'static str {
    match s {
        DefendState::ChangeFormation => "Change Formation",
        DefendState::ChangeUnitFormations => "Change Unit Formations",
        DefendState::Reform => "Reform",
        DefendState::DefendLine => "Defend Line",
    }
}

/// The state names as the original's descriptors spell them (CONFIRMED).
fn shoot_name(s: ShootState) -> &'static str {
    match s {
        ShootState::AdvancingTowardsLine => "ADVANCING_TOWARDS_LINE",
        ShootState::FormOnLine => "FORM_ON_LINE",
        ShootState::CreepForward => "CREEP_FORWARD",
        ShootState::HoldTheLine => "HOLD_THE_LINE",
    }
}

/// Runs a battle headless with the AI controlling `ai_sides`, until it is decided or `max_ticks`
/// pass. Returns the number of ticks run. Handy for tests and tools.
pub fn run_headless(battle: &mut Battle, ai: &mut BattleAi, max_ticks: u32) -> u32 {
    use ntw_sim::battle::model::BattleResult;
    let mut n = 0;
    while battle.battle_result() == BattleResult::Ongoing && n < max_ticks {
        ai.update(battle);
        battle.step();
        n += 1;
    }
    n
}

/// The target battlegroup's rectangle for the outflank point test (PROVISIONAL stand-in for the
/// `0x0078B070` record: the enemy army). Facing: the mean of the enemy units' facings (else
/// towards us, against `our_dir`); width / depth: the units' spread across / along it plus their
/// frontage / 10 m.
fn enemy_rect(enemies: &[&LandUnit], centre: P, our_dir: P) -> outflank::TargetRect {
    let sum = enemies.iter().fold((0.0f32, 0.0f32), |a, u| add(a, (u.facing.cos(), u.facing.sin())));
    let f = norm_or(sum, scale(our_dir, -1.0));
    let r = right_of(f);
    let (mut w, mut dpt) = (0.0f32, 0.0f32);
    for e in enemies {
        let d = sub(e.position, centre);
        w = w.max(dot(d, r).abs() + frontage(e) / 2.0);
        dpt = dpt.max(dot(d, f).abs() + 5.0);
    }
    outflank::TargetRect { centre, angle: outflank::to16(f.1.atan2(f.0)), width: w * 2.0, depth: dpt * 2.0 }
}
