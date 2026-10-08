//! The agents tab, the fog of war's data levels and the agent action popups (`ui/agents.luac`,
//! `agent_options.luac`, `agent_action.luac`).

use super::*;

// ---------------------------------------------------------------------------------------------
// The agents tab (`ui/agents.luac`, CHARACTER_UI_HOOKS.md "Agents panel"). What the panel does with
// its info and which engine calls its buttons make is CONFIRMED from the panel's bytecode (read on
// the install 2026-10-05); what the engine puts in the info and answers is built here from the
// model, each part tagged.

/// The five abilities the agent buttons test (`card.character.Abilities.<key> == true`, CONFIRMED
/// keys read by `ShowAgentButtons`).
pub(super) const AGENT_BUTTON_ABILITIES: [&str; 5] = ["can_assassinate", "can_sabotage", "can_sabotage_army", "can_research", "can_duel"];

/// A spy type: the types the model's assassination gate accepts (`0x009225D0`, CONFIRMED kind test).
fn is_spy_kind(kind: CharacterKind) -> bool {
    matches!(kind, CharacterKind::Rake | CharacterKind::Assassin | CharacterKind::Guerilla)
}

/// A character listed on the agents tab: not a commander type (general, colonel, admiral, captain)
/// and not a minister (off-map). INFERRED: the tab is about agents; the engine's list is not traced.
fn is_listed_agent_kind(kind: CharacterKind) -> bool {
    !matches!(
        kind,
        CharacterKind::General | CharacterKind::Colonel | CharacterKind::Admiral | CharacterKind::Captain | CharacterKind::Minister
    )
}

/// Whether a character has one of [`AGENT_BUTTON_ABILITIES`].
/// - With saved `AgentAbilities` (CONFIRMED data, `CharacterDetails::abilities`): the model's ability
///   level ([`ntw_sim::campaign::agents::ability`], `0x009C7610`) is at least 1, the threshold the
///   model's own action gates use (`duel_chance`, `spy_chance`, `army_sabotage_chance`,
///   `building_sabotage_chance`). `can_assassinate` also needs a spy type, the model's
///   assassination gate.
/// - Without (a character the model made itself): INFERRED from the type, following which type
///   each model action is for: spies (rake, assassin, guerilla) assassinate and sabotage
///   (buildings and armies); gentlemen and scholars duel and research (steal: the model's
///   `steal_step` is a gentleman's). Missionaries have none of the five.
fn agent_has_ability(m: &CampaignModel, c: CharacterId, key: &str) -> bool {
    let Some(kind) = m.world.characters.get(&c).map(|ch| ch.kind) else { return false };
    let saved = m.world.character_details.get(&c).is_some_and(|d| !d.abilities.is_empty());
    if saved {
        return ntw_sim::campaign::agents::ability(m, c, key) >= 1 && (key != "can_assassinate" || is_spy_kind(kind));
    }
    let scholar = matches!(kind, CharacterKind::Gentleman | CharacterKind::EasternScholar);
    match key {
        "can_assassinate" | "can_sabotage" | "can_sabotage_army" => is_spy_kind(kind),
        "can_research" | "can_duel" => scholar,
        _ => false,
    }
}

/// Where an agent is "in" (the original's residences: settlements, ports and other map slots,
/// `SIEGEABLE_GARRISON_RESIDENCE` / `GARRISON_RESIDENCE`, which the model does not load).
/// INFERRED from the model's own fields:
/// - garrisoned in a region → its settlement;
/// - else standing on a non-settlement slot's position (a port, a school, a town; the model's
///   `steal_step` places a thief the same way) → that slot;
/// - else standing on a settlement's position → that settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Residence {
    Settlement(RegionId),
    Slot(RegionId, usize),
}

fn residence_of(m: &CampaignModel, c: CharacterId) -> Option<Residence> {
    let ch = m.world.characters.get(&c)?;
    if let Some(r) = ch.garrisoned_in {
        return Some(Residence::Settlement(r));
    }
    for r in m.world.regions.values() {
        if let Some(i) = r.slots.iter().position(|s| !s.key.starts_with("settlement:") && s.position == Some(ch.position)) {
            return Some(Residence::Slot(r.id, i));
        }
    }
    m.world.regions.values().find(|r| r.settlement.position == ch.position).map(|r| Residence::Settlement(r.id))
}

impl Residence {
    fn region(self) -> RegionId {
        match self {
            Residence::Settlement(r) | Residence::Slot(r, _) => r,
        }
    }
    /// The faction holding it: a slot's holder, else the region's owner.
    fn owner(self, m: &CampaignModel) -> Option<FactionId> {
        let r = m.world.regions.get(&self.region())?;
        Some(match self {
            Residence::Settlement(_) => r.owner,
            Residence::Slot(_, i) => r.slots.get(i).and_then(|s| s.holder).unwrap_or(r.owner),
        })
    }
    /// The address the scripts get: the region's for a settlement (as `SetSelectedEntity`), the
    /// building browser's slot address for a slot. PROVISIONAL (the original's is the residence
    /// object's pointer).
    fn value(self, ui: &CampaignUi) -> Value {
        match self {
            Residence::Settlement(r) => region_value(ui, r),
            Residence::Slot(r, i) => slot_value(ui, r, SlotRef::Slot(i)),
        }
    }
}

/// The agents the agents tab of `region`'s settlement lists, by character id. INFERRED (the
/// engine's list is not traced): every agent type ([`is_listed_agent_kind`]) standing in the region
/// ([`ntw_sim::campaign::economy::in_region`]) that the human's faction knows of (its own always;
/// a foreign one when [`ntw_sim::campaign::agents::knows_character`]: hidden spies stay off the tab).
pub(super) fn listed_agents(ui: &CampaignUi, region: RegionId) -> Vec<CharacterId> {
    let m = ui.model();
    let Some(reg) = m.world.regions.get(&region) else { return Vec::new() };
    let Some(human) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Vec::new() };
    m.world
        .characters
        .values()
        .filter(|c| is_listed_agent_kind(c.kind) && ntw_sim::campaign::economy::in_region(&m, reg, c))
        .filter(|c| ntw_sim::campaign::agents::knows_character(&m, human, c.id))
        .map(|c| c.id)
        .collect()
}

