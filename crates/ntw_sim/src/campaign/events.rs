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
    /// unit's commander does (the unit vtables' slot +0x4C, `0x008E1C20` land / `0x008E2260` naval:
    /// the player's `PromoteUnits` / `CCQ_PROMOTE_COMMANDER`, the console `promote_unit_commander`
    /// and the AI's `CAI_BDI_PROMOTE_UNIT`). Hiring from the pool (`0x00A164C0`) does not fire
    /// it. The original never fires it: the promotion's gate (slot +0x40) is `0x00462CB0`, constant false
    /// (CHARACTERS_FIDELITY.md §12, field promotion gate); ours fires it only when a mod grants the promotion bonus. Model: [`CampaignCommand::PromoteUnit`](super::commands::CampaignCommand::PromoteUnit).
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
    /// Script event `CampaignBuildingDamaged` (CONFIRMED name, string `0x0134FF84`, event vtable
    /// `0x0137CE30`): a building's health was set below 100 (`SetBuildingHealth` `0x00B452A0` posts it for
    /// any value below 100). Fired by a repair's turns that do not finish it. Its context accessors are
    /// `BuildingCompleted`'s (vtable `0x0137CEEC`) less +0x1C and +0x64 (that event's entry of the slot's list at +0xD8 and
    /// its level), the rest read from the slot at event +0x10 in both (CONFIRMED), so the host gives it
    /// the same region, settlement and faction.
    CampaignBuildingDamaged {
        /// The region.
        region: RegionId,
        /// The building's slot.
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
    /// Script event `CharacterLootedSettlement` (CONFIRMED name, "A character loots a settlement": name
    /// getter `0x008BDDA0` → string `0x0134FAF0`; vtable `0x0137CCB8`; `export_triggers.lua`'s
    /// `C_NTW_General_Looter_Good_Trigger` handles it). Posted by the loot choice `0x00B541E0` (at
    /// `0x00B54468`) after the option is applied. Its only context is the character (the event's +0x10,
    /// read by the character getter, vtable +0x1C `0x0044A500`; the faction getter +0x38 and the
    /// settlement / region getters are the null stub `0x00461180`): the commander of the capturing
    /// force, which the capture report keeps from the capture (report +0x8, from force +0x74,
    /// `0x00885F90` called by `0x00B58560` / `0x00B58C00`). Raised only when that force had one: the
    /// poster `0x00E30700` drops the event before Lua when its vtable +0xB8 (`0x008F9EE0`) fails, which
    /// it does for a null character.
    CharacterLootedSettlement {
        /// The looting commander.
        character: CharacterId,
    },
    /// Script event `FactionBecomesLiberationProtectorate` (CONFIRMED name, string `0x01350274`, event
    /// vtable `0x0137D064`): the liberation choice `0x00B4F090` posts it first (`0x00B4F0FE` in the
    /// `spa_napoleon` campaign, `0x00B5089D` in every other), before it puts the capturer's armies out of
    /// the region (`0x008D9DC0`) and passes the region to the liberated faction (`0x00B58A10`),
    /// CONFIRMED. Its one payload is the region's owner at that moment (settlement vtable +0x14
    /// `0x00B2B7D0`), the liberator; its only accessor is the faction (event vtable +0x38 `0x0044A500`,
    /// read by conditions such as `FactionTreasury` `0x008A0860`), so a script sees no region,
    /// settlement or character.
    FactionBecomesLiberationProtectorate {
        /// The liberating faction (the capturer, still the region's owner when the event is posted).
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
    /// Project event: a faction left the game ([`super::CampaignModel::destroy_faction`]; the exe
    /// posts its event +0x108 there, whose script name is not traced).
    FactionDestroyed {
        /// The faction.
        faction: FactionId,
    },
    /// Project event: a faction's campaign outcome plays a movie ([`super::CampaignModel::outcome_movie`];
    /// the exe's movie listener `0x0099B720` queues it on campaign +0x288 and the UI shows it in
    /// `event_movie_panel`). Pushed after the [`Self::FactionDestroyed`] of a human faction.
    OutcomeMovie {
        /// The faction the movie is for.
        faction: FactionId,
        /// The movie and its panel data.
        movie: super::outcome::OutcomeMovie,
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
        /// His hire cost at the place he joins (`hire_cost_at`). Only a human faction is charged it: the hire
        /// `0x00A1B8F0` charges `0x00BAF500(cost, 2)` only when faction +0x6E0 is set, so an AI faction pays
        /// nothing although the event still carries the cost (CONFIRMED, CAMPAIGN_FIDELITY.md §Treasury changes).
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
    /// Project event: a declaration of war asks a human ally to join; the call waits in
    /// `CampaignModel::pending_ally_war_calls` for `RespondToAllyWarCall` (the original raises the UI
    /// event of model +0xB10, vfunc +8, with these five values: `0x00B26C6A` / `0x00B26E5F`).
    AllyWarCallPending {
        /// The ally asked (a human faction).
        ally: FactionId,
        /// The declarer.
        declarer: FactionId,
        /// The declaration's target.
        target: FactionId,
        /// The ally is allied to the declarer (the UI event's first flag).
        allied_to_declarer: bool,
        /// The target's allies' loop asked it (the UI event's second flag).
        asked_by_target_side: bool,
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
    /// Project event: a message for `recipient`'s message feed (the message manager, model +0xFA0,
    /// posts it with `0x009D3250`; the display shows the messages of the human it views). The exe's
    /// delivery gates belong to the display: none for the rebels (`0x008CEEF0`), none while the flag
    /// at model +0xD9C → +0x6C is 1 (meaning UNKNOWN), and for a faction in a revolution (`0x008CA0B0`,
    /// faction +0x814) only when its +0x81C is set (not modelled: no revolutions in the model yet).
    Message {
        /// The faction whose feed gets the message.
        recipient: FactionId,
        /// What the message says.
        message: CampaignMessage,
    },
}

/// A message the model posts to a faction's message feed ([`CampaignEvent::Message`]). Each kind
/// is one entry of the exe's message type table (`0x0145C530`, indexed by type id); its name is
/// the `message_events` type the display looks the text and picture up by (with the recipient's
/// subculture).
#[derive(Debug, Clone, PartialEq)]
pub enum CampaignMessage {
    /// Type 11 `alliance_broken` (`0x009CA7B0`, CONFIRMED by disassembly 2026-10-17): the title is
    /// "<first name> - <second name>" (faction +0x518). Posted by an alliance break (`0x00B13840`,
    /// first = the former ally, second = the breaker) and by a cancelled military access
    /// (`0x00B67B20`, first = the faction losing the access, second = the one cancelling it).
    AllianceBroken {
        /// The first faction of the title.
        first: FactionId,
        /// The second faction of the title.
        second: FactionId,
    },
    /// Type 47 `ceasefire_declared` (`SendCeasefireDeclaredMessage` `0x009D1F20`, CONFIRMED by disassembly
    /// 2026-10-20): the title is the other side's name (faction +0x518), the detail empty. A peace
    /// (`0x00B262C0`) posts one to each side about the other, the target's first, unless the peace
    /// suppresses its messages ([`super::CampaignModel::make_peace_rules`]).
    CeasefireDeclared {
        /// The faction peace was made with (the title).
        other: FactionId,
    },
    /// Type 0x76 `military_access_prompt` (`SendMilitaryAccessPromptMessage` `0x00A0E630`, CONFIRMED by
    /// disassembly 2026-10-20): the title is the granter's name (faction +0x518), the detail empty. The
    /// round end's access step (`0x00B29170`) posts it to the faction holding timed access when the
    /// count-down leaves one turn ([`super::CampaignModel::diplomacy_round_end`]). Nothing answers it:
    /// its text asks the player to negotiate, through the ordinary diplomacy commands.
    MilitaryAccessPrompt {
        /// The faction giving the access (the title).
        granter: FactionId,
    },
    /// Type 0x112 `war_declared` (`0x009DAFE0`, CONFIRMED by disassembly 2026-10-20): the title is
    /// "<declarer name>\n<target name>" (faction +0x518), the detail empty. A declaration between two
    /// major powers (`0x00B268B0`) posts one to every listed faction that is neither side nor in its
    /// call lists, with no human test ([`super::CampaignModel::declare_war_rules`]).
    WarDeclared {
        /// The faction that declared the war (the title's first name).
        declarer: FactionId,
        /// The faction war was declared on (the title's second name).
        target: FactionId,
    },
    /// Type 0x14D `campmap_faction_joins_your_enemies` (`0x009DAE40`, the type table's entry
    /// `0x0145CA64`; CONFIRMED by disassembly 2026-10-20): the title is the joining faction's name
    /// (faction +0x518), the detail empty. Posted only by a war declared with its refresh flag
    /// (`0x00B26700`), i.e. a deal's join-war item ([`super::CampaignModel::join_war_by_deal`]), to
    /// the faction war is declared on, with no major-power or human test.
    FactionJoinsYourEnemies {
        /// The faction that joined the war (the title).
        joiner: FactionId,
    },
    /// A historical event (`0x00A06DA0`, model +0xFA0; CONFIRMED by disassembly): not a type of the
    /// message type table but a message of its own, posted by the round start's historical events
    /// ([`super::historical_events`]) and by the script's `trigger_historical_event` (`0x0096D520`). It
    /// shows the event's title (`events_onscreen_name_<key>`), text (`events_event_text_<key>`) and
    /// picture `ui/eventpics/<recipient's culture>/<events #6>` (`0x00F840E0`).
    HistoricalEvent {
        /// The `events` key.
        event: String,
    },
}

impl CampaignMessage {
    /// The message type's name (the exe's type table entry, the `message_events` key; for
    /// [`CampaignMessage::HistoricalEvent`], outside the table, our own name).
    pub fn type_name(&self) -> &'static str {
        match self {
            CampaignMessage::AllianceBroken { .. } => "alliance_broken",
            CampaignMessage::CeasefireDeclared { .. } => "ceasefire_declared",
            CampaignMessage::MilitaryAccessPrompt { .. } => "military_access_prompt",
            CampaignMessage::WarDeclared { .. } => "war_declared",
            CampaignMessage::FactionJoinsYourEnemies { .. } => "campmap_faction_joins_your_enemies",
            CampaignMessage::HistoricalEvent { .. } => "historical_event",
        }
    }
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
            CampaignEvent::CampaignBuildingDamaged { .. } => "CampaignBuildingDamaged",
            CampaignEvent::CampaignArmiesMerge { .. } => "CampaignArmiesMerge",
            CampaignEvent::CharacterEntersGarrison { .. } => "CharacterEntersGarrison",
            CampaignEvent::PreBattle { .. } => "PreBattle",
            CampaignEvent::BattleCompleted { .. } => "BattleCompleted",
            CampaignEvent::CharacterCompletedBattle { .. } => "CharacterCompletedBattle",
            CampaignEvent::SettlementOccupied { .. } => "SettlementOccupied",
            CampaignEvent::CharacterLootedSettlement { .. } => "CharacterLootedSettlement",
            CampaignEvent::FactionBecomesLiberationProtectorate { .. } => "FactionBecomesLiberationProtectorate",
            CampaignEvent::CharacterEmbarksNavy { .. } => "CharacterEmbarksNavy",
            CampaignEvent::CharacterDisembarksNavy { .. } => "CharacterDisembarksNavy",
            CampaignEvent::StanceChanged { .. }
            | CampaignEvent::CaptureChoicePending { .. }
            | CampaignEvent::AllyWarCallPending { .. }
            | CampaignEvent::CaptureResolved { .. }
            | CampaignEvent::AgentActionResolved { .. }
            | CampaignEvent::CharacterHired { .. }
            | CampaignEvent::CharacterMoved { .. }
            | CampaignEvent::ForceDestroyed { .. }
            | CampaignEvent::FactionDestroyed { .. }
            | CampaignEvent::OutcomeMovie { .. }
            | CampaignEvent::GovernmentChanged { .. }
            | CampaignEvent::ConstructionItemDropped { .. }
            | CampaignEvent::Message { .. } => return None,
        })
    }
}
