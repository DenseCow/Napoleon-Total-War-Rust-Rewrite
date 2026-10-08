//! The `FrontEnd.*` engine functions behind the single-player pages, and the data they read.
//!
//! Every function here is the original's script binding of the same name (handler addresses in
//! `analysis/worker1/script_bindings.tsv`, registrar `0x004587A0`). What each returns was taken
//! from the exe (Ghidra) and from the fields the original page scripts read; the evidence and
//! tags are in `analysis/frontend/FRONTEND_PAGES.md`. Data comes from the install through the
//! script source (packs and loose files, so mods that change the DB or loc apply), and from the
//! player's own save folder (read only).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{Lua, Table, Value};
use ntw_formats::db::{DbTable, DbValue, Schema};
use ntw_formats::preferences::Preferences;

use crate::ScriptSource;
use super::host::{Inner, UiRequest, log};

/// Napoleon's campaigns in the order `EnumerateNapoleonsCampaigns(false)` lists them
/// (CONFIRMED, `0x0046E1A0`; the multiplayer list is `mp_ita/mp_egy/mp_eur`, plus `spa_napoleon`
/// with the Peninsular DLC).
pub const NAPOLEONS_CAMPAIGNS: [&str; 4] = ["tut_napoleon", "ita_napoleon", "egy_napoleon", "eur_napoleon"];

/// The default faction `StartCampaign(<key>)` plays when only a campaign key is given
/// (CONFIRMED, `0x00478840`).
pub fn default_faction(campaign: &str) -> Option<&'static str> {
    Some(match campaign {
        "ita_napoleon" => "ita_french_republic",
        "egy_napoleon" => "egy_french_republic",
        "tut_napoleon" => "tut_france",
        "eur_napoleon" | "spa_napoleon" => "france",
        _ => return None,
    })
}

/// Whether a Napoleon campaign (or the Waterloo battle) is unlocked for a given `nap_unlock`
/// progress value (CONFIRMED rule, `0x0047C820`: tutorial and Italy always; Egypt needs > 1,
/// Europe > 2, `NCAMP_Waterloo` > 3).
pub fn campaign_unlocked(key: &str, nap_unlock: u32) -> bool {
    match key {
        "egy_napoleon" => nap_unlock > 1,
        "eur_napoleon" => nap_unlock > 2,
        "NCAMP_Waterloo" => nap_unlock > 3,
        _ => true,
    }
}

/// One row of the `battles` DB table (13 columns; schema INFERRED by worker2, all 87 rows parse).
#[derive(Debug, Clone, PartialEq)]
pub struct BattleRecord {
    /// Key, e.g. `NHB_Arcole`.
    pub key: String,
    /// Battle type: `napoleon_historic`, `historic`, `classic`, `naval`, `siege`, `campaign_battle`, `Tutorial`.
    pub kind: String,
    /// Sea battle.
    pub is_naval: bool,
    /// Battle specification: a battle `.xml` or a terrain preset folder (`BattleTerrain/Presets/<name>/`).
    pub spec: String,
    /// Small screenshot (historical battles).
    pub screenshot: Option<String>,
    /// The four flags at record +0x5C..+0x5F (meanings UNKNOWN; `EnumerateBattleMaps` reads the
    /// first and, for single/multiplayer lists, the second or third).
    pub flags: [bool; 4],
    /// Intro movie (`NHB_01_Arcole.bik`).
    pub movie: Option<String>,
    /// Year.
    pub year: i32,
}

/// The `battles` table from the install (mods that replace it apply through the VFS).
pub fn read_battles(source: &ScriptSource) -> Vec<BattleRecord> {
    let Some(file) = source.find("db/battles_tables/battles") else { return Vec::new() };
    let Some(schema) = Schema::from_codes("s,s,b,s,o,i,i,b,b,b,b,o,i") else { return Vec::new() };
    let Ok(table) = DbTable::read(&file.bytes, &schema) else { return Vec::new() };
    let s = |v: &DbValue| v.as_str().map(str::to_owned);
    let b = |v: &DbValue| v.as_bool().unwrap_or(false);
    table
        .rows
        .iter()
        .map(|r| BattleRecord {
            key: s(&r[0]).unwrap_or_default(),
            kind: s(&r[1]).unwrap_or_default(),
            is_naval: b(&r[2]),
            spec: s(&r[3]).unwrap_or_default(),
            screenshot: s(&r[4]),
            flags: [b(&r[7]), b(&r[8]), b(&r[9]), b(&r[10])],
            movie: s(&r[11]),
            year: r[12].as_i32().unwrap_or(0),
        })
        .collect()
}

/// The terrain preset folder name (lower case, as `--battle-map` takes it) a battle is fought
/// on: the preset folder itself for map battles, or the `<terrain>`'s `<name>` element of a
/// historical battle's `.xml` (`BattleTerrain/presets/HB_Arcole/` → `hb_arcole`).
pub fn battle_terrain(source: &ScriptSource, rec: &BattleRecord) -> Option<String> {
    let preset_of = |p: &str| -> Option<String> {
        let p = p.replace('\\', "/");
        let lower = p.to_ascii_lowercase();
        let rest = lower.strip_prefix("battleterrain/presets/")?;
        Some(rest.trim_end_matches('/').to_owned())
    };
    if !rec.spec.to_ascii_lowercase().ends_with(".xml") {
        return preset_of(&rec.spec);
    }
    let xml = source.find(&rec.spec)?;
    let text = String::from_utf8_lossy(&xml.bytes);
    // The battle XML names its terrain preset in a <name> element; it is the only <name> whose
    // text starts with BattleTerrain/ (CONFIRMED in every shipped historical battle).
    text.split("<name>").skip(1).filter_map(|s| s.split("</name>").next()).find_map(|n| preset_of(n.trim()))
}

