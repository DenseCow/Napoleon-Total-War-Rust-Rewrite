//! `FrontEnd.*` functions of the custom battle's army page (sp_battle3): the units a faction can
//! field, the default ("balanced") armies and the experience costs. Evidence:
//! `analysis/frontend/FRONTEND_PAGES.md` "Custom battle".
//!
//! - Unit details (`0x00E03410` → `0x00E02260`, CONFIRMED field names): Key, Class, ClassID, Name,
//!   Description, LongDescription, RecruitTime, RecruitCost, UpkeepCost, MPCost, MPCategory, Icon,
//!   InfoPic, Experience, Cap, Command, CommanderClass, Men, DisplayNumber, IsNaval,
//!   IsArtillery, IsFixedArtillery, UnitLimit, SiegeUnit.
//! - `RecruitableUnits(faction, is_naval, era, mask, [funds], has_general, has_admiral)`
//!   (`0x00475B00`; the exe pops its arguments from the top, so the order is CONFIRMED from the
//!   calls in unit_list.lua): the faction's units whose MP category bit is in the mask (art 1,
//!   cav 2, inf 4, ships 7) and that are land / sea as asked. Each entry is the unit details plus
//!   Faction, LateEra, MPCost (early or late cost), XpAdjustedCost, PositionSpecified = false,
//!   Experience = 0, Affordable (cost within the funds), CommanderClass, CommanderConflict, and
//!   IsGeneral + Capabilities {FiringDrill = 0} on land, IsAdmiral + ShipName = "" at sea.
//! - `RetrieveArmyPresets(faction, type, size, era, scale)` (`0x004765F0` and the filler
//!   `0x0045CB50`): `{balanced = setup}`. The setup is `{Faction, Units, Ships, Cost,
//!   CategoryMask = 63}`, with the array part as the category limits `{Max, Actual, Tag}`.
//!   - The faction's preset is found through `battle_type_setup_limits` (type, "balanced", size,
//!     era) → `battle_type_faction_presets` → `battle_type_unit_to_faction_presets`.
//!   - The units are cut to the unit limit: 20 on land, by unit scale at sea (6 / 8 / 10 / 20).
//!   - Then their experience is raised one step at a time, round the units, while the total
//!     stays under the army size's funds (`ArmyFundsForSize`), up to experience 9.
//!   - Without a preset: Units / Ships empty, Cost 0.
//! - `XpAdjustedCost` (`0x00ED49A0`, CONFIRMED): cost × the experience row's multiplier + its
//!   fixed cost (`unit_stats_land_experience_bonuses` / `_naval_`), rounded.
//! - `MPExperienceTables(is_naval)` (`0x00472330`): `{[xp + 1] = {FixedCost, Multiplier}}` of the
//!   same tables (shape INFERRED from army_setup_card.lua's reads).
//!
//! PROVISIONAL:
//! - the faction's unit list is the `uniforms` table (the exe reads the faction record's unit map);
//! - the uniform pictures (`uniforms_table` InfoPic) are not used for Icon / InfoPic;
//! - Command, the extra game-mode filter (`+0x94`) and the preset sort before the cut are not
//!   reproduced;
//! - the era index 0 / 1 / 2 = early / middle / late (INFERRED: the pages pass 2 by default;
//!   the exe tests the unit flags +0x70 / +0x71 / +0x72 by it, CONFIRMED).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{Lua, Table, Value};
use ntw_formats::db::DbValue;
use ntw_formats::db_folder::{RawTable, tables};

use super::army_file::{ArmySetupFile, BattlePrefsFile, Card, Limit};
use super::host::Inner;

/// The unit classes in `unit_class` table order (`ClassID`; general 12, naval_admiral 23,
/// CONFIRMED by the exe's tests `+0x20 == 0xC / 0x17`).
fn class_id(class: &str) -> i32 {
    ntw_campaign::regiments::UNIT_CLASSES.iter().position(|c| *c == class).map_or(-1, |i| i as i32)
}

/// MP category index (`MPCategory`, record +0x6C; order INFERRED: artillery, cavalry, infantry,
/// then the naval ones).
fn mp_category(cat: &str) -> i32 {
    ["mp_artillery", "mp_cavalry", "mp_infantry", "mp_naval_line_of_battle", "mp_naval_small_ship"].iter().position(|c| *c == cat).map_or(0, |i| i as i32)
}

/// `battle_type_setup_limits` key: (battle type, composition, size, era).
type LimitKey = (String, String, String, String);
/// The `ship_names` rows (id, group, name, faction) and each faction's names group.
type ShipNames = (Vec<[String; 4]>, BTreeMap<String, String>);
/// A default army: (unit, experience) list, total cost, category limits [inf, cav, art].
pub type BalancedArmy = (Vec<(String, i32)>, i32, [i32; 3]);

/// The DB data of the army page, read once from the install.
#[derive(Default)]
pub struct ArmyData {
    units: BTreeMap<String, ntw_data::UnitRecord>,
    men: BTreeMap<String, i32>,
    /// faction → its units (`uniforms`).
    faction_units: BTreeMap<String, Vec<String>>,
    /// `unit_stats_land_experience_bonuses` rows by rank (0..9).
    xp_land: BTreeMap<u8, ntw_data::UnitStatsLandExperienceBonuses>,
    /// `unit_stats_naval_experience_bonuses` rows by rank.
    xp_naval: BTreeMap<u8, ntw_data::UnitStatsNavalExperienceBonuses>,
    /// (type, composition, size, era) → (limits [inf, cav, art], id).
    limits: BTreeMap<LimitKey, ([i32; 3], i32)>,
    /// (faction, setup id) → preset id.
    faction_presets: BTreeMap<(String, i32), i32>,
    /// preset id → (unit key, experience) in table order.
    preset_units: BTreeMap<i32, Vec<(String, i32)>>,
}

fn table(inner: &Inner, table: &RawTable) -> Vec<Vec<DbValue>> {
    inner.source.table_rows(table).unwrap_or_default()
}

