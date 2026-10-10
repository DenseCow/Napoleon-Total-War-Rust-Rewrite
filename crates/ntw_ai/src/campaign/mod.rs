//! # Campaign AI v1
//!
//! One call per AI faction per turn: [`take_turn`] reads the campaign model and returns a list of
//! [`AiOrder`]s (recruit, construct, move armies, declare war, make peace). The caller (the turn
//! loop) applies them, e.g. through [`AiOrder::to_campaign_command`] and
//! `CampaignModel::apply`. The AI never changes the model itself and uses only the RNG it is
//! given, so the same model + seed always gives the same orders.
//!
//! ## Structure (mirrors the original, `analysis/ai/AI_RESEARCH.md` §4)
//! The original's campaign AI (`EmpireCampaign\Source\CAI\`, CONFIRMED) is a BDI system
//! (beliefs, desires, intentions) run by a per-faction **manager**. A manager is a DB row
//! (`campaign_ai_managers`) with a list of **behaviours** and their priorities
//! (`campaign_ai_manager_behaviour_junctions`: WAR_AND_PEACE, REGION_DEFENCE, EXPANSION_BEHAVIOUR,
//! GLOBAL_CONSTRUCTION_BEHAVIOUR, ...). A **personality** (`campaign_ai_personalities` and its
//! ~245 tunables) sets budgets and thresholds. v1 follows that shape:
//! 1. resolve the faction's manager and personality;
//! 2. budget the treasury with the personality's `BASIC_SPENDING_BIAS_*` and savings tunables;
//! 3. run the turn as the original's **BDI pool** does ([`bdi`], CONFIRMED processing): one
//!    component per behaviour row, priorities jittered by the raw `PRIORITY_RANDOMIZATION_DESIRE`
//!    with the given RNG, desires and goals deliberated in pool order, then the intentions act in
//!    priority order ([`Node`], [`Action`]).
//!
//! Which manager/personality a faction uses is CONFIRMED (round 4): the startpos/save `FACTION`
//! record stores both keys (the model's `World::ai_keys`, in [`AiFaction::ai_keys`]); without
//! them a PROVISIONAL naming rule is used, see [`FactionAiConfig::resolve`].

pub mod bdi;
pub mod data;
pub mod desires;
pub mod driver;
pub mod keys;
pub use ntw_sim::campaign::region_value;
pub mod research;
pub mod world;

use std::collections::{BTreeMap, BTreeSet};

use ntw_sim::campaign::negotiation::NegotiationAction;
use ntw_sim::campaign::rules::TaxClass;
use ntw_sim::campaign::{CampaignCommand, CampaignEvent, CampaignModel, CommandError, FactionId, ForceId, RegionId, SlotRef, Stance};
use ntw_sim::fixed::Fixed20;
use ntw_sim::rng::CaRng;

pub use data::CampaignAiData;
pub use research::ResearchCand;
pub use world::{AiArmy, AiFaction, AiRegion, AiSchool, AiWorld};

/// What the campaign scripts told the AI (see `ntw_script::ScriptState`; filled by the caller).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptHints {
    /// `add_restricted_unit_record` keys: never recruited.
    pub restricted_units: BTreeSet<String>,
    /// `set_campaign_ai_force_all_factions_boardering_humans_to_have_invasion_behaviour(true)`.
    pub invade_humans: bool,
    /// Faction keys whose armies scripts froze with `disable_movement_for_character` (by faction,
    /// PROVISIONAL granularity: the script call names a character).
    pub frozen_factions: BTreeSet<String>,
}

/// One AI decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiOrder {
    /// Recruit `unit_key` in `region`.
    Recruit {
        /// Region (owned by the faction).
        region: RegionId,
        /// Unit key.
        unit_key: String,
    },
    /// Build (or upgrade to) `level_key` in slot `slot` of `region`
    /// (`CampaignCommand::ConstructBuilding`).
    Construct {
        /// Region.
        region: RegionId,
        /// The slot.
        slot: SlotRef,
        /// Building level key.
        level_key: String,
    },
    /// Move an army (towards) a point.
    MoveForce {
        /// The force.
        force: ForceId,
        /// Destination (fixed point).
        to: (Fixed20, Fixed20),
        /// Why (for logs and tests).
        purpose: MovePurpose,
    },
    /// Attack an enemy army (`CampaignCommand::AttackForce`: moves to it and fights when it
    /// arrives).
    AttackForce {
        /// Our force.
        force: ForceId,
        /// The enemy force.
        target: ForceId,
    },
    /// Declare war.
    DeclareWar {
        /// Us.
        a: FactionId,
        /// Them.
        b: FactionId,
    },
    /// Make peace.
    MakePeace {
        /// Us.
        a: FactionId,
        /// Them.
        b: FactionId,
    },
    /// Army merge (the PROVISIONAL MERGE_UNITS stand-in): `force` joins `into` (`CampaignCommand::MergeForces`).
    Merge {
        /// The force that walks over and gives its units.
        force: ForceId,
        /// The force that receives them.
        into: ForceId,
    },
    /// `TAXATION`: set a class's tax level (`CampaignCommand::SetTaxLevel`).
    SetTax {
        /// Us.
        faction: FactionId,
        /// 0 lower classes, 1 upper classes.
        class: u8,
        /// `taxes_levels` key.
        level: String,
    },
    /// `RESEARCH_TECHNOLOGY`: start researching `tech` at the school in slot `slot` of `region`
    /// (`CampaignCommand::StartResearch`).
    StartResearch {
        /// The region's school.
        region: RegionId,
        /// Slot index in `Region::slots`.
        slot: usize,
        /// Technology key.
        tech: String,
    },
}

/// Why an army moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MovePurpose {
    /// `REGION_DEFENCE`: towards a threatened own settlement.
    Defend(RegionId),
    /// `EXPANSION_BEHAVIOUR`: towards an enemy settlement.
    Attack(RegionId),
}

impl AiOrder {
    /// The campaign model's command for this order. A move with a settlement purpose becomes
    /// `EnterSettlement` (own settlement: garrison it; enemy one: occupy or assault it on
    /// arrival); the model walks as far as the commander's action points reach.
    pub fn to_campaign_command(&self) -> Option<CampaignCommand> {
        Some(match self {
            AiOrder::Recruit { region, unit_key } => CampaignCommand::Recruit { region: *region, unit_key: unit_key.clone(), target: None },
            AiOrder::Construct { region, slot, level_key } => {
                CampaignCommand::ConstructBuilding { region: *region, slot: *slot, level_key: level_key.clone() }
            }
            AiOrder::MoveForce { force, purpose: MovePurpose::Defend(region) | MovePurpose::Attack(region), .. } => {
                CampaignCommand::EnterSettlement { force: *force, region: *region }
            }
            AiOrder::AttackForce { force, target } => CampaignCommand::AttackForce { force: *force, target: *target },
            AiOrder::DeclareWar { a, b } => CampaignCommand::DeclareWar { a: *a, b: *b },
            AiOrder::MakePeace { a, b } => CampaignCommand::MakePeace { a: *a, b: *b },
            AiOrder::Merge { force, into } => CampaignCommand::MergeForces { force: *force, into: *into },
            AiOrder::SetTax { faction, class, level } => CampaignCommand::SetTaxLevel {
                faction: *faction,
                class: if *class == 0 { TaxClass::Lower } else { TaxClass::Upper },
                level: level.clone(),
            },
            AiOrder::StartResearch { region, slot, tech } => {
                CampaignCommand::StartResearch { region: *region, slot: *slot, tech: tech.clone() }
            }
        })
    }
}

/// Which manager and personality a faction runs.
#[derive(Debug, Clone, PartialEq)]
pub struct FactionAiConfig {
    /// `campaign_ai_managers` key.
    pub manager: String,
    /// `campaign_ai_personalities` key.
    pub personality: String,
    /// Behaviour → priority (from the manager).
    pub behaviours: BTreeMap<String, f32>,
}

impl FactionAiConfig {
    /// PROVISIONAL naming rule (the original's assignment is UNKNOWN): manager
    /// `nap_<prefix>_<faction>` if it exists, else `nap_<prefix>_full`; personality
    /// `<prefix>_<faction>` if it exists, else the default. `<prefix>` is the campaign key's
    /// part after an optional `mp_` (e.g. `eur` from `eur_napoleon`). The shipped data fits this
    /// rule (`nap_eur_france` + `eur_france`, `nap_eur_britain`).
    pub fn resolve(data: &CampaignAiData, campaign_key: &str, faction_key: &str) -> Self {
        let c = campaign_key.strip_prefix("mp_").unwrap_or(campaign_key);
        let prefix = c.split('_').next().unwrap_or(c);
        let own = format!("nap_{prefix}_{faction_key}");
        let manager = if data.managers.contains_key(&own) {
            own
        } else {
            let full = format!("nap_{prefix}_full");
            if data.managers.contains_key(&full) { full } else { "nap_eur_full".to_string() }
        };
        let p = format!("{prefix}_{faction_key}");
        let personality = if data.personalities.contains_key(&p) { p } else { data.default_personality.clone() };
        let behaviours = data.managers.get(&manager).cloned().unwrap_or_default();
        FactionAiConfig { manager, personality, behaviours }
    }