/// The agents tab's `GenerateAgentsPanel(info)`: {agents, characters, controlable}.
/// CONFIRMED from `ui/agents.luac`: `agents[i]` and `characters[i]` are parallel lists (the same
/// agent); the panel reads `agents[i].card_id` (the card map's key and the card component's id, so a
/// string) and `agents[i].name` (the card tooltip text, `SetTooltipText(name, true)`), and hands
/// `agents[i]` to the hover tooltip's `InitialiseAgent`, which reads `name` and `agent_type_name`
/// and nothing else; `characters[i]` goes to `Utilities.CreateCharacterCard` and to the buttons
/// (`Abilities.*`, `IsGuerilla`, `Address`); the panel sets `characters[i].PlayerControlled` itself;
/// `controlable` is read and dropped (`GenerateAgentCards` takes two arguments).
/// - `card_id`: PROVISIONAL format `agent_<character id>` (unique per agent);
/// - `name`: the character's name, else his type's (as `character_details`' `Name`);
/// - `agent_type_name`: his type's name in his faction's culture (`agent_culture_details` loc
///   key, [`agent_type_name`]; CONFIRMED 53 keys for 53 rows on the install);
/// - `characters[i]`: [`character_details`] plus `Abilities` ([`agent_has_ability`]);
/// - `controlable`: an empty table (unused, CONFIRMED).
pub(super) fn agents_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, region: RegionId) -> mlua::Result<Value> {
    let agents = lua.create_table()?;
    let characters = lua.create_table()?;
    for (i, c) in listed_agents(ui, region).into_iter().enumerate() {
        let Value::Table(details) = character_details(lua, inner, ui, c)? else { continue };
        let abilities = lua.create_table()?;
        {
            let m = ui.model();
            for key in AGENT_BUTTON_ABILITIES {
                abilities.set(key, agent_has_ability(&m, c, key))?;
            }
        }
        details.set("Abilities", abilities)?;
        let a = lua.create_table()?;
        a.set("card_id", format!("agent_{}", c.0))?;
        a.set("name", details.get::<Value>("Name")?)?;
        // The card's "unitcard" hover tooltip hands `agents[i]` to the tooltip template's
        // `InitialiseAgent`, which reads exactly two fields of it (CONFIRMED from
        // `ui\templates\template.unitcard_tooltip.luac`, the proto at line 115): `name` (the line
        // it shows) and `agent_type_name` (in brackets after it). Everything else that proto does
        // is fixed: the function component is set to the `strat_army` state, the crew and the four
        // `dy_stat` rows are hidden and the tooltip resizes expanded.
        a.set("agent_type_name", character_type_name(inner, ui, c))?;
        agents.set(i + 1, a)?;
        characters.set(i + 1, details)?;
    }
    let t = lua.create_table()?;
    t.set("agents", agents)?;
    t.set("characters", characters)?;
    t.set("controlable", lua.create_table()?)?;
    Ok(Value::Table(t))
}

/// The agent an agent-panel call names (every one passes the agent's `Address`, CONFIRMED arity 1).
fn agent_of(m: &CampaignModel, v: &Value) -> Option<(CharacterId, FactionId)> {
    let c = entity_of(v, TAG_CHARACTER).map(CharacterId)?;
    m.world.characters.get(&c).map(|ch| (c, ch.faction))
}

/// `ValidAssassinationTargets(agent)`: whether at least one character the agent may assassinate
/// exists. Validity is the model's (`assassination_chance` is `Some`, CONFIRMED gate; another
/// faction, as `approach_to` requires) and the target is known to the agent's faction
/// (`knows_character`). PROVISIONAL: the candidates are the whole map -- the original surely limits
/// them (reach, sight), but that path is not traced.
fn valid_assassination_targets(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    use ntw_sim::campaign::agents::{assassination_chance, knows_character};
    m.world
        .characters
        .values()
        .any(|t| t.faction != own && knows_character(m, own, t.id) && assassination_chance(m, agent, t.id).is_some())
}

/// `ValidSabotageArmyTarget(agent)`: whether a force exists the agent may sabotage: another
/// faction's, with a known commander, and `army_sabotage_chance` is `Some` (CONFIRMED gate).
/// PROVISIONAL: the whole map, as [`valid_assassination_targets`].
fn valid_sabotage_army_target(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    use ntw_sim::campaign::agents::{army_sabotage_chance, knows_character};
    m.world.forces.values().any(|f| {
        f.faction != own
            && f.commander.is_some_and(|c| knows_character(m, own, c))
            && army_sabotage_chance(m, agent, f.id).is_some()
    })
}

/// `ValidSabotageTarget(agent)`: whether a building the agent may sabotage stands in his residence
/// (INFERRED: the residence's buildings -- a slot's own building, or the settlement's slots), held by
/// another faction, with `building_sabotage_chance` `Some` (CONFIRMED gate). No residence: false.
fn valid_sabotage_target(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    let Some(res) = residence_of(m, agent) else { return false };
    let Some(r) = m.world.regions.get(&res.region()) else { return false };
    let slots: Vec<usize> = match res {
        Residence::Slot(_, i) => vec![i],
        Residence::Settlement(_) => (0..r.slots.len()).filter(|&i| r.slots[i].key.starts_with("settlement:")).collect(),
    };
    slots.into_iter().any(|i| {
        r.slots[i].holder.unwrap_or(r.owner) != own && ntw_sim::campaign::agents::building_sabotage_chance(m, agent, r.id, i).is_some()
    })
}

/// `ValidDuelTargetsInResidence(agent)`: whether another faction's character known to his own
/// stands in the same residence ([`residence_of`]) and the two may duel (`duel_chance` is `Some`,
/// CONFIRMED gate; the weapon does not change it). INFERRED: "in residence" is that residence.
fn valid_duel_targets_in_residence(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    use ntw_sim::campaign::agents::{duel_chance, knows_character, Weapon};
    let Some(res) = residence_of(m, agent) else { return false };
    m.world.characters.values().any(|t| {
        t.faction != own
            && residence_of(m, t.id) == Some(res)
            && knows_character(m, own, t.id)
            && duel_chance(m, agent, t.id, Weapon::Pistols).is_some()
    })
}

