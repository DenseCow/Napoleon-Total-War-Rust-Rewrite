//! The settlement panel: building slots, construction, forts, the building browser and tree.

use super::*;

/// `building_culture_variants` rows: (building level, culture) → (icon, description key)
/// (schema s,s,o,o,o,o,o reads all 264 rows to the end; columns 0 level, 1 culture, 6 icon
/// (`ui/buildings/icons/<icon>.tga`, CONFIRMED file names; column 3 repeats it for settlement
/// buildings only and is the fallback), 5 description key (loc
/// `building_description_texts_*_description_<key>`, CONFIRMED keys); the others UNKNOWN).
pub fn building_variants(source: &crate::ScriptSource) -> std::collections::BTreeMap<(String, String), (String, String)> {
    let mut out = std::collections::BTreeMap::new();
    for r in source.table_rows_or_empty(&ntw_formats::db_folder::tables::BUILDING_CULTURE_VARIANTS).iter() {
        let s = |i: usize| r.get(i).and_then(|v| v.as_str()).unwrap_or("").to_owned();
        let icon = if s(6).is_empty() { s(3) } else { s(6) };
        out.insert((s(0), s(1)), (icon, s(5)));
    }
    out
}

/// One building slot for the construction panel: a settlement slot, the settlement's
/// fortification slot or its road slot.
struct SlotInfo {
    key: String,
    /// The standing building: (level, health).
    standing: Option<(String, u32)>,
    /// The slot's construction item: (level, turns left, total turns, is a repair).
    building: Option<(String, u32, u32, bool)>,
    /// Construction options (`0x00B43300`).
    options: Vec<ConstructionOption>,
    can_repair: bool,
    repair_cost: i32,
}

/// The panel entry of `slot`, keyed `key` (what `BeginConstruction` & co. get back), or `None`
/// when the slot has nothing to show. CONFIRMED (`0x00B7A0E0`, which `0x00A01F50` and
/// `0x00A021B0` ask for every slot, the walls and the road included): a slot is listed when a
/// building stands in it or is being built there, else only when its option list is not empty.
/// (The shipped `Construction.lua:106` indexes the entry's first building, so an entry with no
/// card would be a script error.)
fn slot_info(m: &CampaignModel, region: RegionId, slot: SlotRef, key: String) -> Option<SlotInfo> {
    let r = m.world.regions.get(&region)?;
    let (_, standing) = r.construction_slot(slot)?;
    let standing = standing.map(|b| (b.level_key.clone(), b.health));
    let building = r.construction.iter().find(|c| c.slot == slot).map(|c| {
        // A repair is an item of the standing level; its length is the repair's own
        // ([`CampaignModel::repair_turns`]), not the level's build time.
        let repair = standing.as_ref().is_some_and(|(l, _)| *l == c.level_key);
        let full = if repair { m.repair_turns(region, slot) } else { m.rules.buildings.get(&c.level_key).map_or(c.turns_remaining, |b| b.turns.max(1)) };
        (c.level_key.clone(), c.turns_remaining, full.max(c.turns_remaining), repair)
    });
    // The model repairs slots and the walls, not the road ([`CampaignModel::can_repair`]). The
    // original offers no road repair either: the road is only on the infrastructure panel, whose
    // `infrastructure` flag hides the repair / demolish buttons (`Construction.lua:408`, `:429`),
    // and a damaged road gets no upgrade cards there (`0x009FBAB0`: upgrades at full health only).
    let options = m.construction_options_in(region, slot, None);
    if standing.is_none() && building.is_none() && options.is_empty() {
        return None;
    }
    // `repair_cost` is the slot's repair cost whatever the repair state (CONFIRMED: the row builder
    // `0x009C8190` writes `0x00B66410(slot)` unconditionally, beside `being_repaired` `0x00B4DA20`
    // and `can_repair` `0x00B1A6B0`; `0x00B66410` gives 0 only for a slot with no building), so a
    // building under repair, or the road, still shows what repairing its damage costs.
    let can_repair = m.can_repair(region, slot);
    let repair_cost = m.repair_cost(region, slot);
    Some(SlotInfo { key, standing, building, options, can_repair, repair_cost })
}

/// The `slot_key` prefix of a region's fortification (walls) slot. PROVISIONAL format: the exe's
/// fortification slot is a `REGION_SLOT` with a key of its own, which the model does not keep.
const WALLS_KEY_PREFIX: &str = "fortification:";
/// The `slot_key` prefix of a region's road slot (PROVISIONAL format, as the walls').
const ROAD_KEY_PREFIX: &str = "road:";

/// The `slot_key` of `slot` in region `r` (`None` when the index names no slot): a settlement
/// or map slot's own `REGION_SLOT` key, or the walls' / road's key.
fn slot_key_of(r: &ntw_sim::campaign::Region, slot: SlotRef) -> Option<String> {
    match slot {
        SlotRef::Walls => Some(fortification_slot_key(&r.key)),
        SlotRef::Slot(i) => r.slots.get(i).map(|s| s.key.clone()),
        SlotRef::Road => Some(road_slot_key(&r.key)),
    }
}

/// The `slot_key` of `region_key`'s fortification (walls) slot.
pub(super) fn fortification_slot_key(region_key: &str) -> String {
    format!("{WALLS_KEY_PREFIX}{region_key}")
}

/// The `slot_key` of `region_key`'s road slot.
pub(super) fn road_slot_key(region_key: &str) -> String {
    format!("{ROAD_KEY_PREFIX}{region_key}")
}

/// Which construction panel a settlement tab shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConstructionPanel {
    /// The construction tab: `BuildSettlementConstructionInfoTable` `0x00A01F50`.
    Settlement,
    /// The infrastructure tab: `BuildSettlementInfrastructureInfoTable` `0x00A021B0`.
    Infrastructure,
}