fn st(v: &DbValue) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

impl ArmyData {
    /// Reads the tables (missing ones stay empty).
    pub(super) fn load(inner: &Inner) -> ArmyData {
        let mut d = ArmyData::default();
        if let Some(t) = inner.source.typed_table::<ntw_data::UnitRecord>() {
            d.units = t.rows().iter().map(|u| (u.key.clone(), u.clone())).collect();
        }
        if let Some(t) = inner.source.typed_table::<ntw_data::UnitStatsLand>() {
            d.men = t.rows().iter().map(|u| (u.key.clone(), u.num_men)).collect();
        }
        for r in table(inner, &tables::UNIFORMS) {
            d.faction_units.entry(st(&r[1])).or_default().push(st(&r[3]));
        }
        // The experience rows by rank; a row whose key is not a rank cannot be looked up as one.
        if let Some(t) = inner.source.typed_table::<ntw_data::UnitStatsLandExperienceBonuses>() {
            d.xp_land = t.rows().iter().filter_map(|r| Some((r.rank.parse().ok()?, r.clone()))).collect();
        }
        if let Some(t) = inner.source.typed_table::<ntw_data::UnitStatsNavalExperienceBonuses>() {
            d.xp_naval = t.rows().iter().filter_map(|r| Some((r.rank.parse().ok()?, r.clone()))).collect();
        }
        for r in table(inner, &tables::BATTLE_TYPE_SETUP_LIMITS) {
            let n = |i: usize| r[i].as_i32().unwrap_or(0);
            d.limits.insert((st(&r[0]), st(&r[1]), st(&r[2]), st(&r[3])), ([n(4), n(5), n(6)], n(10)));
        }
        for r in table(inner, &tables::BATTLE_TYPE_FACTION_PRESETS) {
            d.faction_presets.insert((st(&r[0]), r[1].as_i32().unwrap_or(0)), r[2].as_i32().unwrap_or(0));
        }
        for r in table(inner, &tables::BATTLE_TYPE_UNIT_TO_FACTION_PRESETS) {
            d.preset_units.entry(r[1].as_i32().unwrap_or(0)).or_default().push((st(&r[2]), r[3].as_i32().unwrap_or(0)));
        }
        d
    }

    fn is_naval(&self, key: &str) -> bool {
        self.units.get(key).is_some_and(|u| u.category.starts_with("naval"))
    }

    /// The unit's MP cost in an era (`+0x2C` early, `+0x30` later).
    pub fn mp_cost(&self, key: &str, late: bool) -> i32 {
        self.units.get(key).map_or(0, |u| if late { u.secondary_cost } else { u.recruitment_cost })
    }

    /// `XpAdjustedCost` (see the module docs): the experience row's `adjusted_cost`, the one copy of the
    /// `0x00ED49A0` rule (`ntw_data::GameDatabase::experience_adjusted_cost` uses it too); a rank with no
    /// row leaves the cost alone.
    pub fn xp_cost(&self, key: &str, xp: i32, late: bool) -> i32 {
        let cost = self.mp_cost(key, late);
        let rank = xp.clamp(0, 9) as u8;
        if self.is_naval(key) {
            self.xp_naval.get(&rank).map_or(cost, |r| r.adjusted_cost(cost))
        } else {
            self.xp_land.get(&rank).map_or(cost, |r| r.adjusted_cost(cost))
        }
    }
}

/// The unit details table (see the module docs).
#[allow(clippy::too_many_arguments)]
fn unit_details(lua: &Lua, inner: &Inner, d: &ArmyData, faction: &str, key: &str, xp: i32, late: bool, scale: f32) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let u = d.units.get(key);
    let loc = |k: String| inner.loc.get(&k).map(str::to_owned);
    t.set("Key", key)?;
    t.set("Class", u.map(|u| u.unit_class.clone()).unwrap_or_default())?;
    t.set("ClassID", u.map_or(-1, |u| class_id(&u.unit_class)))?;
    t.set("Name", loc(format!("units_on_screen_name_{key}")).unwrap_or_else(|| u.map(|u| u.dev_name.clone()).unwrap_or_default()))?;
    t.set("Description", loc(format!("unit_description_texts_description_text_{key}")).unwrap_or_default())?;
    t.set("LongDescription", loc(format!("unit_description_texts_long_description_text_{key}")).unwrap_or_default())?;
    t.set("RecruitTime", u.map_or(0, |u| u.unknown_38))?;
    t.set("RecruitCost", u.map_or(0, |u| u.recruitment_cost))?;
    t.set("UpkeepCost", u.map_or(0, |u| u.upkeep))?;
    t.set("MPCost", d.mp_cost(key, late))?;
    t.set("MPCategory", u.map_or(0, |u| mp_category(&u.mp_category)))?;
    // Pictures: `<faction>_<unit>_icon` in ui/units/icons (and _info in ui/units/info), else the
    // bare info key (the same rule as the campaign's unit cards; the exe takes them from
    // `uniforms_table`, PROVISIONAL).
    let info = u.map(|u| u.info_key.clone()).unwrap_or_else(|| key.to_owned());
    let pick = |folder: &str, suffix: &str| {
        [format!("{faction}_{key}_{suffix}"), format!("{faction}_{info}_{suffix}"), info.clone()]
            .into_iter()
            .find(|p| inner.source.find(&format!("ui/units/{folder}/{p}.tga")).is_some())
            .unwrap_or_else(|| info.clone())
    };
    t.set("Icon", format!("data/ui/units/icons/{}", pick("icons", "icon")))?;
    t.set("InfoPic", format!("data/ui/units/info/{}", pick("info", "info")))?;
    t.set("Experience", xp)?;
    t.set("Cap", u.map_or(0, |u| u.unit_cap))?;
    t.set("Command", 0)?;
    let class = u.map_or(-1, |u| class_id(&u.unit_class));
    t.set("CommanderClass", class == 12 || class == 23)?;
    let naval = d.is_naval(key);
    let men = d.men.get(key).copied().unwrap_or(0);
    t.set("Men", men)?;
    t.set("DisplayNumber", if naval { men } else { (men as f32 * scale).round() as i32 })?;
    t.set("IsNaval", naval)?;
    let artillery = u.is_some_and(|u| u.category == "artillery");
    t.set("IsArtillery", artillery)?;
    t.set("IsFixedArtillery", u.is_some_and(|u| u.unit_class == "artillery_fixed"))?;
    t.set("UnitLimit", 0)?;
    t.set("SiegeUnit", false)?;
    Ok(t)
}

