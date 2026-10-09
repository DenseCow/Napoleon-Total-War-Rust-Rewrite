//! The army and navy panels: unit cards, the recruitment tab and the commander pool
//! (`ui/army.luac`, `enlist_commander.luac`).

use super::*;

/// One unit card entry, the fields `template.CampaignUnitCard.lua` reads (CONFIRMED names from
/// its bytecode): Id, Address, Key, Name, Description, Icon, Men, Max, MenAsPercent, Experience,
/// IsNaval, Guns, CommanderType, CharacterPtr, CommandersName, DisplayAsUnit, InTransit,
/// UnitRecord, ... Values from the model and the `units` table; PROVISIONAL where noted.
///
/// `display_as_unit` is the card info's byte `+0x130` the card builder `0x009ABE00` hands out as
/// `DisplayAsUnit` (own Ghidra copy, 2026-10-07; CONFIRMED writers): the army panel's
/// `BuildArmyPanelInfoTable` `0x009FCFB0` clears it (its cards are false), a character's
/// `CommandedUnit` in `BuildCharacterDetailsInfoTable` `0x009AD250` sets it (true, at
/// `0x009ADF80`).
pub(super) fn unit_entry(lua: &Lua, inner: &Inner, ui: &CampaignUi, force: &MilitaryForce, index: usize, mut display_as_unit: bool) -> mlua::Result<Table> {
    let u = &force.units[index];
    let t = lua.create_table()?;
    // Id: a unique component id for the card (INFERRED: ExistingUnitCard matches cards by Id).
    t.set("Id", format!("unit_card_{}", u.id.0))?;
    t.set("Address", unit_value(ui, u.id))?;
    t.set("Key", u.unit_key.as_str())?;
    // UnitRecord: the unit's details table, not its key. The card keeps it and reads its `UnitLimit`
    // and `Key` on mouse-on, then hands it to the unit tooltip's `Initialise` as the unit record
    // (template.CampaignUnitCard.lua:122 / 406-413, CONFIRMED from the bytecode) -- the table a
    // recruitment card's `unit_record` is ([`unit_details`], built once per unit key; INFERRED that
    // both are the same).
    t.set("UnitRecord", unit_details(lua, inner, ui, &u.unit_key)?)?;
    let rec = ui.link.db.unit(&u.unit_key);
    let name = loc(inner, &format!("units_on_screen_name_{}", u.unit_key)).unwrap_or_else(|| rec.map(|r| r.dev_name.clone()).unwrap_or_default());
    t.set("Name", name)?;
    t.set("Description", loc(inner, &format!("unit_description_texts_description_text_{}", u.unit_key)).unwrap_or_default())?;
    // Unit card picture: `ui/units/icons/<faction>_<unit>_icon.tga` (CONFIRMED file names, e.g.
    // `france_inf_line_french_fusiliers_icon.tga`; INFERRED: the owning faction's key, else the
    // bare info key as some generic cards are named). The card script appends ".tga" (CONFIRMED).
    let faction = ui.model().world.factions.get(&force.faction).map(|f| f.key.clone()).unwrap_or_default();
    let info = rec.map(|r| r.info_key.clone()).unwrap_or_else(|| u.unit_key.clone());
    let icon = [format!("{faction}_{}_icon", u.unit_key), format!("{faction}_{info}_icon"), info.clone()]
        .into_iter()
        .find(|p| inner.source.find(&format!("ui/units/icons/{p}.tga")).is_some())
        .unwrap_or(info);
    t.set("Icon", format!("data/ui/units/icons/{icon}"))?;
    t.set("Men", u.men)?;
    t.set("Max", u.max_men)?;
    let pct = if u.max_men > 0 { (u.men * 100) as f32 / u.max_men as f32 } else { 0.0 };
    t.set("MenAsPercent", pct)?;
    t.set("EstimatedMenAsUnary", pct / 100.0)?;
    t.set("ReplenishmentLevel", 0)?;
    t.set("Replenished", false)?;
    t.set("SufferingAttrition", false)?;
    // PROVISIONAL: the unit's chevrons (`unit+0xD48`, printed as "Experience" by `0x005CD340`) are
    // not in the model: the ESF index for a loaded chevron count is UNKNOWN.
    t.set("Experience", 0)?;
    t.set("IsNaval", force.is_navy)?;
    t.set("Guns", 0)?;
    t.set("InTransit", false)?;
    // PromotionCost: what the field promotion of this unit's commander charges
    // ([`CampaignModel::promotion_cost`], PROVISIONAL value; CONFIRMED field and place -- the unit
    // rows carry it and `army.lua:825`'s `SelectedUnitsPromotionCost` sums it over the selected
    // units). Not a General or admiral's own unit: the promotion is of the unit's commander.
    let promo = {
        let m = ui.model();
        m.promotion_cost(force.id, index)
    };
    t.set("PromotionCost", promo.unwrap_or(0))?;
    // What the player may know about this unit (`utilities.lua`'s `knowledge_mask` and
    // `spying_data_level`, CONFIRMED values: mask bits 1 icon / 2 men / 4 guns / 8 experience, the
    // owned mask 15; levels -1 invalid / 0 passive / 1 basic / 2 advanced / 3 owned). The level comes
    // from the model's sight and knowledge (INFERRED mapping, `spying_level_character`); the
    // experience is a PROVISIONAL 0 anyway.
    let level = {
        let m = ui.model();
        match m.faction_by_key(&ui.link.human).map(|h| h.id) {
            Some(human) if force.faction != human => spying_level_unit(&m, human, u.id),
            Some(_) => LEVEL_OWNED,
            None => LEVEL_INVALID,
        }
    };
    t.set("knowledge_mask", knowledge_mask(level))?;
    t.set("spying_data_level", level)?;
    // CommanderType: Utilities.lua's CT_* values (CONFIRMED): 0 primary general, 1 secondary
    // general, 2 primary admiral, 3 secondary admiral, 4 commodore, 5 brigadier, 6 naval unit,
    // 7 land unit. INFERRED: the first unit of a force with a commander is his own card (the
    // startpos armies lead with the general's bodyguard).
    // Every card carries a Portrait (`0x009AA5E0` writes it always, empty unless set below; an
    // admiral's card in character format calls `string.len` on it).
    t.set("Portrait", "")?;
    let mut ct = if force.is_navy { 6 } else { 7 };
    if index == 0
        && let Some(c) = force.commander
    {
        let kind = ui.model().world.characters.get(&c).map(|ch| ch.kind);
        // Portrait: the card snapshot's `0x008C9EF0` -- "data/" + the unit's character's portrait
        // path when his agent type is 0, the General (`0x00F9C6C0`: agent record `+0x2C` == 0),
        // else empty (CONFIRMED, own Ghidra copy 2026-10-07). The card shows it instead of the
        // unit picture when DisplayAsUnit is false, CommanderType is a general or admiral and
        // `string.len(Portrait) > 0`, else its Icon (`template.CampaignUnitCard.lua:66-99`,
        // CONFIRMED from the bytecode, `luac_dump --proto 56`), so an admiral's card always keeps
        // its unit picture: unlike the Lists rows (`character_details`), it needs no fallback for
        // a missing portrait. The path is the character's PORTRAIT_DETAILS card picture (INFERRED:
        // the exe reads the character's `+0x370`; it is the path the character card's CardImage
        // uses too).
        if kind == Some(CharacterKind::General) {
            match portrait_card(&ui.model(), c) {
                Some(card) => t.set("Portrait", format!("data/{card}"))?,
                // PLACEHOLDER: a general with no portrait in the model (the recruitment pool's
                // hires and promoted generals get none yet, BACKLOG §0 "portraits of generated
                // characters") keeps his unit card instead of a portrait-less character card.
                None => display_as_unit = true,
            }
        }
        t.set("CharacterPtr", character_value(ui, c))?;
        ct = match kind {
            Some(CharacterKind::Admiral) => 2,
            Some(CharacterKind::Captain) => 4,
            Some(CharacterKind::Colonel) => 5,
            _ => 0,
        };
        let name = character_name(inner, ui, c).or_else(|| Some(character_type_name(inner, ui, c)));
        t.set("CommandersName", name.unwrap_or_default())?;
        // Attributes = {PrimaryAttributePath, PrimaryLevel, PrimaryAttributeName} (CONFIRMED names:
        // exe strings 0x0136EAE8.., read by the card), the commander's main attribute
        // ([`attributes_table`]).
        t.set("Attributes", attributes_table(lua, inner, ui, Some(c), Pips::Without)?)?;
    }
    t.set("CommanderType", ct)?;
    t.set("DisplayAsUnit", display_as_unit)?;
    Ok(t)
}