/// `GenerateConstructionPanel(info)` for a settlement's construction or infrastructure tab
/// (CONFIRMED names: Construction.lua's ResetConstructionPanel / GenerateConstructionPanel).
///
/// **The exe (own Ghidra copy, 2026-10-07; the walls placement CONFIRMED at runtime and in game).**
/// - Construction tab, `BuildSettlementConstructionInfoTable` `0x00A01F50`: `slots` = one entry per
///   slot of the settlement's slot list, then **the settlement's fortification slot (`+0x1C4`) as the
///   last entry**; `faction_key`; `controlable`. No `infrastructure`, no `fort_ptr`. Runtime: a
///   breakpoint on `0x00A01F50` hit when the user selected London (call chain `0x00A0F14D`
///   `HandleCampaignMapMouseEvent` -> `0x009C38A1` `HandleSettlementSelected` -> `0x0099A492`
///   `ConstructSettlementPanelTabs` -> `0x009DA2EB` `OpenFirstEnabledPanelTab` -> `0x009C7D4C`
///   `ChangeConstructionTabStateGeneratePanel`), and the user saw "Small Star Fort"
///   (`sFortifications1_settlement_fortifications`) as the last card of London's construction panel.
/// - Infrastructure tab, `BuildSettlementInfrastructureInfoTable` `0x00A021B0`: `slots` = the road
///   slot (`+0x1C0`) alone, `faction_key`, `controlable`, and `infrastructure` = true -- which the
///   script turns into `g_infrastructure` (`Construction.lua:75`, `info.infrastructure ~= nil`) and
///   which hides the repair / demolish buttons (`Construction.lua:408`, `:429`).
/// - Each slot entry, `BuildConstructionSlotEntryTable` `0x009FBAB0`: {buildings = {entry...},
///   upgrades = {entry...}}: `buildings` holds the slot's construction item (type 2) or else its
///   standing building (type 1), then, for an empty slot, every construction option (type 3;
///   Construction.lua's CreateBuildingFoundation makes one card per entry); `upgrades` (type 4) is
///   filled when the standing building is at full health. An entry carries the fields
///   CreateBuildingFrame reads: type (Utilities.lua BUILDING_ICON_TYPE_*: 0 empty, 1 built,
///   2 constructing, 3 constructable, 4 upgrade, 5 alternative chain; CONFIRMED values), name,
///   short/long_description, building_key, region_key, slot_key, health, being_repaired,
///   can_repair, can_afford_repair, repair_cost (`0x009C8190`), dismantling, alternate_chain,
///   availability (bit 1 affordable, bit 2 technology present: Construction.lua's locals,
///   CONFIRMED), image, cost, turns_to_completion, percent_complete, technologies.
///
/// The walls card is an ordinary slot card, so the frame's own calls build it:
/// `BeginConstruction` / `BeginUpgrade(building_key, slot_key)`, `CancelConstruction`,
/// `RepairBuilding`, `DemolishBuilding` with the walls' slot key ([`fortification_slot_key`],
/// resolved by [`slot_by_key`] to `SlotRef::Walls`). INFERRED: the panel lists the settlement's
/// own slots (`settlement:` keys). Every slot passes the exe's filter `0x00B7A0E0` ([`slot_info`]:
/// a building, a construction item or a non-empty option list). The walls and the road slot objects
/// exist in every `REGION_SLOT_MANAGER` of all eight shipped startpos files (CONFIRMED,
/// `esf_find` on the install: 72/72 eur, 30/30 egy, 25/25 ita, 31/31 spa, 8/8 tut, mp alike); the
/// model keeps no empty slot, so a mod region without one still gets them (PROVISIONAL).
/// PROVISIONAL: technologies empty, no alternative chains or dismantling.
pub(super) fn construction_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, region: RegionId, panel: ConstructionPanel) -> mlua::Result<Value> {
    let m = ui.model();
    let Some(r) = m.world.regions.get(&region) else { return Ok(Value::Nil) };
    let owner_key = m.world.factions.get(&r.owner).map(|f| f.key.clone()).unwrap_or_default();
    let treasury = m.world.factions.get(&r.owner).map_or(0, |f| f.treasury);
    let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
    let mut slots = Vec::new();
    match panel {
        ConstructionPanel::Settlement => {
            for (i, s) in r.slots.iter().enumerate().filter(|(_, s)| s.key.starts_with("settlement:")) {
                slots.extend(slot_info(&m, region, SlotRef::Slot(i), s.key.clone()));
            }
            // The walls: the last entry (CONFIRMED, see above); every region has the slot.
            slots.extend(slot_info(&m, region, SlotRef::Walls, fortification_slot_key(&r.key)));
        }
        ConstructionPanel::Infrastructure => {
            slots.extend(slot_info(&m, region, SlotRef::Road, road_slot_key(&r.key)));
        }
    }
    let region_key = r.key.clone();
    let controlable = Some(r.owner) == human;
    drop(m);
    // An entry: level, type, its slot, cost, turns, percent complete, availability.
    let entry = |level: &str, kind: i32, s: &SlotInfo, cost: i32, turns: u32, percent: f32, availability: i32| -> mlua::Result<Table> {
        let e = lua.create_table()?;
        e.set("type", kind)?;
        e.set("region_key", region_key.as_str())?;
        e.set("slot_key", s.key.as_str())?;
        let (name, short, long) = building_texts_for(inner, ui, &owner_key, level);
        e.set("building_key", level)?;
        e.set("name", name)?;
        e.set("short_description", short)?;
        e.set("long_description", long)?;
        e.set("image", ui.building_icon(&owner_key, level))?;
        // The repair fields describe the slot's building (0x009C8190 reads them from the slot).
        let on_slot = kind == 1 || kind == 2;
        e.set("health", if on_slot { s.standing.as_ref().map_or(100, |(_, h)| *h) } else { 100 })?;
        e.set("being_repaired", on_slot && s.building.as_ref().is_some_and(|b| b.3))?;
        e.set("can_repair", on_slot && s.can_repair)?;
        e.set("can_afford_repair", on_slot && ntw_sim::campaign::treasury::can_pay_repair(treasury, s.repair_cost))?;
        e.set("repair_cost", if on_slot { s.repair_cost } else { 0 })?;
        e.set("dismantling", false)?;
        e.set("alternate_chain", false)?;
        e.set("availability", availability)?;
        e.set("cost", cost)?;
        e.set("turns_to_completion", turns)?;
        e.set("percent_complete", percent)?;
        e.set("technologies", lua.create_table()?)?;
        Ok(e)
    };
    let availability = |o: &ConstructionOption| i32::from(o.affordable) | (i32::from(o.tech) << 1);
    let list = lua.create_table()?;
    for (n, s) in slots.iter().enumerate() {
        let st = lua.create_table()?;
        let buildings = lua.create_table()?;
        match (&s.building, &s.standing) {
            (Some((level, left, total, _)), _) => {
                let done = total.saturating_sub(*left) as f32 / (*total).max(1) as f32 * 100.0;
                buildings.raw_push(entry(level, 2, s, 0, *left, done, 3)?)?;
            }
            (None, Some((level, _))) => buildings.raw_push(entry(level, 1, s, 0, 0, 100.0, 3)?)?,
            (None, None) => {
                for o in &s.options {
                    buildings.raw_push(entry(&o.level_key, 3, s, o.cost, o.turns, 0.0, availability(o))?)?;
                }
            }
        }
        st.set("buildings", buildings)?;
        let upgrades = lua.create_table()?;
        if s.building.is_none() && s.standing.as_ref().is_some_and(|(_, h)| *h > 99) {
            for o in &s.options {
                upgrades.raw_push(entry(&o.level_key, 4, s, o.cost, o.turns, 0.0, availability(o))?)?;
            }
        }
        st.set("upgrades", upgrades)?;
        list.set(n + 1, st)?;
    }
    let t = lua.create_table()?;
    t.set("slots", list)?;
    t.set("controlable", controlable)?;
    t.set("faction_key", owner_key)?;
    if panel == ConstructionPanel::Infrastructure {
        t.set("infrastructure", true)?;
    }
    Ok(Value::Table(t))
}