/// The fields `RecruitableUnits` / `RetrieveArmyPresets` add to a unit's details.
#[allow(clippy::too_many_arguments)]
fn army_unit(lua: &Lua, inner: &Inner, d: &ArmyData, faction: &str, key: &str, xp: i32, late: bool, scale: f32) -> mlua::Result<Table> {
    let t = unit_details(lua, inner, d, faction, key, xp, late, scale)?;
    t.set("Faction", faction)?;
    t.set("LateEra", late)?;
    t.set("XpAdjustedCost", d.xp_cost(key, xp, late))?;
    t.set("PositionSpecified", false)?;
    let class = d.units.get(key).map_or(-1, |u| class_id(&u.unit_class));
    if d.is_naval(key) {
        t.set("IsAdmiral", class == 23)?;
        t.set("ShipName", "")?;
    } else {
        t.set("IsGeneral", class == 12)?;
        let caps = lua.create_table()?;
        caps.set("FiringDrill", 0)?;
        t.set("Capabilities", caps)?;
    }
    Ok(t)
}

/// The default army: (unit, experience) list and total cost (see the module docs).
pub fn balanced_army(d: &ArmyData, faction: &str, kind: &str, size: i64, era: i64, scale: f32) -> Option<BalancedArmy> {
    let size_key = ["small", "medium", "large"][size.clamp(0, 2) as usize];
    let era_key = ["early", "middle", "late"][era.clamp(0, 2) as usize];
    let (limits, id) = *d.limits.get(&(kind.to_owned(), "balanced".to_owned(), size_key.to_owned(), era_key.to_owned()))?;
    let late = era > 0;
    let naval = kind == "naval";
    let units = d.faction_presets.get(&(faction.to_owned(), id)).and_then(|p| d.preset_units.get(p)).cloned().unwrap_or_default();
    let max = if naval { super::battle_setup::max_units(scale).1 as usize } else { 20 };
    let mut units: Vec<(String, i32)> = units.into_iter().take(max).collect();
    let funds = super::battle_setup::army_funds(size, naval);
    let mut total: i32 = units.iter().map(|(k, x)| d.xp_cost(k, *x, late)).sum();
    loop {
        let mut changed = false;
        for u in units.iter_mut() {
            if u.1 >= 9 {
                continue;
            }
            let next = total - d.xp_cost(&u.0, u.1, late) + d.xp_cost(&u.0, u.1 + 1, late);
            if next < funds {
                total = next;
                u.1 += 1;
                changed = true;
            }
        }
        if !changed || units.is_empty() {
            break;
        }
    }
    Some((units, total, limits))
}

/// Adds the functions to `t`.
pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, t: &Table) -> mlua::Result<()> {
    let data: Rc<std::cell::OnceCell<ArmyData>> = Rc::default();
    let i = inner.clone();
    let dd = data.clone();
    t.set("RecruitableUnits", lua.create_function(move |lua, args: mlua::Variadic<Value>| {
        let d = dd.get_or_init(|| ArmyData::load(&i));
        let faction = args.first().and_then(|v| v.as_string().map(|s| s.to_string_lossy())).unwrap_or_default();
        let naval = args.get(1).and_then(Value::as_boolean).unwrap_or(false);
        let era = args.get(2).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))).unwrap_or(0);
        let mask = args.get(3).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))).unwrap_or(7) as u32;
        // Optional funds: present when 7 arguments are given (the exe tests the top's type).
        let (funds, rest) = if args.len() >= 7 { (args.get(4).and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64))).map(|f| f as i32), 5) } else { (None, 4) };
        let has_general = args.get(rest).and_then(|v| v.as_i64().or_else(|| v.as_boolean().map(i64::from))).unwrap_or(0);
        let has_admiral = args.get(rest + 1).and_then(Value::as_boolean).unwrap_or(false);
        let bits = ((mask & 7) * 8) | (mask & 7);
        let late = era > 0;
        let out = lua.create_table()?;
        let mut n = 0;
        let mut seen = std::collections::BTreeSet::new();
        for key in d.faction_units.get(&faction).into_iter().flatten() {
            let Some(u) = d.units.get(key) else { continue };
            if !seen.insert(key.clone()) || d.is_naval(key) != naval || bits & (1 << mp_category(&u.mp_category)) == 0 {
                continue;
            }
            let ok_era = match era {
                0 => u.unknown_94,
                1 => u.unknown_95,
                _ => u.unknown_96,
            };
            if !ok_era {
                continue;
            }
            let e = army_unit(lua, &i, d, &faction, key, 0, late, 1.0)?;
            let cost = d.mp_cost(key, late);
            e.set("Affordable", funds.is_none_or(|f| cost <= f))?;
            let class = class_id(&u.unit_class);
            let commander = class == 12 || class == 23;
            e.set("CommanderConflict", if commander { has_admiral } else { has_general == 0 })?;
            n += 1;
            out.set(n, e)?;
        }
        Ok(out)
    })?)?;
    let i = inner.clone();
    let dd = data.clone();
    t.set("RetrieveArmyPresets", lua.create_function(move |lua, (faction, kind, size, era, scale): (String, Option<String>, Option<i64>, Option<i64>, Option<f64>)| {
        let d = dd.get_or_init(|| ArmyData::load(&i));
        let kind = kind.unwrap_or_else(|| "classic".into());
        let naval = kind == "naval";
        let (size, era, scale) = (size.unwrap_or(0), era.unwrap_or(0), scale.unwrap_or(1.0) as f32);
        let (units, cost, limits) = balanced_army(d, &faction, &kind, size, era, scale).unwrap_or((Vec::new(), 0, [7, 7, 6]));
        let setup = lua.create_table()?;
        let (land, ships) = (lua.create_table()?, lua.create_table()?);
        let mut counts = [0i32; 3];
        for (key, xp) in &units {
            let e = army_unit(lua, &i, d, &faction, key, *xp, era > 0, scale)?;
            let cat = d.units.get(key).map_or(2, |u| mp_category(&u.mp_category));
            match cat {
                0 => counts[2] += 1,
                1 => counts[1] += 1,
                2 => counts[0] += 1,
                _ => counts[0] += 1,
            }
            let list = if d.is_naval(key) { &ships } else { &land };
            list.set(list.raw_len() + 1, e)?;
        }
        setup.set("Units", land)?;
        setup.set("Ships", ships)?;
        setup.set("Faction", faction.as_str())?;
        setup.set("Cost", cost)?;
        setup.set("CategoryMask", 63)?;
        // Category limits {Max, Actual, Tag}: infantry, cavalry, artillery (limits row columns,
        // INFERRED order); at sea the three ship tags of army_box.lua's default.
        let tags = if naval { ["ship1", "ship2", "ship3"] } else { ["inf", "cav", "art"] };
        for (k, tag) in tags.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Max", limits[k])?;
            e.set("Actual", counts[k])?;
            e.set("Tag", *tag)?;
            setup.set(k + 1, e)?;
        }
        let out = lua.create_table()?;
        out.set("balanced", setup)?;
        Ok(out)
    })?)?;
    let i = inner.clone();
    let dd = data.clone();
    t.set("MPExperienceTables", lua.create_function(move |lua, naval: Option<bool>| {
        let d = dd.get_or_init(|| ArmyData::load(&i));
        let out = lua.create_table()?;
        let rows: Vec<(i32, f32)> = if naval.unwrap_or(false) {
            d.xp_naval.values().map(|r| (r.unknown_1c, r.unknown_20)).collect()
        } else {
            d.xp_land.values().map(|r| (r.unknown_24, r.unknown_28)).collect()
        };
        for (k, (fixed, mult)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("FixedCost", *fixed)?;
            e.set("Multiplier", *mult)?;
            out.set(k + 1, e)?;
        }
        Ok(out)
    })?)?;
    install_files(lua, inner, t, &data)
}