/// `ui/army.luac`'s fort building states (its main chunk, lines 8-10, CONFIRMED by disassembly):
/// `FBS_ABLE` = 0 (`AbleToBuildFort` is `g_fort_building_status == FBS_ABLE`, line 817) and
/// `FBS_UNABLE` = 1 (what `GenerateNavyPanel` sets, line 256).
const FBS_UNABLE: i32 = 1;

/// The army panel's `build_fort_cost`: the exe's `0x004613B0`, which always returns -1 (CONFIRMED).
const FORT_COST_NONE: i32 = -1;

/// The info table `GenerateArmyPanel` / `GenerateNavyPanel` receive: {controlable, commander,
/// military_force, can_build_fort_status, build_fort_cost, units_info = {Units|Ships = {...}}}
/// (CONFIRMED names: Army.lua reads them, strings at 0x0136D630..0x0136D6D0). `can_build_fort_status`
/// and `build_fort_cost` are the army panel's `g_button_fort` state / tooltip (the luac calls
/// `AbleToBuildFort` and `g_button_fort:SetState/SetTooltipText/SetVisible`, CONFIRMED call sites).
///
/// INFERRED (static only, own Ghidra copy, 2026-10-07; not yet seen in the original): the engine's
/// writer is the army panel info builder `0x009FCFB0` (an orphan until now, so the two key strings
/// showed no code reference; it refers to them by immediate push). It writes
/// `can_build_fort_status` = `0x0047AF90()`, a folded `return 1` (= [`FBS_UNABLE`]), and
/// `build_fort_cost` = `0x004613B0()`, a folded `return -1` -- constants, as `BuildFort` never
/// succeeds (see there). So the button is always greyed with its plain inactive tooltip
/// (`ShowArmyButtons`, `Army.lua:1127-1131`: `0 < g_fort_cost` is false). Stays INFERRED until a
/// side-by-side check shows the original's army-panel fort button greyed too.
pub(super) fn force_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, force: ForceId) -> mlua::Result<Value> {
    let f = match ui.model().world.forces.get(&force) {
        Some(f) => f.clone(),
        None => return Ok(Value::Nil),
    };
    let human = ui.model().faction_by_key(&ui.link.human).map(|h| h.id);
    let t = lua.create_table()?;
    t.set("controlable", Some(f.faction) == human)?;
    t.set("commander", f.commander.map(|c| character_value(ui, c)))?;
    t.set("military_force", force_value(ui, f.id))?;
    t.set("can_build_fort_status", FBS_UNABLE)?;
    t.set("build_fort_cost", FORT_COST_NONE)?;
    let units = lua.create_table()?;
    for i in 0..f.units.len() {
        units.set(i + 1, unit_entry(lua, inner, ui, &f, i, false)?)?;
    }
    let info = lua.create_table()?;
    info.set(if f.is_navy { "Ships" } else { "Units" }, units)?;
    t.set("units_info", info)?;
    Ok(Value::Table(t))
}

