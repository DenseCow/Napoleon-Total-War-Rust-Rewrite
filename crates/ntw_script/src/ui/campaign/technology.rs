//! The technology screen (`TechnologyPlayerDetails`).

use super::*;

/// Each technology's `ParentXoffset` / `ParentYoffset` (the tech entry's link to its parent in the
/// tree, which `template.tech_entry.lua:47-94` draws as one horizontal and one vertical
/// `general_purpose_pixel`). Not table columns: the technology table's link step `0x00F21A30`
/// (own Ghidra copy, 2026-10-07; CONFIRMED) computes them at load into record `+0x88` / `+0x84`,
/// which `0x009ABB50` hands out as ParentXoffset / ParentYoffset. For each
/// `technology_required_technology_junctions` row (technology T requires R, in table order) whose
/// R stands in the same tree column as T (records `+0x90` / `+0x94` equal: the building chain's
/// fields, read through the building level; INFERRED equivalent to "same chain", as the three tech
/// chains differ): X = R.pos % 4 − T.pos % 4 and Y = row(T) − row(R), with pos the
/// `tree_column` (C remainder) and row = 2 × building level + (pos > 3) (four positions per row,
/// two rows per level; the level is the building level record's `+0x10`, INFERRED to be its
/// `level`). A later row for the same T overwrites an earlier one; a requirement from another
/// column draws no link (it goes to the record's other list `+0x68..`), and a technology without
/// a same-column requirement keeps 0, 0. (Before 2026-10-07 ours passed the table's columns 6 and
/// 7, which drew a stray link 640 px long across the panel's title.)
///
/// PROVISIONAL until the two readings marked above are traced (what `+0x90` / `+0x94` and the
/// level record's `+0x10` hold): "same column" is read as "same building chain" and the level as
/// `building_levels.level` (CAMPAIGN_UI.md "Technology tree links"). Computed once per HUD
/// ([`CampaignUi::tech_links`]), as the exe does at load.
fn technology_parent_offsets(db: &GameDatabase, required: &[(String, String)]) -> HashMap<String, (i32, i32)> {
    let place = |key: &str| {
        let t = db.technology(key)?;
        let b = db.building_level(&t.building_level)?;
        Some((b.chain.as_str(), b.level, t.tree_column))
    };
    let row = |level: i32, pos: i32| level * 2 + i32::from(pos > 3);
    let mut out = HashMap::new();
    for (t, r) in required {
        let (Some((t_chain, t_level, t_pos)), Some((r_chain, r_level, r_pos))) = (place(t), place(r)) else { continue };
        if t_chain == r_chain {
            out.insert(t.clone(), (r_pos % 4 - t_pos % 4, row(t_level, t_pos) - row(r_level, r_pos)));
        }
    }
    out
}

/// The technology tree's links: the `technology_required_technology_junctions` rows (technology,
/// required) in table order, and each technology's parent offsets ([`technology_parent_offsets`]).
pub(super) struct TechLinks {
    required: Vec<(String, String)>,
    parent_offsets: HashMap<String, (i32, i32)>,
}

impl CampaignUi {
    /// The technology tree's links, read from the data on first use (the data does not change
    /// while the HUD lives).
    fn tech_links(&self, inner: &Inner) -> &TechLinks {
        self.tech_links.get_or_init(|| {
            let required: Vec<(String, String)> =
                small_table(inner, &tables::TECHNOLOGY_REQUIRED_TECHNOLOGY_JUNCTIONS)
                    .iter()
                    .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(1)?.as_str()?.to_owned())))
                    .collect();
            let parent_offsets = technology_parent_offsets(&self.link.db, &required);
            TechLinks { required, parent_offsets }
        })
    }
}