/// The army setup file functions (save, load, validate, list).
fn install_files(lua: &Lua, inner: &Rc<Inner>, t: &Table, data: &Rc<std::cell::OnceCell<ArmyData>>) -> mlua::Result<()> {
    // SaveArmySetup(setup, path, overwrite) (`0x00477F50`, CONFIRMED results): without overwrite,
    // an existing file gives `true` (the page asks before overwriting), otherwise `false,
    // success`; with overwrite, `success`. Written only in NapoleonRust's own user folder.
    let i = inner.clone();
    t.set("SaveArmySetup", lua.create_function(move |_, (setup, path, overwrite): (Table, mlua::LuaString, Option<bool>)| {
        let path = super::host::lua_text(&path);
        let overwrite = overwrite.unwrap_or(false);
        let p = PathBuf::from(&path);
        if !overwrite && p.exists() {
            return Ok(mlua::MultiValue::from_vec(vec![Value::Boolean(true)]));
        }
        let ok = if writable(&i, &p) {
            let bytes = setup_to_file(&setup).to_bytes();
            match p.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&p, bytes)) {
                Ok(()) => true,
                Err(e) => {
                    super::host::log(&i, format!("SaveArmySetup {path}: {e}"));
                    false
                }
            }
        } else {
            super::host::log(&i, format!("SaveArmySetup refused {path}: not in NapoleonRust's user folder"));
            false
        };
        let mut out = Vec::new();
        if !overwrite {
            out.push(Value::Boolean(false));
        }
        out.push(Value::Boolean(ok));
        Ok(mlua::MultiValue::from_vec(out))
    })?)?;
    // LoadArmySetup(path) (`0x004708A0`) → the setup table, or nil when the file does not open.
    let i = inner.clone();
    let dd = data.clone();
    t.set("LoadArmySetup", lua.create_function(move |lua, path: mlua::LuaString| {
        let path = super::host::lua_text(&path);
        let d = dd.get_or_init(|| ArmyData::load(&i));
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| ArmySetupFile::read(&b)) {
            Ok(f) => file_to_setup(lua, &i, d, &f).map(Some),
            Err(e) => {
                super::host::log(&i, format!("LoadArmySetup {path}: {e}"));
                Ok(None)
            }
        }
    })?)?;
    // ValidateArmySetup(setup, is_naval, era) → setup, cost[, error] (see `validate`).
    let i = inner.clone();
    let dd = data.clone();
    t.set("ValidateArmySetup", lua.create_function(move |lua, (setup, naval, era): (Table, Option<bool>, Option<Value>)| {
        let d = dd.get_or_init(|| ArmyData::load(&i));
        let era = era.as_ref().and_then(int_of).unwrap_or(0);
        let (cost, error) = validate(lua, &i, d, &setup, naval.unwrap_or(false), era)?;
        let mut out = vec![Value::Table(setup), Value::Integer(i64::from(cost))];
        if let Some(e) = error {
            out.push(Value::String(lua.create_string(&e)?));
        }
        Ok(mlua::MultiValue::from_vec(out))
    })?)?;
    // EnumerateArmySetups(dir, pattern, era, funds, unit_limit, factions) (`0x0046D410`, filter
    // `0x0045C970`, CONFIRMED): the requester's file list, keeping the setups of the same era and
    // army size, with at most `unit_limit` cards, of a faction in `factions` ({Key} entries); the
    // `.sp_default` / `.mp_default` files are left out. INFERRED: each entry also carries the
    // loaded setup's fields (the filter loads the file into it).
    let i = inner.clone();
    let dd = data.clone();
    t.set("EnumerateArmySetups", lua.create_function(move |lua, args: mlua::Variadic<Value>| {
        let d = dd.get_or_init(|| ArmyData::load(&i));
        let s = |k: usize| args.get(k).and_then(|v| v.as_string().map(|s| s.to_string_lossy())).unwrap_or_default();
        let n = |k: usize| args.get(k).and_then(int_of);
        let (dir, pattern) = (s(0), s(1));
        let (era, funds, limit) = (n(2).unwrap_or(0) as i32, n(3).unwrap_or(0) as i32, n(4).unwrap_or(20) as usize);
        let factions: Vec<String> = match args.get(5) {
            Some(Value::Table(t)) => t.clone().sequence_values::<Table>().flatten().map(|e| get_str(&e, "Key")).collect(),
            _ => Vec::new(),
        };
        let out = lua.create_table()?;
        for (e, path) in list_files(lua, &dir, &pattern)? {
            let name = get_str(&e, "FileName");
            if name.starts_with(".sp_default") || name.starts_with(".mp_default") {
                continue;
            }
            let Ok(f) = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| ArmySetupFile::read(&b)) else { continue };
            if f.era != era || f.army_size != funds || f.cards.len() > limit || !factions.contains(&f.faction) {
                continue;
            }
            let setup = file_to_setup(lua, &i, d, &f)?;
            for pair in setup.pairs::<Value, Value>() {
                let (k, v) = pair?;
                if !e.contains_key(k.clone())? {
                    e.set(k, v)?;
                }
            }
            out.set(out.raw_len() + 1, e)?;
        }
        Ok(out)
    })?)?;
    // SaveBattleSetup(prefs, path, overwrite) (`0x00478100`, CONFIRMED results as SaveArmySetup's;
    // the exe's description: "Overwrite == false -> bool, bool (files exists, save success),
    // overwrite == true -> bool (save success)"). The pages' defaults go to `.sp_default`
    // (the exe also hides that file; not done here). Written only in our own user folder.
    let i = inner.clone();
    t.set("SaveBattleSetup", lua.create_function(move |_, (prefs, path, overwrite): (Table, mlua::LuaString, Option<bool>)| {
        let path = super::host::lua_text(&path);
        let overwrite = overwrite.unwrap_or(false);
        let p = PathBuf::from(&path);
        if !overwrite && p.exists() {
            return Ok(mlua::MultiValue::from_vec(vec![Value::Boolean(true)]));
        }
        let ok = if writable(&i, &p) {
            let bytes = prefs_to_file(&prefs).to_bytes();
            match p.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| write_file(&p, &bytes, is_default(&p))) {
                Ok(()) => true,
                Err(e) => {
                    super::host::log(&i, format!("SaveBattleSetup {path}: {e}"));
                    false
                }
            }
        } else {
            super::host::log(&i, format!("SaveBattleSetup refused {path}: not in NapoleonRust's user folder"));
            false
        };
        let mut out = Vec::new();
        if !overwrite {
            out.push(Value::Boolean(false));
        }
        out.push(Value::Boolean(ok));
        Ok(mlua::MultiValue::from_vec(out))
    })?)?;
    // LoadBattleSetup(path) (`0x004709F0`) → {Preferences, Armies, map}, or nil when the file does
    // not open (see `file_to_prefs`).
    let i = inner.clone();
    let dd = data.clone();
    t.set("LoadBattleSetup", lua.create_function(move |lua, path: Option<mlua::LuaString>| {
        let d = dd.get_or_init(|| ArmyData::load(&i));
        let path = path.as_ref().map(super::host::lua_text).unwrap_or_default();
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| BattlePrefsFile::read(&b)) {
            Ok(f) => file_to_prefs(lua, &i, d, &f).map(Some),
            Err(e) => {
                super::host::log(&i, format!("LoadBattleSetup {path}: {e}"));
                Ok(None)
            }
        }
    })?)?;
    // GenerateShipName(faction, class) (`0x00470430` → `0x004611A0`, CONFIRMED structure: a name
    // generator looked up by the faction key, "Invalid faction" for an unknown one; the call
    // shape CONFIRMED in army_box.lua). The names are the `ship_names` rows (schema s,s,s,o: id,
    // group, name, faction) of the faction's `factions` #44 group (`secondary_names_group` in our
    // schema; INFERRED from the saves: France's naval list in `orig_fr_t1` is `names_french`, its
    // #44, while #43 says `names_english`), usable by it (faction column
    // empty or its key), shown through `ship_names_Ship_Name_<id>`. INFERRED like the campaign's
    // lists (SAVE_COMPAT.md §20): the next name in table order not yet given in this session,
    // starting over when all are used. PROVISIONAL: the class argument is not used.
    let i = inner.clone();
    let given: Rc<std::cell::RefCell<BTreeMap<String, usize>>> = Rc::default();
    let names: Rc<std::cell::OnceCell<ShipNames>> = Rc::default();
    t.set("GenerateShipName", lua.create_function(move |_, (faction, _class): (Option<String>, Value)| {
        let faction = faction.unwrap_or_default();
        let (rows, groups) = names.get_or_init(|| {
            let rows = table(&i, &tables::SHIP_NAMES)
                .iter()
                .map(|r| [st(&r[0]), st(&r[1]), st(&r[2]), r[3].as_str().unwrap_or_default().to_owned()])
                .collect();
            let groups = super::battle_setup::factions(&i).unwrap_or_default().into_iter().map(|f| (f.key.clone(), f.secondary_names_group.clone())).collect();
            (rows, groups)
        });
        let Some(group) = groups.get(&faction) else { return Ok("Invalid faction".to_owned()) };
        let usable: Vec<&[String; 4]> = rows.iter().filter(|r| r[1] == *group && (r[3].is_empty() || r[3] == faction)).collect();
        if usable.is_empty() {
            return Ok(String::new());
        }
        let mut g = given.borrow_mut();
        let n = g.entry(faction).or_insert(0);
        let r = usable[*n % usable.len()];
        *n += 1;
        Ok(i.loc.get(&format!("ship_names_Ship_Name_{}", r[0])).map_or_else(|| r[2].clone(), str::to_owned))
    })?)?;
    // vfs.exists(path, mode) (file_requesters.lua asks it before saving, to offer the overwrite
    // box): true if the file is on disk, else nil (INFERRED from that use; the global's name
    // CONFIRMED there).
    let vfs = lua.create_table()?;
    vfs.set("exists", lua.create_function(|_, (path, _mode): (Option<mlua::LuaString>, Value)| {
        Ok(path.map(|p| super::host::lua_text(&p)).filter(|p| Path::new(p).is_file()).map(|_| true))
    })?)?;
    lua.globals().set("vfs", vfs)?;
    // DirectoryUtils.EnumerateDirectory(dir, "*<ext>") (the file requester's list without an
    // enumerator, e.g. Save army): `list_files` (ui_prelude.lua forwards to it).
    lua.globals().set("__ntw_enumerate_directory", lua.create_function(|lua, (dir, pattern): (String, Option<String>)| {
        let out = lua.create_table()?;
        for (k, (e, _)) in list_files(lua, &dir, &pattern.unwrap_or_default())?.into_iter().enumerate() {
            out.set(k + 1, e)?;
        }
        Ok(out)
    })?)?;
    Ok(())
}