/// The table `EnumerateBattleMaps` / `EnumerateNapoleonsBattleMaps` give per battle
/// (fields CONFIRMED from `0x0045B960`: Key, File, Name, Description, Image, IsNaval, Map,
/// IsHistoric, IsSiege, Movie, Teams).
fn battle_entry(lua: &Lua, inner: &Inner, rec: &BattleRecord) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let is_xml = rec.spec.to_ascii_lowercase().ends_with(".xml");
    t.set("Key", rec.key.as_str())?;
    t.set("File", rec.spec.as_str())?;
    t.set("Name", loc(inner, &format!("battles_localised_name_{}", rec.key)))?;
    t.set("Description", loc(inner, &format!("battles_description_{}", rec.key)))?;
    // Format strings "data/%Sscreenshot_small.tga" / "data/%Spreview_map.tga" are CONFIRMED for
    // preset battles; an .xml battle uses its record's screenshot. Its map preview is taken from
    // its terrain preset folder (INFERRED: the record field the exe uses is not in the DB file).
    let image = if is_xml { rec.screenshot.clone().map(|s| format!("data/{s}")) } else { Some(format!("data/{}screenshot_small.tga", rec.spec)) };
    t.set("Image", image)?;
    t.set("IsNaval", rec.is_naval)?;
    let map = if is_xml {
        battle_terrain(&inner.source, rec).map(|p| format!("data/BattleTerrain/presets/{p}/preview_map.tga"))
    } else {
        Some(format!("data/{}preview_map.tga", rec.spec))
    };
    t.set("Map", map)?;
    t.set("IsHistoric", rec.kind.contains("historic"))?;
    t.set("IsSiege", rec.kind.contains("siege"))?;
    t.set("Movie", rec.movie.clone())?;
    // Teams: two entries {Players} (CONFIRMED shape, `0x0045B960`: battle record +0x4C / +0x50).
    // PROVISIONAL source: for a map preset, the most deployment areas each alliance has in any
    // setup of its `deployment_areas.xml` (the record fields' loader was not found); nothing for
    // other battles.
    let teams = lua.create_table()?;
    if !is_xml && let Some(players) = preset_team_sizes(inner, &rec.spec) {
        for (i, p) in players.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Players", *p)?;
            teams.set(i + 1, e)?;
        }
    }
    t.set("Teams", teams)?;
    Ok(t)
}

/// The most deployment areas of alliance 0 and 1 over the setups of a preset's
/// `deployment_areas.xml` (see `battle_entry`).
fn preset_team_sizes(inner: &Inner, spec: &str) -> Option<[u32; 2]> {
    let path = format!("{}/deployment_areas.xml", spec.trim_end_matches(['/', '\\']));
    let file = inner.source.find(&path)?;
    let doc = ntw_formats::xml::parse_document(&ntw_formats::xml::decode_text(&file.bytes)).ok()?;
    let mut out = [0u32; 2];
    fn walk(e: &ntw_formats::xml::XmlElement, out: &mut [u32; 2]) {
        if e.name.eq_ignore_ascii_case("ALLIANCE")
            && let Some(id) = e.attr_i64("id").filter(|i| (0..2).contains(i))
        {
            let n = e.children_named("deployment_area").count() as u32;
            out[id as usize] = out[id as usize].max(n);
        }
        for c in &e.children {
            walk(c, out);
        }
    }
    walk(&doc, &mut out);
    (out != [0, 0]).then_some(out)
}

fn loc(inner: &Inner, key: &str) -> String {
    inner.loc.get(key).unwrap_or_default().to_owned()
}

/// A save file on disk, as `EnumerateCampaignSaves` lists it.
#[derive(Debug, Clone)]
pub struct SaveFile {
    /// File name with extension, e.g. `auto_save.save`.
    pub file_name: String,
    /// Full path.
    pub path: PathBuf,
    /// Last-modified time, seconds since 1970.
    pub modified: u64,
}

/// Lists `<dir>\*<ext>` (case-insensitive extension). Read only.
pub fn list_saves(dir: &Path, ext: &str) -> Vec<SaveFile> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let ext = ext.to_ascii_lowercase();
    let mut out: Vec<SaveFile> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.to_ascii_lowercase().ends_with(&ext) || !e.file_type().ok()?.is_file() {
                return None;
            }
            let modified = e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            Some(SaveFile { file_name: name, path: e.path(), modified })
        })
        .collect();
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    out
}

/// Whether a name is one the original's save panels can produce, as `ValidateFilename` decides it
/// (`ui\campaign ui\load-save_game.load-save_game.luac`, the function at line 33, read on the install
/// 2026-10-06, **CONFIRMED**).
///
/// The original never *rejects* a whole name: `ValidateFilename` is the text field's
/// `CharacterValidator` (`SetGlobal` at line 264), called per keystroke by
/// `ui\templates\template.text_input.luac`'s `CharacterInput` (line 30), which drops the character
/// unless the validator returns exactly `true`. It therefore encodes two rules:
/// - a character in `/ \ * ? " < > | :` is never typed (the nine-character table it walks);
/// - a character is typed only while `string.length(field) < 100`, so a typed name is **at most 100
///   characters** long.
///
/// A name that reaches us another way (a script's `SetValue`, a harness step, a Lua caller) is not
/// filtered by anything in the original, so this predicate is **our** guard on the paths that turn a
/// name into a file path ([`SaveFolders::resolve`]): a name that the field could not have produced is
/// refused rather than joined onto the save folder. It is deliberately the same rule set, not a
/// stricter one.
pub fn is_valid_save_name(name: &str) -> bool {
    /// The nine characters `ValidateFilename` refuses, in its table's order.
    const REFUSED: [char; 9] = ['/', '\\', '*', '?', '"', '<', '>', '|', ':'];
    name.chars().count() <= 100 && !name.chars().any(|c| REFUSED.contains(&c))
}

/// The file name without the class extension, as the page's `NameFromDetails` makes it (and as
/// it passes it to `LoadCampaign`).
pub fn save_name<'a>(file_name: &'a str, ext: &str) -> &'a str {
    if file_name.len() >= ext.len() && file_name[file_name.len() - ext.len()..].eq_ignore_ascii_case(ext) {
        &file_name[..file_name.len() - ext.len()]
    } else {
        file_name
    }
}

/// The folders of one save class: NapoleonRust's own (written by us) and the original game's
/// (read only).
#[derive(Debug, Clone, Default)]
pub struct SaveFolders {
    /// `<NapoleonRust user folder>\<class folder>`.
    pub ours: Option<PathBuf>,
    /// `<original user folder>\<class folder>`.
    pub original: Option<PathBuf>,
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase();
    norm(a) == norm(b)
}