/// `construction_manager.GenerateFortConstructionPanel(info)` -- a **map fort's** panel (`fFort`,
/// [`CampaignSelection::Fort`]; not the settlement walls, which are [`construction_info`]'s last slot).
/// Two rounds of evidence, and the second is stronger:
///
/// **The exe (own Ghidra copy, §4.4).** The generator is registered at **`0x009C7B50`** (guarded on
/// the manager's `+0xAC`, generator pointer `+0xB4`, bool `+0xB8`) and its info builder is
/// **`0x009FDFE0`** (282 bytes), which logs "Fort tables" and pushes `forts` (the table `0x009FC010`
/// builds), `fort_ptr` = the panel's fort object (`+0x70`; ours the constant 1, PROVISIONAL) and
/// `controlable` (the caller's bool). `0x009FC010` fills
/// every row through **`0x009C9170`**, the construction panel's own row builder (its only caller).
/// `can_build_fort_status` / `build_fort_cost` belong to the army panel's info (`0x009FCFB0`, see
/// [`force_info`]), not to this one, so they are absent here.
///
/// **The panel script (`ui/construction.luac`, CONFIRMED by disassembly -- `Construction.lua`, the
/// function `GenerateFortConstructionPanel` at line 879).** It is a one-slot panel and it says so:
/// - `ResetConstructionPanel(info)` (line 61) reads **`fort_ptr`**, **`controlable`**,
///   **`faction_key`**, `infrastructure` and, if present, `slots`; `g_num_slots` defaults to **1**,
///   which is exactly the fortification's one slot. So `faction_key` and `infrastructure` are needed
///   as well, and `slots` must stay absent.
/// - it loops over `info.forts` and handles only three row types: `BUILDING_ICON_TYPE_BUILT` (1),
///   `BUILDING_ICON_TYPE_CONSTRUCTING` (2) and `BUILDING_ICON_TYPE_UPGRADE` (4). **There is no
///   branch for `BUILDING_ICON_TYPE_CONSTRUCTABLE` (3)**: a constructable row is skipped, leaving
///   `g_building_slot_components` empty, and the two calls it ends with --
///   `SelectPassiveConstructionSlotExclusive(1)` / `SelectExplicitConstructionSlotExclusive(1)` --
///   then trip `assert(g_building_slot_components[1])` at `Construction.lua:624`. So the list must
///   always carry one frame row (1 or 2); an empty fortification slot is shown as a `BUILT` frame
///   with no building key, named `Construction_site` (the module's own string constant).
/// - each frame row it reads: `type`, `name`, `image`, `description`, `long_description`,
///   `building_key`, `slot_key`, `health`, `percent_complete`, `being_repaired`, **`repairable`**,
///   `can_repair`, `can_afford_repair`, `repair_cost`, `turns_to_completion`; each upgrade row also
///   reads **`cost`**, **`affordable`** and **`tech_present`** (as separate keys, not the
///   construction panel's packed `availability` bits).
/// - the frame it creates is a **`BuildingFrame`** template in the construction panel's
///   `ConstructionCardGroup`, and its own script (`template.buildingframe.luac:417`, upvalue
///   `g_fort_ptr`) calls `UpgradeFort(g_fort_ptr)`, `CancelFortRepair(g_fort_ptr)` and
///   `CancelUpgradeFort(g_fort_ptr)` when `fort_ptr` is set, and `BeginConstruction` /
///   `BeginUpgrade` / `CancelConstruction` / `RepairBuilding(building_key, slot_key)` otherwise.
///
/// INFERRED, called out because it is the only guess left here: `description` (the row builder's key
/// that [`construction_info`] has no source for -- ours keeps `short_description`, and here it
/// repeats it, which is what the key name suggests). PROVISIONAL: the script compares
/// `being_repaired` and `dismantling` with the **string** `"true"`, so our booleans do not match and
/// a repair click falls through to the CONSTRUCTING branch -- whether the engine's `SetGlobal`
/// stringifies is UNKNOWN, and the settlement panel's rows have always been booleans here.
pub(super) fn fort_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, region: RegionId) -> mlua::Result<Value> {
    let state = ui.link.state.borrow();
    let m = &state.model;
    let Some(r) = m.world.regions.get(&region) else { return Ok(Value::Nil) };
    let owner_key = m.world.factions.get(&r.owner).map(|f| f.key.clone()).unwrap_or_default();
    let region_key = r.key.clone();
    // `BuildFortPanelRowsTable` `0x009FC010`: the standing level's row, then -- not while building --
    // the next level ([`CampaignModel::map_fort_next_level`], `FindNextFortUpgradeLevel` `0x00B430C0`)
    // as one upgrade row whose `affordable` is `0x0047BA10()`, constant false (CONFIRMED). The fort's
    // own level is PROVISIONAL ([`CampaignModel::map_fort_standing`]); a map fort has no construction or damage in
    // the model, so the standing row is BUILT at full health with no repair.
    let levels = m.map_fort_levels();
    let standing = m.map_fort_standing(region, &levels);
    let next = standing
        .as_ref()
        .and_then(|(index, _)| m.map_fort_next_level(*index, &levels))
        .map(|k| (k.to_owned(), m.construction_cost(region, k), m.rules.buildings.get(k).map_or(1, |b| b.turns.max(1)), m.building_tech_ok(r.owner, k)));
    // A level's upkeep: the construction row builder's `upkeep` key (CONFIRMED). PROVISIONAL which
    // effect that is; ours is `building_maintenance_cost`, the one the model sums for a region's
    // income.
    let upkeep_of = |level: &str| -> i32 {
        m.rules
            .buildings
            .get(level)
            .and_then(|b| b.effects.iter().find(|(k, _)| k == "building_maintenance_cost").map(|(_, v)| *v as i32))
            .unwrap_or(0)
    };
    let upkeeps: HashMap<String, i32> = standing.iter().map(|(_, k)| k).chain(next.iter().map(|n| &n.0)).map(|l| (l.clone(), upkeep_of(l))).collect();
    let controlable = fort_controlable(m, &ui.link.human, region);
    drop(state);

    let entry = |level: &str, kind: i32, cost: i32, turns: u32, percent: f32, affordable: bool, tech: bool| -> mlua::Result<Table> {
        let e = lua.create_table()?;
        let image = ui.building_icon(&owner_key, level);
        let (name, short, long) = building_texts_for(inner, ui, &owner_key, level);
        e.set("type", kind)?;
        e.set("region_key", region_key.as_str())?;
        e.set("slot_key", "")?;
        e.set("building_key", level)?;
        e.set("name", name)?;
        e.set("description", short.as_str())?;
        e.set("short_description", short)?;
        e.set("long_description", long)?;
        e.set("image", image)?;
        e.set("health", 100)?;
        e.set("being_repaired", false)?;
        e.set("repairable", false)?;
        e.set("can_repair", false)?;
        e.set("can_afford_repair", false)?;
        e.set("repair_cost", 0)?;
        e.set("dismantling", false)?;
        e.set("alternate_chain", false)?;
        e.set("availability", i32::from(affordable) | (i32::from(tech) << 1))?;
        // The fort panel's upgrade branch reads these three as separate keys (CONFIRMED).
        e.set("affordable", affordable)?;
        e.set("tech_present", tech)?;
        e.set("upkeep", upkeeps.get(level).copied().unwrap_or(0))?;
        e.set("cost", cost)?;
        e.set("turns_to_completion", turns)?;
        e.set("percent_complete", percent)?;
        e.set("technologies", lua.create_table()?)?;
        Ok(e)
    };
    let forts = lua.create_table()?;
    match standing.as_ref().map(|(_, k)| k) {
        Some(level) => {
            forts.raw_push(entry(level, 1, 0, 0, 100.0, true, true)?)?;
            if let Some((level, cost, turns, tech)) = &next {
                // `affordable` = `0x0047BA10()`: always false in the shipped exe (CONFIRMED).
                forts.raw_push(entry(level, 4, *cost, *turns, 0.0, false, *tech)?)?;
            }
        }
        // No fort chain the owner may hold (made-up data): a BUILT frame with no building, as the
        // panel has no CONSTRUCTABLE branch and asserts on `g_building_slot_components[1]`
        // (Construction.lua:624).
        None => {
            let site = loc(inner, "random_localisation_strings_string_Construction_site").unwrap_or_else(|| "Construction site".into());
            let empty = entry("", 1, 0, 0, 0.0, false, false)?;
            empty.set("name", site)?;
            empty.set("image", "data/ui/buildings/icons/eu_building_placeholder.tga")?;
            forts.raw_push(empty)?;
        }
    }
    let t = lua.create_table()?;
    t.set("forts", forts)?;
    t.set("fort_ptr", 1)?;
    t.set("controlable", controlable)?;
    // Read by the shared ResetConstructionPanel (CONFIRMED, Construction.lua:61); `BuildFortPanelInfoTable`
    // itself pushes only `forts` / `fort_ptr` / `controlable` (CONFIRMED). PROVISIONAL: these two
    // are kept for the script's globals; `infrastructure` set hides the repair / demolish buttons.
    t.set("faction_key", owner_key)?;
    t.set("infrastructure", true)?;
    Ok(Value::Table(t))
}

