//! Player and AI actions as queued commands.
//!
//! The original routes campaign actions through a `CAMPAIGN_COMMAND_QUEUE` whose commands are
//! named `CCQ_*` (CONFIRMED: W1 report "Campaign engine" row, ARCHITECTURE_REPORT §3, DESIGN §3.4).
//! Only two command names are known: `CCQ_END_TURN` and `CCQ_SET_GOVERNORSHIP_TAX_RATE`. The other
//! commands below are our own, modelled on the same idea; their original names and payloads are
//! UNKNOWN.
//!
//! Going through commands (instead of letting the display change the model directly) is what
//! makes the simulation replayable and is what lockstep multiplayer needs (DESIGN §1, §3.4):
//! the same list of commands applied to the same start state always gives the same result.
//!
//! Every command is validated first; a rejected command returns a [`CommandError`] and leaves the
//! model exactly as it was. Once the campaign has started, a faction may only act during its own
//! turn ([`CampaignModel::may_act`]).

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use super::battles::PendingBattle;
use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId, RecruitmentItemId, RegionId};
use super::pathing::{Domain, GridPath};
use super::rules::{TaxClass, OFF_ROAD_COST};
use super::world::{SlotRef, CampaignModel, CampaignUnit, ConstructionItem, MilitaryForce, RecruitmentItem, Stance};
use crate::fixed::{Fixed20, ONE_RAW};

/// One construction option of a region slot ([`CampaignModel::construction_options`]): what the
/// construction panel's card for a level shows (`0x009FBAB0` entries; `affordable` and the
/// "technology present" bit are the entry's availability flags).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructionOption {
    /// The `building_levels` key.
    pub level_key: String,
    /// What building it costs in that region now ([`CampaignModel::construction_cost`]).
    pub cost: i32,
    /// Its construction turns.
    pub turns: u32,
    /// The option is not flagged too dear (flag 1): [`super::treasury::construction_affordable`]. The construct
    /// command needs [`super::treasury::can_pay_construction`].
    pub affordable: bool,
    /// Its required technology is researched.
    pub tech: bool,
}

/// A building level's cost under a percent `modifier` (`0x00B43300`, CONFIRMED): the level's cost (record
/// +0x24, unsigned) as f32, times `modifier + 100.0`, times `0.01` (both f32 products), rounded half to even
/// (FISTP). No clamp: a modifier below −100 gives a negative cost. The repair cost (`0x00B66410`) applies the
/// same step to its base.
pub(crate) fn building_cost(cost: i32, modifier: f32) -> i32 {
    fistp(unsigned_f32(cost) * (modifier + 100.0) * 0.01)
}

/// An int field the exe reads as unsigned and converts to f32 (to double with the 2^32 fix-up
/// `g_adUIntToDoubleFix`, then to f32: one rounding of the exact value, as here).
pub(crate) fn unsigned_f32(v: i32) -> f32 {
    v as u32 as f32
}

/// x87 FISTP of an f32 in the default rounding mode: half to even, and the integer indefinite 0x80000000 for
/// a value out of the int range or NaN.
pub(crate) fn fistp(v: f32) -> i32 {
    let v = v.round_ties_even();
    if (-2_147_483_648.0..2_147_483_648.0).contains(&v) { v as i32 } else { i32::MIN }
}

/// x87/SSE `cvttss2si` of an f32: truncation toward zero, and the integer indefinite 0x80000000 for a value
/// out of the int range or NaN (Rust's `as i32` saturates instead).
pub(crate) fn cvttss2si(v: f32) -> i32 {
    let v = v.trunc();
    if (-2_147_483_648.0..2_147_483_648.0).contains(&v) { v as i32 } else { i32::MIN }
}

/// The chain-keyed cost modifier in a region effect set: bonus type 2, `mod_cost` (id 0), qualified by the
/// chain (`0x00E1EF10(chain, 0)` → `0x00E23DA0(2, chain, 0, 0)`).
fn cost_modifier_in(set: &super::effects::EffectSet, chain: &str) -> f32 {
    set.get_qualified(super::effects::BonusKind::Saved(2), "0", chain)
}

/// One campaign command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CampaignCommand {
    /// `CCQ_END_TURN` (CONFIRMED name). Runs [`CampaignModel::end_turn`].
    EndTurn,
    /// Move a force (with its commander) towards a map position, as far as its action points
    /// reach. Path: [`CampaignModel::plan_path`] (PROVISIONAL grid A*).
    MoveForce {
        /// The force to move.
        force: ForceId,
        /// Destination, in 20-bit fixed point (W3 §2.4).
        to: (Fixed20, Fixed20),
    },
    /// Move a character without a force (an agent, or a general alone).
    MoveCharacter {
        /// The character.
        character: CharacterId,
        /// Destination.
        to: (Fixed20, Fixed20),
    },
    /// Move to an enemy force and attack it (sets the pending battle when it is reached).
    AttackForce {
        /// The attacker.
        force: ForceId,
        /// The force attacked (its faction must be at war with the attacker's).
        target: ForceId,
    },
    /// Move to a friendly force and give it units (up to the campaign's units per force,
    /// `CampaignModel::max_units`).
    MergeForces {
        /// The force giving its units.
        force: ForceId,
        /// The force receiving them.
        into: ForceId,
    },
    /// Move into a settlement: an own settlement is entered; an enemy one (at war) is occupied if
    /// undefended, else assaulted (pending battle).
    EnterSettlement {
        /// The force.
        force: ForceId,
        /// The region whose settlement to enter.
        region: RegionId,
    },
    /// Modelled on `CCQ_EMBARK_NAVY` (CONFIRMED name): an army walks to a navy of its faction and
    /// boards it (see [`super::embark`]).
    Embark {
        /// The army.
        force: ForceId,
        /// The navy to board.
        navy: ForceId,
    },
    /// Modelled on `CCQ_DISEMBARK_NAVY` (CONFIRMED name): the navy carrying the army sails to a
    /// landing place and lands it near `to` (a move order to an embarked army does the same).
    Disembark {
        /// The embarked army.
        force: ForceId,
        /// Where to land (the landing ends the army's turn; it walks on next turn).
        to: (Fixed20, Fixed20),
    },
    /// The `assassinate` order (CONFIRMED name): the agent walks to the target and tries on arrival
    /// ([`super::agents`]).
    Assassinate {
        /// The agent (a spy).
        agent: CharacterId,
        /// The character to kill (another faction's).
        target: CharacterId,
    },
    /// The `duel` order (CONFIRMED name): the challenger walks to the target and they fight
    /// ([`super::agents`]).
    Duel {
        /// The challenger (a gentleman).
        challenger: CharacterId,
        /// The character challenged (another faction's).
        target: CharacterId,
    },
    /// The `army_sabotage` order (CONFIRMED name): the agent walks to the force and sabotages it
    /// ([`super::agents`]).
    SabotageArmy {
        /// The agent.
        agent: CharacterId,
        /// The force (another faction's).
        force: ForceId,
    },
    /// Building sabotage: the agent walks to a slot and damages its building ([`super::agents`]).
    SabotageBuilding {
        /// The agent.
        agent: CharacterId,
        /// The region.
        region: RegionId,
        /// Index into the region's slots.
        slot: usize,
    },
    /// Spying: the agent walks to a settlement or a force and spies on it ([`super::agents::spy_chance`]).
    Spy {
        /// The agent.
        agent: CharacterId,
        /// What he watches.
        target: super::agents::SpyTarget,
    },
    /// Hire a General from his faction's recruitment pool ([`super::pool`]): into an army of his
    /// faction (the original's way: the interface offers the candidates on an army), or, without
    /// one, as a new army at the capital (PROVISIONAL).
    HireGeneral {
        /// The candidate.
        character: CharacterId,
        /// The army he joins and takes command of.
        into: Option<ForceId>,
    },
    /// Hire an admiral from his faction's pool onto a navy of his faction ([`super::pool`],
    /// `0x00A16110`): he takes command; a captain in command goes.
    HireAdmiral {
        /// The candidate.
        character: CharacterId,
        /// The navy.
        fleet: ForceId,
    },
    /// Promote a unit's commander in the field to General (army) or admiral (navy) (the unit
    /// classes' slot 20, `0x008E1C20` / `0x008E2260`, the player's `PromoteUnits` /
    /// `CCQ_PROMOTE_COMMANDER`; [`CampaignModel::promote_unit`](super::CampaignModel::promote_unit)).
    PromoteUnit {
        /// The force.
        force: ForceId,
        /// Index into the force's units.
        unit: usize,
    },
    /// Dismiss a minister (`0x008EB9D0`; see [`super::family`]): he leaves, the post is refilled.
    DismissMinister {
        /// The post holder.
        minister: CharacterId,
    },
    /// Swap two post holders, or (absolute monarchy) seat a spare minister in another's post
    /// (`0x008F3760`; see [`super::family`]).
    AppointMinister {
        /// A post holder or spare minister.
        a: CharacterId,
        /// Another.
        b: CharacterId,
    },
    /// Fight the pending battle with autoresolve.
    Autoresolve,
    /// Modelled on `CCQ_SET_GOVERNORSHIP_TAX_RATE` (CONFIRMED name): one level per faction and class,
    /// applied to all its governorships (one per faction in every shipped campaign).
    SetTaxLevel {
        /// The faction.
        faction: FactionId,
        /// Which class.
        class: TaxClass,
        /// A `taxes_levels` key, e.g. `tax_high`.
        level: String,
    },
    /// Queue a unit for recruitment in a region (W3 §4 `RECRUITMENT_ITEM`).
    Recruit {
        /// The recruiting region; its owner pays.
        region: RegionId,
        /// Unit key (FK to `units`).
        unit_key: String,
        /// The commander the unit is recruited through (his recruitment panel,
        /// [`CampaignModel::commander_recruitment`]), kept on the item (item +0x18: the `CCQ` command's third
        /// value, `0x00936B90` → `0x00B58DD0`, CONFIRMED). `None` from a settlement's own panel and the AI.
        target: Option<CharacterId>,
    },
    /// Remove an item from a region's recruitment queue and refund it.
    CancelRecruitment {
        /// The region.
        region: RegionId,
        /// The queued item (its id stays the same while the queue around it changes).
        item: RecruitmentItemId,
    },
    /// Build or upgrade a building in a slot (`BUILDING_CONSTRUCTION_ITEM` in saves).
    ConstructBuilding {
        /// The region.
        region: RegionId,
        /// The slot.
        slot: SlotRef,
        /// The building level to build (FK `building_levels`).
        level_key: String,
    },
    /// Cancel the construction (or repair) item of a slot and refund what was paid. The original's
    /// `CampaignUI.CancelConstruction(building_key, slot_key)` (`0x009E0F10` → `0x009B7AA0`,
    /// CONFIRMED) queues a command (serializer vtable `0x0136AB78`) with the slot and a flag byte 1;
    /// the full refund is INFERRED (like [`CampaignCommand::CancelRecruitment`]).
    CancelConstruction {
        /// The region.
        region: RegionId,
        /// The slot.
        slot: SlotRef,
    },
    /// Start researching a technology at a school (`0x008EEC90`, CAMPAIGN_FIDELITY.md §Research). A school
    /// that was researching another technology drops it; that progress is kept.
    StartResearch {
        /// The school's region.
        region: RegionId,
        /// Index into the region's slots.
        slot: usize,
        /// The technology key.
        tech: String,
    },
    /// The human's answer to a capture (`CampaignModel::pending_capture`; the original's capture screen, `0x008C0310`).
    ChooseCapture {
        /// Occupy, loot or liberate.
        choice: super::capture::CaptureChoice,
    },
    /// Repair a damaged building (`0x00B66260`; see [`capture`](super::capture)).
    RepairBuilding {
        /// The region.
        region: RegionId,
        /// The slot (a region slot or the walls; the model has no road repair).
        slot: SlotRef,
    },
    /// Demolish a standing building: the slot's building is removed at once, with no refund
    /// (PROVISIONAL: the original queues it — building `0x009B9590` → id `0x84`, fort
    /// `0x009BA250` → id `0x89`, CONFIRMED — and its resolution refund/timing is UNKNOWN).
    /// `slot` is a region slot or the walls; roads cannot be demolished (the original's `CanDemolishBuilding`
    /// `0x009B7920` excludes the `settlement_road` slot, CONFIRMED).
    DemolishBuilding {
        /// The region.
        region: RegionId,
        /// The slot.
        slot: SlotRef,
    },
    /// `a` declares war on `b`. Our own command (scripts use `force_declare_war`, W3 §6.3).
    DeclareWar {
        /// The faction declaring war.
        a: FactionId,
        /// The target.
        b: FactionId,
    },
    /// `a` and `b` make peace. Our own command (scripts use `force_make_peace`, W3 §6.3).
    MakePeace {
        /// First faction.
        a: FactionId,
        /// Second faction.
        b: FactionId,
    },
    /// A diplomatic action of `a` towards `b` that the other side has accepted (the AI's acceptance is §6):
    /// the relationship rules of [`treaties`](super::treaties).
    Diplomacy {
        /// The acting faction.
        a: FactionId,
        /// The other faction.
        b: FactionId,
        /// What is done.
        action: super::treaties::DiplomaticAction,
    },
    /// Change a faction's government type (`0x00B1B5A0`): the `government_type` attitude factor of every
    /// record the faction is a side of is set to drift from the new government's start value (#3) toward its
    /// limit (#2) at 2 a turn ([`change_government`](super::CampaignModel::change_government)).
    ChangeGovernment {
        /// The faction changing government.
        faction: FactionId,
        /// The new government key (e.g., `gov_republic`, `gov_constitutional_monarchy`, `gov_absolute_monarchy`).
        new_government_key: String,
    },
    /// `CCQ_DIPLOMACY_BEGIN_NEGOTIATION` (queued by the two-key `UIDiplomacyNegotiation`, executed
    /// by `0x008AF620`): a new campaign negotiation, whose constructor picks the diplomat's lines
    /// and may draw the campaign RNG ([`CampaignModel::negotiations`],
    /// `CampaignModel::begin_negotiation`). Not a turn action: no turn check (none traced).
    BeginNegotiation {
        /// The proposer.
        proposer: FactionId,
        /// The recipient.
        recipient: FactionId,
    },
    /// `CCQ_DIPLOMACY_END_NEGOTIATION` (executor `0x00932F20` → `0x008BC5D0`): the campaign
    /// negotiation ends, if there is one.
    EndNegotiation,
    /// `CCQ_DIPLOMACY_PROPOSE_REGIONS` (posted by the regions action record's `0x00C4B3C0` from
    /// `negotiation:Propose`, executor `0x00933CC0`): sets the open negotiation's regions record
    /// (`CampaignModel::propose_regions`). Not a turn action (no turn check traced).
    ProposeRegions {
        /// Empty the record instead.
        clear: bool,
        /// Regions the recipient is to give the proposer.
        demanded: Vec<RegionId>,
        /// Regions the proposer is to give the recipient.
        offered: Vec<RegionId>,
    },
    /// `CCQ_DIPLOMACY_PROPOSE_TECHNOLOGIES` (`0x00C4B870`, executor `0x009340A0`): sets the open
    /// negotiation's technology record (`CampaignModel::propose_technologies`).
    ProposeTechnologies {
        /// Empty the record instead.
        clear: bool,
        /// Technology keys the recipient is to give the proposer.
        demanded: Vec<String>,
        /// Technology keys the proposer is to give the recipient.
        offered: Vec<String>,
    },
    /// `CCQ_DIPLOMACY_CLEAR_NEGOTIATION` (executor `0x00932EC0`): the open negotiation's deal is
    /// emptied (`CampaignModel::clear_negotiation`).
    ClearNegotiation,
    /// `CCQ_DIPLOMACY_ACCEPT_DEAL` (executor `0x00932E40` → `0x00C114B0`): the deal of the open
    /// negotiation is applied (`CampaignModel::accept_deal`: its regions and technologies).
    AcceptDeal,
}