    /// The faction's own keys from its `FACTION` record (CONFIRMED source, [`keys`]); each key
    /// that is missing or not in its table falls back to [`FactionAiConfig::resolve`]'s rule
    /// (the original logs "not a valid key for this table" there; its fallback is UNKNOWN).
    pub fn resolve_with(data: &CampaignAiData, campaign_key: &str, faction_key: &str, stored: Option<&keys::FactionAiKeys>) -> Self {
        let mut cfg = Self::resolve(data, campaign_key, faction_key);
        if let Some(k) = stored {
            if let Some(b) = data.managers.get(&k.manager) {
                cfg.manager = k.manager.clone();
                cfg.behaviours = b.clone();
            }
            if data.personalities.contains_key(&k.personality) {
                cfg.personality = k.personality.clone();
            }
        }
        cfg
    }
}

/// The behaviours v1 implements, by their DB names (CONFIRMED names).
pub const IMPLEMENTED: [&str; 8] = [
    "MERGE_UNITS",
    "TAXATION",
    "WAR_AND_PEACE",
    "REGION_DEFENCE",
    "EXPANSION_BEHAVIOUR",
    "GLOBAL_CONSTRUCTION_BEHAVIOUR",
    "EXCESS_RECRUITMENT_BEHAVIOUR",
    "RESEARCH_TECHNOLOGY",
];

/// Settings the caller chooses.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnContext {
    /// Campaign key, e.g. `eur_napoleon` (for the manager/personality rule).
    pub campaign_key: String,
    /// Human-controlled factions (never run by the AI; their armies count as enemies normally).
    pub humans: BTreeSet<FactionId>,
    /// Script hints.
    pub hints: ScriptHints,
}

impl TurnContext {
    /// A context for `campaign_key` with no humans and no script hints. The difficulty reaches the AI
    /// through the model: the factions' handicaps are in their effects
    /// (`ntw_sim::campaign::effects::apply_start_handicaps`, `0x008DD090`), so the prices it reads hold them.
    pub fn new(campaign_key: &str) -> Self {
        TurnContext {
            campaign_key: campaign_key.to_string(),
            humans: BTreeSet::new(),
            hints: ScriptHints::default(),
        }
    }
}

/// `ntw_ai::campaign::take_turn`: the decisions of AI faction `faction` for this turn.
pub fn take_turn(model: &CampaignModel, data: &CampaignAiData, ctx: &TurnContext, faction: FactionId, rng: &mut CaRng) -> Vec<AiOrder> {
    let world = AiWorld::from_model_for(model, Some(faction));
    take_turn_on(&world, data, ctx, faction, rng)
}

/// Like [`take_turn`], on an already built [`AiWorld`] snapshot (lets other model versions feed
/// the AI through their own adapter).
pub fn take_turn_on(world: &AiWorld, data: &CampaignAiData, ctx: &TurnContext, faction: FactionId, rng: &mut CaRng) -> Vec<AiOrder> {
    let Some(me) = world.factions.get(&faction) else { return Vec::new() };
    if ctx.humans.contains(&faction) || me.regions == 0 && world.armies.values().all(|a| a.faction != faction) {
        return Vec::new();
    }
    let cfg = FactionAiConfig::resolve_with(data, &ctx.campaign_key, &me.key, me.ai_keys.as_ref());
    let mut turn = FactionTurn::new(world, data, ctx, faction, cfg);
    turn.run(rng);
    turn.orders
}

/// Like [`take_turn_on`], also returning the faction's BDI pool after the run: every component
/// ([`Node`]) with its final priority, in creation order (for tests and AI logs).
pub fn take_turn_with_plan(world: &AiWorld, data: &CampaignAiData, ctx: &TurnContext, faction: FactionId, rng: &mut CaRng) -> (Vec<AiOrder>, Vec<(Node, f32)>) {
    let Some(me) = world.factions.get(&faction) else { return (Vec::new(), Vec::new()) };
    if ctx.humans.contains(&faction) || me.regions == 0 && world.armies.values().all(|a| a.faction != faction) {
        return (Vec::new(), Vec::new());
    }
    let cfg = FactionAiConfig::resolve_with(data, &ctx.campaign_key, &me.key, me.ai_keys.as_ref());
    let mut turn = FactionTurn::new(world, data, ctx, faction, cfg);
    turn.run(rng);
    (turn.orders, turn.last_pool)
}


/// The work of one faction's turn.
struct FactionTurn<'a> {
    world: &'a AiWorld,
    data: &'a CampaignAiData,
    ctx: &'a TurnContext,
    faction: FactionId,
    key: String,
    cfg: FactionAiConfig,
    construction_budget: i32,
    recruitment_budget: i32,
    /// Upkeep new units may still add this turn (`None`: unknown, no limit).
    upkeep_room: Option<i32>,
    /// Forces already given a move this turn.
    busy: BTreeSet<ForceId>,
    /// Regions already given a recruit / construct order.
    recruited: BTreeSet<RegionId>,
    built: BTreeSet<RegionId>,
    /// Units ordered per region this turn (EXCESS_RECRUITMENT intentions).
    recruits_in: BTreeMap<RegionId, u32>,
    /// Units ordered this turn per unit key, held against each entry's `cap_room` (the model's unit cap).
    ordered_units: BTreeMap<String, usize>,
    /// Units ordered this turn per (region, naval queue), held against each entry's `queue_room`.
    queued_in: BTreeMap<(RegionId, bool), usize>,
    /// Schools already given a RESEARCH_TECHNOLOGY intention this turn (`(region, slot)`), so two
    /// intentions cannot take the same school (the original reserves a school per intention,
    /// CONFIRMED `0x00A74B50` / `0x00A614A0`, which also replaces the school of a gentleman that
    /// already had one).
    researching: BTreeSet<(RegionId, usize)>,
    orders: Vec<AiOrder>,
    /// The pool after the last run: every component and its final priority (for tests and logs).
    last_pool: Vec<(Node, f32)>,
}

impl<'a> FactionTurn<'a> {
    fn new(world: &'a AiWorld, data: &'a CampaignAiData, ctx: &'a TurnContext, faction: FactionId, cfg: FactionAiConfig) -> Self {
        let key = world.factions[&faction].key.clone();
        FactionTurn {
            world,
            data,
            ctx,
            faction,
            key,
            cfg,
            construction_budget: 0,
            recruitment_budget: 0,
            upkeep_room: None,
            busy: BTreeSet::new(),
            recruited: BTreeSet::new(),
            built: BTreeSet::new(),
            recruits_in: BTreeMap::new(),
            ordered_units: BTreeMap::new(),
            queued_in: BTreeMap::new(),
            researching: BTreeSet::new(),
            orders: Vec::new(),
            last_pool: Vec::new(),
        }
    }

    fn t(&self, key: &str, fallback: f32) -> f32 {
        self.data.tunable(&self.cfg.personality, key).unwrap_or(fallback)
    }

    /// The turn as the original's BDI pool runs it ([`bdi`], CONFIRMED processing): one
    /// component per manager junction row (base = the row's priority, jittered by the raw
    /// `PRIORITY_RANDOMIZATION_DESIRE`), deliberated in pool order; their desires and goals are
    /// deliberated as they appear; then the intentions act, highest priority first. The tree
    /// below each behaviour is [`Node`]; what is CONFIRMED and PROVISIONAL is noted there.
    /// Rows are added in name order (PROVISIONAL: the original adds them in junction row order,
    /// which only matters for exact ties and the RNG draw order).
    fn run(&mut self, rng: &mut CaRng) {
        self.budget();
        // WAR_AND_PEACE / DIPLOMACY_MANAGER rows build no behaviour (CONFIRMED, `0x00C90960`); the
        // original's diplomacy is the faction pool's own war-and-peace manager (not decoded).
        // PROVISIONAL: our v1 diplomacy rule runs first, for every AI faction.
        self.war_and_peace();
        let mut pool: bdi::Pool<Node> =
            bdi::Pool::new(self.t("PRIORITY_RANDOMIZATION_DESIRE", 0.0), self.t("PRIORITY_RANDOMIZATION_INTENTION", 0.0));
        let rows: Vec<(String, f32)> = self.cfg.behaviours.iter().map(|(k, v)| (k.clone(), *v)).collect();
        for (name, p) in rows {
            if name == "WAR_AND_PEACE" || name == "DIPLOMACY_MANAGER" {
                continue;
            }
            pool.add(bdi::List::Desire, p, Node::Behaviour(name), rng);
        }
        // The component the manager always adds after the rows (`0x00C74990(10.0)`, id 0xDC,
        // intention list; its step `0x00CCD420` is not decoded).
        pool.add(bdi::List::Intention, 10.0, Node::Other(0xDC), rng);
        let mut d = TurnDeliberation { turn: self, rng, claimed: BTreeSet::new() };
        pool.run(&mut d);
        self.last_pool = pool.iter().map(|(_, c)| (c.payload.clone(), c.priority())).collect();
    }