/// `CampaignUI.ConstructBuildingTree(slot, parent)`'s data (`0x009E1CB0` → `0x009B8830`, CONFIRMED
/// structure): {nodes = {{key, parent (index in nodes, 0 = a root), state, image, tooltip}...},
/// region_key, faction_key, slot_key}, built breadth first:
/// - roots: the levels an empty slot of this type takes (level 0 of a chain allowed in the slot
///   type), each passing the permission test `0x008BE7F0`; children: each node's upgrade levels
///   (`building_upgrades_junction`, record +0x160), each passing it too;
/// - state (`0x009B9120`): "normal" for the slot's standing level and every node above it, else
///   "available" / "unavailable" from `0x009B5A60` (status 0 / 1; 2 hides the node). For a
///   built slot the status asks `0x00DDFD40` (INFERRED: the level could be built there now, cost
///   aside); for an empty slot a level is available when it is one of the slot's roots;
/// - the node's picture is its culture icon ("{<key>:1}<path>", CONFIRMED string pieces
///   0x0132E1B8 / 0x0136F224: the node's component id is the level key), its tooltip the name and
///   the random string `right_click_info` (0xFC) joined by "\n" (INFERRED order).
///
/// PROVISIONAL: the level record's +0x5C flag (hides a node outside some region, `0x00A8B5A0`)
/// is not known and not applied.
fn building_tree(lua: &Lua, inner: &Inner, ui: &CampaignUi, slot: &Value) -> mlua::Result<Value> {
    let m = ui.model();
    let Some((region, index)) = slot_from_entity(ui, slot) else { return Ok(Value::Nil) };
    let Some(r) = m.world.regions.get(&region) else { return Ok(Value::Nil) };
    let Some((slot_type, standing)) = r.construction_slot(index) else { return Ok(Value::Nil) };
    let (slot_type, standing) = (slot_type.to_owned(), standing.map(|b| b.level_key.clone()));
    let Some(slot_key) = slot_key_of(r, index) else { return Ok(Value::Nil) };
    let owner = r.owner;
    let owner_key = m.world.factions.get(&owner).map(|f| f.key.clone()).unwrap_or_default();
    let permitted = |k: &str| {
        m.building_permitted(owner, k)
            && m.rules.buildings.get(k).is_some_and(|b| m.rules.chain_slots.get(&b.chain).is_some_and(|t| t.contains(&slot_type)))
    };
    let roots: Vec<String> = m.rules.buildings.iter().filter(|(k, b)| b.level == 0 && permitted(k)).map(|(k, _)| k.clone()).collect();
    // (level, parent index + 1)
    let mut nodes: Vec<(String, usize)> = roots.iter().map(|k| (k.clone(), 0)).collect();
    let mut i = 0;
    while i < nodes.len() {
        let ups = m.rules.buildings.get(&nodes[i].0).map(|b| b.upgrades_to.clone()).unwrap_or_default();
        for u in ups {
            // A chain never loops, but guard against bad data.
            if permitted(&u) && !nodes.iter().any(|(k, _)| *k == u) {
                nodes.push((u, i + 1));
            }
        }
        i += 1;
    }
    // Available: what the slot can start now by the model's one option rule (researched levels
    // only, as `can_build` checks), less the scripts' restricted levels.
    let startable: Vec<String> = m.construction_options_in(region, index, None).into_iter().filter(|o| o.tech).map(|o| o.level_key).collect();
    let available = |k: &str| startable.iter().any(|s| s == k);
    // The standing level and its ancestors are drawn "normal".
    let mut normal = vec![false; nodes.len()];
    if let Some(s) = &standing
        && let Some(mut at) = nodes.iter().position(|(k, _)| k == s)
    {
        loop {
            normal[at] = true;
            match nodes[at].1 {
                0 => break,
                p => at = p - 1,
            }
        }
    }
    let states: Vec<&str> =
        nodes.iter().enumerate().map(|(j, (k, _))| if normal[j] { "normal" } else if available(k) { "available" } else { "unavailable" }).collect();
    let region_key = r.key.clone();
    drop(m);
    let right_click = loc(inner, "random_localisation_strings_string_right_click_info").unwrap_or_default();
    let list = lua.create_table()?;
    for (j, (key, parent)) in nodes.iter().enumerate() {
        let e = lua.create_table()?;
        e.set("key", key.as_str())?;
        e.set("parent", *parent)?;
        e.set("state", states[j])?;
        e.set("image", ui.building_icon(&owner_key, key))?;
        let name = building_texts(inner, ui, key).0;
        e.set("tooltip", if right_click.is_empty() { name } else { format!("{name}\n{right_click}") })?;
        list.raw_push(e)?;
    }
    let t = lua.create_table()?;
    t.set("nodes", list)?;
    t.set("region_key", region_key)?;
    t.set("faction_key", owner_key)?;
    t.set("slot_key", slot_key)?;
    Ok(Value::Table(t))
}

