//! Events emitted by the campaign model.
//!
//! The original fires named script events: `events.lua` declares 168 global event tables, and the
//! engine calls every function in `events.<Name>` with a `context` (W3 §6.2, §6.4, CONFIRMED).
//! Each variant below whose doc says "script event" uses **exactly** the original name, so that
//! the scripting layer can later forward it to `events.<Name>` unchanged.
//! [`CampaignEvent::script_name`] returns that name.
//!
//! Variants marked "project event" have no matching script event in the original; they exist so
//! that the display can react (e.g. redraw a diplomacy screen). Scripts never see them.

use super::ids::{CharacterId, FactionId, ForceId, RegionId, UnitId};
use super::world::{SlotRef, Stance};

/// Something that happened in the campaign model.
#[derive(Debug, Clone, PartialEq)]
pub enum CampaignEvent {
    /// Script event `FactionRoundStart` (CONFIRMED name, W3 §6.4 / W1 §5).
    FactionRoundStart {
        /// The faction.
        faction: FactionId,
    },
    /// Script event `FactionTurnStart` (CONFIRMED name).
    FactionTurnStart {
        /// The faction.
        faction: FactionId,
    },
    /// Script event `RegionTurnStart` (CONFIRMED name).
    RegionTurnStart {
        /// The region.
        region: RegionId,
    },
    /// Script event `SlotTurnStart` (CONFIRMED name).
    SlotTurnStart {
        /// The region holding the slot.
        region: RegionId,
        /// Slot index within `Region::building_slots`.
        slot: u32,
    },
    /// Script event `UnitTrained` (CONFIRMED name, declared but never registered by the shipped
    /// scripts). INFERRED meaning: a recruitment item has finished.
    UnitTrained {
        /// The force the unit joined.
        force: ForceId,
        /// The new unit.
        unit: UnitId,
    },
    /// Script event `RegionTurnEnd` (CONFIRMED name).
    RegionTurnEnd {
        /// The region.
        region: RegionId,
    },
    /// Script event `CharacterTurnStart` (CONFIRMED name).
    CharacterTurnStart {
        /// The character.
        character: CharacterId,
    },
    /// Script event `UnitTurnEnd` (CONFIRMED name).
    UnitTurnEnd {
        /// The force holding the unit.
        force: ForceId,
        /// The unit.
        unit: UnitId,
    },
    /// Script event `CharacterPromoted` (CONFIRMED name; 30 handlers in `export_triggers.lua` give a
    /// new General or admiral his first traits). CONFIRMED trigger point: `0x00A1A300` fires it when
    /// a character's agent type is set through `0x00A1A2E0`, which only the field promotion of a
    /// unit's commander does (the unit classes' slot 20, `0x008E1C20` land / `0x008E2260` naval:
    /// the player's `PromoteUnits` / `CCQ_PROMOTE_COMMANDER`, the console `promote_unit_commander`
    /// and the AI's `CAI_BDI_PROMOTE_UNIT`). Hiring from the pool (`0x00A164C0`) does not fire
    /// it. Model: [`CampaignCommand::PromoteUnit`](super::commands::CampaignCommand::PromoteUnit).
    CharacterPromoted {
        /// The character.
        character: CharacterId,
    },
    /// Script event `SufferSpyingAttempt` (CONFIRMED: `0x0094C500` builds it for the watched
    /// force's commander before the roll).
    SufferSpyingAttempt {
        /// The watched force's commander.
        character: CharacterId,
    },
    /// Script event `SpyingAttemptSuccess` (CONFIRMED: `0x0094C500` / `0x0094CCB0`, outcomes 0 and 1,
    /// context the spy).
    SpyingAttemptSuccess {
        /// The spy.
        character: CharacterId,
    },
    /// Script event `CharacterFactionSpyAttemptSuccessful` (CONFIRMED: after a successful spying,
    /// for the spy's faction leader when it has one).
    CharacterFactionSpyAttemptSuccessful {
        /// The spy's faction leader.
        character: CharacterId,
    },
    /// Script event `CharacterFactionSuffersSuccessfulSpyAttempt` (CONFIRMED: the same, for the
    /// victim's faction leader).
    CharacterFactionSuffersSuccessfulSpyAttempt {
        /// The victim's faction leader.
        character: CharacterId,
    },
    /// Script event `EspionageAgentApprehended` (CONFIRMED: `0x0094C500` outcomes 2 and 3, context the
    /// watched force's commander; the settlement order has no such event).
    EspionageAgentApprehended {
        /// The commander who caught the spy.
        character: CharacterId,
    },
    /// Script event `SufferAssassinationAttempt` (CONFIRMED: `0x0094ACB0` builds it for the target
    /// before the roll).
    SufferAssassinationAttempt {
        /// The target.
        character: CharacterId,
    },
    /// Script event `AssassinationAttemptSuccess` (CONFIRMED: `0x0094ACB0` outcomes 0 and 1, the agent).
    AssassinationAttemptSuccess {
        /// The assassin.
        character: CharacterId,
    },
    /// Script event `CharacterCriticallyFailsAssassination` (CONFIRMED: outcome 3, the agent, who dies).
    CharacterCriticallyFailsAssassination {
        /// The assassin.
        character: CharacterId,
    },
    /// Script event `SabotageAttemptSuccess` (CONFIRMED: building sabotage `0x0094DEF0` at
    /// `0x0094E51D`, outcomes 0 and 1, the agent).
    SabotageAttemptSuccess {
        /// The saboteur.
        character: CharacterId,
    },
    /// Script event `ArmySabotageAttemptSuccess` (CONFIRMED: `0x0094DA30`, outcomes 0 and 1, an agent
    /// that is not a guerilla).
    ArmySabotageAttemptSuccess {
        /// The saboteur.
        character: CharacterId,
    },
    /// Script event `HarassmentAttemptSuccess` (CONFIRMED: `0x0094DA30`, outcomes 0 and 1, a
    /// guerilla, type 16).
    HarassmentAttemptSuccess {
        /// The guerilla.
        character: CharacterId,
    },
    /// Script event `DuelFought` (CONFIRMED: `0x00934C60` fires it once for each duellist after the
    /// duel ends, the challenger first).
    DuelFought {
        /// One duellist.
        character: CharacterId,
    },
    /// Script event `CharacterBuildsSpyNetwork` (CONFIRMED: `0x008F9A00`, at a human faction's turn
    /// start, for a character with `subterfuge` > 0 whose idle turns (`CHARACTER` #15) are exactly 3;
    /// message 250 `spy_network_established` with it).
    CharacterBuildsSpyNetwork {
        /// The spy.
        character: CharacterId,
    },
    /// Script event `CharacterCreated` (CONFIRMED name; 74 handlers in `export_triggers.lua`, the
    /// starting traits): a character came into being in play (a candidate, a colonel or captain of
    /// a new unit, a successor, a minister; INFERRED: every creation fires it, as `0x00A0DFC0` does
    /// for the generic and historical characters). Characters read from a file do not.
    CharacterCreated {
        /// The new character.
        character: CharacterId,
    },
    /// Script event `CharacterTurnEnd` (CONFIRMED name).
    CharacterTurnEnd {
        /// The character.
        character: CharacterId,
    },
    /// Script event `FactionTurnEnd` (CONFIRMED name).
    FactionTurnEnd {
        /// The faction.
        faction: FactionId,
    },
    /// Script event `MovementPointsExhausted` (CONFIRMED name). INFERRED meaning: a character
    /// has just spent its last movement point.
    MovementPointsExhausted {
        /// The character.
        character: CharacterId,
    },
    /// Script event `GovernorshipTaxRateChanged` (CONFIRMED name). The faction's levels change, and its
    /// governorships' (one per faction in every shipped campaign).
    GovernorshipTaxRateChanged {
        /// The faction.
        faction: FactionId,
    },
    /// Script event `RecruitmentItemIssuedByPlayer` (CONFIRMED name). We emit it for every
    /// accepted `Recruit` command; whether the original also fires it for the AI is UNKNOWN.
    RecruitmentItemIssuedByPlayer {
        /// The region recruiting.
        region: RegionId,
    },
    /// Script event `ResearchCompleted` (CONFIRMED name, an exe string at 0x0134FBF4): a technology
    /// was researched (`0x008EED20`).
    ResearchCompleted {
        /// The faction.
        faction: FactionId,
        /// The technology key.
        technology: String,
    },
    /// Script event `PendingBankruptcy` (CONFIRMED name). INFERRED meaning: the treasury fell
    /// below zero at the faction's income phase.
    PendingBankruptcy {
        /// The faction.
        faction: FactionId,
    },
    /// Script event `BuildingConstructionIssuedByPlayer` (CONFIRMED name; eur scripting.lua
    /// registers it). We fire it for every accepted construction command.
    BuildingConstructionIssuedByPlayer {
        /// The region.
        region: RegionId,
    },
    /// Script event `BuildingCompleted` (CONFIRMED name): a construction item finished.
    BuildingCompleted {
        /// The region.
        region: RegionId,
        /// The slot built in.
        slot: super::world::SlotRef,
    },
    /// Script event `CampaignArmiesMerge` (CONFIRMED name): units moved from one force into another.
    CampaignArmiesMerge {
        /// The force that gave its units.
        force: ForceId,
        /// The force that received them.
        into: ForceId,
    },
    /// Script event `CharacterEntersGarrison` (CONFIRMED name).
    CharacterEntersGarrison {
        /// The character.
        character: CharacterId,
        /// The region whose settlement it entered.
        region: RegionId,
    },
    /// Script event `PreBattle` (CONFIRMED name): a battle is about to be fought.
    PreBattle {
        /// The attacking force.
        attacker: ForceId,
    },
    /// Script event `BattleCompleted` (CONFIRMED name; eur scripting.lua registers it).
    BattleCompleted {
        /// The attacking force (it may have been destroyed).
        attacker: ForceId,
        /// Its faction.
        attacker_faction: FactionId,
        /// True if the attacker won.
        attacker_won: bool,
    },
    /// Script event `CharacterCompletedBattle` (CONFIRMED name; trait triggers use it).
    CharacterCompletedBattle {
        /// The commander.
        character: CharacterId,
    },
    /// Script event `SettlementOccupied` (CONFIRMED name; eur scripting.lua registers it).
    SettlementOccupied {
        /// The region whose settlement was taken.
        region: RegionId,
        /// The new owner.
        faction: FactionId,
    },
    /// Script event `CharacterEmbarksNavy` (CONFIRMED name, "A character embarks on a navy").
    CharacterEmbarksNavy {
        /// The army's commander.
        character: CharacterId,
        /// The navy boarded.
        navy: ForceId,
    },
    /// Script event `CharacterDisembarksNavy` (CONFIRMED name, "A character disembarks a navy").
    CharacterDisembarksNavy {
        /// The army's commander.
        character: CharacterId,
        /// The navy left.
        navy: ForceId,
    },
    /// Project event: a character (and the force it commands) moved along `path` (logic x, z).
    CharacterMoved {
        /// The character.
        character: CharacterId,
        /// Points walked, from the old position to the new one.
        path: Vec<(f32, f32)>,
    },
    /// Project event: a force lost all its units and was removed.
    ForceDestroyed {
        /// The force.
        force: ForceId,
    },
    /// Project event: an agent action was resolved ([`super::agents`]).
    AgentActionResolved {
        /// The acting agent.
        agent: CharacterId,
        /// The target character (the force's commander for an army sabotage; `None` for a building).
        target: Option<CharacterId>,
        /// What he did.
        action: super::agents::AgentAction,
        /// How it went.
        outcome: super::agents::Outcome,
    },
    /// Project event: a General was hired from his faction's recruitment pool ([`super::pool`]).
    CharacterHired {
        /// The General.
        character: CharacterId,
        /// His new army.
        force: ForceId,
        /// What it cost.
        cost: i32,
    },
    /// Project event: a human took a settlement by force; the occupy / loot / liberate choice waits in
    /// `CampaignModel::pending_capture` (the original raises a UI event with the three previews, `0x008F8D00`).
    CaptureChoicePending {
        /// The region taken.
        region: RegionId,
        /// The capturer.
        faction: FactionId,
    },
    /// Project event: a capture choice was applied (`0x008C0310`).
    CaptureResolved {
        /// The region taken.
        region: RegionId,
        /// The capturer.
        faction: FactionId,
        /// What was done.
        choice: super::capture::CaptureChoice,
        /// Money looted (0 unless looted).
        money: i32,
    },
    /// Project event (no script event of this kind is known): the stance between two factions
    /// changed. `a`'s stance towards `b` is now `stance`; `b`'s is `stance.mirror()`.
    StanceChanged {
        /// First faction.
        a: FactionId,
        /// Second faction.
        b: FactionId,
        /// The new stance of `a` towards `b`.
        stance: Stance,
    },
    /// Project diagnostic, never sent to the scripts: a construction item named a slot the region
    /// does not have, so the turn dropped it without refund. The original cannot reach this state
    /// (its item is a child of the slot's own `BUILDING_MANAGER`, CONFIRMED save structure, so no
    /// item outlives its slot and the exe has no drop or refund path for one). The app logs it.
    ConstructionItemDropped {
        /// The region.
        region: RegionId,
        /// The missing slot the item named.
        slot: SlotRef,
        /// The level the item was building.
        level_key: String,
    },
    /// Project event: a faction's government type changed. The `government_type` attitude factors
    /// for all relationships drift from the new government's start value toward the limit.
    GovernmentChanged {
        /// The faction that changed government.
        faction: FactionId,
        /// The old government key.
        old_government: String,
        /// The new government key.
        new_government: String,
    },
}