fn int_of(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
}

fn get_int(t: &Table, k: &str) -> i32 {
    t.get::<Value>(k).ok().as_ref().and_then(int_of).unwrap_or(0) as i32
}

fn get_str(t: &Table, k: &str) -> String {
    t.get::<Value>(k).ok().and_then(|v| v.as_string().map(|s| s.to_string_lossy())).unwrap_or_default()
}

fn get_bool(t: &Table, k: &str) -> bool {
    t.get::<Value>(k).ok().and_then(|v| v.as_boolean()).unwrap_or(false)
}

fn entries(t: &Table, k: &str) -> Vec<Table> {
    t.get::<Table>(k).map(|l| l.sequence_values::<Table>().flatten().collect()).unwrap_or_default()
}

/// The setup table of the army page → the file record (`0x004553B0`: Era, ArmySize, Faction,
/// Name, IsHuman, Ships {Key, Experience, IsAdmiral, ShipName}, Units {Key, Experience,
/// IsGeneral}, array part {Max, Actual, Tag}; CONFIRMED names).
pub fn setup_to_file(setup: &Table) -> ArmySetupFile {
    let mut f = ArmySetupFile {
        era: get_int(setup, "Era"),
        army_size: get_int(setup, "ArmySize"),
        faction: get_str(setup, "Faction"),
        is_human: get_bool(setup, "IsHuman"),
        name: get_str(setup, "Name"),
        ..Default::default()
    };
    for s in entries(setup, "Ships") {
        f.cards.push(Card { key: get_str(&s, "Key"), experience: get_int(&s, "Experience"), commander: get_bool(&s, "IsAdmiral"), ship_name: get_str(&s, "ShipName") });
    }
    for u in entries(setup, "Units") {
        f.cards.push(Card { key: get_str(&u, "Key"), experience: get_int(&u, "Experience"), commander: get_bool(&u, "IsGeneral"), ship_name: String::new() });
    }
    for l in setup.clone().sequence_values::<Table>().flatten() {
        f.limits.push(Limit { max: get_int(&l, "Max"), actual: get_int(&l, "Actual"), tag: get_str(&l, "Tag") });
    }
    f
}