/// Why a command was rejected. A rejected command leaves the model unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// No faction with this id.
    UnknownFaction(FactionId),
    /// No region with this id.
    UnknownRegion(RegionId),
    /// No force with this id.
    UnknownForce(ForceId),
    /// No character with this id (e.g. a force's commander is missing).
    UnknownCharacter(CharacterId),
    /// The force has no commander, so it cannot move (it moves with its commander).
    NoCommander(ForceId),
    /// Not enough movement points to make any progress.
    NotEnoughMovementPoints {
        /// Points the first step costs.
        needed: i32,
        /// Points the commander has left.
        available: i32,
    },
    /// No path to the destination (sea, off the map, ...).
    NoPath,
    /// It is not this faction's turn.
    NotYourTurn(FactionId),
    /// A tax level that `taxes_levels` does not have.
    UnknownTaxLevel(String),
    /// The faction cannot pay.
    InsufficientFunds {
        /// Cost of the action.
        needed: i32,
        /// Current treasury.
        available: i32,
    },
    /// The construct command refused a negative construction cost (a cost modifier below −100): its affordability
    /// test compares unsigned ([`super::treasury::construction_affordable`]), so the cost reads as above 2^31.
    NegativeCost {
        /// The cost.
        cost: i32,
        /// Current treasury.
        available: i32,
    },
    /// The unit key is empty.
    EmptyUnitKey,
    /// A unit key the `units` table does not have.
    UnknownUnit(String),
    /// No building in the region allows this unit, or the faction may not recruit it.
    UnitNotAvailable(String),
    /// The unit's recruitable entry carries building flags ([`ENTRY_BUILDING_FLAGS`]): every building allowing it
    /// is damaged or held by another faction, or a technology it needs is not researched.
    RecruitmentBlocked {
        /// The unit.
        unit_key: String,
        /// Its entry's building flags.
        flags: u32,
    },
    /// The recruitment queue of that kind (land or naval) already holds 10 items.
    NoRecruitmentCapacity,
    /// The faction already holds and has queued as many units of this type as `units` #15 allows.
    UnitCapReached(String),
    /// The region's population is below `recruitment_population_cost` + `minimum_population_after_recruitment`
    /// ([`super::population::RecruitmentPopulation::available`]).
    NotEnoughPopulation,
    /// The region's recruitment queue has no item with this id.
    UnknownRecruitmentItem(RecruitmentItemId),
    /// The region has no such building slot.
    BadSlot(SlotRef),
    /// A building level `building_levels` does not have.
    UnknownBuilding(String),
    /// The building cannot be built in that slot now (wrong slot type, not the next level, ...).
    CannotBuild(String),
    /// Something is already being built in that slot.
    SlotBusy,
    /// A diplomatic command named the same faction twice.
    SameFaction(FactionId),
    /// The two factions are already at war.
    AlreadyAtWar(FactionId, FactionId),
    /// The two factions are not at war.
    NotAtWar(FactionId, FactionId),
    /// The two forces belong to different factions (merge) or the same one (attack).
    WrongFaction,
    /// A battle is pending; it must be resolved first.
    BattlePending,
    /// Autoresolve without a pending battle.
    NoPendingBattle,
    /// Not supported yet (naval battles, merging armies and navies, ...).
    Unsupported(&'static str),
    /// A deal command with no negotiation open.
    NoNegotiation,
    /// The deal is refused (`CampaignModel::ai_refuses_deal`: the AI's evaluation of the regions and
    /// technology records).
    DealRefused,
    /// A technology key the `technologies` table does not have.
    UnknownTechnology(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownFaction(id) => write!(f, "unknown faction {}", id.0),
            CommandError::UnknownRegion(id) => write!(f, "unknown region {}", id.0),
            CommandError::UnknownForce(id) => write!(f, "unknown force {}", id.0),
            CommandError::UnknownCharacter(id) => write!(f, "unknown character {}", id.0),
            CommandError::NoCommander(id) => write!(f, "force {} has no commander", id.0),
            CommandError::NotEnoughMovementPoints { needed, available } => write!(
                f,
                "move needs {needed} movement points but only {available} are left"
            ),
            CommandError::NoPath => write!(f, "no path to the destination"),
            CommandError::NotYourTurn(id) => write!(f, "it is not faction {}'s turn", id.0),
            CommandError::UnknownTaxLevel(l) => write!(f, "unknown tax level {l}"),
            CommandError::InsufficientFunds { needed, available } => {
                write!(f, "costs {needed} but the treasury holds {available}")
            }
            CommandError::NegativeCost { cost, available } => {
                write!(f, "a negative cost of {cost} cannot be paid from a treasury of {available}")
            }
            CommandError::EmptyUnitKey => write!(f, "empty unit key"),
            CommandError::UnknownUnit(k) => write!(f, "unknown unit {k}"),
            CommandError::UnitNotAvailable(k) => write!(f, "unit {k} cannot be recruited here"),
            CommandError::RecruitmentBlocked { unit_key, flags } => write!(f, "unit {unit_key} cannot be recruited here now (flags {flags:#x})"),
            CommandError::NoRecruitmentCapacity => write!(f, "the recruitment queue is full"),
            CommandError::UnitCapReached(k) => write!(f, "the faction already has as many {k} as it may"),
            CommandError::NotEnoughPopulation => write!(f, "the region has too few people to recruit"),
            CommandError::UnknownRecruitmentItem(i) => write!(f, "no recruitment item {}", i.raw()),
            CommandError::BadSlot(slot) => write!(f, "no slot {slot:?}"),
            CommandError::UnknownBuilding(k) => write!(f, "unknown building level {k}"),
            CommandError::CannotBuild(k) => write!(f, "{k} cannot be built there"),
            CommandError::SlotBusy => write!(f, "the slot is already being built on"),
            CommandError::SameFaction(id) => {
                write!(f, "faction {} cannot do diplomacy with itself", id.0)
            }
            CommandError::AlreadyAtWar(a, b) => {
                write!(f, "factions {} and {} are already at war", a.0, b.0)
            }
            CommandError::NotAtWar(a, b) => {
                write!(f, "factions {} and {} are not at war", a.0, b.0)
            }
            CommandError::WrongFaction => write!(f, "wrong faction for this action"),
            CommandError::BattlePending => write!(f, "a battle is pending"),
            CommandError::NoPendingBattle => write!(f, "no battle is pending"),
            CommandError::Unsupported(what) => write!(f, "not supported yet: {what}"),
            CommandError::NoNegotiation => write!(f, "no negotiation is open"),
            CommandError::DealRefused => write!(f, "the deal is refused"),
            CommandError::UnknownTechnology(k) => write!(f, "unknown technology {k}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// A first-in, first-out list of commands waiting to be applied: our version of the original's
/// `CAMPAIGN_COMMAND_QUEUE` (CONFIRMED to exist; its internals are UNKNOWN).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandQueue {
    pending: VecDeque<CampaignCommand>,
}

impl CommandQueue {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a command at the back.
    pub fn push(&mut self, cmd: CampaignCommand) {
        self.pending.push_back(cmd);
    }

    /// Number of waiting commands.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// `true` if no commands are waiting.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// How far a force got towards its goal.
pub(crate) struct Walk {
    pub(crate) events: Vec<CampaignEvent>,
    pub(crate) arrived: bool,
}

/// A planned move, for the display's path preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedMove {
    /// The whole path and its cumulative costs.
    pub path: GridPath,
    /// Index of the last point reachable this turn.
    pub reachable: usize,
}

/// Distance (map units) within which a force counts as having reached a force or settlement it
/// is moving to. PROVISIONAL (the original's contact rule is UNKNOWN).
pub const CONTACT_DISTANCE: f32 = 1.5;

impl CampaignModel {
    /// Applies every queued command in order, emptying the queue. Returns one result per command.
    /// A failing command does not stop the following ones.
    pub fn apply_queue(
        &mut self,
        queue: &mut CommandQueue,
    ) -> Vec<Result<Vec<CampaignEvent>, CommandError>> {
        let mut results = Vec::with_capacity(queue.len());
        while let Some(cmd) = queue.pending.pop_front() {
            results.push(self.apply(cmd));
        }
        results
    }

    /// `CharacterCreated` for every character that exists now and is not in `known`, in id order.
    pub(crate) fn created_since(&self, known: &std::collections::BTreeSet<CharacterId>) -> Vec<CampaignEvent> {
        self.world.characters.keys().filter(|c| !known.contains(c)).map(|&character| CampaignEvent::CharacterCreated { character }).collect()
    }

    /// Validates and applies one command. On success returns the events it caused; on failure the
    /// model is left exactly as it was.
    pub fn apply(&mut self, cmd: CampaignCommand) -> Result<Vec<CampaignEvent>, CommandError> {
        // Characters a command creates (a successor after an assassination, ...) fire
        // `CharacterCreated`; the end of turn fires its own from each step.
        let known: Option<std::collections::BTreeSet<CharacterId>> =
            (!matches!(cmd, CampaignCommand::EndTurn)).then(|| self.world.characters.keys().copied().collect());
        let mut out = self.apply_inner(cmd);
        if let (Ok(events), Some(known)) = (&mut out, known) {
            events.extend(self.created_since(&known));
        }
        out
    }

    fn apply_inner(&mut self, cmd: CampaignCommand) -> Result<Vec<CampaignEvent>, CommandError> {
        // The negotiation commands touch no battle state: a pending battle must not leave a
        // negotiation open with no way to end it.
        // Proposing or clearing deal items only edits the negotiation; accepting a deal moves
        // regions, so it waits for the battle like every other command.
        let exempt = matches!(
            cmd,
            CampaignCommand::Autoresolve
                | CampaignCommand::BeginNegotiation { .. }
                | CampaignCommand::EndNegotiation
                | CampaignCommand::ProposeRegions { .. }
                | CampaignCommand::ProposeTechnologies { .. }
                | CampaignCommand::ClearNegotiation
        );
        if self.pending_battle.is_some() && !exempt {
            return Err(CommandError::BattlePending);
        }
        match cmd {
            CampaignCommand::EndTurn => {
                // A capture still waiting for a choice is settled as an occupation (PROVISIONAL: the original's
                // capture screen is modal, so the turn cannot end with one open).
                let mut events = Vec::new();
                if let Some(p) = self.pending_capture.take() {
                    self.resolve_capture(p, super::capture::CaptureChoice::Occupy, &mut events);
                }
                events.extend(self.end_turn());
                Ok(events)
            }
            CampaignCommand::ChooseCapture { choice } => self.choose_capture(choice),
            CampaignCommand::RepairBuilding { region, slot } => self.repair_building(region, slot),
            CampaignCommand::DemolishBuilding { region, slot } => self.demolish_building(region, slot),
            CampaignCommand::MoveForce { force, to } => {
                let c = self.commander_of(force)?;
                if self.carrier_of(force).is_some() {
                    return self.disembark(force, to);
                }
                self.walk(c, to, 0.0).map(|w| w.events)
            }
            CampaignCommand::MoveCharacter { character, to } => {
                let faction = self.world.characters.get(&character).ok_or(CommandError::UnknownCharacter(character))?.faction;
                self.check_turn(faction)?;
                if let Some(f) = self.force_of(character)
                    && self.carrier_of(f).is_some()
                {
                    return self.disembark(f, to);
                }
                self.walk(character, to, 0.0).map(|w| w.events)
            }
            CampaignCommand::Embark { force, navy } => self.embark(force, navy),
            CampaignCommand::Disembark { force, to } => self.disembark(force, to),
            CampaignCommand::AttackForce { force, .. }
            | CampaignCommand::MergeForces { force, .. }
            | CampaignCommand::EnterSettlement { force, .. }
                if self.carrier_of(force).is_some() =>
            {
                Err(CommandError::Unsupported("the army is aboard a navy: land it first"))
            }
            CampaignCommand::AttackForce { force, target } => self.attack(force, target),
            CampaignCommand::MergeForces { force, into } => self.merge(force, into),
            CampaignCommand::EnterSettlement { force, region } => self.enter_settlement(force, region),
            CampaignCommand::Autoresolve => self.autoresolve(),
            CampaignCommand::Assassinate { agent, target } => self.assassinate(agent, target),
            CampaignCommand::Duel { challenger, target } => self.duel(challenger, target),
            CampaignCommand::SabotageArmy { agent, force } => self.sabotage_army(agent, force),
            CampaignCommand::SabotageBuilding { agent, region, slot } => self.sabotage_building(agent, region, slot),
            CampaignCommand::Spy { agent, target } => self.spy(agent, target),
            CampaignCommand::HireGeneral { character, into } => self.hire_general(character, into),
            CampaignCommand::HireAdmiral { character, fleet } => self.hire_admiral(character, fleet),
            CampaignCommand::PromoteUnit { force, unit } => self.promote_unit(force, unit),
            CampaignCommand::DismissMinister { minister } => self.dismiss_minister(minister),
            CampaignCommand::AppointMinister { a, b } => self.appoint_minister(a, b),
            CampaignCommand::SetTaxLevel { faction, class, level } => self.set_tax_level(faction, class, level),
            CampaignCommand::Recruit { region, unit_key, target } => self.recruit(region, unit_key, target),
            CampaignCommand::CancelRecruitment { region, item } => self.cancel_recruitment(region, item),
            CampaignCommand::ConstructBuilding { region, slot, level_key } => self.construct(region, slot, level_key),
            CampaignCommand::CancelConstruction { region, slot } => self.cancel_construction(region, slot),
            CampaignCommand::StartResearch { region, slot, tech } => {
                let owner = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?.owner;
                self.check_turn(owner)?;
                use super::research::ResearchError as E;
                self.start_research(region, slot, &tech).map(|()| Vec::new()).map_err(|e| {
                    CommandError::Unsupported(match e {
                        E::NotASchool => "no school in that slot",
                        E::AlreadyResearched => "another school is researching that technology",
                        E::NotAvailable => "that technology is not available",
                        E::Unknown => "unknown technology",
                    })
                })
            }
            CampaignCommand::DeclareWar { a, b } => {
                self.check_turn(a)?;
                self.declare_war_rules(a, b)
            }
            CampaignCommand::MakePeace { a, b } => {
                self.check_turn(a)?;
                self.make_peace_rules(a, b)
            }
            CampaignCommand::Diplomacy { a, b, action } => {
                self.check_turn(a)?;
                self.apply_diplomatic_action(a, b, action)
            }
            CampaignCommand::ChangeGovernment { faction, new_government_key } => {
                self.check_turn(faction)?;
                self.change_government(faction, &new_government_key)
            }
            CampaignCommand::BeginNegotiation { proposer, recipient } => {
                for f in [proposer, recipient] {
                    if !self.world.factions.contains_key(&f) {
                        return Err(CommandError::UnknownFaction(f));
                    }
                }
                self.begin_negotiation(proposer, recipient);
                Ok(Vec::new())
            }
            CampaignCommand::EndNegotiation => {
                self.end_negotiation();
                Ok(Vec::new())
            }
            CampaignCommand::ProposeRegions { clear, demanded, offered } => self.propose_regions(clear, demanded, offered).map(|()| Vec::new()),
            CampaignCommand::ProposeTechnologies { clear, demanded, offered } => {
                self.propose_technologies(clear, demanded, offered).map(|()| Vec::new())
            }
            CampaignCommand::ClearNegotiation => self.clear_negotiation().map(|()| Vec::new()),
            CampaignCommand::AcceptDeal => self.accept_deal(),
        }
    }

    pub(crate) fn check_turn(&self, faction: FactionId) -> Result<(), CommandError> {
        if !self.world.factions.contains_key(&faction) {
            return Err(CommandError::UnknownFaction(faction));
        }
        if self.may_act(faction) { Ok(()) } else { Err(CommandError::NotYourTurn(faction)) }
    }

    /// The commander of a force, after checking the force and whose turn it is.
    pub(crate) fn commander_of(&self, force: ForceId) -> Result<CharacterId, CommandError> {
        let f = self.world.forces.get(&force).ok_or(CommandError::UnknownForce(force))?;
        self.check_turn(f.faction)?;
        let c = f.commander.ok_or(CommandError::NoCommander(force))?;
        if !self.world.characters.contains_key(&c) {
            return Err(CommandError::UnknownCharacter(c));
        }
        Ok(c)
    }

    /// The road level (0..=3) of a region: the level index + 1 of its `sRoads` road-slot building,
    /// 0 without one. INFERRED mapping of `sRoads1..3` onto `road_level_1..3`.
    pub fn road_level(&self, region_key: &str) -> u8 {
        let Some(r) = self.world.regions.values().find(|r| r.key == region_key) else { return 0 };
        let Some(b) = &r.road else { return 0 };
        self.rules.buildings.get(&b.level_key).map_or(0, |b| (b.level + 1).clamp(0, 3) as u8)
    }

    /// The path a character would take to `to`, with what it can reach this turn. `None` without
    /// a path. Without terrain the path is a straight line at the off-road cost.
    pub fn plan_path(&self, character: CharacterId, to: (Fixed20, Fixed20)) -> Option<PlannedMove> {
        let ch = self.world.characters.get(&character)?;
        let navy = self.force_of(character).and_then(|f| self.world.forces.get(&f)).is_some_and(|f| f.is_navy)
            || matches!(ch.kind, super::CharacterKind::Admiral | super::CharacterKind::Captain);
        // An army aboard a navy: the transport path; a navy into a port: the port node
        // (`super::embark`, CONFIRMED rules).
        if let Some(f) = self.force_of(character)
            && self.carrier_of(f).is_some()
        {
            return self.plan_transport_move(f, to);
        }
        if navy && let Some(p) = self.plan_port_entry(character, to) {
            return Some(p);
        }
        let from = (ch.position.0.to_f32(), ch.position.1.to_f32());
        let goal = (to.0.to_f32(), to.1.to_f32());
        let path = match &self.terrain {
            Some(t) => {
                let grid = &t.0;
                // Road cost per region index, computed once per search.
                let road: Vec<f32> = grid.region_keys.iter().map(|k| self.rules.road_cost(self.road_level(k))).collect();
                if let Some(pm) = &grid.poly {
                    // The original's search (CONFIRMED algorithm, polypath module docs): road
                    // polygons cost their region's road level, border strips the cheapest of
                    // their regions, everything else the cells' header bytes.
                    let by_id = pm.road_costs(|r| road.get(r).copied().unwrap_or(OFF_ROAD_COST), self.rules.road_cost(0));
                    // Zones of control (zoc module docs): other force commanders' obstacles.
                    // An obstacle the mover's faction cannot see is hidden (mode 5, `0x00B3F4F0`,
                    // CONFIRMED) and is not cut: its faction has a shroud and the obstacle's place is
                    // not visible (`0x00B6A920`), or its character is unknown to it (obstacle slot 9
                    // `0x00B4D600` → [`super::agents::knows_character`]).
                    let own_force = self.force_of(character);
                    let obstacles: Vec<super::zoc::Obstacle> = self
                        .world
                        .forces
                        .values()
                        .filter(|f| Some(f.id) != own_force)
                        .filter_map(|f| {
                            let c = self.world.characters.get(&f.commander?)?;
                            let pos = (c.position.0.to_f32(), c.position.1.to_f32());
                            if !self.sees(ch.faction, pos) || !super::agents::knows_character(self, ch.faction, c.id) {
                                return None;
                            }
                            (c.id != character).then(|| super::zoc::Obstacle {
                                pos,
                                navy: f.is_navy,
                                garrisoned: c.garrisoned_in.is_some(),
                                at_war: f.faction != ch.faction && self.world.stance(ch.faction, f.faction) == Stance::War,
                            })
                        })
                        .collect();
                    let info = super::zoc::MoverInfo { pos: from, navy, agent: own_force.is_none() };
                    let mover = info.mover();
                    // The obstacles cut into the map at run time (rtcut module docs).
                    let overlay = super::zoc::overlay(pm, info, goal, &obstacles);
                    let view = pm.with(&overlay);
                    // The kind 7 rules (movers module docs): every mover walks or sails round
                    // every building footprint but the one it starts in and the one it is sent to.
                    let family = super::movers::Family::of(ch.garrisoned_in.is_some());
                    let open7 = super::movers::open_shared(pm, pm.locate(from.0, from.1, mover, 2), pm.locate(goal.0, goal.1, mover, 2));
                    let is_blocked = |q: usize| super::movers::kind7_closed(&view, family, &open7, q);
                    let raw = view.find_path_avoiding(from, goal, mover, &by_id, &is_blocked)?;
                    let p = super::polysmooth::smooth_view(&view, &raw, &is_blocked);
                    // Run-time pieces are reported as the static polygon they were cut from.
                    let polys = p.polys.iter().map(|&q| view.origin_of(q as usize) as u32).collect();
                    let path = GridPath { points: p.points, costs: p.costs, polys };
                    let reachable = path.reachable(ch.movement_points as f32);
                    return Some(PlannedMove { path, reachable });
                }
                let min = road.iter().copied().fold(OFF_ROAD_COST, f32::min);
                let domain = if navy { Domain::Sea } else { Domain::Land };
                grid.find_path(from, goal, domain, min, |i| {
                    if grid.road[i] {
                        road.get(grid.region[i] as usize).copied().unwrap_or(OFF_ROAD_COST)
                    } else {
                        OFF_ROAD_COST
                    }
                })?
            }
            None => {
                let d = straight_distance(ch.position, to);
                GridPath { points: vec![from, goal], costs: vec![0.0, d * self.rules.road_cost(0)], polys: Vec::new() }
            }
        };
        let reachable = path.reachable(ch.movement_points as f32);
        Some(PlannedMove { path, reachable })
    }

    /// Moves a character towards `to`, stopping `stop_short` map units before it, as far as its
    /// action points reach. Errors (and changes nothing) if it cannot move at all.
    pub(crate) fn walk(&mut self, character: CharacterId, to: (Fixed20, Fixed20), stop_short: f32) -> Result<Walk, CommandError> {
        let plan = self.plan_path(character, to).ok_or(CommandError::NoPath)?;
        let pts = &plan.path.points;
        // The last point to aim for: the first one within `stop_short` of the goal.
        let goal = (to.0.to_f32(), to.1.to_f32());
        let target_idx = if stop_short > 0.0 {
            pts.iter()
                .position(|&(x, z)| ((x - goal.0).powi(2) + (z - goal.1).powi(2)).sqrt() <= stop_short)
                .unwrap_or(pts.len() - 1)
        } else {
            pts.len() - 1
        };
        let end = plan.reachable.min(target_idx);
        let ch = &self.world.characters[&character];
        let available = ch.movement_points;
        if end == 0 {
            if target_idx == 0 {
                return Ok(Walk { events: Vec::new(), arrived: true });
            }
            return Err(CommandError::NotEnoughMovementPoints { needed: plan.path.costs[1].ceil() as i32, available });
        }
        let spent = (plan.path.costs[end].ceil() as i32).min(available);
        let new_pos = (Fixed20::from_f64(pts[end].0 as f64), Fixed20::from_f64(pts[end].1 as f64));
        let path: Vec<(f32, f32)> = pts[..=end].to_vec();
        self.leave_garrison(character);
        let ch = self.world.characters.get_mut(&character).expect("checked");
        ch.movement_points -= spent;
        ch.position = new_pos;
        let mut events = vec![CampaignEvent::CharacterMoved { character, path }];
        if available > 0 && ch.movement_points <= 0 {
            events.push(CampaignEvent::MovementPointsExhausted { character });
        }
        // A navy carries its embarked armies along.
        self.sync_passengers(character);
        // The mover's faction sees from his new place (`0x00A27F10` → `0x00B1B8B0`; the model
        // recomputes the whole set, CHARACTERS_FIDELITY.md §10).
        // The hidden flag after a move (`0x00A285E0` → `0x009D3000`), for the mover and any army he
        // carries.
        let carried: Vec<CharacterId> = self
            .force_of(character)
            .map(|f| self.world.embarked.iter().filter(|(_, n)| **n == f).filter_map(|(a, _)| self.world.forces.get(a).and_then(|x| x.commander)).collect())
            .unwrap_or_default();
        for c in std::iter::once(character).chain(carried) {
            self.update_hidden(c);
        }
        let faction = self.world.characters[&character].faction;
        self.refresh_shroud(faction, false);
        Ok(Walk { events, arrived: end == target_idx })
    }

    fn attack(&mut self, force: ForceId, target: ForceId) -> Result<Vec<CampaignEvent>, CommandError> {
        let c = self.commander_of(force)?;
        let (fa, navy) = {
            let f = &self.world.forces[&force];
            (f.faction, f.is_navy)
        };
        let t = self.world.forces.get(&target).ok_or(CommandError::UnknownForce(target))?.clone();
        if t.faction == fa {
            return Err(CommandError::WrongFaction);
        }
        if self.world.stance(fa, t.faction) != Stance::War {
            return Err(CommandError::NotAtWar(fa, t.faction));
        }
        // Navy against navy is a naval battle (`super::naval`); a navy and an army do not fight.
        if navy != t.is_navy {
            return Err(CommandError::Unsupported("a navy and an army cannot fight"));
        }
        let to = self.force_position(target).ok_or(CommandError::UnknownForce(target))?;
        let walk = self.walk(c, to, CONTACT_DISTANCE)?;
        let mut events = walk.events;
        if walk.arrived {
            // A force inside a settlement is attacked together with the settlement's defenders.
            let settlement = t.commander.and_then(|c| self.world.characters.get(&c)).and_then(|c| c.garrisoned_in);
            let defenders = match settlement {
                Some(r) => self.defenders_of(r),
                None => vec![target],
            };
            self.pending_battle = Some(PendingBattle { attacker: force, defenders, settlement, resume: None });
            events.push(CampaignEvent::PreBattle { attacker: force });
        }
        Ok(events)
    }

    /// Forces inside a region's settlement: its garrison and every army whose commander is
    /// garrisoned there. PROVISIONAL: a garrison of another faction than the owner (left by a deal
    /// or a liberation) is counted too; whom the exe makes fight then is not traced.
    pub fn defenders_of(&self, region: RegionId) -> Vec<ForceId> {
        let Some(r) = self.world.regions.get(&region) else { return Vec::new() };
        let mut out: Vec<ForceId> = r.garrison.into_iter().filter(|f| self.world.forces.contains_key(f)).collect();
        for f in self.world.forces.values() {
            let inside = f.commander.and_then(|c| self.world.characters.get(&c)).is_some_and(|c| c.garrisoned_in == Some(region));
            if inside && !f.is_navy && !out.contains(&f.id) {
                out.push(f.id);
            }
        }
        out
    }

    fn merge(&mut self, force: ForceId, into: ForceId) -> Result<Vec<CampaignEvent>, CommandError> {
        let c = self.commander_of(force)?;
        let a = &self.world.forces[&force];
        let b = self.world.forces.get(&into).ok_or(CommandError::UnknownForce(into))?;
        if a.faction != b.faction || force == into {
            return Err(CommandError::WrongFaction);
        }
        if a.is_navy != b.is_navy {
            return Err(CommandError::Unsupported("merging an army with a navy"));
        }
        if b.units.len() >= self.max_units(b.is_navy) {
            return Err(CommandError::Unsupported("the receiving force is full"));
        }
        let to = self.force_position(into).ok_or(CommandError::UnknownForce(into))?;
        let walk = self.walk(c, to, CONTACT_DISTANCE)?;
        let mut events = walk.events;
        if walk.arrived {
            self.merge_units(force, into, &mut events);
        }
        Ok(events)
    }

    /// Moves `force`'s units into `into` (up to its cap, [`CampaignModel::max_units`]), each with
    /// its attached character. The commander's own unit (the one he is attached to, `UNIT` #10) moves last, so
    /// when not everything fits he stays with what is left. When everything moves, `force` is
    /// gone and its commander goes along as the character attached to his unit (`CHARACTER` #4 = 0,
    /// #5 = the unit), standing where `into` is: the state the original saves for merged armies
    /// (SAVE_COMPAT.md §4, CONFIRMED in its saves).
    pub(crate) fn merge_units(&mut self, force: ForceId, into: ForceId, events: &mut Vec<CampaignEvent>) {
        let room = self.world.forces.get(&into).map_or(0, |f| self.max_units(f.is_navy).saturating_sub(f.units.len()));
        let Some(src) = self.world.forces.get_mut(&force) else { return };
        let commander = src.commander;
        // Stable order with the commander's own unit last.
        src.units.sort_by_key(|u| commander.is_some() && u.character == commander);
        let n = room.min(src.units.len());
        let moved: Vec<CampaignUnit> = src.units.drain(..n).collect();
        let empty = src.units.is_empty();
        if !empty {
            // Keep the source's order otherwise: the commander's unit back in front.
            src.units.sort_by_key(|u| !(commander.is_some() && u.character == commander));
        }
        let to_pos = self.force_position(into);
        self.world.forces.get_mut(&into).expect("checked").units.extend(moved);
        if empty {
            if let Some(c) = commander {
                self.leave_garrison(c);
                if let (Some(ch), Some(p)) = (self.world.characters.get_mut(&c), to_pos) {
                    ch.position = p;
                }
            }
            self.world.forces.remove(&force);
            self.tidy_embarked();
            for r in self.world.regions.values_mut() {
                if r.garrison == Some(force) {
                    r.garrison = None;
                }
                if r.fleet == Some(force) {
                    r.fleet = None;
                }
            }
            events.push(CampaignEvent::ForceDestroyed { force });
        }
        events.push(CampaignEvent::CampaignArmiesMerge { force, into });
    }

    /// The character's force leaves the settlement it garrisons (if any).
    pub(crate) fn leave_garrison(&mut self, character: CharacterId) {
        let Some(ch) = self.world.characters.get_mut(&character) else { return };
        let Some(region) = ch.garrisoned_in.take() else { return };
        let force = self.world.forces.values().find(|f| f.commander == Some(character)).map(|f| f.id);
        if let Some(r) = self.world.regions.get_mut(&region)
            && force.is_some()
            && r.garrison == force
        {
            r.garrison = None;
        }
    }

    pub(crate) fn enter_settlement(&mut self, force: ForceId, region: RegionId) -> Result<Vec<CampaignEvent>, CommandError> {
        let c = self.commander_of(force)?;
        let fa = self.world.forces[&force].faction;
        if self.world.forces[&force].is_navy {
            return Err(CommandError::Unsupported("navies entering ports"));
        }
        let r = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?;
        let owner = r.owner;
        let pos = r.settlement.position;
        let hostile = owner != fa;
        if hostile && self.world.stance(fa, owner) != Stance::War {
            return Err(CommandError::NotAtWar(fa, owner));
        }
        let walk = self.walk(c, pos, CONTACT_DISTANCE)?;
        let mut events = walk.events;
        if !walk.arrived {
            return Ok(events);
        }
        if !hostile {
            // A settlement holds one garrison army (`SIEGEABLE_GARRISON_RESIDENCE` #12, CONFIRMED):
            // an army entering one that has a garrison joins it (INFERRED; PROVISIONAL when the
            // garrison is full: the army then waits outside). A garrison of another faction (left
            // inside by a deal or a liberation, which move no army) is never joined: the army waits
            // outside (PROVISIONAL: what the exe does then is not traced).
            let held = self.world.regions[&region].garrison.filter(|g| *g != force && self.world.forces.contains_key(g));
            if let Some(g) = held {
                if self.world.forces[&g].faction == fa && self.world.forces[&g].units.len() < self.max_units(self.world.forces[&g].is_navy) {
                    self.merge_units(force, g, &mut events);
                }
                return Ok(events);
            }
            let ch = self.world.characters.get_mut(&c).expect("checked");
            ch.position = pos;
            ch.garrisoned_in = Some(region);
            self.world.regions.get_mut(&region).expect("checked").garrison = Some(force);
            events.push(CampaignEvent::CharacterEntersGarrison { character: c, region });
            return Ok(events);
        }
        // An enemy army standing outside the settlement is fought first; the attacker then goes on
        // into the settlement (the battle type with a follow-up target, 0x008F7880 state 7).
        if let Some(blocker) = self.army_outside(region, fa) {
            self.pending_battle = Some(PendingBattle { attacker: force, defenders: vec![blocker], settlement: None, resume: Some(region) });
            events.push(CampaignEvent::PreBattle { attacker: force });
            return Ok(events);
        }
        let defenders = self.defenders_of(region);
        if defenders.is_empty() {
            self.occupy(region, fa, Some(force), &mut events);
            // An undefended settlement: the capture report with the surrender flag clear (PROVISIONAL: which
            // capture variant the original uses here, 0x00B58560 or 0x00B58C00 with the flag set, is not traced).
            self.settle_capture(region, fa, Some(force), false, &mut events);
        } else {
            self.pending_battle = Some(PendingBattle { attacker: force, defenders, settlement: Some(region), resume: None });
            events.push(CampaignEvent::PreBattle { attacker: force });
        }
        Ok(events)
    }

    fn set_tax_level(&mut self, faction: FactionId, class: TaxClass, level: String) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_turn(faction)?;
        if !self.rules.tax_levels.contains_key(&level) {
            return Err(CommandError::UnknownTaxLevel(level));
        }
        // The faction's governorships carry the levels (`GOVERNORSHIP_TAXES`: level index and rate); every
        // faction of the shipped campaigns has one governorship, so the faction's level is its level.
        let index = super::details::TAX_LEVELS.iter().position(|k| *k == level);
        let rate = self.rules.tax_levels.get(&level).copied().unwrap_or(0);
        if let (Some(index), Some(d)) = (index, self.world.faction_details.get_mut(&faction)) {
            for g in d.posts.iter_mut().filter_map(|p| p.governorship.as_mut()) {
                match class {
                    TaxClass::Lower => (g.taxes.lower, g.taxes.lower_rate) = (index as u32, rate),
                    TaxClass::Upper => (g.taxes.upper, g.taxes.upper_rate) = (index as u32, rate),
                }
            }
        }
        let f = self.world.factions.get_mut(&faction).expect("checked");
        match class {
            TaxClass::Lower => f.tax_lower = level,
            TaxClass::Upper => f.tax_upper = level,
        }
        Ok(vec![CampaignEvent::GovernorshipTaxRateChanged { faction }])
    }

    /// Recruitment points of a region: how many units can be in training there at once
    /// (INFERRED meaning). Land (`0x00B61F30`, CONFIRMED): the region's `recruitment_points` effect
    /// (rounded int effect), plus `recruitment_points_home_region` in the owner's capital
    /// (`0x00A8B5A0`: the region is faction `+0x72C`), plus the campaign's [`ai_recruitment_points`](super::features::CampaignFeatures::ai_recruitment_points) for an AI-run owner (1 for France in `mp_eur_napoleon`)
    /// (hard-coded). The region's effects are its buildings' and the owner's faction-wide effects
    /// (`0x00A67530`, [`super::effects::Effects::region`]). Naval:
    /// per port, `naval_recruitment_points` of the port's own building (`0x00B61EE0`, CONFIRMED),
    /// summed over the region's ports (the model's single naval queue).
    pub fn recruitment_points(&self, region: RegionId, naval: bool) -> u32 {
        let owner = self.world.regions.get(&region).map_or(super::FactionId(0), |r| r.owner);
        self.recruitment_points_with(&super::effects::Effects::compute_for(self, owner), region, naval)
    }

    /// [`Self::recruitment_points`] with the effects already computed.
    pub fn recruitment_points_with(&self, fx: &super::effects::Effects, region: RegionId, naval: bool) -> u32 {
        let Some(r) = self.world.regions.get(&region) else { return 0 };
        let sum = |key: &str| -> f32 { fx.region(region, key) };
        if naval {
            // Each port slot has its own queue whose capacity is the int `naval_recruitment_points` of the
            // port's own building (0x00B61EE0, CONFIRMED: the building's local set, any health, no faction
            // part). The model keeps one naval queue per region, so the ports' capacities are summed
            // (PROVISIONAL for a region with more than one port).
            let local = |b: &super::world::BuildingRef| self.rules.effects.building_local().get(&b.level_key).map_or(0, |s| s.get_int("naval_recruitment_points"));
            return r.slots.iter().filter(|s| s.port).filter_map(|s| s.building.as_ref()).map(local).sum::<i32>().max(0) as u32;
        }
        let mut pts = sum("recruitment_points").round_ties_even() as i32;
        if self.world.capital(r.owner) == Some(region) {
            pts += sum("recruitment_points_home_region").round_ties_even() as i32;
        }
        let human = self.turn.humans.contains(&r.owner);
        if !human && let Some(f) = self.world.factions.get(&r.owner) {
            pts += self.rules.features.ai_recruitment_bonus(&f.key);
        }
        pts.max(0) as u32
    }

    /// The region's recruitable list (region +0x1A8, built by `RebuildRegionRecruitablesAndEffects`
    /// `0x00A6AE40` from each slot's building, `BuildSlotBuildingRecruitableList` `0x00B43CA0`; CONFIRMED but for
    /// the two gates tagged PROVISIONAL in the body): the units a building in the region allows
    /// (`building_units_allowed`) that its owner may recruit (`units_to_exclusive_faction_permissions`, and
    /// [`super::rules::CampaignRules::government_may_recruit`]), one entry per unit, sorted by unit key, each with
    /// the building flags that make it unavailable:
    /// * [`ENTRY_DAMAGED`] when the building's health is below 100;
    /// * [`ENTRY_OCCUPIED`] when the slot is held by another faction than the region's owner (`0x00A91FC0`);
    /// * [`ENTRY_BESIEGED`] when the settlement is under siege (its `IsUnderSiege` virtual +0x98): never set,
    ///   the campaign model has no sieges yet (BACKLOG "Sieges on the campaign map");
    /// * [`ENTRY_NO_TECHNOLOGY`] when one of the unit's required technologies is not researched (`0x008AAB60`:
    ///   every `unit_required_technology_junctions` technology, linked by `0x00EA9B10`, must be in state 0).
    ///
    /// A unit allowed by several buildings keeps one entry (`MergeRecruitableEntrySorted` `0x00B08E30`): an
    /// unflagged one replaces a flagged one, an unflagged one stays, and flagged ones OR their flags. The queue
    /// command refuses a flagged entry, and the queue step holds back an item whose entry is flagged
    /// ([`Self::recruitable_entry_flags`], `region_turn`). The road counts as a slot of the owner (as in
    /// [`super::world::Region::effect_buildings`]).
    pub fn recruitable_units(&self, region: RegionId) -> Vec<RecruitableUnit> {
        let Some(r) = self.world.regions.get(&region) else { return Vec::new() };
        let (fkey, government) = self.world.factions.get(&r.owner).map_or(("", ""), |f| (f.key.as_str(), f.government_key.as_str()));
        let flag = |on: bool, flag: u32| if on { flag } else { 0 };
        let sources = r
            .slots
            .iter()
            .filter_map(|s| s.building.as_ref().map(|b| (b, flag(b.is_damaged(), ENTRY_DAMAGED) | flag(r.slot_occupied(s), ENTRY_OCCUPIED))))
            .chain(r.road.as_ref().map(|b| (b, flag(b.is_damaged(), ENTRY_DAMAGED))));
        let mut out: Vec<RecruitableUnit> = Vec::new();
        for (b, building_flags) in sources {
            for u in self.rules.buildings.get(&b.level_key).map(|b| b.units_allowed.as_slice()).unwrap_or(&[]) {
                // PROVISIONAL: two more gates leave a unit out (`0x00B43CA0`), not ported: `0x00AA1660` (unit
                // +0x98, when set, must be in the region's list +0x270) and `0x00A27960` (the unit must not be in
                // the campaign-wide list model +0xFA8 → +0x24). Their sources are not traced.
                if !self.rules.units.contains_key(u) || !self.rules.faction_may_recruit(fkey, u) || !self.rules.government_may_recruit(government, u) {
                    continue;
                }
                let flags = building_flags | if self.unit_tech_ok(r.owner, u) { 0 } else { ENTRY_NO_TECHNOLOGY };
                match out.iter_mut().find(|e| e.unit_key == *u) {
                    Some(e) if e.flags == 0 => {}
                    Some(e) if flags == 0 => e.flags = 0,
                    Some(e) => e.flags |= flags,
                    None => out.push(RecruitableUnit { unit_key: u.clone(), flags }),
                }
            }
        }
        // The list is kept sorted by unit key (`0x00B78140`: `CompareUniStrings`, case-sensitive code units, the
        // order of `str::cmp` on these ASCII keys).
        out.sort_by(|a, b| a.unit_key.cmp(&b.unit_key));
        out
    }

    /// The unavailability flags of the region's recruitable `entry` (one of [`Self::recruitable_units`]): its
    /// building flags plus what `0x00B69BA0` adds (CONFIRMED; the list is priced and flagged by the land queue's
    /// `0x00B31020`, its naval twin for a ship), `cost` being the entry's cost
    /// ([`super::economy::recruitment_cost`]) and `counts` the owner's [`Self::unit_type_counts`] (built once
    /// for all the entries). Any flag makes the queue command refuse the unit, and the recruitment card shows
    /// them as its reasons:
    /// * [`ENTRY_UNIT_CAP`] when the unit has a cap (`units` #15) and the owner's units of that type plus its
    ///   queued items of it reach it (`0x008F68B0`, [`UnitTypeCounts::cap_room`]);
    /// * [`ENTRY_TOO_DEAR`] when the cost is above the owner's treasury compared unsigned
    ///   ([`super::treasury::recruitment_affordable`]), so a faction in debt is never flagged;
    /// * [`ENTRY_NO_POPULATION`] when the region's population (`REGION_FACTORS` #2) is below
    ///   `recruitment_population_cost` + `minimum_population_after_recruitment` (`0x00A89550`,
    ///   [`super::population::RecruitmentPopulation::available`]; never with the shipped data, where both are 0);
    /// * [`ENTRY_QUEUE_FULL`] when the queue of the unit's kind (land or naval) holds [`MAX_QUEUE`] items
    ///   (`0x00B62040`).
    pub fn recruitable_entry_flags(
        &self,
        region: &super::world::Region,
        entry: &RecruitableUnit,
        unit: &super::rules::UnitRules,
        cost: i32,
        counts: &UnitTypeCounts<'_>,
    ) -> u32 {
        let mut flags = entry.flags;
        if counts.cap_room(&entry.unit_key, unit) == Some(0) {
            flags |= ENTRY_UNIT_CAP;
        }
        let treasury = self.world.factions.get(&region.owner).map_or(0, |f| f.treasury);
        if !super::treasury::recruitment_affordable(treasury, cost) {
            flags |= ENTRY_TOO_DEAR;
        }
        if !counts.population.available(region.population) {
            flags |= ENTRY_NO_POPULATION;
        }
        if self.recruitment_queue_room(region, unit.is_naval) == 0 {
            flags |= ENTRY_QUEUE_FULL;
        }
        flags
    }

    /// How many more items the region's queue of that kind (land or naval) takes: [`MAX_QUEUE`] less the items
    /// it holds (`0x00B62040`), 0 when full ([`ENTRY_QUEUE_FULL`]).
    pub fn recruitment_queue_room(&self, region: &super::world::Region, naval: bool) -> usize {
        let used = region.recruitment_queue.iter().filter(|i| self.rules.is_naval_unit(&i.unit_key) == naval).count();
        (MAX_QUEUE as usize).saturating_sub(used)
    }

    /// What `faction`'s unit caps are held against (`0x008F68B0`): per unit key, its live units of that type
    /// (faction +0x7C8, kept by the unit constructors and the destructor `0x0088F870`) plus its queued items of
    /// it (+0x7E4, kept by the item constructor `0x00AF3F80` and destructor `0x00AF7730`). Counted once and
    /// reused for every entry ([`Self::recruitable_entry_flags`], [`UnitTypeCounts::cap_room`]), with the campaign
    /// variables of the population gate ([`super::population::RecruitmentPopulation`]), read once per list.
    pub fn unit_type_counts(&self, faction: FactionId) -> UnitTypeCounts<'_> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let held = self.world.forces.values().filter(|f| f.faction == faction).flat_map(|f| &f.units).map(|u| u.unit_key.as_str());
        let queued = self.world.regions.values().filter(|r| r.owner == faction).flat_map(|r| &r.recruitment_queue).map(|i| i.unit_key.as_str());
        for key in held.chain(queued) {
            *counts.entry(key).or_default() += 1;
        }
        UnitTypeCounts { counts, population: super::population::RecruitmentPopulation::of(&self.rules) }
    }

    fn recruit(&mut self, region: RegionId, unit_key: String, target: Option<CharacterId>) -> Result<Vec<CampaignEvent>, CommandError> {
        if unit_key.is_empty() {
            return Err(CommandError::EmptyUnitKey);
        }
        let owner = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?.owner;
        self.check_turn(owner)?;
        let unit = self.rules.units.get(&unit_key).ok_or_else(|| CommandError::UnknownUnit(unit_key.clone()))?.clone();
        let Some(entry) = self.recruitable_units(region).into_iter().find(|e| e.unit_key == unit_key) else {
            return Err(CommandError::UnitNotAvailable(unit_key));
        };
        // The command (`CCQ` handler `0x00936B90` → `0x00B58DD0`, CONFIRMED) refuses only a full queue and a
        // flagged recruitable entry ([`Self::recruitable_entry_flags`]); it does not look at the recruitment
        // points, which only pace the training (see `region_turn`). What it charges is the entry's cost
        // ([`super::economy::recruitment_cost`]), through the spending category 2 converter, which passes it
        // unchanged ([`super::treasury::pay`]), then the region's population (`0x00AAF190`,
        // [`super::population::RecruitmentPopulation::charged`]).
        let cost = super::economy::recruitment_cost(self, &self.world.regions[&region], &unit_key, &unit);
        let flags = self.recruitable_entry_flags(&self.world.regions[&region], &entry, &unit, cost, &self.unit_type_counts(owner));
        if flags & ENTRY_BUILDING_FLAGS != 0 {
            return Err(CommandError::RecruitmentBlocked { unit_key, flags: flags & ENTRY_BUILDING_FLAGS });
        }
        if flags & ENTRY_QUEUE_FULL != 0 {
            return Err(CommandError::NoRecruitmentCapacity);
        }
        if flags & ENTRY_UNIT_CAP != 0 {
            return Err(CommandError::UnitCapReached(unit_key));
        }
        let faction = self.world.factions.get_mut(&owner).ok_or(CommandError::UnknownFaction(owner))?;
        if flags & ENTRY_TOO_DEAR != 0 {
            return Err(CommandError::InsufficientFunds { needed: cost, available: faction.treasury });
        }
        if flags & ENTRY_NO_POPULATION != 0 {
            return Err(CommandError::NotEnoughPopulation);
        }
        super::treasury::pay(&mut faction.treasury, cost);
        let id = RecruitmentItemId(self.world.alloc_id() as i32);
        let population = super::population::RecruitmentPopulation::of(&self.rules);
        let r = self.world.regions.get_mut(&region).expect("checked");
        r.population = population.charged(r.population);
        r.recruitment_queue.push(RecruitmentItem {
            id,
            unit_key,
            turns_remaining: unit.turns.max(1),
            cost,
            target,
        });
        Ok(vec![CampaignEvent::RecruitmentItemIssuedByPlayer { region }])
    }

    /// The region whose recruitment queue holds `item` (item ids are unique across the world,
    /// [`RecruitmentItemId`]).
    pub fn recruitment_item_region(&self, item: RecruitmentItemId) -> Option<RegionId> {
        self.world.regions.values().find(|r| r.recruitment_queue.iter().any(|i| i.id == item)).map(|r| r.id)
    }

    fn cancel_recruitment(&mut self, region: RegionId, item: RecruitmentItemId) -> Result<Vec<CampaignEvent>, CommandError> {
        let r = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?;
        let owner = r.owner;
        self.check_turn(owner)?;
        let index = r.recruitment_queue.iter().position(|i| i.id == item).ok_or(CommandError::UnknownRecruitmentItem(item))?;
        let item = self.world.regions.get_mut(&region).expect("checked").recruitment_queue.remove(index);
        // A full refund (CONFIRMED): the cancel command calls the cancel path with 1.
        self.cancelled_recruitment_items(region, std::slice::from_ref(&item), |_, _| true);
        Ok(Vec::new())
    }

    /// The cancel path `0x00B1A820`, for `items` already taken out of `region`'s queue (the cancel
    /// command, the turn start's removal of items the region can no longer recruit, an owner change):
    /// the region gets its population back for every item (`0x00A61AA0`, whatever the flag,
    /// [`super::population::RecruitmentPopulation::credited`]); each item `refunded` selects (the flag
    /// the caller passes, 1 = refund) has its item vtable +0x1C (`0x00B5C060` land, `0x00B5C0A0` naval)
    /// credit its stored cost (item +0x20, the entry cost the queue command charged, `0x00AF3F80`) as
    /// income category 3, whose converter passes it unchanged ([`super::treasury::credit`]: 32-bit
    /// wrapping), to the region's owner (`0x00B2B7C0`: the queue's region +0xF4).
    pub(crate) fn cancelled_recruitment_items(
        &mut self,
        region: RegionId,
        items: &[super::world::RecruitmentItem],
        refunded: impl Fn(&super::rules::CampaignRules, &super::world::RecruitmentItem) -> bool,
    ) {
        let population = super::population::RecruitmentPopulation::of(&self.rules);
        let Some(r) = self.world.regions.get_mut(&region) else {
            log::warn!("recruitment items of unknown region {region:?} cancelled: their population and refunds were dropped");
            return;
        };
        r.population = population.credited(r.population, items.len());
        let owner = r.owner;
        let refund = items.iter().filter(|i| refunded(&self.rules, i)).fold(0i32, |sum, i| sum.wrapping_add(i.cost));
        if refund == 0 {
            return;
        }
        match self.world.factions.get_mut(&owner) {
            Some(f) => super::treasury::credit(&mut f.treasury, refund),
            None => log::warn!("region {region:?}: its owner {owner:?} has no faction record; a recruitment refund of {refund} was dropped"),
        }
    }

    /// Can `level_key` be built in that slot now? Checks the slot type against
    /// `building_chain_to_slots` and, for an occupied slot, that it is an upgrade of the standing
    /// building (`building_upgrades_junction`) and that building is at full health (the panel
    /// offers upgrades only then, `0x009FBAB0`, CONFIRMED; the same rule as
    /// [`Self::construction_options`]); an empty slot takes only level 0 of a chain.
    /// Repairs restore full health for the slots and the walls (`RepairBuilding`, and the AI's
    /// `ai_repairs`). A road has no repair path in the model, but nothing in the game damages one:
    /// capture rolls the settlement slots and the walls (`0x00B14930`), sabotage targets slots. Only
    /// a save holding a damaged road could hold one back (PROVISIONAL: the exe's road repair is not
    /// traced).
    /// The building level's required technologies must be researched ([`Self::building_tech_ok`]).
    pub fn can_build(&self, region: RegionId, slot: SlotRef, level_key: &str) -> Result<i32, CommandError> {
        self.checked_option(region, slot, level_key).map(|o| o.cost)
    }

    /// [`Self::can_build`]'s checks, then the level's entry of the slot's option list: what the construct
    /// command charges (`0x00B13D50` rebuilds the list and starts the matching entry).
    fn checked_option(&self, region: RegionId, slot: SlotRef, level_key: &str) -> Result<ConstructionOption, CommandError> {
        let r = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?;
        if !self.rules.buildings.contains_key(level_key) {
            return Err(CommandError::UnknownBuilding(level_key.into()));
        }
        let (slot_type, standing) = r.construction_slot(slot).ok_or(CommandError::BadSlot(slot))?;
        let cannot = || CommandError::CannotBuild(level_key.into());
        // The option rule's own tests, the cheap ones before the effect set: a candidate of the slot
        // ([`Self::is_slot_candidate`]), offered there to the owner ([`Self::level_offered`]), its technology researched
        // (the command's list, flags 1, 1, drops the levels whose technology is missing). Our refusal order
        // (CannotBuild before SlotBusy) is the model's own: the exe's command does nothing either way.
        let candidate = self.is_slot_candidate(standing, level_key);
        if !candidate || self.level_offered(r.owner, slot_type, level_key).is_none() || !self.building_tech_ok(r.owner, level_key) {
            return Err(cannot());
        }
        if r.construction.iter().any(|c| c.slot == slot) {
            return Err(CommandError::SlotBusy);
        }
        let treasury = self.world.factions.get(&r.owner).map_or(0, |f| f.treasury);
        let set = super::economy::region_effect_set(self, r);
        self.slot_option(r.owner, treasury, slot_type, level_key, &set).ok_or_else(cannot)
    }

    /// The construction options of a slot as the original lists them (`0x00B43300`, CONFIRMED
    /// structure): nothing while the slot has a construction item; for a standing building the
    /// levels it upgrades to (`building_upgrades_junction` data order), for an empty slot every
    /// level 0 (key order); each kept when its chain may stand in the slot type
    /// (`building_chain_to_slots`) and the owner may build it ([`Self::building_permitted`]).
    /// Unaffordable levels and levels whose technology is missing stay in the list, marked (flags 1
    /// and 8 there; the panel greys them). A damaged building gets no options (the panel lists
    /// upgrades only at full health, `0x009FBAB0`). The same rule for every slot, the walls and the
    /// road included (ordinary slots in the exe). The permission test rejects the scripts'
    /// restricted levels too ([`World::restricted_buildings`](super::world::World::restricted_buildings)).
    pub fn construction_options(&self, region: RegionId, slot: SlotRef) -> Vec<ConstructionOption> {
        self.construction_options_in(region, slot, None)
    }

    /// The level-0 building keys that may stand in each slot type (`building_chain_to_slots`), in
    /// key order: the empty-slot candidates of [`Self::construction_options`], built once by a
    /// caller that asks for many slots ([`Self::construction_options_in`]).
    pub fn new_levels_by_slot_type(&self) -> BTreeMap<&str, Vec<&str>> {
        let mut out: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (key, b) in self.rules.buildings.iter().filter(|(_, b)| b.level == 0) {
            for t in self.rules.chain_slots.get(&b.chain).map(Vec::as_slice).unwrap_or(&[]) {
                out.entry(t.as_str()).or_default().push(key.as_str());
            }
        }
        out
    }

    /// [`Self::construction_options`] with the empty-slot candidates taken from `new_levels`
    /// ([`Self::new_levels_by_slot_type`]) when given, instead of a scan of every level. The one
    /// option rule the panels, the building browser, the AI and the construct command share.
    pub fn construction_options_in(
        &self,
        region: RegionId,
        slot: SlotRef,
        new_levels: Option<&BTreeMap<&str, Vec<&str>>>,
    ) -> Vec<ConstructionOption> {
        let Some(r) = self.world.regions.get(&region) else { return Vec::new() };
        self.slot_options(r, slot, new_levels, &mut None)
    }

    /// [`Self::construction_options_in`] for every slot of `region` (the slots, the walls, the road), with
    /// the region's effect set built once for all of them instead of once per slot.
    pub fn region_construction_options(
        &self,
        region: RegionId,
        new_levels: Option<&BTreeMap<&str, Vec<&str>>>,
    ) -> Vec<(SlotRef, Vec<ConstructionOption>)> {
        let Some(r) = self.world.regions.get(&region) else { return Vec::new() };
        let mut set = None;
        (0..r.slots.len())
            .map(SlotRef::Slot)
            .chain([SlotRef::Walls, SlotRef::Road])
            .map(|slot| (slot, self.slot_options(r, slot, new_levels, &mut set)))
            .collect()
    }

    /// One slot's option list; `set` holds the region's effect set, built at the first slot that needs it.
    fn slot_options(
        &self,
        r: &super::world::Region,
        slot: SlotRef,
        new_levels: Option<&BTreeMap<&str, Vec<&str>>>,
        set: &mut Option<super::effects::EffectSet>,
    ) -> Vec<ConstructionOption> {
        let Some((slot_type, standing)) = r.construction_slot(slot) else { return Vec::new() };
        if r.construction.iter().any(|c| c.slot == slot) {
            return Vec::new();
        }
        let candidates = self.slot_candidates(slot_type, standing, new_levels);
        if candidates.is_empty() {
            return Vec::new();
        }
        let owner = r.owner;
        let treasury = self.world.factions.get(&owner).map_or(0, |f| f.treasury);
        // The region's effect set, built once for the whole list as `0x00B43880` does.
        let set = set.get_or_insert_with(|| super::economy::region_effect_set(self, r));
        candidates.into_iter().filter_map(|k| self.slot_option(owner, treasury, slot_type, k, set)).collect()
    }

    /// The levels a slot may take next (`0x00B43300`'s candidates; the one rule both the option list and the
    /// construct command use), for a standing building: its upgrades (`building_upgrades_junction` order) when
    /// it is at full health, none when it is damaged. `None` for an empty slot, whose candidates are the
    /// level-0 records (the slot type is tested by [`Self::level_offered`]).
    fn upgrade_candidates(&self, standing: Option<&super::world::BuildingRef>) -> Option<&[String]> {
        let b = standing?;
        let upgrades = self.rules.buildings.get(&b.level_key).map_or(&[][..], |s| s.upgrades_to.as_slice());
        Some(if b.is_damaged() { &[] } else { upgrades })
    }

    /// A slot's candidate levels ([`Self::upgrade_candidates`]): an empty slot takes `new_levels`' list for the
    /// slot type when given, else every level 0 in key order.
    fn slot_candidates<'a>(
        &'a self,
        slot_type: &str,
        standing: Option<&super::world::BuildingRef>,
        new_levels: Option<&BTreeMap<&'a str, Vec<&'a str>>>,
    ) -> Vec<&'a str> {
        match (self.upgrade_candidates(standing), new_levels) {
            (Some(u), _) => u.iter().map(String::as_str).collect(),
            (None, Some(index)) => index.get(slot_type).cloned().unwrap_or_default(),
            (None, None) => self.rules.buildings.iter().filter(|(_, b)| b.level == 0).map(|(k, _)| k.as_str()).collect(),
        }
    }

    /// Is `level_key` one of a slot's candidate levels ([`Self::upgrade_candidates`]; for an empty slot one
    /// lookup instead of a scan)?
    fn is_slot_candidate(&self, standing: Option<&super::world::BuildingRef>, level_key: &str) -> bool {
        match self.upgrade_candidates(standing) {
            Some(u) => u.iter().any(|k| k == level_key),
            None => self.rules.buildings.get(level_key).is_some_and(|b| b.level == 0),
        }
    }

    /// The level's rules when it is offered in a slot of type `slot_type` to `owner`: its chain may stand there
    /// (`building_chain_to_slots`) and the owner may build it ([`Self::building_permitted`]).
    fn level_offered(&self, owner: FactionId, slot_type: &str, k: &str) -> Option<&super::rules::BuildingRules> {
        let b = self.rules.buildings.get(k)?;
        let slot_ok = self.rules.chain_slots.get(&b.chain).is_some_and(|t| t.iter().any(|t| t == slot_type));
        (slot_ok && self.building_permitted(owner, k)).then_some(b)
    }

    /// One level's option entry in a slot of type `slot_type` (`0x00B43300`'s per-level step): none when the
    /// level is not offered there ([`Self::level_offered`]).
    fn slot_option(&self, owner: FactionId, treasury: i32, slot_type: &str, k: &str, set: &super::effects::EffectSet) -> Option<ConstructionOption> {
        let b = self.level_offered(owner, slot_type, k)?;
        let cost = building_cost(b.cost, cost_modifier_in(set, &b.chain));
        let affordable = super::treasury::construction_affordable(treasury, cost);
        Some(ConstructionOption { cost, turns: b.turns.max(1), affordable, tech: self.building_tech_ok(owner, k), level_key: k.to_owned() })
    }

    /// The map fort levels, by level index: every `building_levels` row of a chain whose slot types
    /// include `"fort"` (vanilla: `fFort1_wooden_artillery_fort`, `fFort2_western_artillery_fort`,
    /// `fFort3_star_fort`). As the exe's `BuildFactionFortLevelList` `0x00B42ED0` (CONFIRMED: chains
    /// of slot type `fort`, `0x0134297C`, all of them in one list, with no faction permission test).
    /// The standing and the next level are picked from this list by level index alone
    /// (`FindFortLevelRecordByIndex` `0x00B42EA0`, `FindNextFortUpgradeLevel` `0x00B430C0`,
    /// CONFIRMED): with several fort chains (mods) the first record of an index wins. PROVISIONAL:
    /// the list's order (`0x00AC2290`'s sort key is not traced; ours is level index, then key).
    pub fn map_fort_levels(&self) -> Vec<(i32, String)> {
        let is_fort = |chain: &str| self.rules.chain_slots.get(chain).is_some_and(|t| t.iter().any(|t| t == "fort"));
        let mut levels: Vec<(i32, String)> = self.rules.buildings.iter().filter(|(_, b)| is_fort(&b.chain)).map(|(k, b)| (b.level, k.clone())).collect();
        levels.sort();
        levels
    }

    /// The standing level of `region`'s map fort, from `levels` ([`Self::map_fort_levels`]), as
    /// (level index, level key): the level its `FORT_ARRAY` record names (`Fort::key`; INFERRED
    /// that the record's string the loader `0x00AEB190` stores at `CampaignFort` `+0x188` is the
    /// level key), else level index 0 (PROVISIONAL: none can be built, and no shipped file holds a
    /// fort). The index is the one the upgrade asks for ([`Self::map_fort_next_level`]).
    pub fn map_fort_standing(&self, region: RegionId, levels: &[(i32, String)]) -> Option<(i32, String)> {
        self.world.regions.get(&region)?;
        let loaded = self.world.forts.values().filter(|f| f.region == region).find_map(|f| levels.iter().find(|(_, k)| *k == f.key));
        loaded.or_else(|| levels.iter().find(|(l, _)| *l == 0)).cloned()
    }

    /// Does a map fort stand in `region` (an item of its `FORT_ARRAY`, [`World::forts`](super::world::World::forts))?
    pub fn has_map_fort(&self, region: RegionId) -> bool {
        self.world.forts.values().any(|f| f.region == region)
    }

    /// The level a map fort standing at level index `standing` upgrades to, from `levels`
    /// ([`Self::map_fort_levels`]), as `FindNextFortUpgradeLevel` `0x00B430C0` (CONFIRMED, own
    /// Ghidra copy): it reads the fort's own level index (`CampaignFort` `+0x188`, an int; no level
    /// record is looked up for the standing level), takes the **first** record whose level index is
    /// one above it (`FindFortLevelRecordByIndex` `0x00B42EA0`), and gives none when there is no
    /// such record or the scripts restricted it (`0x009CDA90` on the campaign's restricted set,
    /// [`World::restricted_buildings`](super::world::World::restricted_buildings)) -- a restricted
    /// record is not passed over for another chain's level of the same index. The fort panel asks
    /// this; the upgrade command does nothing in the shipped exe (`ProcessFortUpgradeConstruction`
    /// `0x00B4E620` stops at the constant-false `0x0047BA10`), so ours sends none.
    pub fn map_fort_next_level<'a>(&self, standing: i32, levels: &'a [(i32, String)]) -> Option<&'a str> {
        levels
            .iter()
            .find(|(l, _)| *l == standing + 1)
            .map(|(_, k)| k.as_str())
            .filter(|k| !self.world.restricted_buildings.contains(*k))
    }

    /// May `faction` build `level_key` at all? The construction options (`0x00B43300`) keep only
    /// levels that pass `0x008BE7F0`: a level record holds two keyed sets (+0x124 and +0x140) and
    /// passes when the faction is in the first or the faction's culture record in the second
    /// (CONFIRMED structure). INFERRED: the sets are `building_faction_variants` (faction keys) and
    /// `building_culture_variants` (the faction's culture: `cultures_subcultures` of its subculture): in the shipped DB a nation's
    /// prestige building has only its own faction row and the Peninsular campaign's Spain chains
    /// only `spa_*` rows. Before the sets, the test rejects (CONFIRMED, `0x008BE7F0`):
    /// a faction whose `+0x514` is 0 (`0x008CEEF0`; meaning UNKNOWN, not modelled), and a level the
    /// scripts restricted: `0x009CDA90` scans the campaign's one restricted set (faction `+0xA8` →
    /// `+0x8` → `+0xFA8`, count `+0x10`, records `+0x14`; shared by every faction) for the level record,
    /// here [`World::restricted_buildings`](super::world::World::restricted_buildings). The construct
    /// command runs the same test: `CCQ_BUILDING_CONSTRUCT` (`0x00931C80` → `0x00B13D50`) rebuilds the
    /// slot's options (`0x00B43880` → `0x00B43300`) and starts the item only when the level is among
    /// them, so [`Self::can_build`] asks this. PROVISIONAL: a level with neither list (test fixtures)
    /// is allowed.
    pub fn building_permitted(&self, faction: FactionId, level_key: &str) -> bool {
        let Some(b) = self.rules.buildings.get(level_key) else { return false };
        if self.world.restricted_buildings.contains(level_key) {
            return false;
        }
        if b.factions.is_empty() && b.cultures.is_empty() {
            return true;
        }
        let Some(key) = self.world.factions.get(&faction).map(|f| f.key.as_str()) else { return false };
        b.factions.iter().any(|f| f == key) || self.rules.faction_cultures.get(key).is_some_and(|c| b.cultures.iter().any(|x| x == c))
    }

    /// What building `level_key` in `region` costs now, as the construction card shows it and as the
    /// queued item keeps it (CONFIRMED, CAMPAIGN_FIDELITY.md §Construction cost): the option list
    /// `0x00B43300` computes `cost × (modifier + 100) × 0.01` in f32, rounded half to even (FISTP), with no
    /// clamp, and the construct command (`0x00B13DD0`) charges and stores that entry's cost. The modifier
    /// is [`Self::building_cost_modifier`]. One call builds the region's effect set; for many levels of one
    /// region, [`Self::construction_options_in`] builds it once.
    pub fn construction_cost(&self, region: RegionId, level_key: &str) -> i32 {
        let Some(b) = self.rules.buildings.get(level_key) else { return 0 };
        building_cost(b.cost, self.building_cost_modifier(region, level_key))
    }

    /// The percent cost modifier of `level_key` in `region` (CONFIRMED, `0x00E1EF10(chain, 0)` on the region's
    /// effect set `0x00A67530`): the chain-keyed `mod_cost` entry (bonus type 2, id 0, qualifier = the level's
    /// chain) of the region's effect set ([`super::economy::region_effect_set`]): the region's own buildings
    /// (`building_cost_mod_all` maps to 19 vanilla chains through `effect_bonus_value_building_chain_junctions`,
    /// not to every chain) plus the governing faction's sum (the saved base + difficulty handicap set, faction-wide
    /// buildings such as the steam sawmill's `building_cost_mod_all_global`, researched technologies, government
    /// and ministers). Repairs read the same modifier.
    pub fn building_cost_modifier(&self, region: RegionId, level_key: &str) -> f32 {
        let Some(b) = self.rules.buildings.get(level_key) else { return 0.0 };
        let Some(r) = self.world.regions.get(&region) else { return 0.0 };
        cost_modifier_in(&super::economy::region_effect_set(self, r), &b.chain)
    }

    fn construct(&mut self, region: RegionId, slot: SlotRef, level_key: String) -> Result<Vec<CampaignEvent>, CommandError> {
        let owner = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?.owner;
        self.check_turn(owner)?;
        let option = self.checked_option(region, slot, &level_key)?;
        let (cost, turns) = (option.cost, option.turns);
        let f = self.world.factions.get_mut(&owner).ok_or(CommandError::UnknownFaction(owner))?;
        if !super::treasury::can_pay_construction(f.treasury, cost) {
            // A refused negative cost is always the unsigned test's refusal (it reads as above 2^31).
            let available = f.treasury;
            return Err(if cost < 0 { CommandError::NegativeCost { cost, available } } else { CommandError::InsufficientFunds { needed: cost, available } });
        }
        super::treasury::pay(&mut f.treasury, cost);
        self.world.regions.get_mut(&region).expect("checked").construction.push(ConstructionItem {
            slot,
            level_key,
            turns_remaining: turns,
            cost,
        });
        Ok(vec![CampaignEvent::BuildingConstructionIssuedByPlayer { region }])
    }

    /// `CCQ_BUILDING_CANCEL_CONSTRUCTION` (handler `0x00931C30` → `0x00B1A790` with the command's refund flag 1,
    /// CONFIRMED): the slot's item is deleted and, when its stored cost (item +0x14) is not 0, that cost is
    /// credited back ([`super::treasury::credit`]), for a construction and a repair alike, whatever was charged.
    /// The credit passes the income category-3 converter, which passes it unchanged (CONFIRMED, see treasury.rs).
    fn cancel_construction(&mut self, region: RegionId, slot: SlotRef) -> Result<Vec<CampaignEvent>, CommandError> {
        let r = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?;
        let owner = r.owner;
        self.check_turn(owner)?;
        let index = r.construction.iter().position(|c| c.slot == slot).ok_or(CommandError::BadSlot(slot))?;
        let item = self.world.regions.get_mut(&region).expect("checked").construction.remove(index);
        if let Some(f) = self.world.factions.get_mut(&owner) {
            super::treasury::credit(&mut f.treasury, item.cost);
        }
        Ok(Vec::new())
    }

    /// Places a finished unit as the original does (SAVE_COMPAT.md §4, CONFIRMED from its saves
    /// and exe: queue processor `0x00B71FB0` → `0x00B0A270` creates 0x54-records; list walker
    /// `0x008EF790` consumes context+0xD4 (land) / +0xC4 (naval) lists via holder virtual +0x3C/+0x44,
    /// iterates holder+0xB8/+0xB4 (land) / +0xA8/+0xA4 (naval), calls `0x008A94C0` for each):
    /// * land: into the army garrisoned in the region's settlement, if it is the owner's and has
    ///   room (up to `CampaignModel::max_units`); otherwise a new army led by a new colonel attached to
    ///   the unit, garrisoned in the settlement (CONFIRMED placement branch).
    /// * naval: into the navy that received the port's last ship while it is still in the port
    ///   with room; otherwise a new navy led by a new captain, at the port's slot position (CONFIRMED).
    ///
    /// New ids come from [`World::alloc_id`](super::world::World::alloc_id).
    /// Public for test tooling (the save-compatibility test saves); the turn calls it when a
    /// recruitment item completes.
    pub fn spawn_recruited_unit(&mut self, region: RegionId, unit_key: String) -> CampaignEvent {
        use super::world::{Character, CharacterKind};
        let rules = self.rules.units.get(&unit_key).cloned().unwrap_or_default();
        let naval = rules.is_naval;
        // Full strength, CONFIRMED from the saves: `unit_stats_land.num_men` for land, the sum of the
        // `unit_stats_naval` crew triple for ships ([`super::world::recruited_unit_size`]).
        let men = super::world::recruited_unit_size(&self.rules, &unit_key);
        let reg = &self.world.regions[&region];
        let owner = reg.owner;
        let (spot, garrison) = if naval {
            let port = reg.slots.iter().find(|s| s.port).and_then(|s| s.position).unwrap_or(reg.settlement.position);
            (port, None)
        } else {
            (reg.settlement.position, reg.garrison)
        };
        let joinable = |id: ForceId| {
            self.world.forces.get(&id).is_some_and(|f| {
                f.faction == owner && f.is_navy == naval && f.units.len() < self.max_units(f.is_navy) && f.commander.is_some()
            })
        };
        let join = if naval {
            reg.fleet.filter(|&id| joinable(id) && self.force_position(id) == Some(spot))
        } else {
            garrison.filter(|&id| joinable(id))
        };
        let unit_id = super::ids::UnitId(self.world.alloc_id() as i32);
        let (force_id, character) = match join {
            Some(id) => (id, None),
            None => {
                let kind = if naval { CharacterKind::Captain } else { CharacterKind::Colonel };
                let mp = self.rules.agent_action_points.get(kind.esf_name()).copied().or_else(|| {
                    self.world.characters.values().filter(|c| c.kind == kind).map(|c| c.max_movement_points).max()
                });
                let c = CharacterId(self.world.alloc_id() as i32);
                let f = ForceId(self.world.alloc_id());
                // Inside the settlement when it has no garrison (land only). PROVISIONAL: with a
                // garrison of another faction inside (a deal or a liberation) the recruit stands
                // outside; the exe's spawn check `0x008EF790` is not traced for that case.
                let inside = !naval && self.world.regions[&region].garrison.is_none_or(|g| !self.world.forces.contains_key(&g));
                self.world.characters.insert(
                    c,
                    Character {
                        id: c,
                        faction: owner,
                        kind,
                        position: spot,
                        movement_points: mp.unwrap_or(0),
                        max_movement_points: mp.unwrap_or(0),
                        base_movement_points: mp.unwrap_or(0),
                        garrisoned_in: inside.then_some(region),
                    },
                );
                self.world.forces.insert(f, MilitaryForce { id: f, faction: owner, commander: Some(c), units: Vec::new(), is_navy: naval });
                self.update_sight_radius(c);
                self.name_new_character(c);
                let r = self.world.regions.get_mut(&region).expect("region exists");
                if naval {
                    r.fleet = Some(f);
                } else if inside {
                    r.garrison = Some(f);
                }
                (f, Some(c))
            }
        };
        self.world.forces.get_mut(&force_id).expect("force exists").units.push(CampaignUnit {
            id: unit_id,
            unit_key,
            men,
            max_men: men,
            character,
            officer_name: Default::default(),
        });
        self.name_new_unit(force_id, unit_id);
        CampaignEvent::UnitTrained { force: force_id, unit: unit_id }
    }
}