// ---------------------------------------------------------------------------------------------
pub(super) const TAG_QUEUE_ITEM: usize = 5 << 40;

/// The unit card picture's path without ".tga": `ui/units/icons/<faction>_<unit>_icon`
/// (CONFIRMED file names), else the bare info key (INFERRED fallback).
fn unit_icon(inner: &Inner, db: &GameDatabase, faction: &str, unit_key: &str) -> String {
    let info = db.unit(unit_key).map(|r| r.info_key.clone()).unwrap_or_else(|| unit_key.to_owned());
    let icon = [format!("{faction}_{unit_key}_icon"), format!("{faction}_{info}_icon"), info.clone()]
        .into_iter()
        .find(|p| inner.source.find(&format!("ui/units/icons/{p}.tga")).is_some())
        .unwrap_or(info);
    format!("data/ui/units/icons/{icon}")
}

/// Is `key` a ship? CONFIRMED structure: a naval unit has no `unit_stats_land` row (its stats are in
/// `unit_stats_naval`), and the `units` #2 category is `naval_*` for the war-ships. Both are used so
/// that a vessel whose category is not `naval_*` (the trade ships) still counts. INFERRED: the exe's
/// per-card `is_naval` = `[card+0xA0] != 0` (0x009FE7B0) is one flag on the card; which field it is
/// built from was not decoded, and no script-facing name for it exists.
fn is_ship(db: &GameDatabase, key: &str) -> bool {
    db.unit(key).is_some_and(|u| u.category.starts_with("naval")) || (db.unit(key).is_some() && db.unit_stats(key).is_none())
}

/// The card state a unit category picks (`artillery`, `infantry`, `cavalry`, `naval`: the
/// RecruitmentCard states, CONFIRMED; the mapping from `units` #2 INFERRED). A ship is `naval`
/// whatever its category says (the trade vessels' category is not `naval_*`, and they are ships).
fn card_category(db: &GameDatabase, unit_key: &str) -> &'static str {
    if is_ship(db, unit_key) {
        return "naval";
    }
    match db.unit(unit_key).map(|u| u.category.as_str()).unwrap_or("") {
        "cavalry" => "cavalry",
        "artillery" => "artillery",
        _ => "infantry",
    }
}