/// The file record → the setup table `LoadArmySetup` returns (`0x00461790`): Units / Ships with
/// each card's unit details plus PositionSpecified = false, Experience, CommanderClass, Faction,
/// LateEra, CommanderConflict (a commander card exists and this card is none), IsGeneral or
/// IsAdmiral + ShipName; Faction, Era, ArmySize, Cost (the cards' experience-adjusted costs),
/// Cap (the last card's unit cap), TotalCost, TotalCards (all cards of the file), IsHuman, Name,
/// and the limits as the array part `{Max, Actual, Tag}`. Cards of unknown units are skipped
/// (the exe logs the bad key). PROVISIONAL: the exe also skips cards outside the current
/// battle's category mask (`+0x94`), not tracked here.
fn file_to_setup(lua: &Lua, inner: &Inner, d: &ArmyData, f: &ArmySetupFile) -> mlua::Result<Table> {
    let late = f.era > 0;
    let is_commander = |k: &str| d.units.get(k).is_some_and(|u| matches!(class_id(&u.unit_class), 12 | 23));
    let has_commander = f.cards.iter().any(|c| is_commander(&c.key));
    let (units, ships) = (lua.create_table()?, lua.create_table()?);
    let (mut cost, mut cap) = (0, 0);
    for c in &f.cards {
        let Some(u) = d.units.get(&c.key) else {
            super::host::log(inner, format!("LoadArmySetup: unit {} is not in the units table; skipped", c.key));
            continue;
        };
        let e = army_unit(lua, inner, d, &f.faction, &c.key, c.experience, late, 1.0)?;
        let commander = is_commander(&c.key);
        e.set("CommanderClass", commander)?;
        e.set("CommanderConflict", has_commander && !commander)?;
        if d.is_naval(&c.key) {
            e.set("IsAdmiral", c.commander)?;
            e.set("ShipName", c.ship_name.as_str())?;
            ships.set(ships.raw_len() + 1, e)?;
        } else {
            e.set("IsGeneral", c.commander)?;
            units.set(units.raw_len() + 1, e)?;
        }
        cost += d.xp_cost(&c.key, c.experience, late);
        cap = u.unit_cap;
    }
    let t = lua.create_table()?;
    t.set("Units", units)?;
    t.set("Ships", ships)?;
    t.set("Faction", f.faction.as_str())?;
    t.set("Era", f.era)?;
    t.set("ArmySize", f.army_size)?;
    t.set("Cost", cost)?;
    t.set("Cap", cap)?;
    t.set("TotalCost", cost)?;
    t.set("TotalCards", f.cards.len())?;
    t.set("IsHuman", f.is_human)?;
    t.set("Name", f.name.as_str())?;
    for (k, l) in f.limits.iter().enumerate() {
        let e = lua.create_table()?;
        e.set("Max", l.max)?;
        e.set("Actual", l.actual)?;
        e.set("Tag", l.tag.as_str())?;
        t.set(k + 1, e)?;
    }
    Ok(t)
}