/// `CampaignUI.BuildingBrowserDetails(region)` (0x009DDD40 → 0x009B5AF0, CONFIRMED keys) →
/// `{slots = {entry...}, region = address, region_name}` for the region given, else the selected
/// settlement when the player owns it, else the player's capital. One entry per slot that has a
/// building or could get one (the region's slot list, then its road), in the region's order:
/// `chains` ({key, tooltip} per chain the slot type allows), `slot`, `faction_key`, `slot_key`,
/// `region_key`, `type`, `location`, and either `level`, `max_level`, `building_key`, `image`,
/// `name`, `entry_tooltip1`, `entry_tooltip2` or, for an empty slot, `level` = `max_level` = -1,
/// `building_key` = "empty", `image` = "", `name`, `slot_type`, the two tooltips.
/// `type` (CONFIRMED order of the exe's tests; labels from building_browser.lua): 1 capital (a
/// settlement slot of the faction capital), 2 region capital (any other settlement slot), 3 town,
/// 4 port, 8 resource, 5 farm, 6 road, 7 fortification; other slots are left out.
/// Tooltips (CONFIRMED pieces, random loc strings 0xFC right_click_info, 0xFD
/// left_click_view_tree, 0xFE left_click_return_list, joined with "\n"): name + view tree + right
/// click info / name + return to list + right click info; empty slots: "Construction site" + view
/// tree / + return to list. `max_level`: the highest level of the chain (the exe counts only
/// levels the faction may build: PROVISIONAL). `location`: the settlement name for types 1 and 2,
/// else the slot's loc name (`campaign_map_slots_onscreen_<key>` /
/// `campaign_map_towns_and_ports_onscreen_name_<key>`, CONFIRMED keys). PROVISIONAL: a slot is
/// listed when it has a building or a level-0 building of an allowed chain can be built there;
/// chain tooltips are empty (the exe's chain string +0x24 is UNKNOWN); the road's slot key is ours
/// (`road:<region>`).
fn building_browser_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, arg: Option<RegionId>) -> mlua::Result<Value> {
    let m = ui.model();
    let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
    let selected = match ui.selection.get() {
        CampaignSelection::Settlement(r) if human.is_some() && m.world.regions.get(&r).map(|x| x.owner) == human => Some(r),
        _ => None,
    };
    let region = arg.or(selected).or_else(|| human.and_then(|h| m.world.capital(h)));
    let out = lua.create_table()?;
    let slots = lua.create_table()?;
    out.set("slots", slots.clone())?;
    let Some(r) = region.and_then(|r| m.world.regions.get(&r)) else { return Ok(Value::Table(out)) };
    let owner_key = m.world.factions.get(&r.owner).map(|f| f.key.clone()).unwrap_or_default();
    let capital = m.world.is_capital(r.id);
    let word = |k: &str| loc(inner, &format!("random_localisation_strings_string_{k}")).unwrap_or_default();
    let (view_tree, back, info) = (word("left_click_view_tree"), word("left_click_return_list"), word("right_click_info"));
    let site = word("Construction_site");
    let slot_types = ui.slot_types(inner);
    // The level-0 candidates per slot type, once for all the region's empty slots (as the AI's snapshot).
    let new_levels = m.new_levels_by_slot_type();
    // (slot, key, slot type, building): the region's slots, then the walls, then the road -- the
    // exe's order (`0x009B5AF0` walks the slot manager's list, then `+0x24` fortification, then
    // `+0x20` road; CONFIRMED).
    let list: Vec<(SlotRef, String, String, Option<String>)> = (0..r.slots.len())
        .map(SlotRef::Slot)
        .chain([SlotRef::Walls, SlotRef::Road])
        .filter_map(|slot| {
            let (slot_type, b) = r.construction_slot(slot)?;
            Some((slot, slot_key_of(r, slot)?, slot_type.to_owned(), b.map(|b| b.level_key.clone())))
        })
        .collect();
    let mut n = 0;
    for (slot, key, slot_type, building) in list {
        let kind = if key.starts_with("settlement:") && slot_type != "settlement_road" && slot_type != "settlement_fortification" {
            if capital { 1 } else { 2 }
        } else if slot_type == "settlement_road" {
            6
        } else if slot_type == "settlement_fortification" {
            7
        } else {
            match slot_types.get(&slot_type) {
                Some(t) if t.town => 3,
                Some(t) if t.port => 4,
                Some(t) if t.resource => 8,
                Some(t) if t.farm => 5,
                _ => continue,
            }
        };
        let chains: Vec<&String> = m.rules.chain_slots.iter().filter(|(_, types)| types.contains(&slot_type)).map(|(c, _)| c).collect();
        // An empty slot is listed when a level can be started there (the model's one option
        // rule with the scripts' restricted levels, researched levels only, as `can_build` checks).
        if building.is_none() && !m.construction_options_in(r.id, slot, Some(&new_levels)).iter().any(|o| o.tech) {
            continue;
        }
        let e = lua.create_table()?;
        let ct = lua.create_table()?;
        for (j, c) in chains.iter().enumerate() {
            let ce = lua.create_table()?;
            ce.set("key", c.as_str())?;
            ce.set("tooltip", "")?;
            ct.set(j + 1, ce)?;
        }
        e.set("chains", ct)?;
        e.set("slot", slot_value(ui, r.id, slot))?;
        e.set("faction_key", owner_key.as_str())?;
        e.set("slot_key", key.as_str())?;
        e.set("region_key", r.key.as_str())?;
        e.set("type", kind)?;
        let location = match kind {
            1 | 2 => ui.settlement_name(inner, r.id),
            _ => loc(inner, &format!("campaign_map_slots_onscreen_{key}"))
                .or_else(|| loc(inner, &format!("campaign_map_towns_and_ports_onscreen_name_{key}")))
                .unwrap_or_else(|| ui.settlement_name(inner, r.id)),
        };
        e.set("location", location)?;
        match &building {
            Some(level_key) => {
                let (level, max) = m.rules.chain_levels(level_key).unwrap_or((0, 0));
                let (name, _) = building_texts(inner, ui, level_key);
                e.set("level", level)?;
                e.set("max_level", max)?;
                e.set("building_key", level_key.as_str())?;
                e.set("image", ui.building_icon(&owner_key, level_key))?;
                e.set("entry_tooltip1", format!("{name}\n{view_tree}\n{info}"))?;
                e.set("entry_tooltip2", format!("{name}\n{back}\n{info}"))?;
                e.set("name", name)?;
            }
            None => {
                let name = match kind {
                    3 => word("town"),
                    4 => word("port"),
                    5 => word("farm"),
                    6 => word("road"),
                    7 => word("fortification"),
                    8 => loc(inner, "advice_levels_advice_item_title_-332744214").unwrap_or_default(),
                    _ => String::new(),
                };
                e.set("level", -1)?;
                e.set("max_level", -1)?;
                e.set("building_key", "empty")?;
                e.set("image", "")?;
                e.set("name", name)?;
                e.set("slot_type", slot_type.as_str())?;
                e.set("entry_tooltip1", format!("{site}\n{view_tree}"))?;
                e.set("entry_tooltip2", format!("{site}\n{back}"))?;
            }
        }
        n += 1;
        slots.set(n, e)?;
    }
    out.set("region", region_value(ui, r.id))?;
    out.set("region_name", region_name(&inner.loc, &r.key))?;
    Ok(Value::Table(out))
}