    /// Splits the spendable treasury. CONFIRMED tunables (`BASIC_INCOME_PROPORTIONAL_SAVINGS_PER_TURN`,
    /// `BASIC_SPENDING_BIAS_*`); PROVISIONAL use: savings are that percentage of the gross income
    /// (of the treasury when the snapshot has no income), the rest of the treasury is split by the
    /// biases normalised over all four. The navy and diplomacy shares are kept (v1 neither builds
    /// navies nor pays in diplomacy). With a known income, new units may only add upkeep while
    /// total upkeep stays within the income minus those savings (PROVISIONAL guard against
    /// bankruptcy; the original's financial analysers are UNKNOWN).
    fn budget(&mut self) {
        let me = &self.world.factions[&self.faction];
        let treasury = me.treasury.max(0) as f32;
        let savings_pct = self.t("BASIC_INCOME_PROPORTIONAL_SAVINGS_PER_TURN", 10.0) / 100.0;
        let savings = me.income.map_or(treasury, |i| i.max(0) as f32) * savings_pct;
        if let (Some(income), Some(upkeep)) = (me.income, me.upkeep) {
            self.upkeep_room = Some((income.max(0) as f32 * (1.0 - savings_pct)) as i32 - upkeep);
        }
        let spend = (treasury - savings).max(0.0);
        let c = self.t("BASIC_SPENDING_BIAS_CONSTRUCTION", 0.25);
        let a = self.t("BASIC_SPENDING_BIAS_RECRUITMENT_ARMY", 0.25);
        let n = self.t("BASIC_SPENDING_BIAS_RECRUITMENT_NAVY", 0.4);
        let d = self.t("BASIC_SPENDING_BIAS_DIPLOMATIC", 0.1);
        let total = (c + a + n + d).max(1e-6);
        self.construction_budget = (spend * c / total) as i32;
        self.recruitment_budget = (spend * a / total) as i32;
    }

    fn at_war_with(&self, other: FactionId) -> bool {
        self.world.stance(self.faction, other) == Stance::War
    }

    /// Strength of everything `f` has within `radius` of `pos` (armies and garrisons).
    fn strength_near(&self, f: FactionId, pos: (f64, f64), radius: f64) -> f32 {
        self.world
            .armies
            .values()
            .filter(|a| a.faction == f && !a.is_navy && world::dist(a.position, pos) <= radius)
            .map(|a| self.world.army_strength(a, self.data))
            .sum()
    }

    /// Strength of every enemy (at war) within `radius` of `pos`.
    fn enemy_strength_near(&self, pos: (f64, f64), radius: f64) -> f32 {
        self.world
            .armies
            .values()
            .filter(|a| a.faction != self.faction && !a.is_navy && self.at_war_with(a.faction) && world::dist(a.position, pos) <= radius)
            .map(|a| self.world.army_strength(a, self.data))
            .sum()
    }