/// The technology screen's data (`TechnologyPlayerDetails`, 0x009F9590 → 0x009C5630 →
/// 0x0099A5C0: 0x0099A550 "technologies" + 0x009A5F40 layout + 0x009A71A0 universities; CONFIRMED
/// key names). `{layout = {[category] = {[chain] = {[level] = {tech_entries = {[1..8] = entry},
/// constructed, constructable, building_icon_name, building_name, building_key, faction_key,
/// slots = {{key, address}...}}}}}, faction_key, universities = {...}, starting_university_index}`.
///
/// The tree (CONFIRMED shape): three categories (the screen's tabs: military, industry,
/// enlightenment), each a list of building chains (the layout's columns Civil, Military,
/// Industrial: sAdmin, sArmy, tFactory, the chains that hold technologies), each chain its levels (level < 6, buildable by
/// the faction) in level order, and each level eight positions (a technology's `tree_column`)
/// holding the technologies researched at that building level (`technologies.building_level`).
/// A tech entry (0x009ABB50): Key, Name, BuildingLevel, ChainPosition, PointsRequired,
/// ParentXoffset, ParentYoffset, IconFilename ("Data/UI/Campaign UI/Technologies/<text key>.tga"),
/// LongDescription, ShortDescription, Category, Record, dependancies = {[i] = entry + Status},
/// tech_status (Utilities.lua TECHNOLOGY_STATUS_*: 0 researched, 1 being researched, 2
/// available, 3 available to steal or trade, 4 unavailable, 5 not present).
/// INFERRED / PROVISIONAL: a technology's category (tab) is its key prefix (military / economy /
/// admin), every tab lists all three chains; status: researched when the
/// faction's technology list has it with state 0, being researched with state 1, otherwise
/// available when every required technology is researched, else unavailable (the exe's own test
/// 0x008B7850 is not decoded); research itself is not modelled, so no university is researching.
/// A technology's tab: its key's prefix (military / economy / admin; INFERRED).
fn tech_category(key: &str) -> usize {
    match key.split(|c: char| c.is_ascii_digit() || c == '_').next() {
        Some("military") => 0,
        Some("economy") => 1,
        _ => 2,
    }
}

/// The tech entry table `0x009ABB50` (the keys above but `Record`; `dependancies` and
/// `tech_status` are the callers').
fn tech_entry(lua: &Lua, inner: &Inner, parent_offsets: &HashMap<String, (i32, i32)>, t: &ntw_data::Technology) -> mlua::Result<Table> {
    let (parent_x, parent_y) = parent_offsets.get(t.key.as_str()).copied().unwrap_or((0, 0));
    let e = lua.create_table()?;
    e.set("Key", t.key.as_str())?;
    e.set("Name", loc(inner, &format!("technologies_onscreen_name_{}", t.key)).unwrap_or_else(|| t.key.clone()))?;
    e.set("BuildingLevel", t.building_level.as_str())?;
    e.set("ChainPosition", t.tree_column)?;
    e.set("PointsRequired", t.research_cost)?;
    e.set("ParentXoffset", parent_x)?;
    e.set("ParentYoffset", parent_y)?;
    e.set("IconFilename", format!("Data/UI/Campaign UI/Technologies/{}.tga", t.text_key))?;
    e.set("LongDescription", loc(inner, &format!("technologies_long_description_{}", t.key)).unwrap_or_default())?;
    e.set("ShortDescription", loc(inner, &format!("technologies_short_description_{}", t.key)).unwrap_or_default())?;
    e.set("Category", tech_category(&t.key))?;
    Ok(e)
}

/// The tech entry of `key` for the negotiation's technology lists (`0x009C5AA0` rows); `None`
/// when the technology is not in the data.
pub(super) fn negotiation_tech_entry(lua: &Lua, inner: &Inner, ui: &CampaignUi, key: &str) -> mlua::Result<Option<Table>> {
    let Some(t) = ui.link.db.technology(key) else { return Ok(None) };
    tech_entry(lua, inner, &ui.tech_links(inner).parent_offsets, t).map(Some)
}