/// `GenerateRecruitmentPanel(info)`: {recruitable_units, enqueued_units, faction_colour,
/// uniform_colour, recruitment_capacity, player_owned} (CONFIRMED names: the exe strings the
/// generator itself pushes, `recruitable_units` 0x0136D734, `recruitable_unit` 0x0136D748,
/// the card-id markers at 0x0136D7A0, `recruitment_capacity` 0x0136D87C and `Available`
/// 0x0136D71C, plus Recruitment.lua). Entries carry the fields Recruitment.lua and
/// template.RecruitmentCard.lua read: item_ptr, manager, record, character, unit_record, status
/// ("Available" / "Unavailable" / "Enqueued"), reasons_unavailable (bit i = the i-th reason of the
/// card's list: no slot, unaffordable, population, damaged, occupied, siege, limit, technology,
/// path), name, description, image_path, cost, upkeep, turns, card_id, category, class, experience,
/// slot, faction_key. card_id = "<unit>!recruitable!<n>" / "<unit>!enqueued!<n>" (the card script
/// cuts the id at "!"; the exe strings "!recruitable!" and "!enqueued!" CONFIRMED, the index suffix
/// INFERRED). PROVISIONAL: the entries' experience 0 (a unit being raised is a fresh recruit);
/// reasons_unavailable is the entry's flags (`CampaignModel::recruitable_entry_flags`, `0x00B69BA0`),
/// whose 1 / 2 / 4 / 0x40 are bits 0 / 1 / 2 / 6 of that list (no slot = queue full, unaffordable,
/// population, limit), INFERRED from that match, not from the generator; the queue's turns are the
/// remaining turns.
///
/// Naval (0-E round N+1). The generator is `0x009FE7B0` (14,199 bytes), CONFIRMED as the owner of
/// every string named above. There is no `GenerateNaval` symbol in the exe: the only generator
/// names it registers are `GenerateRecruitmentPanel` (0x009C7C70), `GenerateFortConstructionPanel`
/// (0x009C7B50), `GenerateConstructionPanel` (0x009C7CF0, 0x009C77A0), `GenerateAgentsPanel`
/// (0x009C7AE7), `GenerateArmyPanel` and `GenerateNavyPanel` (0x009C7BE0), so the naval recruitment
/// tab reuses this one generator (CONFIRMED). Inside it a switch on `[obj+0x8]` (0..13, jump table
/// 0x00A01F28) picks a category name, and one case pushes `naval` (0x0130CFFC, at 0x00A01217); every
/// card gets `is_naval` = (`[card+0xA0] != 0`) (0x013305C0, at 0x009FF497, 0x00A004E3, 0x00A00F21).
/// The generator is registered with a bool at `+0xA4` beside its pointer at `+0xA0` (0x009C7C70),
/// the likely land/naval selector (INFERRED), and the panel's `naval_recruitment_tab` component is
/// CONFIRMED (exe string 0x013CC71C, beside `recruitment_tab` 0x013CC70C). The naval half is WIRED
/// (0-E round N+2): the infrastructure/naval tabs below pass `naval = true`, which asks
/// `recruitment_points(region, true)` (the modelled `naval_recruitment_points` per port, `0x00B61EE0`
/// CONFIRMED) and keeps only the cards whose unit category is `naval_*` (`units` #2, the source of the
/// generator's own per-card `is_naval` = `[card+0xA0] != 0`, CONFIRMED). PROVISIONAL: that `naval`
/// argument stands in for the manager bool at `+0xA4`, whose writer is UNKNOWN, and the tab is placed
/// where 0-G's trace of the tab builder `FUN_0099A200` puts it -- see [`Tab::NavalRecruitment`].
/// UNKNOWN: `CampaignShipCard`, a template no shipped script references, so we cannot say whether
/// the naval cards use it.
/// A recruitment item's `unit_record`: the unit's details table, which template.RecruitmentCard.lua
/// keeps and hands to the unit card tooltip (template.unitcard_tooltip.lua's Initialise reads Name,
/// Class, Description, IsNaval, IsArtillery, Men, Guns, Range, Accuracy, Melee, Charge, Defence,
/// Morale; naval: Firepower, Speed, Manoeuvrability, HullStrength, Seamen, Gunners; the card reads
/// Key and UnitLimit). CONFIRMED field names. Values: Class = the class's on-screen name, Range =
/// the projectile's effective range, Melee / Charge / Defence / Morale / Accuracy =
/// `unit_stats_land` melee attack / charge bonus / melee defence / morale / accuracy (INFERRED
/// columns); naval stats are not given (PROVISIONAL).
///
/// `UnitLimit` is always an integer: the exe's card-record builder `0x009AA5E0` sets it with its
/// integer setter (`0x0044DE40`) from the card snapshot (+0x94), never leaving it out (CONFIRMED),
/// and every shipped reader tests `0 < UnitLimit` before the limited-unit text
/// (template.CampaignUnitCard.lua:406, template.recruitmentcard.lua:259, CONFIRMED from the
/// bytecode), so 0 is the "no limit" path. Where the exe's value comes from is UNKNOWN; ours is 0
/// (PROVISIONAL: no unit limit in the model yet).
///
/// The values are worked out once per unit key and kept on the campaign HUD (they come from the
/// database and the localisation, both fixed for the HUD's lifetime, so nothing goes stale); each
/// call hands out a fresh table, so a script that writes to its card's record changes no other
/// card's.
///
/// No borrow of the cache is held while the table is built (building it can run Lua).
fn unit_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, key: &str) -> mlua::Result<Table> {
    let cached = ui.unit_details.borrow().get(key).cloned();
    let d = match cached {
        Some(d) => d,
        None => {
            let d = Rc::new(UnitDetails::new(inner, &ui.link.db, key));
            ui.unit_details.borrow_mut().insert(key.to_owned(), d.clone());
            d
        }
    };
    d.to_table(lua)
}

/// The values of one unit's [`unit_details`] table.
pub(super) struct UnitDetails {
    key: String,
    name: String,
    description: String,
    class: String,
    naval: bool,
    artillery: bool,
    /// From `unit_stats_land`, when the unit has a row there.
    stats: Option<UnitDetailStats>,
}

/// The `unit_stats_land` part of [`UnitDetails`].
struct UnitDetailStats {
    men: i64,
    /// Only for a unit with guns.
    guns: Option<i64>,
    range: i64,
    accuracy: i64,
    melee: i64,
    charge: i64,
    defence: i64,
    morale: i64,
}

impl UnitDetails {
    fn new(inner: &Inner, db: &GameDatabase, key: &str) -> Self {
        let u = db.unit(key);
        let class = u.map(|u| u.unit_class.clone()).unwrap_or_default();
        let s = db.unit_stats(key);
        UnitDetails {
            key: key.to_owned(),
            name: loc(inner, &format!("units_on_screen_name_{key}")).unwrap_or_else(|| key.to_owned()),
            description: loc(inner, &format!("unit_description_texts_description_text_{key}")).unwrap_or_default(),
            class: loc(inner, &format!("unit_class_onscreen_{class}")).unwrap_or(class),
            naval: u.is_some_and(|u| u.category.starts_with("naval")),
            artillery: s.is_some_and(|s| s.is_artillery),
            stats: s.map(|s| UnitDetailStats {
                men: i64::from(s.num_men),
                guns: (s.num_guns > 0).then(|| i64::from(s.num_guns)),
                // Range is always a number, also for a melee unit: the exe's army unit-card builder
                // `0x009AA5E0` sets it on every card with its float setter (`0x0044DEF0`, from the
                // card snapshot +0x5C, no branch; CONFIRMED for that card; INFERRED for the
                // recruitment card's record, whose builder is not traced), and the unit tooltip
                // compares it with 0 for artillery (template.unitcard_tooltip.lua:93). The value is
                // the card snapshot's (`0x008DF190`): the gun type's longest range, else the unit's
                // own projectile's, else 0 (`GameDatabase::unit_card_range`, CONFIRMED).
                range: i64::from(db.unit_card_range(s)),
                accuracy: i64::from(s.accuracy),
                melee: i64::from(s.melee_attack),
                charge: i64::from(s.charge_bonus),
                defence: i64::from(s.melee_defence),
                morale: i64::from(s.morale),
            }),
        }
    }