impl CampaignEvent {
    /// A project diagnostic: never sent to the scripts, but returned to the app to be logged once
    /// (the model has no logger).
    pub fn is_diagnostic(&self) -> bool {
        matches!(self, CampaignEvent::ConstructionItemDropped { .. })
    }

    /// The original script event name, or `None` for project events.
    pub fn script_name(&self) -> Option<&'static str> {
        Some(match self {
            CampaignEvent::FactionRoundStart { .. } => "FactionRoundStart",
            CampaignEvent::FactionTurnStart { .. } => "FactionTurnStart",
            CampaignEvent::RegionTurnStart { .. } => "RegionTurnStart",
            CampaignEvent::SlotTurnStart { .. } => "SlotTurnStart",
            CampaignEvent::UnitTrained { .. } => "UnitTrained",
            CampaignEvent::RegionTurnEnd { .. } => "RegionTurnEnd",
            CampaignEvent::CharacterTurnStart { .. } => "CharacterTurnStart",
            CampaignEvent::UnitTurnEnd { .. } => "UnitTurnEnd",
            CampaignEvent::CharacterCreated { .. } => "CharacterCreated",
            CampaignEvent::CharacterPromoted { .. } => "CharacterPromoted",
            CampaignEvent::SufferSpyingAttempt { .. } => "SufferSpyingAttempt",
            CampaignEvent::SpyingAttemptSuccess { .. } => "SpyingAttemptSuccess",
            CampaignEvent::CharacterFactionSpyAttemptSuccessful { .. } => "CharacterFactionSpyAttemptSuccessful",
            CampaignEvent::CharacterFactionSuffersSuccessfulSpyAttempt { .. } => "CharacterFactionSuffersSuccessfulSpyAttempt",
            CampaignEvent::EspionageAgentApprehended { .. } => "EspionageAgentApprehended",
            CampaignEvent::SufferAssassinationAttempt { .. } => "SufferAssassinationAttempt",
            CampaignEvent::AssassinationAttemptSuccess { .. } => "AssassinationAttemptSuccess",
            CampaignEvent::CharacterCriticallyFailsAssassination { .. } => "CharacterCriticallyFailsAssassination",
            CampaignEvent::SabotageAttemptSuccess { .. } => "SabotageAttemptSuccess",
            CampaignEvent::ArmySabotageAttemptSuccess { .. } => "ArmySabotageAttemptSuccess",
            CampaignEvent::HarassmentAttemptSuccess { .. } => "HarassmentAttemptSuccess",
            CampaignEvent::DuelFought { .. } => "DuelFought",
            CampaignEvent::CharacterBuildsSpyNetwork { .. } => "CharacterBuildsSpyNetwork",
            CampaignEvent::CharacterTurnEnd { .. } => "CharacterTurnEnd",
            CampaignEvent::FactionTurnEnd { .. } => "FactionTurnEnd",
            CampaignEvent::MovementPointsExhausted { .. } => "MovementPointsExhausted",
            CampaignEvent::GovernorshipTaxRateChanged { .. } => "GovernorshipTaxRateChanged",
            CampaignEvent::RecruitmentItemIssuedByPlayer { .. } => "RecruitmentItemIssuedByPlayer",
            CampaignEvent::PendingBankruptcy { .. } => "PendingBankruptcy",
            CampaignEvent::ResearchCompleted { .. } => "ResearchCompleted",
            CampaignEvent::BuildingConstructionIssuedByPlayer { .. } => "BuildingConstructionIssuedByPlayer",
            CampaignEvent::BuildingCompleted { .. } => "BuildingCompleted",
            CampaignEvent::CampaignArmiesMerge { .. } => "CampaignArmiesMerge",
            CampaignEvent::CharacterEntersGarrison { .. } => "CharacterEntersGarrison",
            CampaignEvent::PreBattle { .. } => "PreBattle",
            CampaignEvent::BattleCompleted { .. } => "BattleCompleted",
            CampaignEvent::CharacterCompletedBattle { .. } => "CharacterCompletedBattle",
            CampaignEvent::SettlementOccupied { .. } => "SettlementOccupied",
            CampaignEvent::CharacterEmbarksNavy { .. } => "CharacterEmbarksNavy",
            CampaignEvent::CharacterDisembarksNavy { .. } => "CharacterDisembarksNavy",
            CampaignEvent::StanceChanged { .. }
            | CampaignEvent::CaptureChoicePending { .. }
            | CampaignEvent::CaptureResolved { .. }
            | CampaignEvent::AgentActionResolved { .. }
            | CampaignEvent::CharacterHired { .. }
            | CampaignEvent::CharacterMoved { .. }
            | CampaignEvent::ForceDestroyed { .. }
            | CampaignEvent::GovernmentChanged { .. }
            | CampaignEvent::ConstructionItemDropped { .. } => return None,
        })
    }
}