/// The battle setup table → the file record (`0x00455F00`, CONFIRMED names): era, game_name,
/// password, ranked, spectators, time_of_day, weather, wind, army_size, time_limit, AI_strength,
/// era_string, time_limit_string, weather_string, allowed_funds; `map` {Type, File, Name, IsNaval,
/// TotalPlayers, Description, Image, Map, Key, IsHistoric, Teams {{Players}, {Players}}};
/// `Armies` {[team] = {army setup tables}}.
pub fn prefs_to_file(p: &Table) -> BattlePrefsFile {
    let map = p.get::<Table>("map").ok();
    let ms = |k: &str| map.as_ref().map(|m| get_str(m, k)).unwrap_or_default();
    let mut f = BattlePrefsFile {
        kind: ms("Type"),
        file: ms("File"),
        name: ms("Name"),
        era: get_int(p, "era"),
        time_of_day: get_int(p, "time_of_day"),
        weather: get_str(p, "weather"),
        wind: get_int(p, "wind"),
        army_size: get_int(p, "army_size"),
        time_limit: get_int(p, "time_limit"),
        ai_strength: get_int(p, "AI_strength"),
        total_players: map.as_ref().map_or(0, |m| get_int(m, "TotalPlayers")),
        game_name: get_str(p, "game_name"),
        password: get_str(p, "password"),
        ranked: get_bool(p, "ranked"),
        is_naval: map.as_ref().is_some_and(|m| get_bool(m, "IsNaval")),
        era_string: get_str(p, "era_string"),
        weather_string: get_str(p, "weather_string"),
        time_limit_string: get_str(p, "time_limit_string"),
        allowed_funds: get_int(p, "allowed_funds"),
        description: ms("Description"),
        image: ms("Image"),
        map: ms("Map"),
        spectators: get_int(p, "spectators"),
        key: ms("Key"),
        is_historic: map.as_ref().is_some_and(|m| get_bool(m, "IsHistoric")),
        ..Default::default()
    };
    let teams: Vec<Table> = map.as_ref().map(|m| entries(m, "Teams")).unwrap_or_default();
    let armies: Vec<Table> = entries(p, "Armies");
    for (k, t) in f.teams.iter_mut().enumerate() {
        t.players = teams.get(k).map_or(0, |t| get_int(t, "Players"));
        t.armies = armies.get(k).map(|a| a.clone().sequence_values::<Table>().flatten().map(|s| setup_to_file(&s)).collect()).unwrap_or_default();
    }
    f
}

/// The file record → `LoadBattleSetup`'s table (`0x004709F0`, CONFIRMED names; one flat table, as
/// sp_battle3.lua reads `allowed_funds`, `era` and `map` from it: the exe's "Preferences" is the
/// name of its reference, INFERRED like its "unit" / "preset" / "army" / "Team" ones):
/// time_of_day, weather, wind, army_size, time_limit, AI_strength, players (= TotalPlayers),
/// spectators, game_name, password, era, era_string, weather_string, time_limit_string,
/// allowed_funds, ranked, unit_scale (the game's current preference, not the file's), `Armies`
/// {[team] = {setup tables as `LoadArmySetup` makes them}}, `map` {File, Name, Type, TotalPlayers,
/// IsNaval, Image, Description, Map, Key, IsHistoric, Teams {{Players}, {Players}}}; Teams also at
/// the top (INFERRED from the exe's get / set of "Teams" there).
fn file_to_prefs(lua: &Lua, inner: &Inner, d: &ArmyData, f: &BattlePrefsFile) -> mlua::Result<Table> {
    let p = lua.create_table()?;
    p.set("time_of_day", f.time_of_day)?;
    p.set("weather", f.weather.as_str())?;
    p.set("wind", f.wind)?;
    p.set("army_size", f.army_size)?;
    p.set("time_limit", f.time_limit)?;
    p.set("AI_strength", f.ai_strength)?;
    p.set("players", f.total_players)?;
    p.set("spectators", f.spectators)?;
    p.set("game_name", f.game_name.as_str())?;
    p.set("password", f.password.as_str())?;
    p.set("era", f.era)?;
    p.set("era_string", f.era_string.as_str())?;
    p.set("weather_string", f.weather_string.as_str())?;
    p.set("time_limit_string", f.time_limit_string.as_str())?;
    p.set("allowed_funds", f.allowed_funds)?;
    p.set("ranked", f.ranked)?;
    let scale = inner.prefs.borrow().get("gfx_unit_scale").and_then(|v| v.trim().parse::<i32>().ok()).unwrap_or(3).clamp(0, 3);
    p.set("unit_scale", scale)?;
    let armies = lua.create_table()?;
    for (k, t) in f.teams.iter().enumerate() {
        let list = lua.create_table()?;
        for (n, a) in t.armies.iter().enumerate() {
            list.set(n + 1, file_to_setup(lua, inner, d, a)?)?;
        }
        armies.set(k + 1, list)?;
    }
    let map = lua.create_table()?;
    map.set("File", f.file.as_str())?;
    map.set("Name", f.name.as_str())?;
    map.set("Type", f.kind.as_str())?;
    map.set("TotalPlayers", f.total_players)?;
    map.set("IsNaval", f.is_naval)?;
    map.set("Image", f.image.as_str())?;
    map.set("Description", f.description.as_str())?;
    map.set("Map", f.map.as_str())?;
    map.set("Key", f.key.as_str())?;
    map.set("IsHistoric", f.is_historic)?;
    let teams = lua.create_table()?;
    for (k, t) in f.teams.iter().enumerate() {
        let e = lua.create_table()?;
        e.set("Players", t.players)?;
        teams.set(k + 1, e)?;
    }
    map.set("Teams", teams.clone())?;
    p.set("Teams", teams)?;
    p.set("Armies", armies)?;
    p.set("map", map)?;
    Ok(p)
}

