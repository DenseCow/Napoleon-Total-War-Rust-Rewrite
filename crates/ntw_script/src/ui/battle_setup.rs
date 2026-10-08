//! `FrontEnd.*` functions of the custom battle setup pages (sp_battle2, Play Battle → Land / Sea /
//! Siege). Evidence: `analysis/frontend/FRONTEND_PAGES.md` "Custom battle".
//!
//! - `WindLevelOptionsString()` (`0x0047A260`, CONFIRMED): the `wind_levels` rows sorted by their
//!   order column (record +0x1C; the last column, INFERRED) and joined with `|`. Each row is shown
//!   by its on-screen name (`wind_levels_onscreen_<key>`).
//! - `BattleTypeString(type)` (`0x0046A660`, CONFIRMED): the `battle_types` row's on-screen name
//!   (`battle_types_onscreen_<key>`), nothing for an unknown type.
//! - `BattleWeatherAndTimeOfDayOptionsString(battle)` (`0x0046A860` → `0x0045CEE0`, CONFIRMED):
//!   two option tables for the dropdowns (`template.dropdown_menu.lua` SetOptions takes `{Data,
//!   Value}` entries). They come from the battle's sky types (`battles_to_battle_sky_types_
//!   junctions` → `battle_sky_types`), keeping first appearance order:
//!   - weathers: `{Data = weather key, Value = its on-screen name}`;
//!   - times of day: `{Data = index, Value = its name}` for the indices 0..3 that occur.
//!
//!   The time index is the position in morning, midday, afternoon, evening, night (the exe's string
//!   order). The names are the random loc strings of the same words (INFERRED: the exe asks for
//!   random strings 0x23..0x26). Night (4) is never offered (CONFIRMED: only 0..3 are tested).
//! - `DefaultPrefsForBattle(file)` (`0x0046CF60`, CONFIRMED): for a battle `.xml` only, the table
//!   `{wind = the digit ending weather/wind_level, time_of_day = index of
//!   battle_description/time_of_day, time_limit = battle_description/duration}` with the fields
//!   the file has; nothing for other battle files (map presets).
//! - `LoadBattleSetup` / `SaveBattleSetup`: the `.battle_preferences` files, in `army_setup.rs`.

use std::collections::BTreeMap;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};
use ntw_formats::db::{DbTable, Schema};

use super::host::Inner;

/// Times of day in the exe's order (CONFIRMED string table).
pub const TIMES_OF_DAY: [&str; 5] = ["morning", "midday", "afternoon", "evening", "night"];

fn table(inner: &Inner, path: &str, codes: &str) -> Option<DbTable> {
    let file = inner.source.find(path)?;
    DbTable::read(&file.bytes, &Schema::from_codes(codes)?).ok()
}

fn s(v: &ntw_formats::db::DbValue) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// `wind_levels` keys in their order (see the module docs).
pub fn wind_levels(inner: &Inner) -> Vec<String> {
    let Some(t) = table(inner, "db/wind_levels_tables/wind_levels", "s,s,f,f,i") else { return Vec::new() };
    let mut rows: Vec<(i32, String)> = t.rows.iter().map(|r| (r[4].as_i32().unwrap_or(0), s(&r[0]))).collect();
    rows.sort_by_key(|r| r.0);
    rows.into_iter().map(|r| r.1).collect()
}

/// A battle's weathers and time-of-day indices, in first-appearance order.
pub fn battle_weathers_and_times(inner: &Inner, battle: &str) -> (Vec<String>, Vec<usize>) {
    let Some(j) = table(inner, "db/battles_to_battle_sky_types_junctions_tables/battles_to_battle_sky_types_junctions", "s,s") else {
        return (Vec::new(), Vec::new());
    };
    let Some(skies) = table(inner, "db/battle_sky_types_tables/battle_sky_types", "s,s,s,s,b,s,s,s") else { return (Vec::new(), Vec::new()) };
    let sky: BTreeMap<String, (String, String)> = skies.rows.iter().map(|r| (s(&r[0]), (s(&r[2]), s(&r[3])))).collect();
    let (mut weathers, mut times) = (Vec::new(), Vec::new());
    for r in j.rows.iter().filter(|r| s(&r[0]).eq_ignore_ascii_case(battle)) {
        let Some((w, t)) = sky.get(&s(&r[1])) else { continue };
        if !w.is_empty() && !weathers.contains(w) {
            weathers.push(w.clone());
        }
        if let Some(i) = TIMES_OF_DAY.iter().position(|x| x.eq_ignore_ascii_case(t))
            && !times.contains(&i)
        {
            times.push(i);
        }
    }
    (weathers, times)
}