impl SaveFolders {
    /// The two folders of a class folder name (e.g. `save_games`).
    pub fn of(facts: &super::host::FrontEndFacts, folder: &str) -> SaveFolders {
        SaveFolders { ours: facts.user_dir.as_ref().map(|d| d.join(folder)), original: facts.original_user_dir.as_ref().map(|d| d.join(folder)) }
    }

    /// The folders `EnumerateCampaignSaves(dir, ext)` lists: both when `dir` is our folder of the
    /// class with that extension, else `dir` alone.
    pub fn for_listing(facts: &super::host::FrontEndFacts, dir: &Path, ext: &str) -> SaveFolders {
        let class = ["save_game", "mp_save_game"].into_iter().filter_map(write_class).find(|(e, _)| e.eq_ignore_ascii_case(ext));
        if let Some((_, folder)) = class {
            let both = SaveFolders::of(facts, folder);
            if both.ours.as_deref().is_some_and(|o| same_dir(o, dir)) {
                return both;
            }
        }
        SaveFolders { ours: Some(dir.to_path_buf()), original: None }
    }

    /// Our saves, then the original's whose name none of ours has.
    pub fn list(&self, ext: &str) -> Vec<SaveFile> {
        let mut out = self.ours.as_deref().map(|d| list_saves(d, ext)).unwrap_or_default();
        if let Some(o) = self.original.as_deref().filter(|o| self.ours.as_deref().is_none_or(|d| !same_dir(d, o))) {
            for f in list_saves(o, ext) {
                if !out.iter().any(|g| g.file_name.eq_ignore_ascii_case(&f.file_name)) {
                    out.push(f);
                }
            }
        }
        out
    }

    /// A save by name (without extension) or full path: ours first, then the original's.
    ///
    /// A **relative** name must be one the save panel could have typed ([`is_valid_save_name`], the
    /// original's own `ValidateFilename` rules): no path separator and no `:` , so it cannot name a
    /// file outside the save folder. An absolute path is still taken as given, because that is how
    /// the front end's file requester hands over a browsed path (`LoadCampaign`).
    pub fn resolve(&self, name: &str, ext: &str) -> Option<PathBuf> {
        let p = Path::new(name);
        if p.is_absolute() && p.is_file() {
            return Some(p.to_path_buf());
        }
        if !is_valid_save_name(name) {
            return None;
        }
        let file = if save_name(name, ext).len() == name.len() { format!("{name}{ext}") } else { name.to_owned() };
        [self.ours.as_deref(), self.original.as_deref()].into_iter().flatten().map(|d| d.join(&file)).find(|p| p.is_file())
    }

    /// The newest file of the class that reads as a campaign save (`ContinueCampaign`).
    pub fn latest(&self, ext: &str) -> Option<PathBuf> {
        let mut all = self.list(ext);
        all.sort_by_key(|f| std::cmp::Reverse(f.modified));
        all.into_iter().map(|f| f.path).find(|p| std::fs::read(p).ok().is_some_and(|b| ntw_campaign::read_info(&b).is_ok()))
    }

    /// True for a file directly inside our folder (the only saves NapoleonRust may delete).
    pub fn deletable(&self, path: &Path) -> bool {
        let Some(ours) = self.ours.as_deref() else { return false };
        path.is_file() && path.parent().is_some_and(|p| same_dir(p, ours)) && self.original.as_deref().is_none_or(|o| !same_dir(o, ours))
    }
}

/// Seconds since 1970 (UTC) moved to the local time zone, as the page shows them (the original
/// formats local time, `0x008B86D0`). Windows: `FileTimeToLocalFileTime` (today's bias, so a
/// summer date seen in winter is an hour off; PROVISIONAL). Elsewhere: unchanged.
pub fn to_local_time(secs: u64) -> u64 {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn FileTimeToLocalFileTime(file_time: *const [u32; 2], local: *mut [u32; 2]) -> i32;
        }
        const EPOCH_DIFF: u64 = 11_644_473_600;
        let ft = (secs + EPOCH_DIFF) * 10_000_000;
        let utc = [ft as u32, (ft >> 32) as u32];
        let mut local = [0u32; 2];
        // SAFETY: both pointers are valid FILETIME-sized (two u32) buffers for the call.
        if unsafe { FileTimeToLocalFileTime(&utc, &mut local) } != 0 {
            let l = u64::from(local[0]) | (u64::from(local[1]) << 32);
            return (l / 10_000_000).saturating_sub(EPOCH_DIFF);
        }
    }
    secs
}