fn technology_details(lua: &Lua, inner: &Inner, ui: &CampaignUi) -> mlua::Result<Value> {
    let db = &ui.link.db;
    let m = ui.model();
    let Some(f) = m.faction_by_key(&ui.link.human) else { return Ok(Value::Nil) };
    let (fid, faction_key) = (f.id, f.key.clone());
    let known: HashMap<String, u32> =
        m.world.faction_details.get(&fid).map(|d| d.technologies.iter().cloned().collect()).unwrap_or_default();
    // Required technologies (`technology_required_technology_junctions`: technology, required).
    let TechLinks { required, parent_offsets } = ui.tech_links(inner);
    let status = |key: &str| -> i32 {
        match known.get(key) {
            Some(0) => 0,
            Some(1) => 1,
            _ => {
                let deps_done = required.iter().filter(|(t, _)| t == key).all(|(_, r)| known.get(r) == Some(&0));
                if deps_done { 2 } else { 4 }
            }
        }
    };
    // Buildings standing per level key (constructed) and where.
    let mut standing: HashMap<String, Vec<(String, Value)>> = HashMap::new();
    for r in m.world.regions.values().filter(|r| r.owner == fid) {
        for (i, s) in r.slots.iter().enumerate() {
            if let Some(b) = &s.building {
                standing.entry(b.level_key.clone()).or_default().push((s.key.clone(), slot_value(ui, r.id, SlotRef::Slot(i))));
            }
        }
    }
    drop(m);
    let mut chains: Vec<String> = db.technologies.rows().iter().filter_map(|t| db.building_level(&t.building_level).map(|b| b.chain.clone())).collect();
    chains.sort();
    chains.dedup();
    let entry = |t: &ntw_data::Technology, with_deps: bool| -> mlua::Result<Table> {
        let e = tech_entry(lua, inner, parent_offsets, t)?;
        if with_deps {
            let deps = lua.create_table()?;
            for (i, (_, r)) in required.iter().filter(|(k, _)| *k == t.key).enumerate() {
                if let Some(rt) = db.technology(r) {
                    let d = lua.create_table()?;
                    d.set("Key", rt.key.as_str())?;
                    d.set("Name", loc(inner, &format!("technologies_onscreen_name_{}", rt.key)).unwrap_or_else(|| rt.key.clone()))?;
                    d.set("Status", status(&rt.key))?;
                    deps.set(i + 1, d)?;
                }
            }
            e.set("dependancies", deps)?;
        }
        Ok(e)
    };
    let layout = lua.create_table()?;
    for cat in 0..3 {
        let ct = lua.create_table()?;
        for (ci, chain) in chains.iter().enumerate() {
            let chain_t = lua.create_table()?;
            let mut levels: Vec<&ntw_data::BuildingLevel> = db.building_levels.rows().iter().filter(|b| &b.chain == chain && b.level < 6).collect();
            levels.sort_by_key(|b| b.level);
            for (li, b) in levels.iter().enumerate() {
                let lt = lua.create_table()?;
                let techs = lua.create_table()?;
                let (mut any_open, mut any_unavailable) = (false, false);
                for pos in 0..8 {
                    let te = match db.technologies.rows().iter().find(|t| t.building_level == b.key && t.tree_column == pos && tech_category(&t.key) == cat) {
                        Some(t) => {
                            let s = status(&t.key);
                            any_open |= s <= 3;
                            any_unavailable |= s == 4;
                            let e = entry(t, true)?;
                            e.set("tech_status", s)?;
                            e
                        }
                        None => {
                            let e = lua.create_table()?;
                            e.set("tech_status", 5)?;
                            e
                        }
                    };
                    techs.set(pos + 1, te)?;
                }
                lt.set("tech_entries", techs)?;
                let built = standing.get(&b.key);
                lt.set("constructed", built.is_some() || (any_open && !any_unavailable))?;
                lt.set("constructable", built.is_none())?;
                let icon = ui.building_icon(&faction_key, &b.key);
                let icon_name = icon.rsplit('/').next().unwrap_or("").trim_end_matches(".tga").to_owned();
                lt.set("building_icon_name", icon_name)?;
                lt.set("building_name", building_texts(inner, ui, &b.key).0)?;
                lt.set("building_key", b.key.as_str())?;
                lt.set("faction_key", faction_key.as_str())?;
                let slots = lua.create_table()?;
                for (si, (key, address)) in built.map(|v| v.as_slice()).unwrap_or_default().iter().enumerate() {
                    let s = lua.create_table()?;
                    s.set("key", key.as_str())?;
                    s.set("address", address.clone())?;
                    slots.set(si + 1, s)?;
                }
                lt.set("slots", slots)?;
                chain_t.set(li + 1, lt)?;
            }
            ct.set(ci + 1, chain_t)?;
        }
        layout.set(cat + 1, ct)?;
    }
    let t = lua.create_table()?;
    t.set("layout", layout)?;
    t.set("faction_key", faction_key.as_str())?;
    t.set("universities", lua.create_table()?)?;
    t.set("starting_university_index", 1)?;
    Ok(Value::Table(t))
}

pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // TechnologyPlayerDetails(entity) → see `technology_details`.
    f!("TechnologyPlayerDetails", |lua, inner, ui, _a: Variadic<Value>| technology_details(lua, &inner, &ui));
    Ok(())
}