/// `CharacterInValidEnemyUniversity(agent)`: whether his residence is another faction's school (the
/// model's `school`: a full-health building with `research_points`, CONFIRMED) -- where the model's
/// `steal_step` lets a foreign gentleman steal. INFERRED meaning from the name.
fn in_valid_enemy_university(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    match residence_of(m, agent) {
        Some(Residence::Slot(r, i)) => m.school(r, i).is_some_and(|owner| owner != own),
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// The fog of war as the interface sees it (CHARACTER_UI_HOOKS.md H4). What the player may know
// about a character or a force is the model's own sight and knowledge model (`CampaignModel::sees`
// and `agents::knows_character`, both CONFIRMED, CHARACTERS_FIDELITY.md §10); the five data levels
// and the knowledge bit mask are the original's `Utilities.lua` globals (CONFIRMED values, read on
// the install 2026-10-05, `utilities.lua` lines 55..81).

/// `SPYING_DATA_LEVEL_INVALID` (`-1`): the address is not a character or a force.
pub(super) const LEVEL_INVALID: i32 = -1;
/// `SPYING_DATA_LEVEL_PASSIVE` (`0`): only a marker on the map is known.
pub(super) const LEVEL_PASSIVE: i32 = 0;
/// `SPYING_DATA_LEVEL_BASIC` (`1`): the piece is in the player's lists.
pub(super) const LEVEL_BASIC: i32 = 1;
/// `SPYING_DATA_LEVEL_ADVANCED` (`2`): the player may open its details -- the root's double-click
/// handler only opens the character / unit panel when the level reaches this
/// (`layout.root.lua:1093`, CONFIRMED).
pub(super) const LEVEL_ADVANCED: i32 = 2;
/// `SPYING_DATA_LEVEL_OWNED` (`3`): the player's own faction.
pub(super) const LEVEL_OWNED: i32 = 3;

/// `SPYING_UNIT_DATA_ICON_KNOWN | MEN | GUNS | XP` = 15 = `SPYING_UNIT_DATA_OWNED` (CONFIRMED).
pub(super) const KNOWLEDGE_OWNED: i32 = 15;
/// What is known about a foreign unit without opening it: the icon only (`SPYING_UNIT_DATA_ICON_KNOWN`,
/// CONFIRMED).
const KNOWLEDGE_ICON_ONLY: i32 = 1;

/// The knowledge bit mask for a data level (the `unit_record.knowledge_mask` the card reads,
/// CONFIRMED in `utilities.lua`'s `Initialise`: the owned mask for one's own units).
pub(super) fn knowledge_mask(level: i32) -> i32 {
    match level {
        LEVEL_OWNED => KNOWLEDGE_OWNED,
        LEVEL_ADVANCED => KNOWLEDGE_OWNED,
        LEVEL_BASIC | LEVEL_PASSIVE => KNOWLEDGE_ICON_ONLY,
        _ => 0,
    }
}

/// `CampaignUI.SpyingDataLevelCharacter(address)`: how much the human's faction knows about a
/// character. Arity 1 and the `>= ADVANCED` gate CONFIRMED (`layout.root.lua:1093`); the exe's
/// handler is not traced.
///
/// The mapping onto the model is INFERRED, each step from a CONFIRMED model rule:
/// - own faction → `OWNED`;
/// - a foreign character the faction knows ([`agents::knows_character`]: the hidden-flag test and the
///   exposed lists) and whose cell the shroud currently has visible ([`CampaignModel::sees`]) →
///   `ADVANCED` (the details may be opened);
/// - a known one under the shroud → `BASIC`;
/// - one the faction does not know → `PASSIVE`.
pub(super) fn spying_level_character(m: &CampaignModel, human: FactionId, c: CharacterId) -> i32 {
    let Some(ch) = m.world.characters.get(&c) else { return LEVEL_INVALID };
    if ch.faction == human {
        return LEVEL_OWNED;
    }
    if !ntw_sim::campaign::agents::knows_character(m, human, c) {
        return LEVEL_PASSIVE;
    }
    let at = (ch.position.0.to_f32(), ch.position.1.to_f32());
    if m.sees(human, at) { LEVEL_ADVANCED } else { LEVEL_BASIC }
}

/// `CampaignUI.SpyingDataLevelUnit(address)`: the same for a force's unit row, read from the
/// commander the unit belongs to. Arity 1 CONFIRMED (`layout.root.lua:1093`). INFERRED mapping (the
/// model's knowledge is per character, §10).
pub(super) fn spying_level_unit(m: &CampaignModel, human: FactionId, unit: UnitId) -> i32 {
    let Some(owner) = m.world.forces.values().find(|f| f.units.iter().any(|u| u.id == unit)) else {
        return LEVEL_INVALID;
    };
    // The player's own force is OWNED whether or not it has a commander (review 0-G: a commanderless
    // own force used to fall through to PASSIVE).
    if owner.faction == human {
        return LEVEL_OWNED;
    }
    match owner.commander {
        Some(c) => spying_level_character(m, human, c),
        None => LEVEL_PASSIVE,
    }
}

// ---------------------------------------------------------------------------------------------
// The agent action popups (`ui/campaign ui/agent_options.luac` and `agent_action.luac`, read on the
// install 2026-10-05, CONFIRMED): the agents panel's buttons are the engine's `CampaignUI.Agent*`
// calls, which open the options popup; a button there asks the engine for the target list and, when
// it is not empty, the action popup lists the targets; picking one calls `Instigate*` (or
// `SabotageArmy`, `MoveIntoTarget`). The targets are handed over as a table per action.

/// The targets `CampaignUI.RequestDuelTargets(agent, target)` offers: a duel partner in the agent's
/// own residence ([`valid_duel_targets_in_residence`], its gate CONFIRMED) -- the list the action
/// popup shows, each row a character.
fn duel_targets(m: &CampaignModel, agent: CharacterId, own: FactionId) -> Vec<CharacterId> {
    use ntw_sim::campaign::agents::{Weapon, duel_chance, knows_character};
    let Some(res) = residence_of(m, agent) else { return Vec::new() };
    let mut out: Vec<CharacterId> = m
        .world
        .characters
        .values()
        .filter(|t| t.id != agent && t.faction != own && residence_of(m, t.id) == Some(res) && knows_character(m, own, t.id))
        .filter(|t| duel_chance(m, agent, t.id, Weapon::Pistols).is_some())
        .map(|t| t.id)
        .collect();
    out.sort_by_key(|c| c.0);
    out
}

/// The targets `CampaignUI.RequestAssassinationTargets(agent, target)` offers (the model's
/// assassination gate, CONFIRMED; the candidate set PROVISIONAL as in
/// [`valid_assassination_targets`]).
fn assassination_targets(m: &CampaignModel, agent: CharacterId, own: FactionId) -> Vec<CharacterId> {
    use ntw_sim::campaign::agents::{assassination_chance, knows_character};
    let mut out: Vec<CharacterId> = m
        .world
        .characters
        .values()
        .filter(|t| t.id != agent && t.faction != own && knows_character(m, own, t.id))
        .filter(|t| assassination_chance(m, agent, t.id).is_some())
        .map(|t| t.id)
        .collect();
    out.sort_by_key(|c| c.0);
    out
}

/// The targets `CampaignUI.RequestSabotageTargets(agent, target)` offers: the buildings in the
/// agent's own residence the model's sabotage gate accepts (CONFIRMED gate; the residence INFERRED,
/// as in [`valid_sabotage_target`]). Each row is a (region, slot) pair -- the popup's
/// `sabotage_entry` template.
fn sabotage_targets(m: &CampaignModel, agent: CharacterId, own: FactionId) -> Vec<(RegionId, usize)> {
    let Some(res) = residence_of(m, agent) else { return Vec::new() };
    let Some(r) = m.world.regions.get(&res.region()) else { return Vec::new() };
    let slots: Vec<usize> = match res {
        Residence::Slot(_, i) => vec![i],
        Residence::Settlement(_) => (0..r.slots.len()).filter(|&i| r.slots[i].key.starts_with("settlement:")).collect(),
    };
    slots
        .into_iter()
        .filter(|&i| r.slots.get(i).is_some_and(|s| s.building.is_some()))
        .filter(|&i| r.slots[i].holder.unwrap_or(r.owner) != own)
        .filter(|&i| ntw_sim::campaign::agents::building_sabotage_chance(m, agent, r.id, i).is_some())
        .map(|i| (r.id, i))
        .collect()
}

/// One row of a `Request*Targets` list: the fields the shipped row templates read, taken off the
/// model's own gate so the picker shows the real number.
///
/// CONFIRMED field names, and the row is the **address field `Address`**, not `target`:
/// - `ui\templates\template.character_duel_info_pane.luac`, `InitCharacter(action, row)`
///   (proto at line 15, arity 2 CONFIRMED from `agent_action.lua:21` pc 79-83): `row.Address`,
///   `row.Name`, `row.Chance` and `row.Faction.{Name, Key}`, plus `row.Faction.FlagPath` which
///   `agent_action.lua:21` pc 37-40 concatenates into the template's picture argument.
/// - `ui\templates\template.sabotage_entry.luac`, `Initialise(row)`: the same `Address`, `Name`,
///   `Chance` and `Faction.FlagPath`, plus `row.IconFilename`, `row.ShortDescription`, `row.Level`
///   and `row.MaxLevel` for the icon, the tooltip and the six level pips.
///
/// Both templates' buttons hand `CampaignUI.Instigate*(m_attacker, row.Address)` back -- the
/// **address**, not the row (`character_duel_info_pane.lua:52/57` and `sabotage_entry.lua:32`, each
/// `UIComponent(this):Parent("agent_action"):LuaCall("InstigateDuel"/"InstigateAssassination"/"InstigateSabotage", m_target)`).
struct TargetRow {
    address: Value,
    faction: FactionId,
    chance: i32,
    /// A character row's on-screen name, or a sabotage row's building name.
    name: String,
    /// Sabotage rows only; empty for a character row.
    icon: String,
    short: String,
    level: i32,
    max_level: i32,
    /// The row's character, whose `Attributes` ([`attributes_table`]) the card shows: it reads
    /// `Attributes.PrimaryAttributePath` (`utilities.lua:107` pc 29-31) and its template the rest.
    /// `None` for a sabotage row.
    character: Option<CharacterId>,
}

/// The candidate rows of one `Request*Targets` call.
fn target_candidates(ui: &CampaignUi, inner: &Inner, action: &str, agent: CharacterId, own: FactionId) -> Vec<TargetRow> {
    let m = ui.model();
    match action {
        "duel" => duel_targets(&m, agent, own)
            .into_iter()
            .map(|c| {
                // The chance the model's duel is actually rolled at. `CampaignModel::duel` fights with
                // `duel_weapon` (`0x00AAAC30`, CONFIRMED rule): the target picks the weapon with the
                // *lower* chance for the challenger, a coin flip on a tie -- so the number is the
                // smaller of the two `duel_chance`s (`0x00922940`, CONFIRMED), whichever way the
                // flip goes. (Review 0-E: this used to show the pistols chance alone, which is not
                // the number the duel is rolled at whenever swords are worse for the challenger.)
                // INFERRED that the original's row shows the same number: the row carries one
                // `Chance` and the shipped pane does not say which weapon it is for.
                use ntw_sim::campaign::agents::{Weapon, duel_chance};
                let chance = duel_chance(&m, agent, c, Weapon::Pistols)
                    .into_iter()
                    .chain(duel_chance(&m, agent, c, Weapon::Swords))
                    .min()
                    .unwrap_or(0);
                TargetRow {
                    name: character_name(inner, ui, c).unwrap_or_else(|| character_type_name(inner, ui, c)),
                    address: character_value(ui, c),
                    faction: m.world.characters[&c].faction,
                    chance,
                    icon: String::new(),
                    short: String::new(),
                    level: 0,
                    max_level: 0,
                    character: Some(c),
                }
            })
            .collect(),
        "assassinate" => assassination_targets(&m, agent, own)
            .into_iter()
            .map(|c| {
                let chance = ntw_sim::campaign::agents::assassination_chance(&m, agent, c).unwrap_or(0);
                TargetRow {
                    name: character_name(inner, ui, c).unwrap_or_else(|| character_type_name(inner, ui, c)),
                    address: character_value(ui, c),
                    faction: m.world.characters[&c].faction,
                    chance,
                    icon: String::new(),
                    short: String::new(),
                    level: 0,
                    max_level: 0,
                    character: Some(c),
                }
            })
            .collect(),
        _ => {
            let owner = residence_of(&m, agent).and_then(|x| x.owner(&m)).unwrap_or(FactionId(-1));
            sabotage_targets(&m, agent, own)
                .into_iter()
                .map(|(region, i)| {
                    let chance = ntw_sim::campaign::agents::building_sabotage_chance(&m, agent, region, i).unwrap_or(0);
                    let level_key = m
                        .world
                        .regions
                        .get(&region)
                        .and_then(|r| r.slots.get(i))
                        .and_then(|s| s.building.as_ref())
                        .map(|b| b.level_key.clone())
                        .unwrap_or_default();
                    let owner_key = m.world.factions.get(&owner).map(|f| f.key.clone()).unwrap_or_default();
                    let name = loc(inner, &format!("buildings_name_{level_key}")).unwrap_or_else(|| level_key.clone());
                    let short = loc(inner, &format!("buildings_short_description_{level_key}")).unwrap_or_default();
                    TargetRow {
                        address: slot_value(ui, region, SlotRef::Slot(i)),
                        faction: owner,
                        chance,
                        name,
                        icon: ui.building_icon(&owner_key, &level_key),
                        short,
                        level: 0,
                        max_level: 0,
                        character: None,
                    }
                })
                .collect()
        }
    }
}

/// The model commands the agent actions queue. One entry per `CampaignUI` call; a call whose
/// addresses do not name what it needs returns `None` and nothing is queued.
fn agent_action_command(ui: &CampaignUi, name: &str, agent: &Value, target: &Value) -> Option<CampaignCommand> {
    let a = entity_of(agent, TAG_CHARACTER).map(CharacterId);
    match name {
        "InstigateDuel" => Some(CampaignCommand::Duel { challenger: a?, target: entity_of(target, TAG_CHARACTER).map(CharacterId)? }),
        "InstigateAssassination" => Some(CampaignCommand::Assassinate { agent: a?, target: entity_of(target, TAG_CHARACTER).map(CharacterId)? }),
        "InstigateSabotage" => {
            let (region, slot) = slot_from_entity(ui, target)?;
            // Only a building of the region's slot list (the walls and road are not sabotage targets).
            let slot = match slot {
                SlotRef::Slot(i) if ui.model().world.regions.get(&region).is_some_and(|r| i < r.slots.len()) => i,
                _ => return None,
            };
            Some(CampaignCommand::SabotageBuilding { agent: a?, region, slot })
        }
        "SabotageArmy" => Some(CampaignCommand::SabotageArmy { agent: a?, force: entity_of(target, TAG_FORCE).map(|f| ForceId(f as u32))? }),
        _ => None,
    }
}


// ---------------------------------------------------------------------------------------------
// The agent action mask. `ui\campaign ui\agent_options.luac`'s `Initialise(src, target, mask, pct,
// pct)` (arity 5 CONFIRMED, proto at line 34) shows one button per action whose bit is in `mask`
// (`bit.band(mask, action.Mask) ~= 0`, pc 45-56) and **divorces** the button from the panel when it
// is not, so a bit that is not set is a button that does not appear at all.
//
// **Which parameter is which is CONFIRMED from the same prototype (0-E round 5):** pc 0-1 store
// `R[0]`/`R[1]` as the upvalues `m_src`/`m_target`, pc 47 reads `R[2]` as the mask, pc 82 reads `R[3]`
// into the Infiltrate button's `percent_dy` and pc 116 reads `R[4]` into the Sabotage Army button's.
// So the engine call is exactly
// `OpenAgentOptionsPopup(src, target, mask, infiltrate_pct, sabotage_army_pct)`, and the two
// percentages are the last two arguments -- not, as round 3 recorded, "one of Initialise's last two
// arguments".

/// What the player acted on when the agent action menu opened: `agent_options.Initialise`'s
/// `m_target`.
///
/// **CONFIRMED (0-E round 5) from the exe's own registration document, not from a decompile.**
/// `CampaignUI.MoveIntoTarget` is registered at `0x004295D0` with the five-instruction shape the
/// whole binding family uses (`push <doc>; push <name>; push <fn>; mov ecx, <id>; call 0x00998C50`
/// -- `RegionsPublicOrders` does the same at `0x00429E90`, which is the control that makes the
/// reading a reading rather than a coincidence): the doc is at `0x01364B50`, the name `MoveIntoTarget`
/// at `0x01364BB8`, the function at `0x009EE150`. The doc reads, verbatim:
///
/// ```text
/// In: Character (agent), character or settlement (target), bool (research - steal if enemy settlement)
/// ```
///
/// So **`m_target` is a character or a settlement**, never a bare military force, and the third
/// argument is the flag that separates the two research actions -- which is why exactly `research`
/// (16) and `steal_research` (32) pass it and `visit`/`embed`/`counterspy` do not
/// (`agent_options.lua:149`/`:154` against `:139`/`:144`/`:164`, all CONFIRMED).
///
/// This also explains `Initialise`'s first instruction. `string.find(tostring(target), "CHARACTER")`
/// (`agent_options.lua:2-9`) is how one popup serves both kinds: only the character branch builds the
/// `CampaignCharacter` handle the teardown releases at `:89`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMenuTarget {
    Character(CharacterId),
    Settlement(RegionId),
}

/// The military force an [`AgentMenuTarget`] names, for the Sabotage Army percentage.
///
/// **INFERRED**, and named as such because the original's choice is the engine's: the documented
/// target kinds are a character and a settlement, and `agent_options.lua:159` calls
/// `CampaignUI.SabotageArmy(agent, force)` with a force the engine picked without asking. A force has
/// no position of its own in the model -- it moves with its commander (`MilitaryForce`'s own doc) --
/// so a target character is read as "the force he commands" and a target settlement as "the foreign
/// force whose commander stands in it" (`residence_of`, the same residence reading the agents tab
/// uses). The named alternative is *any* force the agent may sabotage anywhere, which is what
/// [`valid_sabotage_army_target`] tests; that reading is not refuted by anything here, it is just not
/// the one the target-based menu implies.
fn agent_menu_force(m: &CampaignModel, agent: CharacterId, target: AgentMenuTarget, own: FactionId) -> Option<ForceId> {
    use ntw_sim::campaign::agents::{army_sabotage_chance, knows_character};
    m.world
        .forces
        .values()
        .filter(|f| {
            f.faction != own && f.commander.is_some_and(|c| knows_character(m, own, c)) && army_sabotage_chance(m, agent, f.id).is_some()
        })
        .find(|f| {
            let c = f.commander.expect("filtered above");
            match target {
                AgentMenuTarget::Character(t) => t == c,
                AgentMenuTarget::Settlement(r) => residence_of(m, c) == Some(Residence::Settlement(r)),
            }
        })
        .map(|f| f.id)
}

/// The two numbers `agent_options.Initialise` puts on the **Infiltrate** and **Sabotage Army**
/// buttons, in that order.
///
/// **CONFIRMED which is which** -- the prototype reads `R[3]` into the button whose `Mask` is
/// `AGENT_ACTION_RAKE_EMBED` (`agent_options.lua:71-87`) and `R[4]` into the one whose `Mask` is
/// `AGENT_ACTION_RAKE_SABOTAGE_ARMY` (`:88-120`), each as `tostring(percent) .. "%"`. So they are
/// `Initialise`'s fourth and fifth parameters.
///
/// **CONFIRMED what they mean** -- a chance of success, in percent, from the game's own advisor
/// (loc, 0-E round 5): *"To infiltrate a city, select your spy and then right-click on the city in
/// question. A menu will appear giving you the options of sabotage, assassination or infiltration.
/// The percentages show the spy's chances of success in each activity."* and *"As with all subterfuge
/// actions there is a percentage chance of success, and failure may mean capture and execution."*
/// (These are the strings behind `advisor\1207..1210_campaign_advice_ui_agent_options_panel_*`,
/// four mp3s that exist for nothing else.)
///
/// **Which model chance each is: INFERRED / PROVISIONAL (review 0-E downgraded this from
/// "real").** The formulas used are the model's existing CONFIRMED ones; *that these are the
/// numbers the original shows* is not. The free model's own exe reading (recorded from
/// `llvm-objdump`, no kept decompile) says the two engine call sites (`0x009C1E20` / `0x009C1EE0`,
/// reached from `0x00A0C3CD` / `0x00A0C442`) ask one interface for the **agent alone**, with ids
/// **5** and **11** (`0x009D1FC0` / `0x009D2000`) -- which are the indices of `can_spy` and
/// `can_sabotage_army` in [`ntw_sim::campaign::agents::ABILITIES`]. If that reading holds, the
/// original's numbers **do not depend on the target**, while both formulas below do (a settlement's
/// protector, a force's size and commander). So either the interface turns an ability id into a
/// target-free number (an ability level, or some other per-agent value) and these are the wrong
/// numbers, or the reading is incomplete. Named lead: decompile `0x009D1FC0` and keep it.
/// - **Infiltrate** = [`spy_chance`] on the target settlement (`0x00922A70`, CONFIRMED formula),
///   chosen because the advisor says *"Select the infiltrate option. If the spy has enough movement
///   points he will automatically try to enter the city."* and infiltrating is spying on arrival.
/// - **Sabotage Army** = [`army_sabotage_chance`] (`0x00922BA0`, CONFIRMED formula) on the force the
///   target names ([`agent_menu_force`], INFERRED -- see there).
///
/// **0 when the target is nothing we can compute from**, which is honest rather than blank: with no
/// target the original had one of these queries answer something and we do not know what.
pub(super) fn agent_options_percentages(
    m: &CampaignModel,
    agent: CharacterId,
    own: FactionId,
    target: Option<AgentMenuTarget>,
) -> (i32, i32) {
    use ntw_sim::campaign::agents::{SpyTarget, army_sabotage_chance, spy_chance};
    let infiltrate = match target {
        Some(AgentMenuTarget::Settlement(r)) => spy_chance(m, agent, SpyTarget::Settlement(r)).unwrap_or(0),
        _ => 0,
    };
    let sabotage_army = target
        .and_then(|t| agent_menu_force(m, agent, t, own))
        .and_then(|x| army_sabotage_chance(m, agent, x))
        .unwrap_or(0);
    (infiltrate, sabotage_army)
}

/// The nine action bits, CONFIRMED from the module's main chunk (`agent_options.lua:0`, pc 13-85):
/// it computes `bit.lshift(1, n)` for `n` = 0..8 and pairs each with its action name in the order
/// `assassinate`(1), `sabotage`(2), `embed`(3), `research`(4), `steal_research`(5), `duel`(6),
/// `visit`(0), `sabotage_army`(7), `counterspy`(8). The array it stores holds **eight** entries
/// (`SETLIST n=8`, pc 86), so `counterspy`'s bit is computed and dropped -- which is why the popup
/// can never show its Counterspy button even though the layout has one.
pub(super) mod action_bit {
    // Five of the nine are named but never set: see `agent_options_mask` below, which is where the
    // reason is written down. They are here so the table is the shipped one, complete.
    #![allow(dead_code)]
    pub const VISIT: i32 = 1 << 0;
    pub const ASSASSINATE: i32 = 1 << 1;
    pub const SABOTAGE: i32 = 1 << 2;
    pub const EMBED: i32 = 1 << 3;
    pub const RESEARCH: i32 = 1 << 4;
    pub const STEAL_RESEARCH: i32 = 1 << 5;
    pub const DUEL: i32 = 1 << 6;
    pub const SABOTAGE_ARMY: i32 = 1 << 7;
    /// Built and dropped by the shipped module, so the popup never shows this one.
    pub const COUNTERSPY: i32 = 1 << 8;
}

/// The mask `agent_options.Initialise` is given for `src`.
///
/// **The mask is the engine's**, and only four of its nine bits can be derived from rules we
/// already hold. Each of those four is exactly what the popup's own button does when clicked
/// (`agent_options.lua:109/118/128`, all CONFIRMED): `Duel`, `Assassinate` and `Sabotage` call
/// `CampaignUI.Request*Targets(src, target)`, close themselves, and open the target popup only
/// `if 0 < #targets`. So the bit is set iff that list is not empty -- the same predicate our
/// `Request*Targets` bindings use, which keeps the popup and the list in step (a bit whose list is
/// empty would show a button that then opens nothing).
///
/// **The other five bits are not set, and the reason changed in 0-E round 5.** It used to be "the
/// contract of `MoveIntoTarget` is UNKNOWN"; it is not any more. `MoveIntoTarget`'s parameter kinds
/// and its third flag are CONFIRMED from the exe's own registration document (see
/// [`AgentMenuTarget`]): `(agent, character-or-settlement target[, steal-research])`. What is still
/// missing is the *behaviour* -- what the agent does on arrival, which is the model's agent-order
/// machinery and not a scripting fact. Queueing a bare `CampaignCommand::MoveCharacter` would put an
/// agent in a foreign city with nothing to do there, which is worse than no button. So:
/// - `visit`(1), `embed`(8), `research`(16), `steal_research`(32) and `counterspy`(256) all end in
///   `CampaignUI.MoveIntoTarget(src, target[, true])` (`agent_options.lua:139/144/149/154/164`,
///   CONFIRMED, no other caller), and `MoveIntoTarget` stays a logging stub so the route can never
///   raise. **Next round's target, now sharp:** `embed`'s on-arrival action is the model's
///   `CampaignModel::spy(agent, SpyTarget::Settlement(...))` (`0x0094CCB0`, CONFIRMED) on the advisor's
///   own wording, so wiring `embed` needs the *move-then-resolve* step and nothing else.
/// - `embed` and `sabotage_army` are the two actions `Initialise` puts a percentage on
///   (`percent_dy`, pc 71-87 and 88-120); those two numbers are the model's own chances (which chance each one is: INFERRED), see
///   [`agent_options_percentages`].
///
/// `target` is still accepted and ignored here, deliberately: our `Request*Targets` bindings ignore
/// their second argument too (see [`target_candidates`]), so narrowing here would offer an action the
/// list then refuses. INFERRED that the original narrows the mask by the picked target -- which the
/// two engine call sites are consistent with, as each of them is reached only after a target test.
pub(super) fn agent_options_mask(m: &CampaignModel, src: CharacterId, own: FactionId) -> i32 {
    let mut mask = 0;
    if !assassination_targets(m, src, own).is_empty() {
        mask |= action_bit::ASSASSINATE;
    }
    if !sabotage_targets(m, src, own).is_empty() {
        mask |= action_bit::SABOTAGE;
    }
    if !duel_targets(m, src, own).is_empty() {
        mask |= action_bit::DUEL;
    }
    if valid_sabotage_army_target(m, src, own) {
        mask |= action_bit::SABOTAGE_ARMY;
    }
    mask
}

/// The [`AgentMenuTarget`] a script argument names, if any: a character address is a character, a
/// region (settlement) address is a settlement, anything else (or nothing) is no target. The two are
/// what the exe's own `MoveIntoTarget` document allows, so nothing else is invented here.
pub(super) fn menu_target_of(m: &CampaignModel, v: &Value) -> Option<AgentMenuTarget> {
    if let Some(c) = entity_of(v, TAG_CHARACTER)
        && m.world.characters.contains_key(&CharacterId(c))
    {
        return Some(AgentMenuTarget::Character(CharacterId(c)));
    }
    entity_of(v, TAG_REGION).map(|r| AgentMenuTarget::Settlement(RegionId(r as u32)))
}

/// Calls a global of the HUD root layout's script -- the engine side of the `root:LuaCall(name, ...)`
/// the shipped scripts use to reach it (`agent_options.lua:109`, `agent_action.lua:49`).
/// `layout.root.lua:1187` is `OpenAgentActionPopup(action, src, targets)` (arity 3 CONFIRMED) and
/// `layout.root.lua:1191` is `OpenAgentOptionsPopup(src, target, mask, pct, pct)` (arity 5); both
/// forward to `panel_manager.OpenPanel(<panel>, nil, "Initialise", ...)`.
pub(super) fn call_root_global(lua: &Lua, inner: &Inner, name: &str, args: impl mlua::IntoLuaMulti) -> bool {
    let Some(root) = inner.root_node() else { return false };
    let r = (|| -> mlua::Result<bool> {
        let mut a = args.into_lua_multi(lua)?;
        a.push_front(Value::String(lua.create_string(name)?));
        a.push_front(addr(root));
        call_entry::<bool>(lua, inner, root, "__ntw_call_if_defined", a)
    })();
    match r {
        Ok(b) => b,
        Err(e) => {
            log(inner, format!("ERROR in {name}: {e}"));
            false
        }
    }
}

/// The engine side of the agents panel's action buttons: open the target picker for the button's
/// action. `ui\agents.luac` calls `CampaignUI.AgentRakeAssassinate` / `AgentRakeSubterfuge` /
/// `AgentGentlemanDuel` with the selected agent's `Address` and nothing else (arity 1 CONFIRMED);
/// what the original's exe did next is the three lines `agent_options.Assassinate` / `.Sabotage` /
/// `.Duel` run (CONFIRMED, `agent_options.lua:118/128/109`): ask `Request*Targets(src, target)`,
/// and if the list is not empty hand it to the root's `OpenAgentActionPopup(action, src, rows)`.
/// INFERRED that the panel's own button skips the `agent_options` menu in between: that popup's
/// first act is `string.find(tostring(target), "CHARACTER")` (pc 2-9), so it is for a target the
/// player picked on the map, not for a button on the settlement's agents tab.
///
/// An action with no valid target opens nothing and says so in the log -- the original is silent
/// there (`#targets > 0` is its only gate), but a no-op that logs is the difference between a
/// known gap and a broken button.
fn open_agent_action_popup(lua: &Lua, inner: &Inner, ui: &CampaignUi, agent: &Value, action: &str) {
    let m = ui.model();
    let Some((c, own)) = agent_of(&m, agent) else {
        log(inner, format!("agent action {action}: the argument is not a character address"));
        return;
    };
    let rows = target_candidates(ui, inner, action, c, own);
    if rows.is_empty() {
        log(inner, format!("agent action {action}: no valid target"));
        return;
    }
    match target_rows(lua, inner, ui, &rows) {
        Ok(t) => {
            call_root_global(lua, inner, "OpenAgentActionPopup", (action, agent.clone(), t));
        }
        Err(e) => log(inner, format!("agent action {action}: {e}")),
    }
}

/// The list the target popups get: one table per [`TargetRow`], with the field names the shipped row
/// templates read (see [`TargetRow`] for the CONFIRMED list).
fn target_rows(lua: &Lua, inner: &Inner, ui: &CampaignUi, rows: &[TargetRow]) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for (i, row) in rows.iter().enumerate() {
        let e = lua.create_table()?;
        e.set("Address", row.address.clone())?;
        e.set("Name", row.name.clone())?;
        e.set("Chance", row.chance)?;
        e.set("IconFilename", row.icon.clone())?;
        e.set("ShortDescription", row.short.clone())?;
        e.set("Level", row.level)?;
        e.set("MaxLevel", row.max_level)?;
        let fac = lua.create_table()?;
        let key = ui.model().world.factions.get(&row.faction).map(|f| f.key.clone()).unwrap_or_default();
        let flag = ui.link.db.faction(&key).map(|r| r.flag_path.clone()).unwrap_or_default();
        let name = faction_name(inner, &ui.link.db, &key);
        fac.set("Key", key)?;
        fac.set("Name", name)?;
        fac.set("FlagPath", flag.clone())?;
        e.set("Faction", fac)?;
        // `Utilities.CreateCharacterCard` concatenates `"{Flags:1}" .. info.Flag` with no guard
        // (`utilities.lua:111` pc 4-6), so this one is mandatory: it is the flag picture the
        // character card draws, the same `<folder>/small.tga` form the diplomacy lists use
        // (`FactionDetails`, line 1632).
        e.set("Flag", format!("{flag}/small.tga"))?;
        // `CampaignCharacterCard.Initialise(info)` indexes `info.Attributes` without a guard (reached
        // through `Utilities.CreateCharacterCard`, whose `LuaCall("Initialise", info, ...)` is the
        // call `utilities.lua:122` reports), and `CreateCharacterCard` itself reads
        // `info.Attributes.PrimaryAttributePath` (pc 29-31). Same shape the agents tab builds.
        e.set("Attributes", attributes_table(lua, inner, ui, row.character, Pips::With)?)?;
        t.set(i + 1, e)?;
    }
    Ok(t)
}


pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // IsCharacterPlayerControlled(character) → true if the character belongs to the human player's faction.
    // Called by agent panel scripts to enable/disable action buttons (CONFIRMED call in Agents.lua).
    f!("IsCharacterPlayerControlled", |_l, inner, ui, entity: Value| {
        let Some(c) = entity_of(&entity, TAG_CHARACTER).map(CharacterId) else { return Ok(false) };
        let m = ui.model();
        let Some(ch) = m.world.characters.get(&c) else { return Ok(false) };
        let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
        Ok(human.is_some_and(|h| ch.faction == h))
    });
    // CharactersRelationshipToPlayersFaction(address) → 0..3 (see `relationship_to_players_faction`).
    // An address that is not a character answers nil, as before the binding (UNKNOWN in the exe).
    f!("CharactersRelationshipToPlayersFaction", |_l, _inner, ui, entity: Value| {
        let Some(c) = entity_of(&entity, TAG_CHARACTER).map(CharacterId) else { return Ok(None) };
        let m = ui.model();
        let Some(ch) = m.world.characters.get(&c) else { return Ok(None) };
        let Some(human) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(None) };
        Ok(Some(relationship_to_players_faction(ch.faction == human, m.world.stance(human, ch.faction))))
    });
    // The agent panel's action calls (CHARACTER_UI_HOOKS.md "Agents panel"). CONFIRMED from
    // `ui/agents.luac`: each takes only the selected agent's Address (arity 1): the duel, assassinate,
    // subterfuge and sabotage-army buttons; `AgentEmbarkOrDisembark` only with a current card during
    // the player's turn; `AgentCardSelectionChanged` gets the card's `ItemAddress`.
    // `ShowAgents`, `ShowAgentButtons`, `AgentCardPosition`, `SelectAgentCard`,
    // `ShowStealingTechnologies` and `AgentsPanelActive` are script globals, not `CampaignUI.*`
    // calls, so they are not bound.
    //
    // The three that have a target list now open it (see [`open_agent_action_popup`]): the click
    // reaches the picker, the picker reaches `Instigate*`, and that queues the model's command.
    // `AgentCardSelectionChanged` is a card-group callback, not an action, and
    // `AgentEmbarkOrDisembark` needs an agent embark the model does not have -- both stay no-ops.
    for (name, action) in [
        ("AgentGentlemanDuel", "duel"),
        ("AgentRakeAssassinate", "assassinate"),
        ("AgentRakeSubterfuge", "sabotage"),
    ] {
        let ui2 = ui.clone();
        let inner2 = inner.clone();
        t.raw_set(
            name,
            lua.create_function(move |lua, agent: Value| {
                open_agent_action_popup(lua, &inner2, &ui2, &agent, action);
                Ok(())
            })?,
        )?;
    }
    for name in ["AgentCardSelectionChanged", "AgentEmbarkOrDisembark"] {
        t.raw_set(name, lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
    }
    // AgentRogueSabotageArmy(agent): the harass / sabotage-army button. STILL A NO-OP, and now for a
    // named reason: its only target is an **army**, and unlike the three above there is no
    // `Request*Targets` list for one -- `SabotageArmy(agent, force)` is called straight from the
    // popup button (`agent_options.lua:159`), so the original's exe picked the force itself. With no
    // force list to show, opening a picker we cannot fill would be a worse answer than a logged gap.
    t.raw_set("AgentRogueSabotageArmy", {
        let inner2 = inner.clone();
        lua.create_function(move |_, _: Variadic<Value>| {
            log(&inner2, "agent action sabotage_army: no army target list (named open item)".to_owned());
            Ok(())
        })?
    })?;
    // CampaignUI.MoveIntoTarget(src, target [, research]) -- what five of the nine `agent_options`
    // actions end in (`agent_options.lua:139/144/149/154/164`, CONFIRMED, and no other caller).
    // **The contract is known** (0-E round 5): the exe's registration document reads
    // "In: Character (agent), character or settlement (target), bool (research - steal if enemy
    // settlement)" -- see [`AgentMenuTarget`]. What is still missing is the behaviour on arrival,
    // which is the model's agent-order machinery, so this stays a deliberate no-op that LOGS rather
    // than silently succeeding, and the five bits that reach it stay out of [`agent_options_mask`].
    t.raw_set("MoveIntoTarget", {
        let inner2 = inner.clone();
        lua.create_function(move |_, _: Variadic<Value>| {
            log(&inner2, "MoveIntoTarget is not implemented (named open item)".to_owned());
            Ok(())
        })?
    })?;
    // CanAgentEmbarkOrDisembark(agent) -> false. Arity 1 CONFIRMED; asked only in a port residence.
    // PROVISIONAL answer: the model has no agent embark (only a force's `Embark` / `Disembark`).
    t.raw_set("CanAgentEmbarkOrDisembark", lua.create_function(|_, _: Variadic<Value>| Ok(false))?)?;
    // CampaignUI.__CanHarrass(agent): the answer `agent_options.Initialise` gets from
    // `CampaignCharacter(m_src):CanHarrass()` (pc 92-96, arity 0 CONFIRMED) to choose the Sabotage
    // Army button's state. The same two questions the agents panel asks for that button
    // (`agents.lua:77-108`): the ability, then a valid target. INFERRED: that the engine's
    // `CanHarrass` asks exactly those two is our reading -- the method is the exe's and is not traced.
    f!("__CanHarrass", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| agent_has_ability(&m, c, "can_sabotage_army") && valid_sabotage_army_target(&m, c, own)))
    });
    // The engine's entry to the agent action menu: `root:LuaCall("OpenAgentOptionsPopup", src,
    // target, mask, pct, pct)` -- `layout.root.lua:1191` (arity 5 CONFIRMED) forwards it to
    // `panel_manager.OpenPanel("agent_options", nil, "Initialise", ...)`, which is what the original's
    // exe called when the player acts on a target with an agent. Ours is the same call with the mask
    // [`agent_options_mask`] computes and the two percentages [`agent_options_percentages`]
    // computes. On `CampaignUI` with a `__` prefix because the engine half is a host call:
    // `UiScriptHost::agent_options_popup` goes through this binding, so the host and the scripts
    // cannot drift apart.
    f!("__OpenAgentOptionsPopup", |l, inner, ui, (src, target): (Value, Value)| {
        let m = ui.model();
        let Some((c, own)) = agent_of(&m, &src) else { return Ok(false) };
        let mask = agent_options_mask(&m, c, own);
        let (infiltrate, sabotage_army) = agent_options_percentages(&m, c, own, menu_target_of(&m, &target));
        Ok(call_root_global(l, &inner, "OpenAgentOptionsPopup", (src, target, mask, infiltrate, sabotage_army)))
    });
    // The agent action popups (`agent_options.luac` / `agent_action.luac`, CONFIRMED on the install
    // 2026-10-05). The agents panel's three buttons open the target picker above; these are the calls
    // that picker makes, and they are wired to the model's commands:
    //   RequestDuelTargets(agent, target) / RequestAssassinationTargets / RequestSabotageTargets
    //     -> a list of targets (each with `target` and `Faction.FlagPath`, CONFIRMED fields);
    //        empty when there is none, and the popup is then not opened (`#targets > 0`);
    //   InstigateDuel(agent, target) / InstigateAssassination / InstigateSabotage -> the model's
    //     `Duel` / `Assassinate` / `SabotageBuilding` (the target of a duel or an assassination is
    //     the row's `target`, a character's address; a sabotage target is a slot's);
    //   SabotageArmy(agent, force) -> the model's `SabotageArmy`.
    // The second argument of the Request* calls is the interaction's other party, which
    // `agent_options.lua:34` shows to be a character address; INFERRED: the currently selected
    // entity, and it is not needed for the candidate lists (the model's own gates decide).
    for (name, action) in [("RequestDuelTargets", "duel"), ("RequestAssassinationTargets", "assassinate"), ("RequestSabotageTargets", "sabotage")] {
        let ui2 = ui.clone();
        let inner2 = inner.clone();
        t.raw_set(
            name,
            lua.create_function(move |lua, (agent, _target): (Value, Value)| {
                let m = ui2.model();
                let rows = agent_of(&m, &agent).map(|(c, own)| target_candidates(&ui2, &inner2, action, c, own)).unwrap_or_default();
                target_rows(lua, &inner2, &ui2, &rows)
            })?,
        )?;
    }
    // InstigateDuel / InstigateAssassination / InstigateSabotage(agent, target) and
    // SabotageArmy(agent, force): the player's own action, queued as the model's command. A duel
    // target is a character's address (the popup's `character_duel_info_pane` row), a sabotage
    // target a slot's (`sabotage_entry` row), CONFIRMED (`agent_action.lua:21`). The weapon of a
    // duel is the model's AI rule (PROVISIONAL, CHARACTER_UI_HOOKS.md H3).
    for name in ["InstigateDuel", "InstigateAssassination", "InstigateSabotage", "SabotageArmy"] {
        let ui2 = ui.clone();
        t.raw_set(
            name,
            lua.create_function(move |_, (agent, target): (Value, Value)| {
                if let Some(c) = agent_action_command(&ui2, name, &agent, &target) {
                    ui2.push(CampaignRequest::Command(c));
                }
                Ok(())
            })?,
        )?;
    }
    // SpyingDataLevelCharacter(address) / SpyingDataLevelUnit(address): what the player may know about
    // a character or a unit (the root's double-click handler opens the details only from
    // `SPYING_DATA_LEVEL_ADVANCED` up, `layout.root.lua:1093`; arity 1 CONFIRMED). The levels are the
    // model's sight and knowledge (see `spying_level_character`).
    f!("SpyingDataLevelCharacter", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        let Some(human) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(LEVEL_INVALID) };
        Ok(entity_of(&v, TAG_CHARACTER).map_or(LEVEL_INVALID, |c| spying_level_character(&m, human, CharacterId(c))))
    });
    f!("SpyingDataLevelUnit", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        let Some(human) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(LEVEL_INVALID) };
        Ok(entity_of(&v, TAG_UNIT).map_or(LEVEL_INVALID, |u| spying_level_unit(&m, human, UnitId(u))))
    });
    // The questions `ShowAgentButtons` asks, each with the agent's Address (CONFIRMED names, arity and
    // use as the button's active flag). An address that is not a character answers false / nil.
    // CharacterResidence(agent) -> the residence address or nil (INFERRED from the model, see `Residence`).
    f!("CharacterResidence", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).and_then(|(c, _)| residence_of(&m, c)).map_or(Value::Nil, |r| r.value(&ui)))
    });
    // IsCharacterInPortResidence(agent) -> his residence is a port slot (INFERRED: `RegionSlot::port`).
    f!("IsCharacterInPortResidence", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(match agent_of(&m, &v).and_then(|(c, _)| residence_of(&m, c)) {
            Some(Residence::Slot(r, i)) => m.world.regions.get(&r).and_then(|r| r.slots.get(i)).is_some_and(|s| s.port),
            _ => false,
        })
    });
    // CharacterInEnemyResidence(agent) -> his residence is held by another faction (INFERRED: "enemy"
    // as the model's agent actions take it, any other faction; no stance test).
    f!("CharacterInEnemyResidence", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        let Some((c, own)) = agent_of(&m, &v) else { return Ok(false) };
        Ok(residence_of(&m, c).and_then(|r| r.owner(&m)).is_some_and(|o| o != own))
    });
    // ValidAssassinationTargets(agent): model gate CONFIRMED, candidate set PROVISIONAL (whole map).
    f!("ValidAssassinationTargets", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| valid_assassination_targets(&m, c, own)))
    });
    // ValidSabotageTarget(agent): a building in his residence (INFERRED), model gate CONFIRMED.
    f!("ValidSabotageTarget", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| valid_sabotage_target(&m, c, own)))
    });
    // ValidSabotageArmyTarget(agent): model gate CONFIRMED, candidate set PROVISIONAL (whole map).
    f!("ValidSabotageArmyTarget", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| valid_sabotage_army_target(&m, c, own)))
    });
    // CharacterInValidEnemyUniversity(agent): another faction's school (INFERRED meaning; the model's
    // school test CONFIRMED).
    f!("CharacterInValidEnemyUniversity", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| in_valid_enemy_university(&m, c, own)))
    });
    // ValidDuelTargetsInResidence(agent): a duel partner in the same residence (INFERRED), model
    // gate CONFIRMED.
    f!("ValidDuelTargetsInResidence", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| valid_duel_targets_in_residence(&m, c, own)))
    });
    Ok(())
}