    /// A fresh table with these values.
    fn to_table(&self, lua: &Lua) -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.set("Key", self.key.as_str())?;
        t.set("Name", self.name.as_str())?;
        t.set("Description", self.description.as_str())?;
        t.set("Class", self.class.as_str())?;
        t.set("IsNaval", self.naval)?;
        t.set("IsArtillery", self.artillery)?;
        t.set("UnitLimit", 0)?;
        if let Some(s) = &self.stats {
            t.set("Men", s.men)?;
            if let Some(g) = s.guns {
                t.set("Guns", g)?;
            }
            t.set("Range", s.range)?;
            t.set("Accuracy", s.accuracy)?;
            t.set("Melee", s.melee)?;
            t.set("Charge", s.charge)?;
            t.set("Defence", s.defence)?;
            t.set("Morale", s.morale)?;
        }
        Ok(t)
    }
}

/// `GenerateRecruitmentPanel(info)` for the recruitment tab. `naval` asks for the naval half of the
/// same generator (the `naval_recruitment_tab`, see [`Tab::NavalRecruitment`]): the region's naval
/// recruitment capacity instead of the land one, and only the ships among the region's recruitable
/// units. See the module note above for what is CONFIRMED and what is PROVISIONAL here.
pub(super) fn recruitment_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, region: RegionId, naval: bool) -> mlua::Result<Value> {
    let m = ui.model();
    let Some(r) = m.world.regions.get(&region) else { return Ok(Value::Nil) };
    let owner_key = m.world.factions.get(&r.owner).map(|f| f.key.clone()).unwrap_or_default();
    let treasury = m.world.factions.get(&r.owner).map_or(0, |f| f.treasury);
    let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
    let capacity = m.recruitment_points(region, naval);
    let recruitable = m.recruitable_units(region);
    let is_naval = |k: &str| is_ship(&ui.link.db, k);
    // The naval tab shows the ships only (`units` #2 category, CONFIRMED source of the generator's
    // own `is_naval`), both among the recruitable units and in the queue.
    let shown = |k: &String| !naval || is_naval(k);
    let recruitable: Vec<&String> = recruitable.iter().filter(|k| shown(k)).collect();
    let queue: Vec<(RecruitmentItemId, String, u32, i32)> =
        r.recruitment_queue.iter().filter(|q| shown(&q.unit_key)).map(|q| (q.id, q.unit_key.clone(), q.turns_remaining, q.cost)).collect();
    // The price is the region's recruitable entry cost (`0x00B31020` → `0x00B0D220`: `units` #7 with the
    // region's cost effects), the value the queue command charges and the item records, and the reasons are
    // the entry's flags (`0x00B69BA0`): both come from the model functions the queue command uses, so the
    // card can never drift from the command. `upkeep` stays the unit type's own `UpkeepCost` (`card+0x3C`),
    // which is what the panel shows.
    let set = economy::region_effect_set(&m, r);
    let counts = m.unit_type_counts(r.owner);
    let rules: Vec<(i32, i32, u32, u32)> = recruitable
        .iter()
        .map(|k| {
            m.rules.units.get(*k).map_or((0, 0, 1, 0), |u| {
                let cost = economy::recruitment_cost_in(&m.rules, &set, k, u);
                (cost, u.upkeep, u.turns, m.recruitable_entry_flags(r, k, u, cost, &counts))
            })
        })
        .collect();
    let player_owned = Some(r.owner) == human;
    drop(m);
    ui.forget_finished_queue_items();
    let db = &ui.link.db;
    let entry = |key: &str, status: &str, cost: i32, upkeep: i32, turns: u32, card_id: String, slot: Option<usize>| -> mlua::Result<Table> {
        let e = lua.create_table()?;
        e.set("manager", region_value(ui, region))?;
        e.set("record", key)?;
        e.set("unit_record", unit_details(lua, inner, ui, key)?)?;
        e.set("is_naval", is_naval(key))?;
        e.set("status", status)?;
        e.set("name", loc(inner, &format!("units_on_screen_name_{key}")).unwrap_or_else(|| key.to_owned()))?;
        e.set("description", loc(inner, &format!("unit_description_texts_description_text_{key}")).unwrap_or_default())?;
        e.set("image_path", unit_icon(inner, db, &owner_key, key))?;
        e.set("cost", cost)?;
        e.set("upkeep", upkeep)?;
        e.set("turns", turns)?;
        e.set("turns_to_completion", turns)?;
        e.set("card_id", card_id)?;
        e.set("category", card_category(db, key))?;
        e.set("class", db.unit(key).map(|u| u.unit_class.clone()).unwrap_or_default())?;
        e.set("experience", 0)?;
        e.set("faction_key", owner_key.as_str())?;
        e.set("affordable", treasury::recruitment_affordable(treasury, cost))?;
        if let Some(s) = slot {
            e.set("slot", s)?;
        }
        Ok(e)
    };
    let units = lua.create_table()?;
    for (i, (key, (cost, upkeep, turns, reasons))) in recruitable.iter().zip(rules).enumerate() {
        let e = entry(key.as_str(), if reasons == 0 { "Available" } else { "Unavailable" }, cost, upkeep, turns, format!("{key}!recruitable!{i}"), None)?;
        e.set("reasons_unavailable", reasons)?;
        units.set(i + 1, e)?;
    }
    let enqueued = lua.create_table()?;
    for (i, (item, key, turns, cost)) in queue.iter().enumerate() {
        let e = entry(key, "Enqueued", *cost, 0, *turns, format!("{key}!enqueued!{i}"), Some(i))?;
        // The item's own id is the payload, not its queue position: an `item_ptr` a script kept
        // names the same item after other items were cancelled. Each item's address is interned in
        // `CampaignUi::addresses` the first time it is shown and kept while the item is queued
        // (`CampaignUi::forget_finished_queue_items`).
        e.set("item_ptr", ui.entity(TAG_QUEUE_ITEM, item.raw()))?;
        e.set("reasons_unavailable", 0)?;
        enqueued.set(i + 1, e)?;
    }
    recruitment_table(lua, ui, units, enqueued, &owner_key, capacity, player_owned)
}