/// The region and slot of a construction panel `slot_key`: a `REGION_SLOT` key, or the walls' /
/// road's own key ([`fortification_slot_key`], [`road_slot_key`]).
pub(super) fn slot_by_key(ui: &CampaignUi, key: &str) -> Option<(RegionId, SlotRef)> {
    let m = ui.model();
    // The walls' / road's own keys ([`fortification_slot_key`], [`road_slot_key`]) name the region.
    let walls = key.strip_prefix(WALLS_KEY_PREFIX);
    let road = key.strip_prefix(ROAD_KEY_PREFIX);
    m.world.regions.values().find_map(|r| {
        if walls == Some(r.key.as_str()) {
            Some((r.id, SlotRef::Walls))
        } else if road == Some(r.key.as_str()) {
            Some((r.id, SlotRef::Road))
        } else {
            r.slots.iter().position(|s| s.key == key).map(|i| (r.id, SlotRef::Slot(i)))
        }
    })
}

/// A building level's on-screen name and short description for the owner's culture (see
/// `construction_info`; any culture's variant when the faction's own is missing).
pub(super) fn building_texts(inner: &Inner, ui: &CampaignUi, level: &str) -> (String, String) {
    let (name, short, _) = building_texts_long(inner, ui, level);
    (name, short)
}

/// [`building_texts`] plus the long description (`building_description_texts_long_description_*`).
fn building_texts_long(inner: &Inner, ui: &CampaignUi, level: &str) -> (String, String, String) {
    building_texts_for(inner, ui, &ui.link.human, level)
}

/// A building level's name, short and long description for `faction_key`'s culture variant (any
/// culture's variant when that one is missing). `FortDetails` uses the fort owner's: the exe's
/// details filler `0x008B1480` runs on the fort's faction (`+0x210`, INFERRED owner), for the
/// texts and the icon alike.
fn building_texts_for(inner: &Inner, ui: &CampaignUi, faction_key: &str, level: &str) -> (String, String, String) {
    let (c, desc) = ui.building_variant(faction_key, level).map_or_else(|| (String::new(), level.to_owned()), |(c, _, d)| (c, d));
    let name = loc(inner, &format!("building_culture_variants_name_{level}{c}")).unwrap_or_else(|| level.to_owned());
    let short = loc(inner, &format!("building_description_texts_short_description_{desc}")).unwrap_or_default();
    let long = loc(inner, &format!("building_description_texts_long_description_{desc}")).unwrap_or_default();
    (name, short, long)
}

pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // BeginConstruction(building_key, slot_key) / BeginUpgrade(building_key, slot_key): build in an
    // empty slot / upgrade the standing building (CONFIRMED calls in template.BuildingFrame.lua;
    // both become the model's ConstructBuilding).
    // `template.buildingframe.luac:417` calls `BeginConstruction(building_key, slot_key)` and
    // `BeginUpgrade(building_key, slot_key)` (CONFIRMED); both become the model's ConstructBuilding.
    for name in ["BeginConstruction", "BeginUpgrade"] {
        let ui2 = ui.clone();
        t.raw_set(name, lua.create_function(move |_, a: Variadic<Value>| {
            let level = a.iter().find_map(value_str);
            if let (Some(level_key), Some((region, slot))) = (level, slot_of_args(&ui2, &a)) {
                ui2.push(CampaignRequest::Command(CampaignCommand::ConstructBuilding { region, slot, level_key }));
            }
            Ok(())
        })?)?;
    }
    // CancelConstruction(building_key, slot_key): cancel the slot's construction or repair item
    // (CONFIRMED call in template.buildingframe.luac:417, a click on a building under construction
    // or on one marked "being repaired"; handler 0x009E0F10 → 0x009B7AA0 queues the slot's cancel).
    f!("CancelConstruction", |_l, inner, ui, a: Variadic<Value>| {
        if let Some((region, slot)) = slot_of_args(&ui, &a) {
            ui.push(CampaignRequest::Command(CampaignCommand::CancelConstruction { region, slot }));
        }
        Ok(())
    });
    // RepairBuilding(building_key, slot_key): the repair button on a damaged building
    // (Construction.lua's RepairCurrentSelection, CONFIRMED call; handler 0x009F1300) → the
    // model's RepairBuilding.
    f!("RepairBuilding", |_l, inner, ui, a: Variadic<Value>| {
        // The model refuses a road repair (`CampaignModel::can_repair`, PROVISIONAL).
        if let Some((region, slot)) = slot_of_args(&ui, &a) {
            ui.push(CampaignRequest::Command(CampaignCommand::RepairBuilding { region, slot }));
        }
        Ok(())
    });
    // CanDemolishBuilding(slot_key) -- CONFIRMED shape, from `Construction.lua:366`
    // (`construction.DemolishCurrentSelection`: it reads the frame's `slot_key` and `building_key`
    // globals with `UIComponent:GlobalExists` -- a getter, CONFIRMED in ui_prelude.lua -- and calls
    // `CampaignUI.CanDemolishBuilding(slot_key)`, comparing the answer with `true`). Handler
    // `0x009E0930` -> `0x009B7920` = slot resolve `0x009BB6E0` && not the `settlement_road` slot
    // (`0x00A8B970`) && slot free of pending work (`0x00A91FC0`, INFERRED) && a selected item
    // present -- CONFIRMED shape, 0-G round N+2. -> `CampaignModel::can_demolish`, which refuses
    // the road as the exe does.
    f!("CanDemolishBuilding", |_l, inner, ui, a: Variadic<Value>| {
        Ok(slot_of_args(&ui, &a).is_some_and(|(r, s)| ui.model().can_demolish(r, s)))
    });
    // DemolishBuilding(building_key, slot_key) -- CONFIRMED, from two call sites with different
    // argument lists: `Construction.lua:366` passes the frame's two globals, and
    // `ui/campaign ui/building_information_scripts/building_information.luac:36` passes only the
    // slot key (it picks between `DemolishBuilding` and `DemolishFort` on `g_details.slot_key`).
    // Handler `0x009E2380` -> `0x009B9590` -> queue id `0x84`, CONFIRMED, 0-G round N+2 ->
    // `CampaignCommand::DemolishBuilding` on the slot the key names.
    f!("DemolishBuilding", |_l, inner, ui, a: Variadic<Value>| {
        if let Some((region, slot)) = slot_of_args(&ui, &a) {
            ui.push(CampaignRequest::Command(CampaignCommand::DemolishBuilding { region, slot }));
        }
        Ok(())
    });
    // The map fort's actions. CONFIRMED names and, from the panel script's own bytecode, their
    // argument lists: `template.buildingframe.luac:417` (the frame's select handler, upvalue
    // `g_fort_ptr`) and `Construction.lua:366` / `:383` call
    // - `CampaignUI.UpgradeFort(g_fort_ptr)`      (frame, type UPGRADE)
    // - `CampaignUI.CancelUpgradeFort(g_fort_ptr)` (frame, type CONSTRUCTING)
    // - `CampaignUI.CancelFortRepair(g_fort_ptr)`  (frame, `being_repaired`)
    // - `CampaignUI.DemolishFort(g_fort_ptr)`      (Construction.lua:366)
    // - `CampaignUI.RepairFort(g_fort_ptr)`        (Construction.lua:383)
    // only when `fort_ptr` is set, i.e. on the map fort's own panel ([`fort_info`]); and
    // `CampaignUI.BuildFort(g_general)` from `ui/army.luac`'s `g_button_fort` (`Army.lua:382`).
    // The settlement walls are NOT built through these: they are the settlement construction
    // panel's last slot card ([`construction_info`]), built by `BeginConstruction` & co.
    // (CONFIRMED at runtime and in game, 2026-10-07; UI_FIDELITY.md fort section).
    //
    // `UpgradeFort` is `CCQ_UPGRADE_FORT` (registered by `0x00423480`), executed by `0x009389A0` ->
    // `ProcessFortUpgradeConstruction` `0x00B4E620`, whose first step asks `0x0047BA10`, a folded
    // folded function that always returns false, and returns: the shipped exe never upgrades a map fort
    // (CONFIRMED by the bytes). So ours sends nothing either.
    //
    // `BuildFort` is `CCQ_CHARACTER_BUILD_FORT` (`0x0041FF00`), executed by `0x00932670` ->
    // `0x009200F0` on the army's general: after the character's action check it asks `0x00462CB0`,
    // a constant-false stub, so it never builds and only posts the failure event (`0xCE`; success
    // would be `0xD6`). CONFIRMED by the bytes: the field fort is not buildable in the shipped game,
    // so ours does nothing too, and `force_info` reports the button as `FBS_UNABLE` (INFERRED in
    // game). PLACEHOLDER: the failure event (advisor / message) is not posted.
    //
    // RepairFort / CancelUpgradeFort / CancelFortRepair / DemolishFort: CONFIRMED (own Ghidra copy)
    // that each handler -- `0x009F1370`, `0x009E1240`, `0x009E0F80`, `0x009E23D0` -- reads one fort
    // userdata argument and sends a command carrying only it. PLACEHOLDER: no-ops -- the model's map
    // fort (`World::forts`) has no building, health or queue of its own, and no shipped startpos or
    // save holds a fort (`FORT_ARRAY` is empty in all of them), nor can one be built (above).
    f!("BuildFort", |_l, inner, ui, _general: Variadic<Value>| Ok(()));
    for name in ["UpgradeFort", "RepairFort", "CancelUpgradeFort", "CancelFortRepair", "DemolishFort"] {
        t.raw_set(name, lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
    }
    // FortDetails(fort_ptr, building_key) / FortEffects(...): CONFIRMED call (2026-10-07,
    // `template.buildingframe.luac` proto at line 372, the frame's tooltip):
    // `FortDetails(g_fort_ptr, building_key)` / `FortEffects(g_fort_ptr, building_key)` -- the
    // card's own `building_key` -- handed to the tooltip's `InitialiseBuilding`.
    // CONFIRMED body (handler `0x009E49D0` -> `0x009B2AA0` -> `0x009AA250`, own Ghidra copy): the
    // second argument, when given and not nil, is a key; an empty key means the fort's standing
    // level (`0x00B42E40`), any other key is looked up in **all** of `building_levels` (no
    // fortification check). The table always carries the same eight keys, filled from that level:
    // `Key`, `Name`, `ShortDescription`, `LongDescription`, `IconFilename`, `InfoFilename`, `Level`,
    // `MaxLevel` -- no `region_key` / `slot_key`. The fort is the map fort (`fFort`, see
    // [`fort_info`]; its standing level is [`CampaignModel::map_fort_standing`]). With no level (ours: no fort
    // chain the owner may hold, which the exe's always-built fort entity never lacks; or an unknown
    // key, which the exe logs as "not a valid key"; or no fort addressed, e.g. a settlement selected)
    // the details stay default-constructed: INFERRED empty strings and 0, which
    // the tooltip shows without the nil-description error (`TechTreeItem_Tooltip.lua:75`).
    // INFERRED: `IconFilename` / `InfoFilename` are the culture variant's icon under
    // `data/ui/buildings/icons/` and `.../info/` (the exe's two path prefixes). A region or fort
    // address argument names the fort; otherwise the selection's ([`fort_region`]).
    f!("FortDetails", |lua, inner, ui, args: Variadic<Value>| {
        let region = fort_region(&ui, &args);
        let m = ui.model();
        // No fort addressed (no selection, no fort argument): still the eight default keys, never
        // nil -- the tooltip indexes the table unconditionally.
        let r = region.and_then(|r| m.world.regions.get(&r));
        let owner = r.map_or_else(|| m.faction_by_key(&ui.link.human).map(|f| f.id), |r| Some(r.owner));
        let owner_key = owner.and_then(|o| m.world.factions.get(&o)).map(|f| f.key.clone()).unwrap_or_default();
        let shown = match args.get(1).and_then(value_str).filter(|k| !k.is_empty()) {
            None => region.and_then(|reg| m.map_fort_standing(reg, &m.map_fort_levels()).map(|(_, k)| k)),
            Some(k) if m.rules.buildings.contains_key(&k) => Some(k),
            Some(k) => {
                log(&inner, format!("WARN CampaignUI.FortDetails: '{k}' is not a valid building level key"));
                None
            }
        };
        let levels = shown.as_deref().and_then(|k| m.rules.chain_levels(k));
        drop(m);
        let (key, (name, short, long), (icon, info), (level, max)) = match &shown {
            Some(key) => {
                // The info picture only for a real variant: the placeholder icon has none.
                let files = ui.building_variant(&owner_key, key).map_or_else(
                    || (ui.building_icon(&owner_key, key), String::new()),
                    |(_, stem, _)| {
                        let stem = stem.to_ascii_lowercase();
                        (format!("data/ui/buildings/icons/{stem}.tga"), format!("data/ui/buildings/info/{stem}.tga"))
                    },
                );
                (key.clone(), building_texts_for(&inner, &ui, &owner_key, key), files, levels.unwrap_or_default())
            }
            None => (String::new(), Default::default(), (String::new(), String::new()), (0, 0)),
        };
        let t = lua.create_table()?;
        t.set("Key", key)?;
        t.set("Name", name)?;
        t.set("ShortDescription", short)?;
        t.set("LongDescription", long)?;
        t.set("InfoFilename", info)?;
        t.set("IconFilename", icon)?;
        t.set("Level", level)?;
        t.set("MaxLevel", max)?;
        Ok(Value::Table(t))
    });
    // PROVISIONAL: empty, as `BuildingEffects` / `TechEffects` above (the effect texts are not built).
    t.raw_set("FortEffects", lua.create_function(|lua, _: Variadic<Value>| lua.create_table())?)?;
    // InformLootingSelection("loot" | "occupy" | "liberate"): the capture screen's answer
    // (settlement_captured.lua's LootSettlement / OccupySettlement / LiberateSettlement, CONFIRMED
    // strings) → the model's ChooseCapture.
    f!("InformLootingSelection", |_l, inner, ui, choice: Option<String>| {
        use ntw_sim::campaign::CaptureChoice;
        let c = match choice.as_deref() {
            Some("loot") => Some(CaptureChoice::Loot),
            Some("occupy") => Some(CaptureChoice::Occupy),
            Some("liberate") => Some(CaptureChoice::Liberate),
            _ => None,
        };
        if let Some(choice) = c {
            ui.push(CampaignRequest::Command(CampaignCommand::ChooseCapture { choice }));
        }
        Ok(())
    });
    // TriggerBuildingCardSelectedEvent(building_key) (0x009FA0D0): posts a UI event with the key
    // (event vtable 0x0136C30C, INFERRED for the advisor). No listener in our game: a no-op.
    f!("TriggerBuildingCardSelectedEvent", |_l, inner, ui, _a: Variadic<Value>| Ok(()));
    // BuildingDetails(building_key, region_key, faction_key, slot_key) → the building tooltip's
    // details: {Key, Name, ShortDescription, Level, MaxLevel, ...} (CONFIRMED names read by
    // template.TechTreeItem_Tooltip.lua; "initialises a table of details about the building",
    // CONFIRMED description). Level and MaxLevel are 0-based: the chain level index and the highest
    // index in the chain, as the browser's `level` / `max_level`. CONFIRMED from the tooltip's pip
    // function (`TechTreeItem_Tooltip.lua:172`, lines 177-184): it shows pips 1..MaxLevel+1 as
    // "empty" and 1..Level+1 as "filled", finding each `Level Pip <n>`; 1-based values ran one pip
    // past the template's last and errored (`pip` nil at line 179). The binding of the two
    // arguments to Level / MaxLevel is INFERRED (the caller was not dumped).
    // BuildingEffects / TechEffects → effect lists; PROVISIONAL: empty (the effect texts are not
    // built yet).
    f!("BuildingDetails", |lua, inner, ui, (key, region_key, _f, slot_key): (Option<String>, Value, Value, Value)| {
        let Some(key) = key else { return Ok(Value::Nil) };
        let (level, max) = ui.model().rules.chain_levels(&key).unwrap_or((0, 0));
        let t = lua.create_table()?;
        let (name, desc) = building_texts(&inner, &ui, &key);
        t.set("Key", key.as_str())?;
        t.set("Name", name)?;
        t.set("ShortDescription", desc)?;
        t.set("Level", level)?;
        t.set("MaxLevel", max)?;
        t.set("region_key", region_key)?;
        t.set("slot_key", slot_key)?;
        Ok(Value::Table(t))
    });
    // BuildingBrowserDetails(region) → see `building_browser_details`.
    // __BuildingTreeNodes(slot): the tree view data (see `building_tree`); campaign_prelude.lua's
    // ConstructBuildingTree builds the components from it.
    f!("__BuildingTreeNodes", |lua, inner, ui, slot: Value| building_tree(lua, &inner, &ui, &slot));
    f!("BuildingBrowserDetails", |lua, inner, ui, region: Value| {
        let r = entity_of(&region, TAG_REGION).map(|r| RegionId(r as u32));
        building_browser_details(lua, &inner, &ui, r)
    });
    for name in ["BuildingEffects", "TechEffects"] {
        t.raw_set(name, lua.create_function(|lua, _: Variadic<Value>| lua.create_table())?)?;
    }
    Ok(())
}