/// Adds the functions to `t`.
pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, t: &Table) -> mlua::Result<()> {
    let loc = |inner: &Inner, key: String, fallback: &str| inner.loc.get(&key).map_or_else(|| fallback.to_owned(), str::to_owned);
    let i = inner.clone();
    t.set("WindLevelOptionsString", lua.create_function(move |_, ()| {
        Ok(wind_levels(&i).iter().map(|k| loc(&i, format!("wind_levels_onscreen_{k}"), k)).collect::<Vec<_>>().join("|"))
    })?)?;
    let i = inner.clone();
    t.set("BattleTypeString", lua.create_function(move |lua, kind: String| {
        let known = table(&i, "db/battle_types_tables/battle_types", "s").is_some_and(|t| t.rows.iter().any(|r| s(&r[0]) == kind));
        if !known {
            return Ok(MultiValue::new());
        }
        Ok(MultiValue::from_vec(vec![Value::String(lua.create_string(loc(&i, format!("battle_types_onscreen_{kind}"), &kind))?)]))
    })?)?;
    let i = inner.clone();
    t.set("BattleWeatherAndTimeOfDayOptionsString", lua.create_function(move |lua, battle: String| {
        let (weathers, times) = battle_weathers_and_times(&i, &battle);
        let wt = lua.create_table()?;
        for (n, w) in weathers.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Data", w.as_str())?;
            e.set("Value", loc(&i, format!("battle_weather_types_onscreen_{w}"), w))?;
            wt.set(n + 1, e)?;
        }
        let tt = lua.create_table()?;
        let mut n = 0;
        for (idx, word) in TIMES_OF_DAY.iter().enumerate().take(4) {
            if times.contains(&idx) {
                n += 1;
                let e = lua.create_table()?;
                e.set("Data", idx)?;
                e.set("Value", loc(&i, format!("random_localisation_strings_string_{word}"), word))?;
                tt.set(n, e)?;
            }
        }
        Ok((wt, tt))
    })?)?;
    let i = inner.clone();
    t.set("DefaultPrefsForBattle", lua.create_function(move |lua, file: String| {
        if !file.to_ascii_lowercase().ends_with(".xml") {
            return Ok(None);
        }
        let Some(f) = i.source.find(&file) else { return Ok(None) };
        let Ok(doc) = ntw_formats::xml::parse_bytes(&f.bytes) else { return Ok(None) };
        let prefs = lua.create_table()?;
        if let Some(w) = doc.find("weather").and_then(|w| w.child("wind_level"))
            && let Some(d) = w.text.trim().chars().last().and_then(|c| c.to_digit(10))
        {
            prefs.set("wind", d)?;
        }
        if let Some(bd) = doc.find("battle_description") {
            if let Some(t) = bd.child("time_of_day").and_then(|t| TIMES_OF_DAY.iter().position(|x| x.eq_ignore_ascii_case(t.text.trim()))) {
                prefs.set("time_of_day", t)?;
            }
            if let Some(d) = bd.child("duration") {
                prefs.set("time_limit", d.text.trim())?;
            }
        }
        Ok(Some(prefs))
    })?)?;

    // ArmyFundsForSize(size 0..2, is_naval) (`0x0046A4F0` → `0x004A2910`, CONFIRMED tables
    // `0x0131A190` / `0x0131A19C`): land 5000 / 10000 / 14000, sea 5000 / 14000 / 24000.
    t.set("ArmyFundsForSize", lua.create_function(|_, (size, naval): (Option<i64>, Option<bool>)| Ok(army_funds(size.unwrap_or(0), naval.unwrap_or(false))))?)?;
    // UnitScaleFactor([index]) → scale, index (`0x004795F0`, CONFIRMED table `0x01392770`:
    // 0.25, 0.5, 0.75, 1.0). Without an index the current setting: the preference read is the
    // unit scale (INFERRED: our preferences key `gfx_unit_scale`, 0..3).
    let i = inner.clone();
    t.set("UnitScaleFactor", lua.create_function(move |_, index: Option<i64>| {
        let idx = index.or_else(|| i.prefs.borrow().get("gfx_unit_scale").and_then(|v| v.trim().parse().ok())).unwrap_or(3).clamp(0, 3) as usize;
        Ok((UNIT_SCALES[idx], idx))
    })?)?;
    // MaxUnitsFromUnitScaleFactor(scale) → 20, the artillery-type limit for that scale
    // (`0x00475800` → `0x00851550`, `0x00DACE60`, CONFIRMED: 6 / 8 / 10 / 20 by scale index).
    t.set("MaxUnitsFromUnitScaleFactor", lua.create_function(|_, scale: Option<f64>| Ok(max_units(scale.unwrap_or(1.0) as f32)))?)?;
    // BuildCpuName(id) → the random loc string `cpu_player` ("CPU %d") with the id
    // (`0x0046AFB0`, CONFIRMED).
    let i = inner.clone();
    t.set("BuildCpuName", lua.create_function(move |_, id: i64| {
        let fmt = i.loc.get("random_localisation_strings_string_cpu_player").unwrap_or("CPU %d").to_owned();
        Ok(fmt.replacen("%d", &id.to_string(), 1))
    })?)?;
    // FactionListForBattles(is_naval, era) → { {Key, Name, FlagPath, UniformColour,
    // PrimaryColour}, ... } (`0x0046EB10`, entry `0x0045C010`, CONFIRMED field names): the factions
    // marked for custom land (+0x4D) or sea (+0x4E) battles, sorted by name. INFERRED: those
    // flags are the `factions` columns #10 / #11. PROVISIONAL: the era's alternative flag and
    // colours (+0x4F / +0x50 select them) are not used: flag folder, primary / secondary colours.
    let i = inner.clone();
    t.set("FactionListForBattles", lua.create_function(move |lua, (naval, _era): (Option<bool>, Option<i64>)| {
        let out = lua.create_table()?;
        let Some(rows) = factions(&i) else { return Ok(out) };
        let naval = naval.unwrap_or(false);
        let mut list: Vec<(String, &ntw_data::FactionRecord)> = rows
            .iter()
            .filter(|f| if naval { f.unknown_66 } else { f.unknown_65 })
            .map(|f| (i.loc.get(&format!("factions_screen_name_{}", f.key)).map_or_else(|| f.screen_name.clone(), str::to_owned), f))
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        for (n, (_, f)) in list.iter().enumerate() {
            out.set(n + 1, faction_entry(lua, &i, f)?)?;
        }
        Ok(out)
    })?)?;
    Ok(())
}