/// The `GenerateRecruitmentPanel` info table around its two card lists: the faction's colours,
/// the capacity and whether the player owns the manager.
fn recruitment_table(lua: &Lua, ui: &CampaignUi, units: Table, enqueued: Table, faction_key: &str, capacity: u32, player_owned: bool) -> mlua::Result<Value> {
    let t = lua.create_table()?;
    t.set("recruitable_units", units)?;
    t.set("enqueued_units", enqueued)?;
    if let Some(rec) = ui.link.db.faction(faction_key) {
        let c = |c: [u8; 3]| -> mlua::Result<Table> {
            let ct = lua.create_table()?;
            ct.set("r", c[0])?;
            ct.set("g", c[1])?;
            ct.set("b", c[2])?;
            Ok(ct)
        };
        t.set("faction_colour", c(rec.primary_colour())?)?;
        t.set("uniform_colour", c(rec.secondary_colour())?)?;
    }
    t.set("recruitment_capacity", capacity)?;
    t.set("player_owned", player_owned)?;
    Ok(Value::Table(t))
}

/// An army's recruitment panel with nothing to recruit (PLACEHOLDER, see `generate_current_tab`):
/// the army's faction's colours, no cards, capacity 0.
pub(super) fn empty_recruitment_info(lua: &Lua, ui: &CampaignUi, commander: CharacterId) -> mlua::Result<Value> {
    let (faction_key, player_owned) = {
        let m = ui.model();
        let faction = m.world.characters.get(&commander).and_then(|c| m.world.factions.get(&c.faction));
        (faction.map(|f| f.key.clone()).unwrap_or_default(), faction.is_some_and(|f| f.key == ui.link.human))
    };
    recruitment_table(lua, ui, lua.create_table()?, lua.create_table()?, &faction_key, 0, player_owned)
}

// ---------------------------------------------------------------------------------------------
// The commander pool panel (`ui/enlist_commander_scripts/enlist_commander.luac`, read on the
// install 2026-10-05, CONFIRMED): the army / navy panel's Promote button opens it
// (`army.lua:582` `panel_manager:OpenPanel("enlist_commander", ..., "InitEnlistCommander",
// g_panel_is_navy, g_military_force)`), which asks the engine for the candidates and the pool state
// and then the player picks one.

/// The pool a force recruits from: admirals for a navy, generals for an army (the panel's own
/// `SetAsRecruitmentType(is_navy)` switch, CONFIRMED).
fn pool_kind_of(m: &CampaignModel, force: ForceId) -> Option<(ntw_sim::campaign::pool::PoolKind, FactionId)> {
    use ntw_sim::campaign::pool::PoolKind;
    let f = m.world.forces.get(&force)?;
    Some((if f.is_navy { PoolKind::Admiral } else { PoolKind::General }, f.faction))
}

/// `CampaignUI.AvailableCommandersForRecruitment(force, is_navy)`: the pool of the faction the force
/// belongs to. CONFIRMED names and arity (2: the force's address, `is_navy`) from
/// `enlist_commander.lua:38`, which reads `CurrentNumGenerals`, `MaxGeneralsAllowed`,
/// `MaxDistanceToTrack`, `DistanceToCapital`, `TurnsToNextPoolFill` and `#table` candidates.
///
/// Per candidate CONFIRMED fields (`enlist_commander_entry.lua:14`): `commander_pointer` (what
/// `CampaignUI.PromoteUnits` is handed), `Name`, `RecruitmentCost`, `Attributes.PrimaryLevel`,
/// `UniqueId`, `IsRecruitable`, `InfoImage`, `Traits`.
///
/// - the price is the model's hire cost measured at the force ([`CampaignModel::hire_cost_into`],
///   the CONFIRMED formula: 400 + 300 × rank + the distance part);
/// - `DistanceToCapital` / `MaxDistanceToTrack`: the force's distance to its capital and
///   `character_recruitment_max_distance` (the panel divides them for its bar; INFERRED pair, the
///   model's own distance part uses the same maximum);
/// - `TurnsToNextPoolFill`: the pool timer's turns;
/// - `CurrentNumGenerals` / `MaxGeneralsAllowed`: the faction's commanders of that kind in the world
///   and that count plus the pool's cap (`character_recruitment_pool_cap`).
///
///   **PROVISIONAL, and the absence is now a checked negative rather than a gap.** The shipped
///   data contains no general limit to read: `db\campaign_variables_tables\campaign_variables`
///   holds exactly ten `character_recruitment*` keys (base cost, cost per command star, max
///   distance, pool cap, and six refill rates) and **none** of them is a cap on the number of
///   generals; `db\effect_bonus_value_basic_junction_tables\effect_bonus_value_basic_junction`
///   has four `character_recruitment*` rows and none grants such a bonus either; and no key
///   containing `generals` or `num_generals` exists in any of the 86,977 files in `data.pack`.
///   So **there is no data-level general limit in the vanilla game**, and whatever the original
///   shows here is either a fixed constant, a difficulty setting or something not in the DB.
///   Ours answers "commanders of that kind in the world" and "that plus the pool cap"; the panel
///   only uses the pair for its "3 / 5" label, so the number is cosmetic.
///
/// One row of the commander pool panel (`enlist_commander_entry.lua`), gathered from the model.
struct CommanderRow {
    character: CharacterId,
    cost: Option<i32>,
    type_name: String,
    recruitable: bool,
    historical: bool,
    /// At most four trait keys (CONFIRMED: the entry shows four).
    traits: Vec<String>,
    card: String,
}