/// Straight-line distance between two fixed-point positions, in map units (f64 maths from the
/// exact fixed-point values, so it is deterministic).
pub fn straight_distance(from: (Fixed20, Fixed20), to: (Fixed20, Fixed20)) -> f32 {
    let dx = (i64::from(to.0.raw()) - i64::from(from.0.raw())) as f64 / f64::from(ONE_RAW);
    let dz = (i64::from(to.1.raw()) - i64::from(from.1.raw())) as f64 / f64::from(ONE_RAW);
    (dx * dx + dz * dz).sqrt() as f32
}

impl CampaignModel {
    /// An army of a faction at war with `attacker_faction` standing outside `region`'s settlement
    /// (not garrisoned, within twice [`CONTACT_DISTANCE`] of it; the distance is PROVISIONAL).
    pub(crate) fn army_outside(&self, region: RegionId, attacker_faction: super::FactionId) -> Option<ForceId> {
        let r = self.world.regions.get(&region)?;
        let (sx, sy) = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
        self.world
            .forces
            .values()
            .filter(|f| !f.is_navy && f.faction != attacker_faction && self.world.stance(attacker_faction, f.faction) == Stance::War)
            .filter_map(|f| {
                let c = self.world.characters.get(&f.commander?)?;
                if c.garrisoned_in.is_some() {
                    return None;
                }
                let d = (c.position.0.to_f32() - sx).powi(2) + (c.position.1.to_f32() - sy).powi(2);
                (d <= (2.0 * CONTACT_DISTANCE).powi(2)).then_some((d, f.id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }
}

/// The most items a recruitment queue holds (`0x00B62040`: full when its count is above 9, CONFIRMED).
pub const MAX_QUEUE: u32 = 10;

/// What a faction's recruitable entries are flagged against, built once per list
/// ([`CampaignModel::unit_type_counts`]): its live units plus queued items per unit key (its unit caps) and the
/// population gate's campaign variables.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitTypeCounts<'a> {
    counts: BTreeMap<&'a str, usize>,
    population: super::population::RecruitmentPopulation,
}

impl UnitTypeCounts<'_> {
    /// How many more units of `unit_key` the faction may queue before its cap (`units` #15) flags the entry:
    /// the cap less its units and queued items of the type, 0 at or past the cap; `None` without a cap.
    pub fn cap_room(&self, unit_key: &str, unit: &super::rules::UnitRules) -> Option<usize> {
        (unit.unit_cap > 0).then(|| (unit.unit_cap as usize).saturating_sub(self.counts.get(unit_key).copied().unwrap_or(0)))
    }
}

/// Recruitable entry flag (`0x00B69BA0`, [`CampaignModel::recruitable_entry_flags`]): the queue is full.
pub const ENTRY_QUEUE_FULL: u32 = 0x01;
/// Recruitable entry flag: the cost is above the treasury (compared unsigned).
pub const ENTRY_TOO_DEAR: u32 = 0x02;
/// Recruitable entry flag: the region's population is too small (`0x00A89550`).
pub const ENTRY_NO_POPULATION: u32 = 0x04;
/// Recruitable entry flag (`0x00B43CA0`, [`CampaignModel::recruitable_units`]): the building is damaged.
pub const ENTRY_DAMAGED: u32 = 0x08;
/// Recruitable entry flag: the building's slot is held by another faction than the region's owner.
pub const ENTRY_OCCUPIED: u32 = 0x10;
/// Recruitable entry flag: the settlement is under siege (never set: the campaign model has no sieges yet).
pub const ENTRY_BESIEGED: u32 = 0x20;
/// Recruitable entry flag: the faction holds and has queued as many units of the type as its cap allows.
pub const ENTRY_UNIT_CAP: u32 = 0x40;
/// Recruitable entry flag (`0x008AAB60`): a technology the unit needs is not researched.
pub const ENTRY_NO_TECHNOLOGY: u32 = 0x80;
/// The flags the region's recruitable list itself carries ([`CampaignModel::recruitable_units`]); the others are
/// added when the list is priced (`0x00B69BA0`). Only these hold back a queued item (`0x00B5AD90`).
pub const ENTRY_BUILDING_FLAGS: u32 = ENTRY_DAMAGED | ENTRY_OCCUPIED | ENTRY_BESIEGED | ENTRY_NO_TECHNOLOGY;

/// One entry of a region's recruitable list ([`CampaignModel::recruitable_units`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitableUnit {
    /// The unit (`units` key).
    pub unit_key: String,
    /// Its building flags ([`ENTRY_BUILDING_FLAGS`]); 0 = recruitable as far as the buildings go.
    pub flags: u32,
}