/// Unit scale factors by index (`0x01392770`, CONFIRMED).
pub const UNIT_SCALES: [f32; 4] = [0.25, 0.5, 0.75, 1.0];

/// See `ArmyFundsForSize`.
pub fn army_funds(size: i64, naval: bool) -> i32 {
    let i = size.clamp(0, 2) as usize;
    if naval { [5000, 14000, 24000][i] } else { [5000, 10000, 14000][i] }
}

/// See `MaxUnitsFromUnitScaleFactor`: (20, limit by the first scale index not below `scale`).
pub fn max_units(scale: f32) -> (i32, i32) {
    let idx = UNIT_SCALES.iter().position(|s| scale <= *s).unwrap_or(3);
    (20, [6, 8, 10, 20][idx])
}

pub(super) fn factions(inner: &Inner) -> Option<Vec<ntw_data::FactionRecord>> {
    let path = <ntw_data::FactionRecord as ntw_data::DbRecord>::path();
    let f = inner.source.find(&path)?;
    ntw_data::Table::<ntw_data::FactionRecord>::from_bytes(&f.bytes).ok().map(|t| t.rows().to_vec())
}

/// A faction's custom-battle details table (`0x0045C010`: Key, Name, FlagPath, UniformColour,
/// PrimaryColour; see `FactionListForBattles`).
fn faction_entry(lua: &Lua, inner: &Inner, f: &ntw_data::FactionRecord) -> mlua::Result<Table> {
    let colour = |c: [u8; 3]| -> mlua::Result<Table> {
        let ct = lua.create_table()?;
        ct.set("r", c[0])?;
        ct.set("g", c[1])?;
        ct.set("b", c[2])?;
        Ok(ct)
    };
    let e = lua.create_table()?;
    e.set("Key", f.key.as_str())?;
    e.set("Name", inner.loc.get(&format!("factions_screen_name_{}", f.key)).map_or_else(|| f.screen_name.clone(), str::to_owned))?;
    e.set("FlagPath", f.flag_path.as_str())?;
    e.set("UniformColour", colour(f.secondary_colour())?)?;
    e.set("PrimaryColour", colour(f.primary_colour())?)?;
    Ok(e)
}

/// `FactionDetails(key[, era])` of the front end (`0x0046E950`, CONFIRMED: the faction's details
/// table as in `FactionListForBattles`, nothing for an unknown key). PROVISIONAL: the era variant
/// (`0x00457C80`) is not used.
pub(super) fn install_faction_details(lua: &Lua, inner: &Rc<Inner>, t: &Table) -> mlua::Result<()> {
    let i = inner.clone();
    t.set("FactionDetails", lua.create_function(move |lua, (key, _era): (String, Option<i64>)| {
        let Some(rows) = factions(&i) else { return Ok(None) };
        match rows.iter().find(|f| f.key == key) {
            Some(f) => faction_entry(lua, &i, f).map(Some),
            None => Ok(None),
        }
    })?)?;
    Ok(())
}

/// The custom battle's identity functions that have no counterpart without the
/// multiplayer layer:
/// - `MPLocalPlayerId()` (`0x00474870`, the network layer's local id): PLACEHOLDER 0 offline.
pub(super) fn install_offline(lua: &Lua, t: &Table) -> mlua::Result<()> {
    t.set("MPLocalPlayerId", lua.create_function(|_, ()| Ok(0))?)?;
    Ok(())
}