fn commanders_for_recruitment(lua: &Lua, inner: &Inner, ui: &CampaignUi, force: ForceId) -> mlua::Result<Value> {
    // Everything the model answers, gathered first (the model's borrow is a `Ref`, so it must be
    // dropped before the tables are built).
    let gathered = {
        let m = ui.model();
        let Some((kind, faction)) = pool_kind_of(&m, force) else { return Ok(Value::Nil) };
        let list = m.world.faction_details.get(&faction).map(|d| match kind {
            ntw_sim::campaign::pool::PoolKind::General => &d.general_pool,
            ntw_sim::campaign::pool::PoolKind::Admiral => &d.admiral_pool,
        });
        let (candidates, timer) = list.cloned().unwrap_or_default();
        let purse = m.world.factions.get(&faction).map_or(0, |f| f.treasury);
        let at = m.force_position(force);
        let capital = m.world.faction_details.get(&faction).and_then(|d| d.capital);
        let distance = match (at, capital) {
            (Some(at), Some(r)) => m
                .world
                .regions
                .get(&r)
                .map(|reg| {
                    let (dx, dz) = (reg.settlement.position.0.to_f32() - at.0.to_f32(), reg.settlement.position.1.to_f32() - at.1.to_f32());
                    (dx * dx + dz * dz).sqrt()
                })
                .unwrap_or(0.0),
            _ => 0.0,
        };
        let rows: Vec<CommanderRow> = candidates
            .iter()
            .map(|&c| {
                let cost = m.hire_cost_into(c, force);
                let details = m.world.character_details.get(&c);
                CommanderRow {
                    character: c,
                    cost,
                    type_name: character_type_name(inner, ui, c),
                    recruitable: cost.is_some_and(|c| c <= purse),
                    historical: details.and_then(|d| d.historical_key.as_deref()).is_some(),
                    traits: details.map(|d| d.traits.iter().take(4).map(|t| t.key.clone()).collect()).unwrap_or_default(),
                    card: portrait_card(&m, c).map(str::to_owned).unwrap_or_default(),
                }
            })
            .collect();
        let in_command = m
            .world
            .characters
            .values()
            .filter(|c| c.faction == faction)
            .filter(|c| match kind {
                ntw_sim::campaign::pool::PoolKind::General => c.kind == CharacterKind::General,
                ntw_sim::campaign::pool::PoolKind::Admiral => c.kind == CharacterKind::Admiral,
            })
            .count();
        (rows, in_command, m.pool_cap(), m.rules.var("character_recruitment_max_distance", 1000.0), distance, timer.saturating_sub(m.calendar.turns_elapsed))
    };
    let (rows, in_command, cap, max_distance, distance, turns) = gathered;
    let t = lua.create_table()?;
    for (i, row_in) in rows.into_iter().enumerate() {
        let CommanderRow { character: c, cost, type_name, recruitable, historical, traits, card } = row_in;
        // The character's own name if the model has one (our own save writers keep it), else his type.
        let name = character_name(inner, ui, c).unwrap_or(type_name);
        let row = lua.create_table()?;
        row.set("commander_pointer", character_value(ui, c))?;
        row.set("Name", name)?;
        // A string: the row puts it straight into `dy_cost`'s state text (CONFIRMED).
        row.set("RecruitmentCost", cost.unwrap_or(0).to_string())?;
        // Only the `Primary*` fields (the entry reads no pips).
        row.set("Attributes", attributes_table(lua, inner, ui, Some(c), Pips::Without)?)?;
        // PROVISIONAL format (the original's is the agent record's pointer); unique per candidate.
        row.set("UniqueId", format!("commander_{}", c.0))?;
        row.set("IsRecruitable", recruitable)?;
        row.set("InfoImage", if card.is_empty() { String::new() } else { format!("data/{card}") })?;
        let traits_table = lua.create_table()?;
        for (j, key) in traits.iter().enumerate() {
            // The entry shows at most four traits (CONFIRMED: `enlist_commander_entry.lua:36`).
            // `ui\templates\character_trait_entry.luac` then hands each row to `InitialiseTrait`
            // (the proto at line 5: `Name` into `tx_enables`, `IconFilename` into `effect_icon`)
            // and `SetTraitTooltip` (the proto at line 17), which calls
            // `Utilities.GetEffectList(row.Effects, row.AttributeEffects)`. That function takes
            // `#attribute_effects` on its *second* argument first (utilities.lua:182, pc 2), so
            // **both fields must be tables or the panel raises** -- which is exactly what stopped
            // the enlist-commander panel: the click reached `PanelManager.OpenPanel`, built every
            // row, then died inside the first trait's tooltip, so no commander could be picked.
            // CONFIRMED: the two field names, that they are lists, and the four the tooltip also
            // reads (`ColourText`, `ExplanationText`, `RemovalText`).
            // PROVISIONAL: the lists are empty and the three texts empty -- the effect
            // descriptions behind a trait come from a table we do not load.
            let e = lua.create_table()?;
            e.set("Name", key.as_str())?;
            e.set("IconFilename", String::new())?;
            e.set("Effects", lua.create_table()?)?;
            e.set("AttributeEffects", lua.create_table()?)?;
            e.set("ColourText", String::new())?;
            e.set("ExplanationText", String::new())?;
            e.set("RemovalText", String::new())?;
            traits_table.set(j + 1, e)?;
        }
        row.set("Traits", traits_table)?;
        // A historical candidate is badged (the model's `historical_key`, CONFIRMED kept).
        row.set("IsHistorical", historical)?;
        t.set(i + 1, row)?;
    }
    t.set("CurrentNumGenerals", in_command)?;
    t.set("MaxGeneralsAllowed", in_command + cap)?;
    t.set("MaxDistanceToTrack", max_distance)?;
    t.set("DistanceToCapital", distance)?;
    t.set("TurnsToNextPoolFill", turns)?;
    Ok(Value::Table(t))
}

pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // IsMergingUnit(unit) → true while the unit is part of a merge the player is setting up
    // (0x009EC670 → 0x00A0B8F0, CONFIRMED bool result; meaning INFERRED from the name). Unit
    // merging is not in our HUD yet (PROVISIONAL), so false.
    f!("IsMergingUnit", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    f!("UnitSelectionChanged", |_l, inner, ui, _a: Variadic<Value>| Ok(()));
    // The commander pool (the army / navy panel's Promote button, `army.lua:582` ->
    // `enlist_commander.lua`, CONFIRMED on the install 2026-10-05). CanRecruitCommander(force,
    // is_navy) is asked with the force and the panel's kind (`army.lua:708`); the second argument
    // of both calls is redundant for us (the force knows whether it is a navy).
    f!("CanRecruitCommander", |_l, _inner, ui, (force, _is_navy): (Value, Value)| {
        let m = ui.model();
        Ok(entity_of(&force, TAG_FORCE).map(|f| m.can_recruit_commander(ForceId(f as u32))).unwrap_or(false))
    });
    f!("AvailableCommandersForRecruitment", |lua, inner, ui, (force, _is_navy): (Value, Option<bool>)| {
        let Some(f) = entity_of(&force, TAG_FORCE).map(|f| ForceId(f as u32)) else { return Ok(Value::Nil) };
        commanders_for_recruitment(lua, &inner, &ui, f)
    });
    // PromoteUnits(force, commander): what the enlist-commander panel calls when the player confirms
    // a candidate (`enlist_commander.lua:106`, CONFIRMED arity 2) -- the original's name for hiring
    // a pool candidate into a force. The model: `HireGeneral { into }` for an army,
    // `HireAdmiral { fleet }` for a navy (CONFIRMED).
    f!("PromoteUnits", |_l, _inner, ui, (force, commander): (Value, Value)| {
        let cmd = {
            let m = ui.model();
            match (entity_of(&force, TAG_FORCE).map(|f| ForceId(f as u32)), entity_of(&commander, TAG_CHARACTER).map(CharacterId)) {
                (Some(f), Some(c)) => match m.world.forces.get(&f).map(|x| x.is_navy) {
                    Some(true) => Some(CampaignCommand::HireAdmiral { character: c, fleet: f }),
                    Some(false) => Some(CampaignCommand::HireGeneral { character: c, into: Some(f) }),
                    None => None,
                },
                _ => None,
            }
        };
        if let Some(c) = cmd {
            ui.push(CampaignRequest::Command(c));
        }
        Ok(())
    });
    // CanPromoteUnit(unit card address): the exe's gate for the field promotion of a unit's commander
    // (`0x009E0AF0`, the unit's slot 16; arity 1 CONFIRMED from `army.lua:692`, which asks it with
    // the card's `ItemAddress` only when the unit is not a General or admiral). Answered from the
    // model's own conditions ([`CampaignModel::can_promote_unit`]).
    f!("CanPromoteUnit", |_l, _inner, ui, unit: Value| {
        let m = ui.model();
        let Some(u) = entity_of(&unit, TAG_UNIT).map(UnitId) else { return Ok(false) };
        Ok(m.world.forces.values().any(|f| m.can_promote_unit(f.id, f.units.iter().position(|x| x.id == u).unwrap_or(usize::MAX))))
    });
    // RecruitUnit(character, manager, record): queue a unit (CONFIRMED call in
    // template.RecruitmentCard.lua; the manager is the recruiting region's, the record the unit key,
    // INFERRED). CancelRecruitment(item_ptr): remove that queue item (CONFIRMED call).
    f!("RecruitUnit", |_l, inner, ui, (_c, manager, record): (Value, Value, Option<String>)| {
        if let (Some(r), Some(unit_key)) = (entity_of(&manager, TAG_REGION), record) {
            ui.push(CampaignRequest::Command(CampaignCommand::Recruit { region: RegionId(r as u32), unit_key }));
        }
        Ok(())
    });
    f!("CancelRecruitment", |_l, inner, ui, item: Value| {
        let item = entity_of(&item, TAG_QUEUE_ITEM).map(RecruitmentItemId::from_raw);
        let found = item.and_then(|i| ui.model().recruitment_item_region(i).map(|r| (r, i)));
        if let Some((region, item)) = found {
            ui.push(CampaignRequest::Command(CampaignCommand::CancelRecruitment { region, item }));
        }
        Ok(())
    });
    // UnitScaleFactor([index]) → scale, index (0x009FAB10; the same description as the front end's
    // 0x004795F0, see battle_setup.rs): the unit card tooltip multiplies a unit's men by it.
    f!("UnitScaleFactor", |_l, inner, ui, index: Option<i64>| {
        let idx = index.or_else(|| inner.prefs.borrow().get("gfx_unit_scale").and_then(|v| v.trim().parse().ok())).unwrap_or(3).clamp(0, 3) as usize;
        Ok((super::super::battle_setup::UNIT_SCALES[idx], idx))
    });
    Ok(())
}