/// True if `path` lies in NapoleonRust's own user folder (the only place setups are written).
fn writable(inner: &Inner, path: &Path) -> bool {
    inner.facts.user_dir.as_ref().is_some_and(|d| path.starts_with(d))
}

/// `ValidateArmySetup(setup, is_naval, era)` (`0x00479690`) → (setup, cost[, error]).
/// CONFIRMED structure: the land units are checked in a land battle, the ships at sea; a unit
/// stays when its record exists, it is available in the era (`+0x70..+0x72`), fewer than 20
/// units were kept, its MP cost is above 0 and the faction fields it; each kept entry gets
/// MPCost and Men, and the cost adds up its experience-adjusted cost. The error is the
/// localised `invalid_units` when the setup holds the other kind of units (ships in a land
/// battle or units at sea), `no_valid_units` when no unit stays (INFERRED message keys: the exe
/// takes them by index `0x15D` / `0x15C`). INFERRED: dropped entries leave the list.
/// PROVISIONAL: the battle's category mask (`+0x94`) is not tested.
fn validate(lua: &Lua, inner: &Inner, d: &ArmyData, setup: &Table, naval: bool, era: i64) -> mlua::Result<(i32, Option<String>)> {
    let faction = get_str(setup, "Faction");
    let late = era > 0;
    let fielded: std::collections::BTreeSet<&str> = d.faction_units.get(&faction).into_iter().flatten().map(String::as_str).collect();
    let (land, sea) = (entries(setup, "Units"), entries(setup, "Ships"));
    let mut cost = 0;
    let mut kept_any = false;
    for (list, key) in [(&land, "Units"), (&sea, "Ships")] {
        if list.is_empty() || (key == "Ships") != naval {
            continue;
        }
        let kept = lua.create_table()?;
        let mut n = 0;
        for e in list.iter() {
            let k = get_str(e, "Key");
            let xp = get_int(e, "Experience");
            let Some(u) = d.units.get(&k) else { continue };
            let in_era = match era {
                0 => u.unknown_94,
                1 => u.unknown_95,
                _ => u.unknown_96,
            };
            let mp = d.mp_cost(&k, late);
            if n >= 20 || !in_era || mp <= 0 || !fielded.contains(k.as_str()) {
                continue;
            }
            cost += d.xp_cost(&k, xp, late);
            e.set("MPCost", mp)?;
            e.set("Men", d.men.get(&k).copied().unwrap_or(0))?;
            n += 1;
            kept.set(n, e.clone())?;
        }
        kept_any |= n > 0;
        setup.set(key, kept)?;
    }
    let wrong_kind = (!land.is_empty() && naval) || (!sea.is_empty() && !naval);
    let error = if wrong_kind {
        Some("invalid_units")
    } else if !kept_any {
        Some("no_valid_units")
    } else {
        None
    };
    let text = error.map(|k| {
        let key = format!("random_localisation_strings_string_{k}");
        inner.loc.get(&key).map_or(key.clone(), str::to_owned)
    });
    Ok((cost, text))
}

/// The files of `dir` matching `*<ext>` as the file requester lists them: `{FileName, Path,
/// Date, DateString}` (the requester reads FileName, Date and DateString, CONFIRMED in
/// file_requester.lua). A missing folder lists nothing.
pub(super) fn list_files(lua: &Lua, dir: &str, pattern: &str) -> mlua::Result<Vec<(Table, PathBuf)>> {
    let ext = pattern.trim_start_matches('*');
    let mut out = Vec::new();
    // Hidden files (the pages' own `.sp_default` settings) are not listed (INFERRED: why the exe hides them).
    for f in super::frontend::list_saves(Path::new(dir), ext).into_iter().filter(|f| !is_hidden(&f.path)) {
        let e = lua.create_table()?;
        e.set("FileName", f.file_name.as_str())?;
        e.set("Path", f.path.display().to_string())?;
        e.set("Date", f.modified as f64)?;
        e.set("DateString", super::frontend::date_string(super::frontend::to_local_time(f.modified)))?;
        out.push((e, f.path));
    }
    Ok(out)
}

/// The custom battle pages' own settings files (`.sp_default` / `.mp_default` in the name; the
/// battle preferences writer `0x0048D2C0` hides them, CONFIRMED: `SetFileAttributesA` normal
/// before writing, hidden after, for those names only).
fn is_default(p: &Path) -> bool {
    let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    name.contains(".sp_default") || name.contains(".mp_default")
}

/// Writes a file, hidden or not (as the exe's writer leaves it).
fn write_file(p: &Path, bytes: &[u8], hidden: bool) -> std::io::Result<()> {
    use std::io::Write;
    // A hidden file cannot be recreated without the hidden flag (the exe sets it back to
    // normal first); removing it first does the same.
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_ATTRIBUTE_HIDDEN / FILE_ATTRIBUTE_NORMAL.
        o.attributes(if hidden { 0x2 } else { 0x80 });
    }
    #[cfg(not(windows))]
    let _ = hidden;
    o.open(p)?.write_all(bytes)
}

/// True for a hidden file (Windows attribute; never elsewhere).
fn is_hidden(p: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        std::fs::metadata(p).is_ok_and(|m| m.file_attributes() & 0x2 != 0)
    }
    #[cfg(not(windows))]
    {
        let _ = p;
        false
    }
}