/// `dd/mm/yyyy hh:mm` of a time in seconds since 1970, the format of the layout's sample row
/// ("12/02/2008 16:06", CONFIRMED text in `sp_load_game`); the page passes local time
/// ([`to_local_time`]).
pub fn date_string(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil-from-days (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{d:02}/{m:02}/{y} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

/// Write classes known to `FileExtenstionAndPathForWriteClass` (CONFIRMED names and the
/// extension/folder tables of the exe, `0x0046EE50`).
pub fn write_class(class: &str) -> Option<(&'static str, &'static str)> {
    Some(match class {
        "save_game" => (".save", "save_games"),
        "mp_save_game" => (".save_multiplayer", "save_games_multiplayer"),
        "replays" => (".replay", "replays"),
        "battle_prefs" => (".battle_preferences", "battle_preferences"),
        "army_setup" => (".army_setup", "army_setups"),
        _ => return None,
    })
}

/// Reads our preferences copy, or seeds it from the original's file (read only), or starts empty.
pub fn load_preferences(user_dir: Option<&Path>, original_user_dir: Option<&Path>) -> Preferences {
    let ours = user_dir.map(|d| d.join("scripts").join("preferences.script.txt"));
    if let Some(b) = ours.as_ref().and_then(|p| std::fs::read(p).ok()) {
        return Preferences::read(&b);
    }
    if let Some(b) = original_user_dir.and_then(|d| std::fs::read(d.join("scripts").join("preferences.script.txt")).ok()) {
        return Preferences::read(&b);
    }
    Preferences::default()
}

/// Writes our preferences copy (never the original's file).
pub fn save_preferences(user_dir: Option<&Path>, prefs: &Preferences) -> std::io::Result<()> {
    let Some(dir) = user_dir else { return Ok(()) };
    let dir = dir.join("scripts");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("preferences.script.txt"), prefs.to_bytes())
}

/// Adds the `FrontEnd.*` functions of the single-player pages to `t`.
pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, t: &Table) -> mlua::Result<()> {
    let battles: Rc<RefCell<Option<Vec<BattleRecord>>>> = Rc::new(RefCell::new(None));
    let get_battles = {
        let inner = inner.clone();
        let battles = battles.clone();
        move || -> Vec<BattleRecord> {
            battles.borrow_mut().get_or_insert_with(|| read_battles(&inner.source)).clone()
        }
    };

    // The save folders. The original lists and writes `<its user folder>\save_games\` only
    // (CONFIRMED `0x0046EE50`). NapoleonRust writes its saves to its own user folder
    // (`FrontEndFacts::user_dir`), so the write class names that folder, and the load-game page
    // lists ours plus the original's (read only; ours win on equal names). PROVISIONAL: the
    // merged list is ours, not the original's behaviour.
    let names: Rc<RefCell<std::collections::HashMap<String, PathBuf>>> = Rc::default();

    // FileExtenstionAndPathForWriteClass(class) → extension, folder (with a trailing separator).
    let i = inner.clone();
    t.set("FileExtenstionAndPathForWriteClass", lua.create_function(move |_, class: String| {
        let Some((ext, folder)) = write_class(&class) else { return Ok((None, None)) };
        let base = i.facts.user_dir.as_ref().or(i.facts.original_user_dir.as_ref());
        let dir = base.map(|d| format!("{}\\", d.join(folder).display()));
        Ok((Some(ext.to_owned()), dir))
    })?)?;

    // EnumerateCampaignSaves(dir, "*<ext>") → { {FileName, Path, Date, DateString, TimePlayed}, ... }
    // (fields read by sp_load_game.lua; TimePlayed = the header's turn number, INFERRED). For our
    // own save folder the original's saves of the same class are listed too (see above).
    let i = inner.clone();
    let n2 = names.clone();
    t.set("EnumerateCampaignSaves", lua.create_function(move |lua, (dir, pattern): (String, Option<String>)| {
        let ext = pattern.unwrap_or_default().trim_start_matches('*').to_owned();
        let folders = SaveFolders::for_listing(&i.facts, Path::new(&dir), &ext);
        let files = folders.list(&ext);
        let out = lua.create_table()?;
        let mut names = n2.borrow_mut();
        for (n, f) in files.iter().enumerate() {
            names.insert(save_name(&f.file_name, &ext).to_ascii_lowercase(), f.path.clone());
            let e = lua.create_table()?;
            e.set("FileName", f.file_name.as_str())?;
            e.set("Path", f.path.display().to_string())?;
            e.set("Date", f.modified as f64)?;
            e.set("DateString", date_string(to_local_time(f.modified)))?;
            let turn = std::fs::read(&f.path).ok().and_then(|b| ntw_campaign::read_info(&b).ok()).map(|i| i.header.turn_number);
            e.set("TimePlayed", turn.unwrap_or(0))?;
            out.set(n + 1, e)?;
        }
        Ok(out)
    })?)?;
    t.set("EnumerateMultiplayerCampaignSaves", t.get::<Value>("EnumerateCampaignSaves")?)?;

    // GetExtendedSaveGameInfo(path) → {Faction, FlagPath, LeaderPortrait, TimePlayed, Year, Season,
    // Maps} (CONFIRMED field names, 0x008982C0), from the save's SAVE_GAME_HEADER. Maps[theatre] =
    // the header's region-ownership picture (MAPS item), shown on the page's `map_<map>` component
    // through UIImage(...):SetComponentTexture; here a run-time image (one per theatre, replaced on
    // the next call).
    let i = inner.clone();
    t.set("GetExtendedSaveGameInfo", lua.create_function(move |lua, path: String| {
        let info = match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| ntw_campaign::read_info(&b).map_err(|e| e.to_string())) {
            Ok(info) => info,
            Err(e) => {
                log(&i, format!("GetExtendedSaveGameInfo {path}: {e}"));
                return Ok(None);
            }
        };
        let h = &info.header;
        let t = lua.create_table()?;
        t.set("Faction", h.faction_key.as_str())?;
        t.set("FlagPath", h.flag_path.as_str())?;
        t.set("LeaderPortrait", h.portrait.as_str())?;
        t.set("TimePlayed", h.turn_number)?;
        t.set("Year", h.year)?;
        // The header stores the season key (e.g. "winter"); the screen shows its loc text (INFERRED).
        let season = i.loc.get(&format!("seasons_onscreen_{}", h.season_name.to_ascii_lowercase())).map(str::to_owned).unwrap_or_else(|| h.season_name.clone());
        t.set("Season", season)?;
        let maps = lua.create_table()?;
        for m in h.maps.iter().filter(|m| m.width > 0 && m.height > 0 && m.pixels.len() >= (m.width * m.height) as usize) {
            let key = format!("save_map_{}", m.theatre);
            let mut world = i.world.borrow_mut();
            let version = world.runtime_images.get(&key).map_or(0, |r| r.version + 1);
            let mut img = super::world::RuntimeImage::from_rgba(m.width, m.height, m.rgba());
            img.version = version;
            world.runtime_images.insert(key.clone(), img);
            world.generation += 1;
            maps.set(m.theatre.as_str(), format!("{}{key}", super::world::RUNTIME_IMAGE_PREFIX))?;
        }
        t.set("Maps", maps)?;
        Ok(Some(t))
    })?)?;

    // LoadCampaign(name) and ContinueCampaign(): hand over to the game (campaign mode). The page
    // passes the save's name without folder or extension (its row global `m_name`, CONFIRMED in
    // sp_load_game.lua); the exe completes it with the save class's folder and extension
    // (`0x0047CC70` → the file system object's path builder, write class 0, CONFIRMED). A full
    // path is accepted too.
    let i = inner.clone();
    let n2 = names.clone();
    t.set("LoadCampaign", lua.create_function(move |_, name: String| {
        let listed = n2.borrow().get(&name.to_ascii_lowercase()).cloned();
        match listed.or_else(|| SaveFolders::of(&i.facts, "save_games").resolve(&name, ".save")) {
            Some(path) => i.requests.borrow_mut().push(UiRequest::LoadCampaign(path)),
            None => log(&i, format!("LoadCampaign: no save {name}")),
        }
        Ok(())
    })?)?;
    let i = inner.clone();
    t.set("ContinueCampaign", lua.create_function(move |_, ()| {
        // CONFIRMED (`0x0046CF00` → `0x0085F7D0`): "Load the most recently saved campaign game", the
        // newest file of the save class that reads as a save. Ours and the original's folders.
        if let Some(path) = SaveFolders::of(&i.facts, "save_games").latest(".save") {
            i.requests.borrow_mut().push(UiRequest::LoadCampaign(path));
        }
        Ok(())
    })?)?;
    // DirectoryUtils.DeleteFiles(paths) (the page's Delete button, after its confirmation box):
    // deletes only saves in NapoleonRust's own save folder; the original's saves are read only.
    let i = inner.clone();
    lua.globals().set("__ntw_delete_saves", lua.create_function(move |_, paths: Vec<String>| {
        let folders = SaveFolders::of(&i.facts, "save_games");
        for p in paths {
            let path = PathBuf::from(&p);
            if folders.deletable(&path) {
                match std::fs::remove_file(&path) {
                    Ok(()) => log(&i, format!("DirectoryUtils.DeleteFiles: deleted {p}")),
                    Err(e) => log(&i, format!("DirectoryUtils.DeleteFiles {p}: {e}")),
                }
            } else {
                log(&i, format!("DirectoryUtils.DeleteFiles refused {p}: not in NapoleonRust's save folder"));
            }
        }
        Ok(())
    })?)?;

    // BuildCredits(parent) → the credits pages (ui/credits.rs, `0x0046B100`).
    let i = inner.clone();
    t.set("BuildCredits", lua.create_function(move |lua, parent: Value| match super::host::node_of(&parent) {
        Some(p) => super::credits::build_credits(lua, &i, p).map(Some),
        None => {
            log(&i, "BuildCredits: no parent component".into());
            Ok(None)
        }
    })?)?;
    // EnableCreditScreenMusic(on): the credits music while the page is shown. PLACEHOLDER: the
    // front end's music is not switched yet (the call is accepted silently).
    t.set("EnableCreditScreenMusic", lua.create_function(|_, _on: Option<bool>| Ok(()))?)?;

    // EnumerateNapoleonsCampaigns(multiplayer) → { {Key, Name, Description, StartDate, BulletList,
    // Unlocked}, ... } (CONFIRMED fields and list, 0x0046E1A0).
    let i = inner.clone();
    t.set("EnumerateNapoleonsCampaigns", lua.create_function(move |lua, mp: Option<bool>| {
        let keys: Vec<&str> = if mp.unwrap_or(false) {
            let mut k = Vec::new();
            if i.facts.spanish_campaign {
                k.push("spa_napoleon");
            }
            k.extend(["mp_ita_napoleon", "mp_egy_napoleon", "mp_eur_napoleon"]);
            k
        } else {
            NAPOLEONS_CAMPAIGNS.to_vec()
        };
        let out = lua.create_table()?;
        for (n, key) in keys.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Key", *key)?;
            e.set("Name", loc(&i, &format!("campaigns_onscreen_name_{key}")))?;
            e.set("Description", loc(&i, &format!("campaigns_description_{key}")))?;
            // StartDate: the start position's year (INFERRED; the exe reads it from the campaign record).
            let year = i.source.find(&format!("campaigns/{key}/startpos.esf")).and_then(|f| ntw_campaign::read_info(&f.bytes).ok()).map(|c| c.header.date.map_or(c.header.year, |d| d.year));
            e.set("StartDate", year.unwrap_or(0))?;
            // UNKNOWN: the bullet-list text (no matching loc key found).
            e.set("BulletList", "")?;
            e.set("Unlocked", campaign_unlocked(key, i.facts.nap_unlock))?;
            out.set(n + 1, e)?;
        }
        Ok(out)
    })?)?;

    // GetWaterlooBattleDetails() → name, description, specification file, unlocked
    // (CONFIRMED order of use in sp_episodic_campaign.lua; record NCAMP_Waterloo, 0x004706F0).
    let i = inner.clone();
    let gb = get_battles.clone();
    t.set("GetWaterlooBattleDetails", lua.create_function(move |_, ()| {
        let Some(rec) = gb().into_iter().find(|b| b.key == "NCAMP_Waterloo") else { return Ok((None, None, None, false)) };
        Ok((
            Some(loc(&i, "battles_localised_name_NCAMP_Waterloo")),
            Some(loc(&i, "battles_description_NCAMP_Waterloo")),
            Some(rec.spec),
            campaign_unlocked("NCAMP_Waterloo", i.facts.nap_unlock),
        ))
    })?)?;

    // StartCampaign(key) or StartCampaign(key, faction, ...) (CONFIRMED, 0x00478840).
    let i = inner.clone();
    t.set("StartCampaign", lua.create_function(move |_, (campaign, faction): (String, Option<String>)| {
        let faction = faction.or_else(|| default_faction(&campaign).map(str::to_owned)).unwrap_or_default();
        i.requests.borrow_mut().push(UiRequest::StartCampaign { campaign, faction });
        Ok(())
    })?)?;

    // EnumerateNapoleonsBattleMaps() → entries of type napoleon_historic plus UnlockLevel
    // (CONFIRMED, 0x0046DFF0): 7 = locked DLC, else <stored progress> + 2. PROVISIONAL: every
    // battle counts as unlocked (level 2) like the retail game's flag (see FRONTEND_PAGES.md), and
    // a DLC battle counts as owned when its specification file is installed.
    let i = inner.clone();
    let gb = get_battles.clone();
    t.set("EnumerateNapoleonsBattleMaps", lua.create_function(move |lua, ()| {
        let out = lua.create_table()?;
        let mut n = 0;
        for rec in gb().iter().filter(|b| b.kind == "napoleon_historic") {
            let e = battle_entry(lua, &i, rec)?;
            let owned = i.source.find(&rec.spec).is_some();
            e.set("UnlockLevel", if owned { 2 } else { 7 })?;
            n += 1;
            out.set(n, e)?;
        }
        Ok(out)
    })?)?;

    // EnumerateBattleMaps(type, flag) → entries of that type (CONFIRMED filter, 0x0046D7C0: the
    // record's first flag or the retail "all maps" flag, then its second flag if `flag` is true,
    // else its third). The retail flag is taken as set (PROVISIONAL, see FRONTEND_PAGES.md).
    let i = inner.clone();
    let gb = get_battles.clone();
    t.set("EnumerateBattleMaps", lua.create_function(move |lua, (kind, flag): (String, Option<bool>)| {
        let out = lua.create_table()?;
        let mut n = 0;
        for rec in gb().iter().filter(|b| b.kind.eq_ignore_ascii_case(&kind)) {
            let ok = if flag.unwrap_or(false) { rec.flags[1] } else { rec.flags[2] };
            if ok {
                n += 1;
                out.set(n, battle_entry(lua, &i, rec)?)?;
            }
        }
        Ok(out)
    })?)?;

    // StartBattle(setup, players): hand over to the game (battle mode) with the setup's battle key.
    let i = inner.clone();
    let gb = get_battles.clone();
    t.set("StartBattle", lua.create_function(move |_, (setup, players): (Value, Value)| {
        let (key, teams) = match &setup {
            Value::Table(t) => (t.get::<Option<String>>("Key")?, t.get::<Option<Table>>("__teams")?),
            _ => (None, None),
        };
        let Some(key) = key else {
            log(&i, "StartBattle: the setup names no battle".into());
            return Ok(());
        };
        let map = gb().iter().find(|b| b.key == key).and_then(|r| battle_terrain(&i.source, r));
        // A custom battle: the setup carries the teams' army setups (UIBattleSetup:SetDetails) and
        // the players list says whose army is whose ({Alliance, Army, IsHuman, Name, Handicap},
        // 0-based indices; CONFIRMED fields in sp_battle3.lua).
        if let (Some(teams), Value::Table(players)) = (teams, &players) {
            let mut armies = Vec::new();
            for p in players.sequence_values::<Table>() {
                let p = p?;
                let alliance: u32 = p.get::<Option<u32>>("Alliance")?.unwrap_or(0);
                let army: u32 = p.get::<Option<u32>>("Army")?.unwrap_or(0);
                let human: bool = p.get::<Option<bool>>("IsHuman")?.unwrap_or(false);
                let Some(setup) = teams.get::<Option<Table>>(alliance + 1)?.and_then(|t| t.get::<Option<Table>>(army + 1).ok().flatten()) else {
                    continue;
                };
                let faction: String = setup.get::<Option<String>>("Faction")?.unwrap_or_default();
                let mut units = Vec::new();
                for list in ["Units", "Ships"] {
                    if let Some(l) = setup.get::<Option<Table>>(list)? {
                        for u in l.sequence_values::<Table>() {
                            let u = u?;
                            if let Some(k) = u.get::<Option<String>>("Key")? {
                                units.push((k, u.get::<Option<f64>>("Experience")?.unwrap_or(0.0).max(0.0) as u32));
                            }
                        }
                    }
                }
                armies.push(super::host::CustomArmy { alliance, army, faction, human, units });
            }
            i.requests.borrow_mut().push(UiRequest::StartCustomBattle { battle: key, map, armies });
            return Ok(());
        }
        i.requests.borrow_mut().push(UiRequest::StartBattle { battle: key, map });
        Ok(())
    })?)?;

    // BuildInfoSetup({Key=..., File=...}) → the battle description for UIHistoricBattleSetup /
    // UIBattleSetup (CONFIRMED description, 0x0046BA90). Ours keeps the table as given plus the
    // record's entry fields.
    let i = inner.clone();
    let gb = get_battles.clone();
    t.set("BuildInfoSetup", lua.create_function(move |lua, descr: Table| {
        let key: Option<String> = descr.get("Key")?;
        if let Some(rec) = key.and_then(|k| gb().into_iter().find(|b| b.key == k)) {
            let e = battle_entry(lua, &i, &rec)?;
            for pair in e.pairs::<Value, Value>() {
                let (k, v) = pair?;
                if descr.raw_get::<Value>(k.clone())?.is_nil() {
                    descr.raw_set(k, v)?;
                }
            }
        }
        Ok(descr)
    })?)?;

    // Internal: the alliances and armies of a battle specification .xml (for the prelude's
    // UIHistoricBattleSetup:RetrieveDetails(): { Alliances = { { {Faction, Name}, ... }, ... } }).
    // Read from the <alliance>/<army> elements (CONFIRMED structure of the shipped battle files);
    // Name = the army general's <name>.
    let i = inner.clone();
    t.set("__BattleAlliances", lua.create_function(move |lua, file: String| {
        let out = lua.create_table()?;
        let Some(xml) = i.source.find(&file) else { return Ok(out) };
        let text = String::from_utf8_lossy(&xml.bytes).into_owned();
        let between = |s: &str, tag: &str| -> Option<String> {
            let open = format!("<{tag}>");
            let a = s.find(&open)? + open.len();
            let b = s[a..].find(&format!("</{tag}>"))? + a;
            Some(s[a..b].trim().to_owned())
        };
        for (ai, alliance) in text.split("<alliance").skip(1).enumerate() {
            let alliance = alliance.split("</alliance>").next().unwrap_or("");
            let armies = lua.create_table()?;
            for (ri, army) in alliance.split("<army>").skip(1).enumerate() {
                let army = army.split("</army>").next().unwrap_or("");
                let a = lua.create_table()?;
                a.set("Faction", between(army, "faction"))?;
                a.set("Name", army.split("<general").nth(1).and_then(|g| between(g, "name")))?;
                armies.set(ri + 1, a)?;
            }
            out.set(ai + 1, armies)?;
        }
        Ok(out)
    })?)?;
    // LocalPlayerName(): the preferences' local_player_name (CONFIRMED key in the original's file).
    let i = inner.clone();
    t.set("LocalPlayerName", lua.create_function(move |_, ()| Ok(i.prefs.borrow().get("local_player_name").unwrap_or("").to_owned()))?)?;
    // PlayMovieInComponent / ShowDemoMovie: PLACEHOLDER, no Bink decoder yet (nothing plays).
    for name in ["PlayMovieInComponent", "ShowDemoMovie", "StopDemoMovie"] {
        t.set(name, lua.create_function(|_, _: mlua::Variadic<Value>| Ok(()))?)?;
    }
    t.set("MoviePlaying", lua.create_function(|_, ()| Ok(false))?)?;
    // GenerateRegionOwnershipMaps(theatres, campaign, faction) → { theatre = image }: PLACEHOLDER (no
    // generated ownership images yet, so the theatre maps stay blank). CloseMapDataFile: no-op.
    t.set("GenerateRegionOwnershipMaps", lua.create_function(|lua, _: mlua::Variadic<Value>| lua.create_table())?)?;
    t.set("CloseMapDataFile", lua.create_function(|_, _: mlua::Variadic<Value>| Ok(()))?)?;

    // CampaignDetails(key, multiplayer) → {Key, Name, Description, StartYear, Factions, GameTypes}
    // (CONFIRMED field names, 0x0046BCC0, and the fields grand_campaign_scripts/main_panel.lua
    // reads). Factions is keyed by faction key: {Key, Name, Description, LeaderPortrait, FlagPath,
    // VictoryConditions, ListOrder}. Source: the campaign's startpos.esf (its playable factions,
    // in file order = ListOrder) and the factions DB table (flag folder) and loc (names).
    // UNKNOWN: the faction descriptions, leader portraits, victory-condition texts and game
    // types (left empty).
    let i = inner.clone();
    t.set("CampaignDetails", lua.create_function(move |lua, (key, _mp): (String, Option<bool>)| {
        let info = i.source.find(&format!("campaigns/{key}/startpos.esf")).and_then(|f| ntw_campaign::read_info(&f.bytes).ok());
        let factions_db = i
            .source
            .find("db/factions_tables/factions")
            .and_then(|f| ntw_data::Table::<ntw_data::FactionRecord>::from_bytes(&f.bytes).ok());
        let t = lua.create_table()?;
        t.set("Key", key.as_str())?;
        t.set("Name", loc(&i, &format!("campaigns_onscreen_name_{key}")))?;
        t.set("Description", loc(&i, &format!("campaigns_description_{key}")))?;
        let factions = lua.create_table()?;
        if let Some(info) = &info {
            t.set("StartYear", info.header.date.map_or(info.header.year, |d| d.year))?;
            for (n, p) in info.players.iter().filter(|p| p.is_playable).enumerate() {
                let f = lua.create_table()?;
                f.set("Key", p.faction_key.as_str())?;
                let name = i.loc.get(&format!("factions_screen_name_{}", p.faction_key)).map(str::to_owned).or_else(|| {
                    factions_db.as_ref().and_then(|d| d.get(&p.faction_key)).map(|r| r.screen_name.clone())
                });
                f.set("Name", name)?;
                f.set("Description", "")?;
                f.set("FlagPath", factions_db.as_ref().and_then(|d| d.get(&p.faction_key)).map(|r| r.flag_path.clone()))?;
                // The startpos header holds the default faction's portrait (INFERRED use).
                if p.faction_key == info.header.faction_key {
                    f.set("LeaderPortrait", info.header.portrait.as_str())?;
                }
                // PROVISIONAL game types (the per-faction list lives in data not decoded yet): the
                // short campaign and the historical (long) one, labelled by their loc texts; options are
                // {Value = label, Data = 1-based type} as template.dropdown_menu.lua reads them.
                let types = lua.create_table()?;
                let vcs = lua.create_table()?;
                for (n, ty) in ["short", "long"].iter().enumerate() {
                    let o = lua.create_table()?;
                    o.set("Value", loc(&i, &format!("random_localisation_strings_string_campaign_type_{ty}")))?;
                    o.set("Data", n + 1)?;
                    types.set(n + 1, o)?;
                    // UNKNOWN: the victory-condition text of each game type.
                    vcs.set(n + 1, "")?;
                }
                f.set("GameTypes", types)?;
                f.set("VictoryConditions", vcs)?;
                f.set("StartingRegions", "")?;
                f.set("ListOrder", n + 1)?;
                factions.set(p.faction_key.as_str(), f)?;
            }
        }
        t.set("Factions", factions)?;
        t.set("GameTypes", lua.create_table()?)?;
        Ok(t)
    })?)?;

    // TheatreList(key) → { <theatre> = {Map = image}, ... } (CONFIRMED names "theatres"/"theatre"/
    // "Map", 0x00478FA0). INFERRED: one theatre per campaign map, keyed as sp_load_game.lua's
    // theatre_lookup names them (map_nap_europe → europe_main); the Map image is UNKNOWN, so none
    // is given (PLACEHOLDER).
    let i = inner.clone();
    t.set("TheatreList", lua.create_function(move |lua, key: String| {
        let out = lua.create_table()?;
        let info = i.source.find(&format!("campaigns/{key}/startpos.esf")).and_then(|f| ntw_campaign::read_info(&f.bytes).ok());
        if let Some(info) = info {
            let theatre = match info.map_key.as_str() {
                "nap_italy" => "italy_main",
                "nap_egypt" => "egypt_main",
                "nap_europe" => "europe_main",
                "nap_spain" => "spain_main",
                other => other,
            };
            out.set(theatre, lua.create_table()?)?;
        }
        Ok(out)
    })?)?;

    // OpenMapDataFile(key) → start year, season (CONFIRMED description: "Start date and season
    // for that campaign"), from the startpos header.
    let i = inner.clone();
    t.set("OpenMapDataFile", lua.create_function(move |_, key: String| {
        let info = i.source.find(&format!("campaigns/{key}/startpos.esf")).and_then(|f| ntw_campaign::read_info(&f.bytes).ok());
        Ok(info.map(|c| (c.header.date.map_or(c.header.year, |d| d.year), c.header.season_name)).unzip())
    })?)?;

    // Engine objects the scripts only pass along (to UIPrefsInterface / UIMPInterface).
    for name in ["RetrieveGameCore", "MultiplayerBaseInterface", "DropInInterface", "MPInterface", "MPOnlinePresence"] {
        let tag = name.to_owned();
        t.set(name, lua.create_function(move |lua, ()| {
            let o = lua.create_table()?;
            o.set("__engine_object", tag.as_str())?;
            Ok(o)
        })?)?;
    }
    // Events and state the original keeps for other systems; nothing to do here.
    // FrontEnd.FrontEnd(): the front-end object passed to UIBattleSetup:SetDetails (UNKNOWN use; nil here).
    for name in ["FrontEnd", "ClearPreviousSetup", "TriggerSliderUpdateEvent", "TriggerMessageBoxOpenedEvent", "ResumeFrontEndMovie", "StopAllMovies", "SetCurrentGameType"] {
        t.set(name, lua.create_function(|_, _: mlua::Variadic<Value>| Ok(()))?)?;
    }
    // PreviousBattleSetup() → nil when no previous battle is stored (CONFIRMED description).
    t.set("PreviousBattleSetup", lua.create_function(|_, ()| Ok(Value::Nil))?)?;
    // FormatMinutesString(n) → "%d minutes" (CONFIRMED description; the loc text is UNKNOWN).
    t.set("FormatMinutesString", lua.create_function(|_, n: f64| Ok(format!("{} minutes", n as i64)))?)?;

    // Preferences, for the UIPrefsInterface object in the prelude.
    let p = lua.create_table()?;
    let i = inner.clone();
    p.set("get", lua.create_function(move |_, key: String| Ok(i.prefs.borrow().get(&key).map(str::to_owned)))?)?;
    let i = inner.clone();
    p.set("set", lua.create_function(move |_, (key, value): (String, Value)| {
        let mut prefs = i.prefs.borrow_mut();
        match value {
            Value::Boolean(b) => prefs.set_bool(&key, b),
            Value::Integer(n) => prefs.set_f64(&key, n as f64),
            Value::Number(n) => prefs.set_f64(&key, n),
            Value::String(s) => prefs.set(&key, &s.to_string_lossy()),
            _ => {}
        }
        Ok(())
    })?)?;
    let i = inner.clone();
    p.set("save", lua.create_function(move |_, ()| {
        if let Err(e) = save_preferences(i.facts.user_dir.as_deref(), &i.prefs.borrow()) {
            log(&i, format!("could not write our preferences copy: {e}"));
        }
        Ok(())
    })?)?;
    lua.globals().set("__prefs", p)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_saves_can_be_deleted_and_names_resolve() {
        let base = std::env::temp_dir().join(format!("ntw_save_folders_{}", std::process::id()));
        let (ours, orig) = (base.join("ours").join("save_games"), base.join("orig").join("save_games"));
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::create_dir_all(&orig).unwrap();
        for p in [ours.join("x.save"), orig.join("x.save"), orig.join("y.save")] {
            std::fs::write(p, b"").unwrap();
        }
        let f = SaveFolders { ours: Some(ours.clone()), original: Some(orig.clone()) };
        assert!(f.deletable(&ours.join("x.save")));
        assert!(!f.deletable(&orig.join("x.save")));
        assert!(!f.deletable(&ours.join("missing.save")));
        assert!(!SaveFolders { ours: None, original: Some(orig.clone()) }.deletable(&orig.join("x.save")));
        // The same folder given as both: nothing is ours to delete.
        assert!(!SaveFolders { ours: Some(orig.clone()), original: Some(orig.clone()) }.deletable(&orig.join("x.save")));
        assert_eq!(f.resolve("x", ".save"), Some(ours.join("x.save")));
        assert_eq!(f.resolve("y.save", ".save"), Some(orig.join("y.save")));
        assert_eq!(f.resolve("z", ".save"), None);
        let names: Vec<String> = f.list(".save").into_iter().map(|s| s.file_name).collect();
        assert_eq!(names, vec!["x.save", "y.save"]);
        assert_eq!(save_name("A b.SAVE", ".save"), "A b");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `is_valid_save_name` is the original's `ValidateFilename` rule set, and `resolve` refuses a
    /// name the save panel could not have produced instead of joining it onto the save folder.
    #[test]
    fn a_save_name_the_panel_could_not_type_is_refused() {
        assert!(is_valid_save_name("Napoleon at Austerlitz"));
        assert!(is_valid_save_name(""));
        assert!(is_valid_save_name(&"a".repeat(100)), "100 characters is the maximum");
        assert!(!is_valid_save_name(&"a".repeat(101)), "101 is one too many");
        for c in ['/', '\\', '*', '?', '"', '<', '>', '|', ':'] {
            assert!(!is_valid_save_name(&format!("a{c}b")), "{c} is refused");
        }
        // A path that escapes the save folder is not a name, so it is not resolved as one.
        let base = std::env::temp_dir().join(format!("ntw_save_name_{}", std::process::id()));
        let ours = base.join("save_games");
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::write(base.join("outside.save"), b"").unwrap();
        let f = SaveFolders { ours: Some(ours.clone()), original: None };
        assert_eq!(f.resolve("../outside", ".save"), None);
        assert_eq!(f.resolve("..\\outside", ".save"), None);
        let absolute = base.join("outside.save");
        assert_eq!(f.resolve(&absolute.to_string_lossy(), ".save"), Some(absolute), "an absolute path is still taken as given");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn date_strings_match_the_layout_sample_format() {
        // 2008-02-12 16:06 UTC.
        assert_eq!(date_string(1_202_832_360), "12/02/2008 16:06");
        assert_eq!(date_string(0), "01/01/1970 00:00");
    }

    #[test]
    fn unlock_rule() {
        assert!(campaign_unlocked("ita_napoleon", 1));
        assert!(campaign_unlocked("tut_napoleon", 1));
        assert!(!campaign_unlocked("egy_napoleon", 1));
        assert!(campaign_unlocked("egy_napoleon", 2));
        assert!(!campaign_unlocked("eur_napoleon", 2));
        assert!(campaign_unlocked("NCAMP_Waterloo", 4));
    }
}