    /// `EXCESS_RECRUITMENT_BEHAVIOUR` (ctor `0x00D26BB0`, id 0x16D, step `0x00D68380`, CONFIRMED
    /// structure, round 6; round 4 filed it under MERGE_UNITS): the **new-unit recruitment**; it makes
    /// `CAI_BDI_UNIT_RECRUITMENT_NEW` intentions (class from the save `0x00CA7230`). Entries: the
    /// faction's regions that have recruitment options (`CAI_UNIT_AVAILABILITY_ANALYSIS_REGION`,
    /// 16-byte options with two costs); each weighted by `x²`, `x` = its
    /// `CAI_REGION_DEFENCE_STRENGTH_ANALYSIS` value, turned into `1 − x²/Σx²` and normalised to
    /// sum 1 (all `1/n` when that sum is 0; with `Σx² = 0` the original divides by zero and the
    /// NaN weights make the walk stop at the first entry — reproduced). Then up to `2n` draws while
    /// both budgets are positive: `u = rng.float_range(0, 1)`, walk the weights (`u −= w` while
    /// `u > 0`), option `rng.int_range(0, options − 1)`, subtract its costs, one intention linked
    /// with mult 1.0. PROVISIONAL inputs: `x` = our land strength within
    /// [`world::GARRISON_RADIUS`] (rounded), the options = the region's recruitable land units by
    /// key (the model's unflagged entries, [`world::AiRecruitable`]), the unit budget = Σ free recruitment
    /// capacity of the entries with cost 1 per unit (the original's analyser `+0x60` is UNKNOWN), the money =
    /// our recruitment budget (the original: treasury minus two committed amounts) with the entry's cost (the
    /// model's, what the queue command charges); region order = id.
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(0.0 < u)` keeps the exe's NaN handling
    fn recruitment_draws(&self, rng: &mut CaRng) -> Vec<(RegionId, String, i32)> {
        // (region, its (unit key, cost) options, x²)
        type Entry = (RegionId, Vec<(String, i32)>, f32);
        let mut entries: Vec<Entry> = Vec::new();
        let mut unit_budget = 0.0f32;
        for r in self.world.regions.values().filter(|r| r.owner == self.faction) {
            let opts: Vec<(String, i32)> = r
                .recruitable
                .iter()
                .filter(|e| e.flags == 0 && e.cost > 0 && !self.ctx.hints.restricted_units.contains(&e.unit_key))
                .map(|e| (e.unit_key.clone(), e.cost))
                .collect();
            if opts.is_empty() {
                continue;
            }
            let x = self.strength_near(self.faction, r.position, world::GARRISON_RADIUS).round() as i32;
            unit_budget += r.recruit_capacity.unwrap_or(if r.recruiting > 0 { 0 } else { 1 }) as f32;
            entries.push((r.id, opts, (x as f32) * (x as f32)));
        }
        let sum: f32 = entries.iter().map(|e| e.2).sum();
        let mut w: Vec<f32> = entries.iter().map(|e| 1.0 - (1.0 / sum) * e.2).collect();
        let total: f32 = w.iter().sum();
        if total == 0.0 {
            let even = 1.0 / entries.len() as f32;
            w.iter_mut().for_each(|v| *v = even);
        } else {
            w.iter_mut().for_each(|v| *v *= 1.0 / total);
        }
        let mut money = self.recruitment_budget as f32;
        let mut out = Vec::new();
        let mut count = entries.len() * 2;
        while unit_budget > 0.0 && money > 0.0 && count != 0 {
            count -= 1;
            let mut u = rng.float_range(0.0, 1.0);
            let mut pick = None;
            for (i, wi) in w.iter().enumerate() {
                u -= wi;
                if !(0.0 < u) {
                    pick = Some(i);
                    break;
                }
            }
            let Some(i) = pick else { continue };
            let (rid, opts, _) = &entries[i];
            let k = rng.int_range(0, opts.len() as i32 - 1) as usize;
            let (unit, cost) = &opts[k];
            unit_budget -= 1.0;
            money -= *cost as f32;
            out.push((*rid, unit.clone(), *cost));
        }
        out
    }

    /// A `CAI_BDI_UNIT_RECRUITMENT_NEW` intention acting: recruit `unit` in `rid` when the region
    /// still has capacity, the money and the upkeep room allow it (PROVISIONAL checks).
    fn recruit_unit(&mut self, rid: RegionId, unit: &str, cost: i32) {
        let r = &self.world.regions[&rid];
        let cap = r.recruit_capacity.unwrap_or(if r.recruiting > 0 { 0 } else { 1 });
        let used = self.recruits_in.get(&rid).copied().unwrap_or(0);
        let upkeep = self.data.units.get(unit).map_or(0, |i| i.upkeep);
        if used >= cap || cost > self.recruitment_budget || self.upkeep_room.is_some_and(|room| upkeep > room) || self.no_room_for(rid, unit) {
            return;
        }
        self.recruitment_budget -= cost;
        if let Some(room) = &mut self.upkeep_room {
            *room -= upkeep;
        }
        *self.recruits_in.entry(rid).or_default() += 1;
        self.order_recruit(rid, unit);
    }

    /// True when this turn's orders already use up what the model left for `unit` in `rid`: the room its unit
    /// cap leaves (the entry's `cap_room`) or the free places of the region's queue of its kind
    /// (`queue_room`, [`world::AiRecruitable`]), so one more would be refused. A unit with no entry has none.
    fn no_room_for(&self, rid: RegionId, unit: &str) -> bool {
        let Some(e) = self.world.regions[&rid].recruitable.iter().find(|e| e.unit_key == unit) else { return true };
        let queued = self.queued_in.get(&(rid, e.naval)).copied().unwrap_or(0);
        queued >= e.queue_room || e.cap_room.is_some_and(|room| self.ordered_units.get(unit).copied().unwrap_or(0) >= room)
    }

    /// Records a recruit order of `unit` in `rid` against the cap and queue rooms ([`Self::no_room_for`]).
    fn order_recruit(&mut self, rid: RegionId, unit: &str) {
        let naval = self.world.regions[&rid].recruitable.iter().find(|e| e.unit_key == unit).is_some_and(|e| e.naval);
        *self.queued_in.entry((rid, naval)).or_default() += 1;
        *self.ordered_units.entry(unit.to_string()).or_default() += 1;
        self.recruited.insert(rid);
        self.orders.push(AiOrder::Recruit { region: rid, unit_key: unit.to_string() });
    }

    /// Army merging, PROVISIONAL stand-in for the region-group goal's undecoded missions (the
    /// original has `CAI_BDIM_MULTI_MERGE_AT` / `CAI_BDIM_MERGE_UNITS` missions; who makes them is
    /// UNKNOWN; MERGE_UNITS itself is recruitment, [`Self::recruitment_draws`]): smallest land
    /// armies first, each joins the nearest other own land army that it can reach this turn
    /// (movement points / road cost) and that is at least as big, when the two fit in one force
    /// (the campaign's units per army, [`AiWorld::max_units_per_army`]). Garrisons (no commander)
    /// never move. Runs once per turn.
    fn merge_units(&mut self) {
        let me = self.faction;
        let cap = self.world.max_units_per_army;
        let per_unit = self.world.move_cost_per_unit.max(f32::EPSILON) as f64;
        let mut armies: Vec<&AiArmy> = self
            .world
            .armies
            .values()
            .filter(|a| a.faction == me && !a.is_navy && a.commander.is_some() && !a.units.is_empty())
            .collect();
        armies.sort_by(|a, b| a.units.len().cmp(&b.units.len()).then(a.id.cmp(&b.id)));
        // Planned unit counts (a merge moves units before later ones are planned).
        let mut size: BTreeMap<ForceId, usize> = armies.iter().map(|a| (a.id, a.units.len())).collect();
        let mut gone: BTreeSet<ForceId> = BTreeSet::new();
        for a in &armies {
            if self.busy.contains(&a.id) || gone.contains(&a.id) || a.movement_points <= 0 {
                continue;
            }
            let reach = a.movement_points as f64 / per_unit;
            let n = size[&a.id];
            let best = armies
                .iter()
                .filter(|b| b.id != a.id && !gone.contains(&b.id))
                .filter(|b| size[&b.id] >= n && size[&b.id] + n <= cap)
                .map(|b| (world::dist(a.position, b.position), b.id))
                .filter(|(d, _)| *d <= reach)
                .min_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
            if let Some((_, into)) = best {
                self.orders.push(AiOrder::Merge { force: a.id, into });
                self.busy.insert(a.id);
                gone.insert(a.id);
                *size.get_mut(&into).expect("planned") += n;
            }
        }
    }

    /// `TAXATION` (ctor `0x00BE04A0`, id 0x139, step `0x00C2BBB0`, CONFIRMED structure, round 6):
    /// the refresh (`0x00C5EF60`) re-arms the behaviour once its earlier intentions are gone; the
    /// step drops its old intentions, then per tax domain of the faction (list `+0x1A8`): one
    /// `CAI_BDI_TAX_EXEMPT_REGION` intention (id 0xE4) per region whose exemption flag (`+0xE4`)
    /// differs from the analysis' choice (belief `0x00A74CF0`, list `+0x114`), and for each class
    /// (0 lower, 1 upper) whose level is not **2**, one `CAI_BDI_SET_TAX_LEVEL` intention (id 0xE3)
    /// to level 2; all linked with mult 1.0. So the AI always runs level 2. INFERRED: level index
    /// 2 = the third `taxes_levels` row by rate ("normal" of five). PROVISIONAL: one domain (the
    /// faction), no exemptions (the analysis is not decoded and the model has no exemption).
    fn taxation(&mut self) {
        let me = self.faction;
        let Some((level, _)) = self.world.tax_levels.get(2).cloned() else { return };
        let current = self.world.factions[&me].tax.clone();
        for (class, cur) in current.iter().enumerate() {
            if *cur != level {
                self.orders.push(AiOrder::SetTax { faction: me, class: class as u8, level: level.clone() });
            }
        }
    }

    /// `RESEARCH_TECHNOLOGY` (ctor `0x00BDFF00`, id 0x10F, vtable `0x01382578`, step `0x00C2AD60`,
    /// CONFIRMED structure round 7). The step collects the candidate entries, sorts them by the
    /// exe's own key **descending** (skipping −1) and makes one `CAI_BDI_GOAL_RESEARCH_TECHNOLOGY`
    /// goal (vtable `0x013825B4`, type 0x110) per entry, then links them with **mult 1.0, ×0.95,
    /// ×0.9025, … in list order** (CONFIRMED: `FUN_00CB2560(goal, 0, m, 0)` with `m` starting at
    /// `0x3F800000` and multiplied by `0.95` after each entry, `0x00C2AD60`).
    ///
    /// CONFIRMED: the goals are **per gentleman** (`goal+0x10C` is a character, slot 3 dirties
    /// `gentleman+0x34`), the behaviour drops a gentleman that already has a goal
    /// (`FUN_00CF0D30(gentleman+0x34)`), and the goal's refresh (`0x00C5F2C0` → `0x00C3FF10`)
    /// asks an analyser for **one** technology, non-zero or the goal stays finished. The exe's
    /// candidate list is that analyser's list over each gentleman's own technology list
    /// (`FUN_00C40310`, `FUN_00AAF1F0`), then our allies' and neutrals' gentlemen (those whose
    /// faction entry `+0x160` is 0 and `+0x188` is 0) sorted by character attribute 8
    /// (`FUN_00C4CCB0(8)`). **PROVISIONAL:** the sort key (`FUN_00C288B0`) is a pointer chase
    /// through `+0xFC → +0x1E0 → +8` and is not decoded, so our candidates are the faction's
    /// available technologies ordered by research cost (cheapest first), which is deterministic and
    /// keeps the cheapest technologies (the ones that open the tree) moving first.
    fn research_goals(&self) -> Vec<(String, f32)> {
        research::goal_multipliers(&self.available_technologies())
    }

    /// The technologies this faction may start now: state 2 (`AVAILABLE`, CONFIRMED the state
    /// research may start in) in the snapshot, with the goal multiplier `1.0 × 0.95^k` in the
    /// exe's order ([`research::goal_multipliers`]).
    fn available_technologies(&self) -> Vec<ResearchCand> {
        let Some(techs) = &self.world.factions[&self.faction].technologies else { return Vec::new() };
        let Some(known) = &self.world.technologies else { return Vec::new() };
        let mut out: Vec<ResearchCand> = techs
            .iter()
            .filter(|&(_, &s)| s == ntw_sim::campaign::research::state::AVAILABLE)
            .filter_map(|(k, _)| {
                let t = known.get(k)?;
                Some(ResearchCand { key: k.clone(), cost: t.cost, thread: t.thread })
            })
            .collect();
        out.sort_by(|a, b| a.cost.cmp(&b.cost).then(a.key.cmp(&b.key)));
        out
    }

    /// One RESEARCH_TECHNOLOGY intention (`0x00E5`, vtable `0x013828A0`): reserve a school for the
    /// technology and start it there (CONFIRMED `0x00C2E350` → `FUN_00AA4B80` → `0x008EEC90`,
    /// the research start; the intention is dropped when the start fails, CONFIRMED slot 12
    /// `0x00CE4720`). The exe's school is the one in the chosen gentleman's settlement
    /// (`0x00A3B080` / `0x00A614A0` register it for the faction); the model has no per-gentleman
    /// school, so **PROVISIONAL** we take the free school with the best rate for this technology's
    /// thread (`ResearchParts::rate`, the research rate of `ntw_sim::campaign::research`), ties by
    /// region id then slot.
    fn start_research(&mut self, tech: &str) {
        // The school is already reserved for this technology somewhere in our own regions: nothing
        // to do (the original's reservation list `0x00A614A0` is per gentleman, one school each).
        let already = self.world.regions.values().filter(|r| r.owner == self.faction).any(|r| {
            r.schools.iter().flatten().any(|s| s.researching.as_deref() == Some(tech))
        });
        if already {
            return;
        }
        let thread = self.world.technologies.as_ref().and_then(|t| t.get(tech)).map_or(2, |t| t.thread);
        let Some((region, slot)) = self.best_free_school(thread) else { return };
        self.researching.insert((region, slot));
        self.orders.push(AiOrder::StartResearch { region, slot, tech: tech.to_string() });
    }

    /// The free school of our own regions with the best rate for `thread`, ties by region id then
    /// slot (PROVISIONAL choice, see [`Self::start_research`]).
    fn best_free_school(&self, thread: usize) -> Option<(RegionId, usize)> {
        let rate = |s: &AiSchool| s.rate.get(thread).copied().unwrap_or(0.0);
        let mut best: Option<(f32, RegionId, usize)> = None;
        for r in self.world.regions.values().filter(|r| r.owner == self.faction) {
            for s in r.schools.iter().flatten() {
                if s.id == 0 || s.researching.is_some() || self.researching.contains(&(r.id, s.slot)) {
                    continue;
                }
                let candidate = (rate(s), r.id, s.slot);
                if best.as_ref().is_none_or(|b| {
                    (candidate.0, std::cmp::Reverse(candidate.1), std::cmp::Reverse(candidate.2))
                        > (b.0, std::cmp::Reverse(b.1), std::cmp::Reverse(b.2))
                }) {
                    best = Some(candidate);
                }
            }
        }
        best.map(|(_, r, s)| (r, s))
    }

    /// `WAR_AND_PEACE`: peace when losing badly, war on a weak, unfriendly neighbour.
    ///
    /// CONFIRMED tunables: `BAWP_WEIGHTING_*` (five weights), `RELATIONSHIP_MANAGEMENT_ENEMIES_UNDER`,
    /// `FACTIONAL_NONCACHING_ANALYSER_BADLY_LOSE_MINIMUM_WINNING_FORCE_MULTIPLIER_ARMIES` (0.6),
    /// `WAR_AND_PEACE_MANAGER_PROPORTION_OF_REQUIRED_FORCES_TO_START_WAR` (80 %).
    /// PROVISIONAL: the five attitude terms and how they combine (a weighted mean, 0 = enemy,
    /// 1 = friend).
    fn war_and_peace(&mut self) {
        let me = self.faction;
        let my_strength = self.world.faction_strength(me, self.data);
        let badly_lose = self.t("FACTIONAL_NONCACHING_ANALYSER_BADLY_LOSE_MINIMUM_WINNING_FORCE_MULTIPLIER_ARMIES", 0.6);
        let start_war = self.t("WAR_AND_PEACE_MANAGER_PROPORTION_OF_REQUIRED_FORCES_TO_START_WAR", 80.0) / 100.0;
        let enemies_under = self.t("RELATIONSHIP_MANAGEMENT_ENEMIES_UNDER", 0.1);
        let neighbours = self.world.neighbour_factions(me);
        let others: Vec<FactionId> = self.world.factions.keys().copied().filter(|f| *f != me).collect();
        for other in others {
            let o = &self.world.factions[&other];
            if o.regions == 0 && !self.world.armies.values().any(|a| a.faction == other) {
                continue; // dead or off-map faction
            }
            let their = self.world.faction_strength(other, self.data);
            let stance = self.world.stance(me, other);
            if stance == Stance::War {
                // HELP_ALLY_AT_WAR (PROVISIONAL rule): no separate peace while an ally of ours is
                // still at war with them.
                let ally_fighting = self.world.factions.keys().any(|x| {
                    *x != other
                        && matches!(self.world.stance(me, *x), Stance::Allied | Stance::Patron | Stance::Protectorate)
                        && self.world.stance(*x, other) == Stance::War
                });
                if ally_fighting {
                    continue;
                }
                // Peace when we are losing badly and the scripts allow it on both sides (we may
                // propose it, they accept it from us: `force_diplomacy`, the model's rule). The other
                // side accepts unless it is human (no UI here) or would win easily.
                let easy = self.t("FACTIONAL_NONCACHING_ANALYSER_EASY_WIN_MINIMUM_WINNING_FORCE_MULTIPLIER_ARMIES", 1.5);
                if my_strength < their * badly_lose
                    && !self.ctx.humans.contains(&other)
                    && their < my_strength * easy * 2.0
                    && self.world.may_propose(me, other, NegotiationAction::Peace.option())
                    && self.world.may_accept(other, me, NegotiationAction::Peace.option())
                {
                    self.orders.push(AiOrder::MakePeace { a: me, b: other });
                }
                continue;
            }
            if stance != Stance::Neutral || !self.cfg.behaviours.contains_key("EXPANSION_BEHAVIOUR") {
                continue;
            }
            let adjacent = neighbours.contains(&other);
            if !adjacent {
                continue;
            }
            let invade_human = self.ctx.hints.invade_humans && self.ctx.humans.contains(&other);
            let w = |k: &str| self.t(k, 20.0);
            let terms = [
                (w("BAWP_WEIGHTING_ADJACENCY"), if adjacent { 0.0 } else { 1.0 }),
                (w("BAWP_WEIGHTING_ARE_ALLIES_ALREADY"), 0.0),
                (w("BAWP_WEIGHTING_DIPLOMATIC_RELATIONS"), 0.0),
                (w("BAWP_WEIGHTING_INTERFACTION_RELATIONS"), 0.5),
                (w("BAWP_WEIGHTING_MILITARY_STRENGTH"), their / (my_strength + their).max(1e-6)),
            ];
            let wsum: f32 = terms.iter().map(|t| t.0).sum::<f32>().max(1e-6);
            let attitude: f32 = terms.iter().map(|t| t.0 * t.1).sum::<f32>() / wsum;
            let weak = my_strength * start_war >= their * 2.0;
            if (attitude < enemies_under && weak || invade_human)
                && self.world.may_propose(me, other, NegotiationAction::War.option())
            {
                self.orders.push(AiOrder::DeclareWar { a: me, b: other });
            }
        }
    }

    /// The analyser value of a region as the integer the desires use (`0x00A63FD0` on the
    /// composite value analyser 0x56): the CONFIRMED base and composite formulas
    /// ([`region_value`]). The base is [`region_value::stored_or_formula`] of
    /// [`AiRegion::base_value`] (PROVISIONAL as noted there). The model has no region groups, so our own regions
    /// take the own branch with no change entry and the others the default NEW state; the
    /// loss-likelihood counts are 0, and the capital / ×3 / +5000 tests are off (PROVISIONAL).
    fn region_value(&self, r: RegionId) -> i32 {
        let reg = &self.world.regions[&r];
        let base = region_value::stored_or_formula(reg.base_value, reg.gdp);
        let m = region_value::Multipliers::from_tunables(|k, d| self.t(k, d));
        let own = reg.owner == self.faction;
        let c = region_value::Composite {
            own_group: own,
            change: region_value::GroupChange::New,
            group_regions: 0,
            level: 0,
            count_a: 0,
            count_b: 0,
            capital: false,
            triple: false,
            bonus: false,
        };
        region_value::composite(base, &c, &m)
    }

    /// REGION_DEFENCE's desires (CONFIRMED, `0x00D0E090` / `0x00CD4AE0`): one per own region, in
    /// region order, mult `value × (1 − k) × N / total + k` with `k` =
    /// `BASIC_DESIRES_DEFEND_REGION_GOAL_BASE_COMPONENT_PROPORTION` and the region value of
    /// analyser 0x56 ([`Self::region_value`]), [`desires::defend_region_multipliers`].
    fn region_defence_desires(&self) -> Vec<(RegionId, f32)> {
        let k = self.t("BASIC_DESIRES_DEFEND_REGION_GOAL_BASE_COMPONENT_PROPORTION", 25.0);
        let cands: Vec<(RegionId, i32)> =
            self.world.regions.values().filter(|r| r.owner == self.faction).map(|r| (r.id, self.region_value(r.id))).collect();
        desires::defend_region_multipliers(&cands, k)
    }

    /// REGION_GROUP_DEFENCE's goals: one `CAI_BDI_GOAL_REGION_GROUP_DEFENCE` per own region group
    /// (CONFIRMED class, step `0x00D65650`; the group value is the sum of its regions' values,
    /// CONFIRMED analyser 0x57), multipliers [`desires::expansion_multipliers`] with
    /// `BASIC_DESIRES_DEFEND_REGION_GROUPS_GOAL_BASE_COMPONENT_PROPORTION` (CONFIRMED). The groups
    /// are PROVISIONAL ([`AiWorld::region_groups`]: connected blocks of our regions under the
    /// nearest-settlements adjacency).
    fn region_group_goals(&self) -> Vec<(Vec<RegionId>, f32)> {
        if self.ctx.hints.frozen_factions.contains(&self.key) {
            return Vec::new();
        }
        let k = self.t("BASIC_DESIRES_DEFEND_REGION_GROUPS_GOAL_BASE_COMPONENT_PROPORTION", 25.0);
        let groups = self.world.region_groups(self.faction);
        let cands: Vec<(usize, i32)> = groups
            .iter()
            .enumerate()
            .map(|(i, g)| (i, g.iter().fold(0i32, |s, r| s.saturating_add(self.region_value(*r)))))
            .collect();
        desires::expansion_multipliers(&cands, k).into_iter().map(|(i, m)| (groups[i].clone(), m)).collect()
    }

    /// The regions of a group that get a `CAI_BDI_GOAL_RECRUIT_STRENGTH_IN_REGION` goal
    /// (CONFIRMED: those with a non-zero count in the group goal's list, linked with
    /// `0.5 + 0.5^(i+1)` in list order). PROVISIONAL: the count is not decoded; our stand-in
    /// takes the group's regions next to a region of a faction we are at war with, in region id
    /// order.
    fn group_border_regions(&self, group: &[RegionId]) -> Vec<RegionId> {
        group
            .iter()
            .copied()
            .filter(|r| self.world.nearest_regions(*r).iter().any(|o| self.at_war_with(self.world.regions[o].owner)))
            .collect()
    }

    /// Whether a REGION_DEFENCE desire acts (PROVISIONAL; the desire class `0x01378770` is not
    /// decoded): the enemy strength within [`world::THREAT_RADIUS`] beats ours times
    /// `REGION_DEFENCE_STRENGTH_ANALYSER_INTERNAL_FORCE_SCALING` % (CONFIRMED tunable).
    fn is_threatened(&self, rid: RegionId) -> bool {
        let scaling = self.t("REGION_DEFENCE_STRENGTH_ANALYSER_INTERNAL_FORCE_SCALING", 100.0) / 100.0;
        let r = &self.world.regions[&rid];
        let threat = self.enemy_strength_near(r.position, world::THREAT_RADIUS);
        threat > 0.0 && threat > self.strength_near(self.faction, r.position, world::THREAT_RADIUS) * scaling
    }

    /// Invasion targets (PROVISIONAL stand-in: the original's invasion missions come from goals
    /// not decoded yet: the REGION_GROUP_DEFENCE goal links missions made by the factory
    /// `0x00CEEB00` with mult 1.0, and EXPANSION_BEHAVIOUR spawns a component, id 0x175, when the
    /// faction grows): enemy (at war) regions within twice [`world::THREAT_RADIUS`] of one of
    /// `ours`' regions, in region id order.
    fn expansion_targets(&self, group: &[RegionId]) -> Vec<RegionId> {
        let me = self.faction;
        let ours: Vec<(f64, f64)> = group.iter().map(|r| self.world.regions[r].position).collect();
        self.world
            .regions
            .values()
            .filter(|r| r.owner != me && self.at_war_with(r.owner))
            .filter(|r| ours.iter().any(|p| world::dist(*p, r.position) <= 2.0 * world::THREAT_RADIUS))
            .map(|r| r.id)
            .collect()
    }

    /// One REGION_DEFENCE desire: the nearest free army attacks the strongest enemy army near the
    /// settlement when it beats it by `MINIMUM_ABSOLUTE_STRENGTH_MULTIPLE_ARMIES` % (CONFIRMED
    /// tunable, PROVISIONAL use), else falls back into the settlement; and a recruit there.
    fn defend_region(&mut self, rid: RegionId) {
        let pos = self.world.regions[&rid].position;
        if let Some(force) = self.nearest_free_army(pos) {
            let min_multiple = self.t("MINIMUM_ABSOLUTE_STRENGTH_MULTIPLE_ARMIES", 80.0) / 100.0;
            let ours = self.world.army_strength(&self.world.armies[&force], self.data);
            let target = self
                .world
                .armies
                .values()
                .filter(|a| !a.is_navy && a.commander.is_some() && a.faction != self.faction && self.at_war_with(a.faction))
                .filter(|a| world::dist(a.position, pos) <= world::THREAT_RADIUS)
                .map(|a| (self.world.army_strength(a, self.data), a.id))
                .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
            match target {
                Some((s, t)) if ours >= s * min_multiple && self.world.armies[&force].movement_points > 0 => {
                    self.busy.insert(force);
                    self.orders.push(AiOrder::AttackForce { force, target: t });
                }
                _ => self.move_towards(force, pos, MovePurpose::Defend(rid)),
            }
        }
        self.recruitment(Some(rid));
    }

    /// One EXPANSION desire: the free army with the smallest distance ×
    /// `HLP_DISTANCE_MULTIPLIER_FOR_ENEMY` whose strength reaches the target's defence ×
    /// `MINIMUM_ABSOLUTE_STRENGTH_MULTIPLE_ARMIES` % moves on it (CONFIRMED tunables, PROVISIONAL
    /// use).
    fn expand_to(&mut self, rid: RegionId) {
        let r = &self.world.regions[&rid];
        let (pos, owner) = (r.position, r.owner);
        let min_multiple = self.t("MINIMUM_ABSOLUTE_STRENGTH_MULTIPLE_ARMIES", 80.0) / 100.0;
        let dist_mult = self.t("HLP_DISTANCE_MULTIPLIER_FOR_ENEMY", 3.0) as f64;
        let defence = self.strength_near(owner, pos, world::GARRISON_RADIUS);
        let best = self
            .world
            .armies
            .values()
            .filter(|a| a.faction == self.faction && !a.is_navy && a.commander.is_some() && a.movement_points > 0 && !self.busy.contains(&a.id))
            .filter(|a| self.world.army_strength(a, self.data) >= defence * min_multiple)
            .map(|a| (world::dist(a.position, pos) * dist_mult, a.id))
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some((_, force)) = best {
            self.move_towards(force, pos, MovePurpose::Attack(rid));
        }
    }

    /// The nearest army of ours with a commander and movement left that has no orders yet.
    fn nearest_free_army(&self, pos: (f64, f64)) -> Option<ForceId> {
        if self.ctx.hints.frozen_factions.contains(&self.key) {
            return None;
        }
        self.world
            .armies
            .values()
            .filter(|a| a.faction == self.faction && !a.is_navy && a.commander.is_some() && a.movement_points > 0 && !self.busy.contains(&a.id))
            .min_by(|a, b| world::dist(a.position, pos).total_cmp(&world::dist(b.position, pos)).then(a.id.cmp(&b.id)))
            .map(|a| a.id)
    }

    /// Orders `force` towards `target`. The model plans the path and walks as far as the
    /// commander's action points reach (`CampaignModel::walk`); a settlement purpose becomes
    /// `EnterSettlement`, which acts on arrival.
    fn move_towards(&mut self, force: ForceId, target: (f64, f64), purpose: MovePurpose) {
        let a = &self.world.armies[&force];
        let d = world::dist(a.position, target);
        if d < 1e-6 {
            return;
        }
        if a.movement_points <= 0 {
            return;
        }
        self.busy.insert(force);
        self.orders.push(AiOrder::MoveForce { force, to: (Fixed20::from_f64(target.0), Fixed20::from_f64(target.1)), purpose });
    }

    /// Recruitment (`EXCESS_RECRUITMENT_BEHAVIOUR`, and `REGION_DEFENCE` for one region):
    /// one unit per region per turn (PROVISIONAL capacity) while the army budget lasts.
    /// The category comes from the composition targets (CONFIRMED tables:
    /// `RECRUITMENT_ARMY_CATEGORY_BASE_PROPORTIONS_*` and `cdir_unit_balances`; PROVISIONAL
    /// blend: their mean); within the category, the best `cdir_unit_qualities` quality per cost.
    /// The candidates are the region's unflagged recruitable entries at the model's price
    /// ([`world::AiRecruitable`]).
    fn recruitment(&mut self, only: Option<RegionId>) {
        let mut regions: Vec<RegionId> = self
            .world
            .regions
            .values()
            .filter(|r| r.owner == self.faction && only.is_none_or(|o| o == r.id))
            .map(|r| r.id)
            .collect();
        // Regions with the biggest threat first, then id order.
        regions.sort_by(|a, b| {
            let ta = self.enemy_strength_near(self.world.regions[a].position, world::THREAT_RADIUS);
            let tb = self.enemy_strength_near(self.world.regions[b].position, world::THREAT_RADIUS);
            tb.total_cmp(&ta).then(a.cmp(b))
        });
        for rid in regions {
            if self.recruited.contains(&rid) {
                continue;
            }
            let r = &self.world.regions[&rid];
            // Free recruitment points when the snapshot knows them; else (PROVISIONAL) nothing
            // new while the queue is busy.
            if r.recruit_capacity.map_or(r.recruiting > 0, |c| c == 0) {
                continue;
            }
            let Some(category) = self.wanted_category() else { return };
            let mut best: Option<(f32, String, i32, i32)> = None;
            for e in &r.recruitable {
                let (unit, cost) = (&e.unit_key, e.cost);
                if e.flags != 0 || self.ctx.hints.restricted_units.contains(unit) || self.no_room_for(rid, unit) {
                    continue;
                }
                let Some(info) = self.data.units.get(unit) else { continue };
                if world::balance_group(info) != category {
                    continue;
                }
                if cost > self.recruitment_budget || cost <= 0 || self.upkeep_room.is_some_and(|room| info.upkeep > room) {
                    continue;
                }
                let value = info.quality.unwrap_or(info.battle_cost) as f32 / cost as f32;
                if best.as_ref().is_none_or(|(v, k, _, _)| value > *v || (value == *v && unit < k)) {
                    best = Some((value, unit.clone(), cost, info.upkeep));
                }
            }
            if let Some((_, unit, cost, upkeep)) = best {
                self.recruitment_budget -= cost;
                if let Some(room) = &mut self.upkeep_room {
                    *room -= upkeep;
                }
                self.order_recruit(rid, &unit);
            }
        }
    }

    /// The land category whose share in our armies is furthest below its target.
    fn wanted_category(&self) -> Option<&'static str> {
        let mut counts: BTreeMap<&'static str, f32> = BTreeMap::new();
        let mut total = 0.0f32;
        for a in self.world.armies.values().filter(|a| a.faction == self.faction && !a.is_navy) {
            for u in &a.units {
                if let Some(info) = self.data.units.get(&u.key) {
                    *counts.entry(world::balance_group(info)).or_default() += 1.0;
                    total += 1.0;
                }
            }
        }
        // Army size bucket for cdir_unit_balances: the average army.
        let n_armies = self.world.armies.values().filter(|a| a.faction == self.faction && !a.is_navy).count().max(1);
        let size = (total / n_armies as f32).round() as i32;
        let mut best: Option<(f32, &'static str)> = None;
        for cat in ["infantry", "cavalry", "artillery"] {
            let personality = match cat {
                "infantry" => self.t("RECRUITMENT_ARMY_CATEGORY_BASE_PROPORTIONS_INFANTRY", 0.6),
                "cavalry" => {
                    self.t("RECRUITMENT_ARMY_CATEGORY_BASE_PROPORTIONS_CAVALRY", 0.2)
                        + self.t("RECRUITMENT_ARMY_CATEGORY_BASE_PROPORTIONS_DRAGOONS", 0.1)
                }
                _ => self.t("RECRUITMENT_ARMY_CATEGORY_BASE_PROPORTIONS_ARTILLERY", 0.1),
            };
            let cdir = self
                .data
                .unit_balances
                .iter()
                .find(|b| b.config == "default" && b.group == cat && size.max(1) >= b.min_units && size.max(1) <= b.max_units)
                .map(|b| b.target);
            let target = cdir.map_or(personality, |c| (c + personality) / 2.0);
            let share = counts.get(cat).copied().unwrap_or(0.0) / total.max(1.0);
            let deficit = target - share;
            if best.is_none_or(|(d, _)| deficit > d) {
                best = Some((deficit, cat));
            }
        }
        best.map(|(_, c)| c)
    }

    /// `GLOBAL_CONSTRUCTION_BEHAVIOUR`: build or upgrade, one item per region per turn, best score
    /// first. Score = the personality's `BASE_CONSTRUCTION_BIAS_*` for the chain's category
    /// (CONFIRMED tunables; INFERRED mapping military→MILITARY, money/agriculture→ECONOMIC,
    /// research→EDUCATION, happiness→HAPPINESS, government→PRESTIGE) divided by cost (PROVISIONAL).
    /// Candidates: the snapshot's `build_options`, what the model accepts now ([`AiWorld::from_model`]: its
    /// option rule leaves out levels the owner may not build, the scripts' restricted ones included).
    fn construction(&mut self) {
        let mut options: Vec<(f32, RegionId, SlotRef, String, i32)> = Vec::new();
        for r in self.world.regions.values().filter(|r| r.owner == self.faction) {
            if r.constructing {
                continue;
            }
            for o in &r.build_options {
                let (slot, next, cost) = (o.slot, &o.level_key, o.cost);
                if cost <= 0 {
                    continue;
                }
                let Some(ni) = self.data.buildings.get(next) else { continue };
                let cat = self.data.chain_category.get(&ni.chain).map(String::as_str).unwrap_or("");
                let bias_key = match cat {
                    "military" => "BASE_CONSTRUCTION_BIAS_MILITARY",
                    "money" | "agriculture" => "BASE_CONSTRUCTION_BIAS_ECONOMIC",
                    "research" => "BASE_CONSTRUCTION_BIAS_EDUCATION",
                    "happiness" => "BASE_CONSTRUCTION_BIAS_HAPPINESS",
                    "government" => "BASE_CONSTRUCTION_BIAS_PRESTIGE",
                    _ => continue,
                };
                let score = self.t(bias_key, 0.1) / cost as f32;
                options.push((score, r.id, slot, next.clone(), cost));
            }
        }
        options.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
        for (_, rid, slot, level_key, cost) in options {
            if self.built.contains(&rid) || cost > self.construction_budget {
                continue;
            }
            self.construction_budget -= cost;
            self.built.insert(rid);
            self.orders.push(AiOrder::Construct { region: rid, slot, level_key });
        }
    }
}

/// What a component of the faction's [`bdi::Pool`] stands for.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A manager junction row (CONFIRMED: one component per row except WAR_AND_PEACE and
    /// DIPLOMACY_MANAGER, base = its priority).
    Behaviour(String),
    /// A REGION_DEFENCE desire for one own region (CONFIRMED: vtable `0x01378770`, multipliers;
    /// its deliberation is not decoded: PROVISIONAL one defence intention, mult 1.0, while the
    /// region is threatened).
    DefendRegion(RegionId),
    /// REGION_GROUP_DEFENCE's `CAI_BDI_GOAL_REGION_GROUP_DEFENCE` for one own region group
    /// (CONFIRMED class and multipliers; groups PROVISIONAL).
    GroupDefence(Vec<RegionId>),
    /// `CAI_BDI_GOAL_RECRUIT_STRENGTH_IN_REGION` (CONFIRMED class and link multipliers; its own
    /// deliberation is not decoded: PROVISIONAL one recruit intention, mult 1.0).
    RecruitStrength(RegionId),
    /// `CAI_BDI_GOAL_RESEARCH_TECHNOLOGY` for one technology (CONFIRMED class vtable `0x013825B4`,
    /// type 0x110 and the `1.0 × 0.95^k` link multipliers; the candidate list and the school are
    /// PROVISIONAL, see [`research`]).
    ResearchTechnology(String),
    /// An intention: the action our turn takes (see [`Action`]).
    Act(Action),
    /// A component we do not model (by its type id).
    Other(u16),
}

/// The actions of our intentions. CONFIRMED: [`Action::RecruitUnit`] (EXCESS_RECRUITMENT's
/// `CAI_BDI_UNIT_RECRUITMENT_NEW`); the others are PROVISIONAL stand-ins (the v1 rules) for the
/// original's `CAI_BDIM_*` missions, construction and tax intentions.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Defend(RegionId),
    Expand(RegionId),
    Recruit(Option<RegionId>),
    /// Region, unit key, cost.
    RecruitUnit(RegionId, String, i32),
    Construct,
    Merge,
    Tax,
    /// `RESEARCH_TECHNOLOGY`'s `CAI_BDI_*` intention (id 0xE5): start `tech` at a school.
    Research(String),
}

/// The faction turn's classes for the pool ([`bdi::Deliberate`]).
struct TurnDeliberation<'t, 'a> {
    turn: &'t mut FactionTurn<'a>,
    rng: &'t mut CaRng,
    /// Invasion targets already taken this turn.
    claimed: BTreeSet<RegionId>,
}

impl TurnDeliberation<'_, '_> {
    fn child(&mut self, pool: &mut bdi::Pool<Node>, parent: bdi::CompId, list: bdi::List, node: Node, mult: f32) {
        let id = pool.add(list, 0.0, node, self.rng);
        pool.link(parent, id, 0.0, mult, 0);
    }

    fn act(&mut self, pool: &mut bdi::Pool<Node>, parent: bdi::CompId, a: Action) {
        self.child(pool, parent, bdi::List::Intention, Node::Act(a), 1.0);
    }
}

impl bdi::Deliberate<Node> for TurnDeliberation<'_, '_> {
    fn deliberate(&mut self, pool: &mut bdi::Pool<Node>, id: bdi::CompId) {
        let node = pool.payload(id).clone();
        match node {
            Node::Behaviour(name) => match name.as_str() {
                "REGION_DEFENCE" => {
                    for (r, m) in self.turn.region_defence_desires() {
                        self.child(pool, id, bdi::List::Desire, Node::DefendRegion(r), m);
                    }
                }
                "REGION_GROUP_DEFENCE" => {
                    for (g, m) in self.turn.region_group_goals() {
                        self.child(pool, id, bdi::List::Desire, Node::GroupDefence(g), m);
                    }
                }
                "EXPANSION_BEHAVIOUR" => {
                    if !self.turn.ctx.hints.frozen_factions.contains(&self.turn.key) {
                        let own: Vec<RegionId> =
                            self.turn.world.regions.values().filter(|r| r.owner == self.turn.faction).map(|r| r.id).collect();
                        for t in self.turn.expansion_targets(&own) {
                            if self.claimed.insert(t) {
                                self.act(pool, id, Action::Expand(t));
                            }
                        }
                    }
                }
                "EXCESS_RECRUITMENT_BEHAVIOUR" => {
                    for (r, unit, cost) in self.turn.recruitment_draws(self.rng) {
                        self.act(pool, id, Action::RecruitUnit(r, unit, cost));
                    }
                }
                // PROVISIONAL: GLOBAL_CONSTRUCTION's own slots do nothing in the original (its
                // step is an empty stub; construction desires come from elsewhere, UNKNOWN).
                "GLOBAL_CONSTRUCTION_BEHAVIOUR" => self.act(pool, id, Action::Construct),
                "MERGE_UNITS" => self.act(pool, id, Action::Merge),
                "TAXATION" => self.act(pool, id, Action::Tax),
                // RESEARCH_TECHNOLOGY (step 0x00C2AD60): one goal per candidate technology, linked
                // with the CONFIRMED 1.0 x 0.95^k multipliers in the exe's order.
                "RESEARCH_TECHNOLOGY" => {
                    for (tech, m) in self.turn.research_goals() {
                        self.child(pool, id, bdi::List::Desire, Node::ResearchTechnology(tech), m);
                    }
                }
                _ => {}
            },
            Node::DefendRegion(r) => {
                if self.turn.is_threatened(r) {
                    self.act(pool, id, Action::Defend(r));
                }
            }
            Node::GroupDefence(g) => {
                let mut f = 0.5f32;
                for r in self.turn.group_border_regions(&g) {
                    self.child(pool, id, bdi::List::Desire, Node::RecruitStrength(r), f + 0.5);
                    f *= 0.5;
                }
            }
            Node::RecruitStrength(r) => self.act(pool, id, Action::Recruit(Some(r))),
            // A research goal's deliberation (0x00C2E350, CONFIRMED): one intention, mult 1.0.
            Node::ResearchTechnology(tech) => self.act(pool, id, Action::Research(tech)),
            Node::Act(a) => match a {
                Action::Defend(r) => self.turn.defend_region(r),
                Action::Expand(r) => self.turn.expand_to(r),
                Action::Recruit(r) => self.turn.recruitment(r),
                Action::RecruitUnit(r, unit, cost) => self.turn.recruit_unit(r, &unit, cost),
                Action::Construct => self.turn.construction(),
                Action::Merge => self.turn.merge_units(),
                Action::Tax => self.turn.taxation(),
                Action::Research(tech) => self.turn.start_research(&tech),
            },
            Node::Other(_) => {}
        }
        pool.set_state(id, bdi::State::Active);
    }
}

pub fn apply_orders(model: &mut CampaignModel, orders: &[AiOrder]) -> usize {
    let (mut events, mut rejected) = (Vec::new(), Vec::new());
    apply_orders_with_events(model, orders, &mut events, &mut rejected)
}

/// Like [`apply_orders`], collecting every event the commands caused and every order the model refused
/// with its error (the driver reports them, [`driver::AiTurnReport::rejected`], and the app logs each kind once).
pub fn apply_orders_with_events(
    model: &mut CampaignModel,
    orders: &[AiOrder],
    events: &mut Vec<CampaignEvent>,
    rejected: &mut Vec<(AiOrder, CommandError)>,
) -> usize {
    let mut accepted = 0;
    for (order, cmd) in orders.iter().filter_map(|o| Some((o, o.to_campaign_command()?))) {
        match model.apply(cmd) {
            Ok(ev) => {
                accepted += 1;
                events.extend(ev);
            }
            Err(e) => rejected.push((order.clone(), e)),
        }
        if model.pending_battle.is_some()
            && let Ok(ev) = model.apply(CampaignCommand::Autoresolve)
        {
            events.extend(ev);
        }
    }
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::campaign::rules::CampaignRules;
    use ntw_sim::campaign::{BuildingRef, Region, RegionSlot, Settlement, World};

    fn model(rules: Option<CampaignRules>) -> CampaignModel {
        use ntw_sim::calendar::{Calendar, Date, HALF_EARLY};
        let slot = RegionSlot {
            key: "test_slot_0".into(),
            slot_type: "test_slot".into(),
            building: Some(BuildingRef { level_key: "test_building_level".into(), health: 100 }),
            position: None,
            port: false,
            holder: None,
            id: 0,
        };
        let region = Region {
            id: RegionId(1),
            key: "test_region".into(),
            owner: FactionId(1),
            settlement: Settlement { key: "settlement:test_region:town".into(), position: Default::default() },
            slots: vec![slot],
            road: None,
            fortification: None,
            population: 0,
            base_gdp: 0,
            gdp: 0,
            wealth_growth_offset: 0,
            discontent_growth: 0,
            town_wealth: 0,
            town_wealth_growth: 0,
            tax_exempt: false,
            religions: Vec::new(),
            class_bases: Vec::new(),
            population_state: Default::default(),
            recruitment_queue: Vec::new(),
            construction: Vec::new(),
            garrison: None,
            fleet: None,
        };
        let mut w = World::default();
        w.regions.insert(region.id, region);
        let mut m = CampaignModel::new(Calendar::new(Date { year: 1805, season: 1, month: 0, half: HALF_EARLY }, 0), ntw_sim::rng::CaRng::new(1), w);
        if let Some(rules) = rules {
            m.rules = std::sync::Arc::new(rules);
        }
        m
    }

    /// Review: the construction table fallback repeated the model's script-restriction test. It is gone: the
    /// AI builds only from the model's own option list (permission and restriction tests included), empty
    /// for a model without rules.
    #[test]
    fn the_model_snapshot_always_carries_the_model_option_rule() {
        let keys = |m: &CampaignModel| -> Vec<String> {
            AiWorld::from_model(m).regions[&RegionId(1)].build_options.iter().map(|o| o.level_key.clone()).collect()
        };
        assert_eq!(keys(&model(None)), Vec::<String>::new(), "no rules: nothing is permitted");
        let mut m = model(Some(CampaignRules::test_rules()));
        assert_eq!(keys(&m), ["test_building_level_2"]);
        m.world.restricted_buildings.insert("test_building_level_2".into());
        assert_eq!(keys(&m), Vec::<String>::new(), "a script-restricted level is left out by the model's rule");
    }

    /// The AI's peace and war gates read the model's `force_diplomacy` permissions (one home:
    /// `Relationship::diplomacy_options`) through its snapshot, with the model's own rule.
    #[test]
    fn the_snapshot_carries_the_models_diplomacy_permissions() {
        let mut m = model(None);
        let (a, b) = (FactionId(1), FactionId(2));
        m.world.relationships.entry((a, b)).or_default().diplomacy_options[NegotiationAction::War.option()] = 2;
        m.world.relationships.entry((b, a)).or_default().diplomacy_options[NegotiationAction::Peace.option()] = 1;
        m.world.relationships.entry((b, FactionId(3))).or_default();
        let w = AiWorld::from_model(&m);
        assert_eq!(w.diplomacy_options.len(), 2, "only relationships with a permission set");
        for (x, y) in [(a, b), (b, a)] {
            for o in 0..15 {
                assert_eq!((w.may_propose(x, y, o), w.may_accept(x, y, o)), (m.may_propose(x, y, o), m.may_accept(x, y, o)));
            }
        }
        assert!(!w.may_propose(a, b, NegotiationAction::War.option()));
        assert!(!w.may_accept(b, a, NegotiationAction::Peace.option()), "b declines peace from a");
    }

    /// Review (0b-recruit): the AI kept its own recruit price (`units` #4 × its handicap) and ignored the unit
    /// cap. It now reads the model's recruitable entries: the price the queue command charges (the handicap is
    /// already in the faction's effects) and the entry flags, so it never orders a capped or unaffordable unit,
    /// and within a turn it never orders past the cap's room.
    #[test]
    fn the_ai_recruits_only_unflagged_entries_at_the_models_price() {
        use ntw_sim::campaign::commands::{ENTRY_TOO_DEAR, ENTRY_UNIT_CAP};
        use ntw_sim::campaign::{CampaignUnit, Faction, GovernmentType, MilitaryForce, UnitId, economy};
        let mut rules = CampaignRules::test_rules();
        rules.units.get_mut("test_recruit").unwrap().unit_cap = 2;
        let mut m = model(Some(rules));
        let me = FactionId(1);
        m.world.factions.insert(
            me,
            Faction {
                id: me,
                key: "test_faction".into(),
                treasury: 100_000,
                government: GovernmentType::AbsoluteMonarchy,
                government_key: String::new(),
                tax_lower: "tax_normal".into(),
                tax_upper: "tax_normal".into(),
                diplomacy: BTreeMap::new(),
            },
        );
        // The faction already holds one of the two `test_recruit` its cap allows.
        let held = CampaignUnit { id: UnitId(7), unit_key: "test_recruit".into(), men: 100, max_men: 100, character: None, officer_name: Default::default() };
        m.world.forces.insert(ForceId(9), MilitaryForce { id: ForceId(9), faction: me, commander: None, units: vec![held], is_navy: false });
        let (data, ctx) = (CampaignAiData::default(), TurnContext::new("test_campaign"));
        let cfg = FactionAiConfig { manager: String::new(), personality: String::new(), behaviours: BTreeMap::new() };
        let region = m.world.regions[&RegionId(1)].clone();
        let price = |m: &CampaignModel, k: &str| economy::recruitment_cost(m, &region, k, &m.rules.units[k]);

        // The snapshot carries the model's price (#7, not the #4 cost) and flags.
        let w = AiWorld::from_model_for(&m, Some(me));
        let entries = &w.regions[&RegionId(1)].recruitable;
        assert_eq!(entries.iter().map(|e| e.unit_key.as_str()).collect::<Vec<_>>(), ["test_recruit", "test_unit"]);
        for e in entries {
            assert_eq!(e.cost, price(&m, &e.unit_key));
            assert_ne!(e.cost, m.rules.units[&e.unit_key].cost, "not the units #4 cost");
            assert_eq!(e.flags, 0);
        }
        assert_eq!(entries[0].cap_room, Some(1));

        // Within a turn the AI orders the capped unit only as often as the cap leaves room for.
        let mut turn = FactionTurn::new(&w, &data, &ctx, me, cfg.clone());
        turn.recruitment_budget = 100_000;
        turn.recruit_unit(RegionId(1), "test_recruit", entries[0].cost);
        turn.recruit_unit(RegionId(1), "test_recruit", entries[0].cost);
        assert_eq!(turn.orders, [AiOrder::Recruit { region: RegionId(1), unit_key: "test_recruit".into() }]);
        assert_eq!(apply_orders(&mut m, &turn.orders), 1, "the model accepts what the AI orders");

        // At the cap the entry is flagged and the draws never offer it; what they offer is at the model's price.
        let w = AiWorld::from_model_for(&m, Some(me));
        let capped = &w.regions[&RegionId(1)].recruitable[0];
        assert_eq!((capped.unit_key.as_str(), capped.flags & ENTRY_UNIT_CAP), ("test_recruit", ENTRY_UNIT_CAP));
        let mut turn = FactionTurn::new(&w, &data, &ctx, me, cfg.clone());
        turn.recruitment_budget = 100_000;
        let mut drawn = 0;
        for seed in 0..20 {
            for (rid, unit, cost) in turn.recruitment_draws(&mut CaRng::new(seed)) {
                assert_eq!((rid, unit.as_str(), cost), (RegionId(1), "test_unit", price(&m, "test_unit")));
                drawn += 1;
            }
        }
        assert!(drawn > 0);
        turn.recruit_unit(RegionId(1), "test_recruit", capped.cost);
        assert!(turn.orders.is_empty(), "a capped unit is never ordered");

        // Unaffordable: every entry is flagged too dear and nothing is drawn.
        m.world.factions.get_mut(&me).unwrap().treasury = 10;
        let w = AiWorld::from_model_for(&m, Some(me));
        assert!(w.regions[&RegionId(1)].recruitable.iter().all(|e| e.flags & ENTRY_TOO_DEAR != 0));
        let turn = FactionTurn::new(&w, &data, &ctx, me, cfg.clone());
        assert!((0..20).all(|seed| turn.recruitment_draws(&mut CaRng::new(seed)).is_empty()));

        // The queue's free places bound the orders too: with one place left (9 of 10 land items queued) the AI
        // orders one unit, however many recruitment points the region has.
        m.world.factions.get_mut(&me).unwrap().treasury = 100_000;
        let queued = m.world.regions[&RegionId(1)].recruitment_queue.len();
        let item = m.world.regions[&RegionId(1)].recruitment_queue[0].clone();
        let queue = &mut m.world.regions.get_mut(&RegionId(1)).unwrap().recruitment_queue;
        queue.extend((queued..9).map(|i| ntw_sim::campaign::RecruitmentItem { id: ntw_sim::campaign::RecruitmentItemId(500 + i as i32), unit_key: "test_unit".into(), ..item.clone() }));
        let mut w = AiWorld::from_model_for(&m, Some(me));
        assert!(w.regions[&RegionId(1)].recruitable.iter().all(|e| e.queue_room == 1 && e.flags & ENTRY_UNIT_CAP == e.flags));
        w.regions.get_mut(&RegionId(1)).unwrap().recruit_capacity = Some(5);
        let mut turn = FactionTurn::new(&w, &data, &ctx, me, cfg);
        turn.recruitment_budget = 100_000;
        for _ in 0..3 {
            turn.recruit_unit(RegionId(1), "test_unit", price(&m, "test_unit"));
        }
        assert_eq!(turn.orders.len(), 1);
        assert_eq!(apply_orders(&mut m, &turn.orders), 1);
    }
}
