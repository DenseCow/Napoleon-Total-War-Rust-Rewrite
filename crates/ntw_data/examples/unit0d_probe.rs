//! Research probe for 0-D (units / animation / terrain / trees), read-only, prints to stdout.
//!
//! Sub-commands:
//! ```text
//! enum      every distinct value of every unit_stats_land column, with counts
//! stance    every unit: its class / training level and whether its animation table has
//!           STAND_TRAINED next to STAND (the idle-stance test's data side)
//! tables    every animation table, with the STAND / STAND_TRAINED slot pairs it resolves
//! treescale every preset battle map's TREE_ITEM scale byte: histogram and ranges
//! ```
use std::collections::BTreeMap;

use ntw_data::GameDatabase;
use ntw_formats::battle_animation::AnimationTables;
use ntw_formats::battle_terrain::{self, BattleMap};
use ntw_formats::db::{DbTable, Schema};
use ntw_formats::pack::Vfs;

/// One printed column of `unit_stats_land`: its name and how to render a row's value.
type Col<'a> = (&'static str, Box<dyn Fn(&ntw_data::UnitStatsLand) -> String + 'a>);

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).expect("open install");
    match args.first().map(String::as_str).unwrap_or("") {
        "enum" => enums(&vfs),
        "stance" => stance(&vfs),
        "tables" => tables(&vfs),
        "treescale" => treescale(&vfs),
        "flagpole" => flagpole(&vfs),
        "flagbones" => flagbones(&vfs),
        "equipbones" => equipbones(&vfs),
        "flagcloth" => flagcloth(&vfs),
        "wind" => wind(&vfs),
        "flagtex" => flagtex(&vfs),
        "manclass" => manclass(&vfs),
        "flagsplit" => flagsplit(&vfs),
        "treepermap" => treepermap(&vfs),
        "treegroup" => treegroup(&vfs),
        "treescale_clamps" => treescale_clamps(&vfs),
        "flagparents" => flagparents(&vfs),
        "flagcands" => flagcands(&vfs),
        "slotfort" => slotfort(&vfs),
        "wallcount" => wallcount(&vfs),
        _ => eprintln!("usage: unit0d_probe enum|stance|tables|treescale|flagpole|flagbones|equipbones|flagcloth|wind|flagtex|manclass|flagsplit|treepermap|treegroup|treescale_clamps|flagparents|flagcands|slotfort"),
    }
}

/// The **settlement's own fortification mesh**: `slots_art`'s `settlement` row per culture ->
/// `slots_templates_models` -> `<folder>\<stem>_<n>_slot_fortifications_lvl<level>.rigid_model`,
/// with `<level>` 0 for "not fortified yet" and the `sFortifications` chain level + 1 after it.
/// Prints, for every culture and every `<n>`, which levels exist and how big each mesh is next to
/// the plain `<stem>_<n>_slot.rigid_model` the map draws for an unfortified settlement -- which is
/// what says whether the fortification mesh *replaces* the plain one or is drawn over it.
fn slotfort(vfs: &Vfs) {
    let db = GameDatabase::from_vfs(vfs).unwrap();
    let set_levels: Vec<i32> = db
        .building_levels
        .iter()
        .filter(|l| l.chain == "sFortifications")
        .map(|l| l.level)
        .collect();
    println!("== sFortifications chain levels: {set_levels:?}");

    let size = |path: &str| -> Option<(usize, usize, [f32; 3], [f32; 3])> {
        let b = vfs.read(path).ok()?;
        let m = ntw_formats::rigid_model::RigidModel::read(&b).ok()?;
        let (mut lo, mut hi, mut nv, mut ni) = ([f32::MAX; 3], [f32::MIN; 3], 0usize, 0usize);
        for rm in &m.meshes {
            for v in &rm.vertices {
                for k in 0..3 {
                    lo[k] = lo[k].min(v.position[k]);
                    hi[k] = hi[k].max(v.position[k]);
                }
            }
            nv += rm.vertices.len();
            ni += rm.indices.len();
        }
        Some((nv, ni, lo, hi))
    };

    for r in db.campaign.slot_art.iter().filter(|r| r.slot_type == "settlement") {
        let Some(key) = r.template.as_deref() else { println!("{}: no template", r.culture); continue };
        let Some(t) = db.campaign.slot_template_model(key) else { println!("{key}: not a template"); continue };
        let folder = t.folder.replace('/', "\\");
        let stem = t.model.to_ascii_lowercase();
        println!("\n== culture {:<16} template {:<20} {:<10} {}", r.culture, key, t.model, folder);
        for n in 1..=6 {
            let plain = format!("{folder}\\{stem}_{n}_slot.rigid_model");
            let p = size(&plain);
            let mut line = format!("  n={n}  plain {}", match &p {
                Some((v, i, lo, hi)) => format!("{v}v/{i}i y {:.2}..{:.2} x {:.2}..{:.2}", lo[1], hi[1], lo[0], hi[0]),
                None => "MISSING".into(),
            });
            for level in ["0", "1", "2"] {
                let f = format!("{folder}\\{stem}_{n}_slot_fortifications_lvl{level}.rigid_model");
                line.push_str(&match size(&f) {
                    Some((v, i, lo, hi)) => format!(" | lvl{level} {v}v/{i}i y {:.2}..{:.2} x {:.2}..{:.2}", lo[1], hi[1], lo[0], hi[0]),
                    None => format!(" | lvl{level} -"),
                });
            }
            if p.is_some() || !line.ends_with(" | lvl2 -") {
                println!("{line}");
            }
        }
    }

    // What each level's meshes are *made of*: the mesh names and the textures they ask for. A
    // fortification mesh that asks for its own texture is drawn over the plain city, not instead
    // of it; one that asks for the city's own texture is a variant of it.
    println!("\n== mesh names and textures, plain vs fortification levels");
    for (culture, key, n) in [("european", "EU_Settlement", 2), ("middle_east", "OTT_Settlement", 2)] {
        let Some(t) = db.campaign.slot_template_model(key) else { continue };
        let folder = t.folder.replace('/', "\\");
        let stem = t.model.to_ascii_lowercase();
        for suffix in [format!("{stem}_{n}_slot.rigid_model"), format!("{stem}_{n}_slot_fortifications_lvl0.rigid_model"), format!("{stem}_{n}_slot_fortifications_lvl1.rigid_model"), format!("{stem}_{n}_slot_fortifications_lvl2.rigid_model")] {
            let path = format!("{folder}\\{suffix}");
            let Ok(b) = vfs.read(&path) else { println!("  {culture} {suffix}: MISSING"); continue };
            let Ok(m) = ntw_formats::rigid_model::RigidModel::read(&b) else { println!("  {culture} {suffix}: unreadable"); continue };
            println!("  {culture} {suffix}");
            for rm in &m.meshes {
                println!(
                    "      {}v  base {:?}  diffuse {:?}  normal {:?}  gloss {:?}",
                    rm.vertices.len(),
                    rm.material.base_name,
                    rm.material.diffuse.as_ref().map(|t| &t.name),
                    rm.material.normal.as_ref().map(|t| &t.name),
                    rm.material.gloss.as_ref().map(|t| &t.name)
                );
            }
        }
    }
}

/// How many settlements actually carry a fortification building in the shipped start positions --
/// the number that decides whether the walls are visible without building any first.
fn wallcount(vfs: &Vfs) {
    let files = ntw_formats::campaign_map::GameFiles { vfs }.list("campaigns/");
    let files: Vec<&String> = files.iter().filter(|p| p.to_ascii_lowercase().ends_with("startpos.esf")).collect();
    println!("== {} shipped start positions", files.len());
    let gf = ntw_formats::campaign_map::GameFiles { vfs };
    for f in &files {
        let Ok(b) = gf.read(f) else { println!("{f}: unreadable"); continue };
        let hay = String::from_utf8_lossy(&b);
        let count = |needle: &str| hay.matches(needle).count();
        println!(
            "{f:56} sFortifications {} fort1 {} fort2 {}",
            count("sFortifications"),
            count("sFortifications1_"),
            count("sFortifications2_")
        );
    }
    println!("\n== the settlement fortification level keys the table has");
    let db = GameDatabase::from_vfs(vfs).unwrap();
    for l in db.building_levels.iter().filter(|l| l.chain == "sFortifications") {
        println!("  {} level {} turns {} cost {}", l.key, l.level, l.construction_turns, l.cost);
    }
}

/// The last segment of a faction flag folder: `data\ui\flags\britain` -> `britain`.
fn flag_stem(p: &str) -> String {
    p.rsplit(['\\', '/']).next().unwrap_or(p).to_owned()
}

/// `faction_group` without its `_group` suffix: `great_britain_group` -> `great_britain`.
fn flag_group(f: &ntw_data::FactionRecord) -> String {
    f.faction_group.strip_suffix("_group").unwrap_or(&f.faction_group).to_owned()
}

/// The string handed to the battle flag lookup (`0x01227BD0` completes `flag_` on it): which of
/// the `factions` columns can be it? Prints every faction against every candidate column and
/// whether `flag_<candidate>.tga` is a key of the shipped battle atlas, and which candidates
/// DISAGREE with each other -- those are the ones an in-game comparison would settle.
fn flagcands(vfs: &Vfs) {
    let db = GameDatabase::from_vfs(vfs).unwrap();
    let atlas = ntw_formats::texture_atlas::TaiAtlas::read(
        &String::from_utf8(vfs.read(r"rigidmodels\flags\textures\flags.tai").expect("flags.tai")).expect("utf8"),
    )
    .expect("parse flags.tai");
    let in_atlas = |k: &str| atlas.find(&format!("flag_{k}.tga")).is_some();

    let mut names: std::collections::BTreeSet<String> = atlas.entries.keys().cloned().collect();
    println!("== the atlas holds {} images:", names.len());
    for n in &names {
        println!("   {n}");
    }
    names.remove("flag_default.tga");
    println!("   (minus flag_default.tga -> {})", names.len());

    // candidate columns -> the key they name for a faction
    type Candidate1 = fn(&ntw_data::FactionRecord) -> String;
    type Candidate = (&'static str, Candidate1);
    let cands: Vec<Candidate> = vec![
        ("key", |f: &ntw_data::FactionRecord| f.key.clone()),
        ("flag_path", |f: &ntw_data::FactionRecord| flag_stem(&f.flag_path)),
        ("model_faction", |f: &ntw_data::FactionRecord| f.model_faction.clone()),
        ("subculture", |f: &ntw_data::FactionRecord| f.subculture.clone()),
        ("republic_flag_path", |f: &ntw_data::FactionRecord| f.republic_flag_path.as_deref().map(flag_stem).unwrap_or_default()),
        ("rebel_flag_path", |f: &ntw_data::FactionRecord| f.rebel_flag_path.as_deref().map(flag_stem).unwrap_or_default()),
        ("faction_group", flag_group),
    ];
    let rows: Vec<_> = db.factions.iter().collect();
    let dependents: Vec<_> = rows.iter().copied().filter(|f| !in_atlas(&f.key)).collect();
    println!("\n== {} factions, {} of them with no flag_<key>.tga of their own", rows.len(), dependents.len());
    for f in &rows {
        let own = in_atlas(&f.key);
        if own {
            continue;
        }
        let got: Vec<String> = cands.iter().map(|(n, g)| format!("{n}={}{}", g(f), if in_atlas(&g(f)) { "" } else { "*" })).collect();
        println!("{:<24} {}", f.key, got.join("  "));
    }
    println!("   (* = no flag_<value>.tga in the atlas)");
    for (n, g) in &cands {
        let all = rows.iter().filter(|f| in_atlas(&g(f))).count();
        let dep = dependents.iter().filter(|f| in_atlas(&g(f))).count();
        let empty = dependents.iter().filter(|f| g(f).is_empty()).count();
        println!("{n:<20} resolves {all}/{} factions, {dep}/{} dependents, {empty} dependents give nothing", rows.len(), dependents.len());
    }
    // Where do two candidates that both resolve disagree? Those factions are the decisive ones.
    let resolving: Vec<&Candidate> = cands.iter().filter(|(n, _)| *n != "key").collect();
    println!("\n== disagreements between resolving candidates (for the 39 dependents)");
    for f in &dependents {
        let vals: std::collections::BTreeMap<&str, String> =
            resolving.iter().map(|(n, g)| (*n, g(f))).filter(|(_, v)| in_atlas(v)).collect();
        let distinct: std::collections::BTreeSet<&String> = vals.values().collect();
        if distinct.len() > 1 {
            let parts: Vec<String> = vals.iter().map(|(n, v)| format!("{n}={v}")).collect();
            println!("{:<24} {}", f.key, parts.join("  "));
        }
    }
    // Images no candidate names: if a flag exists for no faction at all, some other lookup uses it.
    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for f in &rows {
        for (_, g) in &cands {
            used.insert(format!("flag_{}.tga", g(f)));
        }
    }
    let unused: Vec<&String> = names.iter().filter(|n| !used.contains(*n)).collect();
    println!("\n== {} atlas images no candidate column names:", unused.len());
    for u in unused {
        println!("   {u}");
    }
}

/// The dependent factions that have no `flag_<key>.tga` of their own in `flags.tai`: which
/// shipped column says whose flag they fly. Prints every `factions` row with its faction group,
/// its flag-folder columns and whether its own flag image exists in the atlas.
fn flagparents(vfs: &Vfs) {
    let db = GameDatabase::from_vfs(vfs).unwrap();
    let atlas = ntw_formats::texture_atlas::TaiAtlas::read(
        &String::from_utf8(vfs.read(r"rigidmodels\flags\textures\flags.tai").expect("flags.tai")).expect("utf8"),
    )
    .expect("parse flags.tai");
    let own = |key: &str| atlas.find(&format!("flag_{key}.tga")).is_some();
    let rows: Vec<_> = db.factions.iter().collect();
    println!("== all {} faction rows (key, group, category, flag_path, republic, rebel, own image)", rows.len());
    for f in &rows {
        println!(
            "{:<22} group {:<24} cat {:<16} flag {:<28} rep {:<28} reb {:<22} own={}",
            f.key, f.faction_group, f.category, f.flag_path, f.republic_flag_path.as_deref().unwrap_or("-"), f.rebel_faction.as_deref().unwrap_or("-"), own(&f.key)
        );
    }
    let missing: Vec<_> = rows.iter().copied().filter(|f| !own(&f.key)).collect();
    println!("\n== the {} factions with no own image", missing.len());
    // Does the last segment of `flag_path` name an image in the *battle* atlas?
    let last = |p: &str| p.rsplit(['\\', '/']).next().unwrap_or(p).to_owned();
    let mut via_flag_path = 0;
    let mut via_republic = 0;
    let mut neither = Vec::new();
    for f in &missing {
        let stem = last(&f.flag_path);
        if own(&stem) {
            via_flag_path += 1;
        } else if f.republic_flag_path.as_deref().is_some_and(|r| own(&last(r))) {
            via_republic += 1;
        } else {
            neither.push(f.key.clone());
        }
    }
    println!("flag_path's last segment is a battle-atlas key for {via_flag_path} of {}", missing.len());
    println!("republic_flag_path's for {via_republic} more");
    println!("neither: {neither:?}");
    let mut by_stem: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for f in &missing {
        by_stem.entry(Box::leak(last(&f.flag_path).into_boxed_str())).or_default().push(f.key.as_str());
    }
    for (stem, keys) in &by_stem {
        println!("{stem:<22} <- {}", keys.join(" "));
    }
    let groups: std::collections::BTreeMap<&str, Vec<&str>> =
        missing.iter().fold(std::collections::BTreeMap::new(), |mut m, f| {
            m.entry(f.faction_group.as_str()).or_default().push(f.key.as_str());
            m
        });
    for (group, keys) in &groups {
        // Does the group name a real faction that does have its own image?
        let stem = group.strip_suffix("_group").unwrap_or(group);
        let hit = own(stem);
        let sibling = rows.iter().filter(|f| f.faction_group == *group).map(|f| f.key.as_str()).find(|k| own(k));
        println!("{group:<26} -> {stem:<20} own image: {hit}   a sibling with one: {sibling:?}\n{:<28}{}", "", keys.join(" "));
    }
}

/// Every distinct value of the string / small-int columns of `unit_stats_land`, with counts.
fn enums(vfs: &Vfs) {
    let db = GameDatabase::from_vfs(vfs).unwrap();
    let n = db.unit_stats_land.rows().len();
    println!("unit_stats_land rows {n}");
    let cols: Vec<Col<'_>> = vec![
        ("category", Box::new(|s: &ntw_data::UnitStatsLand| format!("{:?}", db_unit_category(&db, s)))),
        ("unit_class", Box::new(|s: &ntw_data::UnitStatsLand| format!("{:?}", db_unit_class(&db, s)))),
        ("training_level", Box::new(|s: &ntw_data::UnitStatsLand| s.training_level.clone())),
        ("ai_role", Box::new(|s: &ntw_data::UnitStatsLand| format!("{:?}", s.unknown_238))),
        ("animation_culture_set", Box::new(|s: &ntw_data::UnitStatsLand| s.animation_culture_set.clone())),
        ("man_animation_type", Box::new(|s: &ntw_data::UnitStatsLand| s.man_animation_type.clone())),
        ("man_entity", Box::new(|s: &ntw_data::UnitStatsLand| s.man_entity.clone())),
        ("weapon_anim_group", Box::new(|s: &ntw_data::UnitStatsLand| s.weapon_anim_group.clone())),
        ("drill_set", Box::new(|s: &ntw_data::UnitStatsLand| s.drill_set.clone())),
        ("melee_animation_category", Box::new(|s: &ntw_data::UnitStatsLand| s.melee_animation_category.clone())),
        ("melee_weapon_type", Box::new(|s: &ntw_data::UnitStatsLand| s.melee_weapon_type.clone())),
        ("missile_weapon_class", Box::new(|s: &ntw_data::UnitStatsLand| format!("{:?}", s.missile_weapon_class))),
        ("gun_type", Box::new(|s: &ntw_data::UnitStatsLand| format!("{:?}", s.gun_type))),
        ("default_ranks", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.default_ranks))),
        ("unknown_205(militia)", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_205))),
        ("unknown_20c", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_20c))),
        ("unknown_20f", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_20f))),
        ("unknown_238(guerrilla)", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_238))),
        ("is_artillery", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.is_artillery))),
    ];
    for (name, get) in cols {
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        for s in db.unit_stats_land.iter() {
            *m.entry(get(s)).or_default() += 1;
        }
        println!("\n== {name}: {} distinct", m.len());
        for (v, c) in &m {
            println!("   {c:5}  {v}");
        }
    }
}

fn db_unit_category<'a>(db: &'a GameDatabase, s: &ntw_data::UnitStatsLand) -> Option<&'a str> {
    db.units.get(&s.key).map(|u| u.category.as_str())
}
fn db_unit_class<'a>(db: &'a GameDatabase, s: &ntw_data::UnitStatsLand) -> Option<&'a str> {
    db.units.get(&s.key).map(|u| u.unit_class.as_str())
}

/// Does a unit's men's animation table carry the `_TRAINED` stand/walk/run family next to the
/// plain one? Cross-tabbed with the training level and the unit class.
fn stance(vfs: &Vfs) {
    let db = GameDatabase::from_vfs(vfs).unwrap();
    let tables = AnimationTables::from_vfs(vfs).unwrap();
    let mut cross: BTreeMap<(String, String, bool, bool), usize> = BTreeMap::new();
    for s in db.unit_stats_land.iter() {
        let t = &s.man_animation_type;
        let trained = !tables.resolve(t, "STAND_TRAINED").is_empty();
        let plain = !tables.resolve(t, "STAND").is_empty();
        let cat = db.units.get(&s.key).map(|u| u.unit_class.clone()).unwrap_or_default();
        *cross.entry((s.training_level.clone(), cat, trained, plain)).or_default() += 1;
    }
    println!("(training_level, unit_class, has STAND_TRAINED, has STAND) -> units");
    for ((lvl, cat, tr, pl), c) in &cross {
        println!("{c:5}  {lvl:<16} {cat:<28} trained={tr} stand={pl}");
    }
    println!("\n== per unit");
    for s in db.unit_stats_land.iter() {
        let t = &s.man_animation_type;
        let n_t = tables.resolve(t, "STAND_TRAINED").len();
        let n_s = tables.resolve(t, "STAND").len();
        let w_t = tables.resolve(t, "WALK_TRAINED_2").len();
        let w_s = tables.resolve(t, "WALK_2").len();
        let d_t = tables.resolve(t, "DEATH_STAND_TRAINED_1").len();
        let cat = db.units.get(&s.key).map(|u| u.unit_class.clone()).unwrap_or_default();
        println!(
            "{:<34} {:<26} {:<15} {:<16} table {:<24} trained {n_t}/{n_s} walk {w_t}/{w_s} death {d_t}",
            s.key, cat, s.training_level, s.animation_culture_set, t
        );
    }
}

/// Every animation table, and which of the stand / walk / run families it resolves.
fn tables(vfs: &Vfs) {
    let tables = AnimationTables::from_vfs(vfs).unwrap();
    let mut all_names: Vec<&str> = tables.table_names().collect();
    all_names.sort();
    let names = all_names;
    let mut with = 0;
    for name in &names {
        let row: Vec<String> = [
            ("STAND", 1usize),
            ("STAND_TRAINED", 0),
            ("WALK_2", 0),
            ("WALK_TRAINED_2", 0),
            ("RUN_2", 0),
            ("RUN_TRAINED_2", 0),
            ("DEATH_STAND_1", 0),
            ("DEATH_STAND_TRAINED_1", 0),
        ]
        .iter()
        .map(|(s, _)| format!("{s}={}", tables.resolve(name, s).len()))
        .collect();
        let t = tables.resolve(name, "STAND_TRAINED").len();
        with += usize::from(t > 0);
        println!("{name:<30} {}", row.join(" "));
    }
    println!("\n{} tables, {with} with STAND_TRAINED", names.len());
}

/// Every preset battle map's tree-instance scale byte.
fn treescale(vfs: &Vfs) {
    let names = battle_terrain::list_presets(vfs);
    let mut all = [0usize; 256];
    let mut total = 0usize;
    let mut maps = 0usize;
    let mut per_species: BTreeMap<String, (usize, u32, u32, BTreeMap<u32, usize>)> = BTreeMap::new();
    let mut per_map: Vec<(String, usize, u32, u32)> = Vec::new();
    for name in &names {
        let Ok(map) = BattleMap::load(vfs, name) else { continue };
        maps += 1;
        let mut n = 0;
        let (mut lo, mut hi) = (255u32, 0u32);
        for list in &map.trees {
            for g in &list.groups {
                let e = per_species.entry(g.species.clone()).or_insert((0, 255, 0, BTreeMap::new()));
                for i in &g.instances {
                    all[i.variation as usize] += 1;
                    total += 1;
                    n += 1;
                    lo = lo.min(i.variation as u32);
                    hi = hi.max(i.variation as u32);
                    e.0 += 1;
                    e.1 = e.1.min(i.variation as u32);
                    e.2 = e.2.max(i.variation as u32);
                    *e.3.entry(i.variation as u32).or_default() += 1;
                }
            }
        }
        if n > 0 {
            per_map.push((name.clone(), n, lo, hi));
        }
    }
    println!("{maps} maps, {total} tree instances");
    println!("\n== global histogram (value: count)");
    let mut row = String::new();
    for (v, n) in all.iter().enumerate() {
        if *n > 0 {
            row.push_str(&format!("{v}:{n} "));
        }
    }
    println!("{row}");
    let nonzero: Vec<usize> = (0..256).filter(|&v| all[v] > 0).collect();
    let lo = *nonzero.first().unwrap();
    let hi = *nonzero.last().unwrap();
    let sum: usize = (0..256).map(|v| v * all[v]).sum();
    println!("min {lo} max {hi} distinct {} mean {:.1}", nonzero.len(), sum as f32 / total as f32);
    println!("\n== per map (instances, min, max)");
    for (n, c, lo, hi) in &per_map {
        println!("{n:<32} {c:7} {lo:4} {hi:4}");
    }
    println!("\n== per species: n, min, max, distinct count, top values");
    for (k, (c, lo, hi, hist)) in &per_species {
        let mut top: Vec<(&u32, &usize)> = hist.iter().collect();
        top.sort_by_key(|(v, c)| (std::cmp::Reverse(**c), **v));
        let tops: Vec<String> = top.iter().take(4).map(|(v, c)| format!("{v}x{c}")).collect();
        println!("{k:<44} {c:7} {lo:4} {hi:4} {:4}  {}", hist.len(), tops.join(" "));
    }
    // The raw training-level table, for the enum order.
    let bytes = vfs.read(r"db\entity_training_levels_tables\entity_training_levels").unwrap();
    let t = DbTable::read(&bytes, &Schema::from_codes("s,f").unwrap()).unwrap();
    println!("\n== entity_training_levels ({} rows, file order)", t.rows.len());
    for r in &t.rows {
        println!("   {r:?}");
    }
}

/// The flagpole rigid attachment in `unitmodels\euro_equipment.variant_weighted_mesh`: its
/// vertices' bounding box, next to the verlet particle coordinates of
/// `rigidmodels\verletitems\standard_bearer_flag.logic`.
fn flagpole(vfs: &Vfs) {
    use ntw_formats::weighted_mesh::WeightedMesh;
    let path = r"unitmodels\euro_equipment.variant_weighted_mesh";
    let mesh = WeightedMesh::read(&vfs.read(path).unwrap()).unwrap();
    println!("{path}: version {}, {} pieces, {} attachments", mesh.version, mesh.pieces.len(), mesh.attachments.len());
    println!("scalars {:?}", mesh.scalars);
    for a in &mesh.attachments {
        if !a.name.to_ascii_lowercase().contains("flag") && !a.name.to_ascii_lowercase().contains("mast") {
            continue;
        }
        let vs = a.vertex_size;
        let n = a.vertices.len() / vs;
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for i in 0..n {
            for k in 0..3 {
                let v = a.vertices[i * vs + k];
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
        }
        println!(
            "{:<40} v={n} idx={} vs={vs} min [{:.3}, {:.3}, {:.3}] max [{:.3}, {:.3}, {:.3}]  tex {:?} {:?}",
            a.name, a.indices.len(), lo[0], lo[1], lo[2], hi[0], hi[1], hi[2], a.unknown, a.textures
        );
        // The first few vertices raw, so the layout is visible.
        for i in 0..n.min(4) {
            println!("    v{i}: {:?}", &a.vertices[i * vs..i * vs + vs]);
        }
    }
}
/// The flagpole's bone and the standard bearer's skeleton: which bone carries
/// `rigid_equip_euro_flagpole01`, and where that bone sits in the standing clip.
fn flagbones(vfs: &Vfs) {
    use ntw_formats::anim::Anim;
    use ntw_formats::unit_model::{EQUIPMENT_CONTAINERS, EquipmentLibrary};
    let lib = EquipmentLibrary::from_vfs(vfs).unwrap();
    println!("equipment pieces {}", lib.len());
    for name in ["rigid_equip_euro_flagpole01", "rigid_equip_euro_flagpole01_lod1", "rigid_equip_euro_hanger01"] {
        match lib.find(name) {
            Some((p, stem)) => {
                let vs = p.lod.decode_vertices(ntw_formats::unit_variant::VariantVertexFormat::Bytes64);
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for v in &vs {
                    for k in 0..3 {
                        lo[k] = lo[k].min(v.positions[0][k]);
                        hi[k] = hi[k].max(v.positions[0][k]);
                    }
                }
                println!(
                    "{name:<36} bone {:?} verts {} idx {} min [{:.3}, {:.3}, {:.3}] max [{:.3}, {:.3}, {:.3}] stem {stem}",
                    p.bone, vs.len(), p.lod.indices.len(), lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
                );
            }
            None => println!("{name}: not found"),
        }
    }
    println!("containers {:?}", EQUIPMENT_CONTAINERS);
    for clip in [
        "Animations/MEN/FLAG_BEARER/FLA_STAND/FLA_StandT.anim",
        "Animations/MUSKET/MUS_STAND/MUS_Stand.anim",
    ] {
        let Ok(b) = vfs.read(clip) else { println!("{clip}: missing"); continue };
        let a = Anim::read(&b).unwrap();
        println!("\n{clip}: {} fps, {:.3} s, {} bones, {} frames", a.frame_rate, a.duration, a.bones.len(), a.frames.len());
        for (i, bo) in a.bones.iter().enumerate() {
            println!("  {i:3} {:<28} parent {:?}", bo.name, bo.parent);
        }
        // Frame 0 bone origins, so a bone can be placed in metres.
        let wm = a.world_matrices(0);
        for (i, m) in wm.iter().enumerate() {
            println!("  bone {i:3} {:<28} origin ({:.3}, {:.3}, {:.3})", a.bones[i].name, m[12], m[13], m[14]);
        }
    }
}

/// Every equipment piece's bone and every weighted-mesh attachment name, plus the three
/// `Weapon` bones' axes in the standard bearer's standing clip.
fn equipbones(vfs: &Vfs) {
    use ntw_formats::anim::Anim;
    use ntw_formats::weighted_mesh::WeightedMesh;
    let mut bones: BTreeMap<Option<u32>, Vec<String>> = BTreeMap::new();
    for (path, _) in ntw_formats::unit_model::EQUIPMENT_CONTAINERS {
        let mesh = ntw_formats::unit_variant::VariantPartMesh::read(&vfs.read(path).unwrap()).unwrap();
        if let ntw_formats::unit_variant::VariantPartMeshBody::EquipmentContainer { pieces, .. } = mesh.body {
            for p in &pieces {
                bones.entry(p.bone).or_default().push(p.name.clone());
            }
        }
    }
    for (b, names) in &mut bones {
        names.sort();
        println!("bone {b:?}: {} pieces  {}", names.len(), names.join(" "));
    }
    let mesh = WeightedMesh::read(&vfs.read(r"unitmodels\euro_equipment.variant_weighted_mesh").unwrap()).unwrap();
    println!("\n{} weighted-mesh attachments:", mesh.attachments.len());
    for (i, a) in mesh.attachments.iter().enumerate() {
        println!("  {i:3} {}", a.name);
    }
    let a = Anim::read(&vfs.read("Animations/MEN/FLAG_BEARER/FLA_STAND/FLA_StandT.anim").unwrap()).unwrap();
    let wm = a.world_matrices(0);
    for i in [0usize, 1, 2, 3, 19, 29, 30, 6, 10, 13] {
        let m = wm[i];
        println!(
            "bone {i:3} {:<14} origin ({:.3}, {:.3}, {:.3}) x ({:.3}, {:.3}, {:.3}) y ({:.3}, {:.3}, {:.3}) z ({:.3}, {:.3}, {:.3})",
            a.bones[i].name, m[12], m[13], m[14], m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]
        );
    }
}

/// The flag texture atlas: the raw bytes of `flags.tai` (so its layout is visible) and the size
/// of every flag DDS in the folder.
fn flagtex(vfs: &Vfs) {
    for p in [
        r"rigidmodels\flags\textures\flags.tai",
        r"rigidmodels\flags\textures\exp_flags.tai",
    ] {
        let Ok(b) = vfs.read(p) else { println!("{p}: missing"); continue };
        println!("\n{p}: {} bytes", b.len());
        for (row, chunk) in b.chunks(16).enumerate() {
            let hex: Vec<String> = chunk.iter().map(|c| format!("{c:02x}")).collect();
            let asc: String = chunk.iter().map(|c| if c.is_ascii_graphic() { *c as char } else { '.' }).collect();
            println!("  {:06x}  {:<47}  {}", row * 16, hex.join(" "), asc);
        }
    }
    println!();
    for p in vfs.list(r"rigidmodels\flags\textures\").into_iter().filter(|p| p.to_ascii_lowercase().ends_with(".dds")) {
        let Ok(b) = vfs.read(p) else { continue };
        match ntw_formats::dds::Dds::parse(&b) {
            Ok(d) => println!("{p:60} {}x{} mips {}", d.width, d.height, d.mip_count),
            Err(e) => println!("{p:60} dds error {e}"),
        }
    }
    println!("\nflag_*.tga / .tga anywhere in the flags folder or the pack:");
    for p in vfs.list("").into_iter().filter(|p| {
        let l = p.to_ascii_lowercase();
        l.contains("flag") && (l.ends_with(".tga") || l.ends_with(".dds"))
    }) {
        println!("  {p}");
    }
}

/// Every shipped battle file's `weather/prevailing_wind`: the vector the flag cloth and the
/// trees would take their wind from. Ranges and the direction convention matter.
fn wind(vfs: &Vfs) {
    let mut files: Vec<String> = vfs
        .list("")
        .into_iter()
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            (l.contains("historical_battles\\") || l.contains("\\battles\\") || l.contains("custom_battles\\")) && l.ends_with(".xml")
        })
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    println!("{} candidate battle files", files.len());
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut lo = [f32::MAX; 2];
    let mut hi = [f32::MIN; 2];
    let mut none = 0usize;
    for f in &files {
        let Ok(b) = vfs.read(f) else { continue };
        let Ok(spec) = ntw_formats::battle_spec::BattleSpec::parse(&b) else { continue };
        match spec.prevailing_wind {
            None => none += 1,
            Some(w) => {
                *seen.entry(format!("{:.4} {:.4}", w.0, w.1)).or_default() += 1;
                for k in 0..2 {
                    lo[k] = lo[k].min([w.0, w.1][k]);
                    hi[k] = hi[k].max([w.0, w.1][k]);
                }
                if f.to_ascii_lowercase().contains("austerlitz") {
                    println!("  {f}: prevailing_wind ({:.4}, {:.4})", w.0, w.1);
                }
            }
        }
    }
    println!("no prevailing_wind: {none} files; {seen:?}");
    println!("x range {:.4} .. {:.4}   y range {:.4} .. {:.4}", lo[0], hi[0], lo[1], hi[1]);
}

/// The drawn pole piece and the cloth it carries: every equipment piece with "flagpole" in its
/// name (bone, bounds in bone-local metres), then the whole shipped `.logic` with each rope's
/// rest length and each pin's rigid target, and the flag texture atlas.
fn flagcloth(vfs: &Vfs) {
    use ntw_formats::unit_variant::VariantVertexFormat;
    println!("== equipment pieces named *flagpole*");
    for (path, _) in ntw_formats::unit_model::EQUIPMENT_CONTAINERS {
        let mesh = ntw_formats::unit_variant::VariantPartMesh::read(&vfs.read(path).unwrap()).unwrap();
        if let ntw_formats::unit_variant::VariantPartMeshBody::EquipmentContainer { pieces, .. } = mesh.body {
            for p in &pieces {
                if !p.name.to_ascii_lowercase().contains("flagpole") {
                    continue;
                }
                let vs = p.lod.decode_vertices(VariantVertexFormat::Bytes64);
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for v in &vs {
                    for k in 0..3 {
                        lo[k] = lo[k].min(v.positions[0][k]);
                        hi[k] = hi[k].max(v.positions[0][k]);
                    }
                }
                println!(
                    "{path}\n  {:<34} bone {:?} verts {} idx {}\n    bone-local min [{:.4}, {:.4}, {:.4}] max [{:.4}, {:.4}, {:.4}]  (extent {:.3} x {:.3} x {:.3})",
                    p.name, p.bone, vs.len(), p.lod.indices.len(),
                    lo[0], lo[1], lo[2], hi[0], hi[1], hi[2],
                    hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]
                );
            }
        }
    }

    println!("\n== verlet items");
    let mut names: Vec<String> = vfs
        .list(r"rigidmodels\verletitems\")
        .into_iter()
        .filter(|p| p.ends_with(".logic"))
        .map(|p| p.to_owned())
        .collect();
    names.sort();
    for path in &names {
        let text = String::from_utf8(vfs.read(path).unwrap()).unwrap();
        let l = ntw_formats::verlet::VerletLogic::read(&text).unwrap();
        println!("\n{path}: version {}, {} rigids, {} items", l.version, l.rigids.len(), l.items.len());
        for r in &l.rigids {
            println!("  rigid {}: mass {} radius {}", r.name, r.mass, r.radius);
            for (n, p) in &r.particles {
                println!("    particle {n:<8} ({:.4}, {:.4}, {:.4})", p.p[0], p.p[1], p.p[2]);
            }
        }
        for it in &l.items {
            println!(
                "  verlet_item {}: mass {} area {} texture {} drag {} gravity {} particles {} triangles {} ropes {}",
                it.name, it.mass, it.surface_area, it.texture_prefix, it.drag_coefficient, it.gravity_coefficient,
                it.particles.len(), it.triangles.len(), it.ropes.len()
            );
            for (i, p) in &it.particles {
                let t = p.t.map_or("      ".to_string(), |t| format!("({:.3}, {:.3})", t[0], t[1]));
                println!("    particle {i:>3} ({:>8.4}, {:>8.4}, {:>8.4}) t {}", p.p[0], p.p[1], p.p[2], t);
            }
            let mut min_r = f32::MAX;
            let mut max_r = f32::MIN;
            for r in &it.ropes {
                let (pa, pb, kind) = match (r.b, &r.rigid) {
                    (Some(b), _) => (it.particles[&r.a].p, it.particles[&b].p, "cloth"),
                    (None, Some(rg)) => {
                        let (name, part) = rg.rsplit_once('.').unwrap();
                        let rigid = l.rigid(name).unwrap();
                        (it.particles[&r.a].p, rigid.particles[part].p, "rigid")
                    }
                    _ => continue,
                };
                let d = ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt();
                min_r = min_r.min(d);
                max_r = max_r.max(d);
                println!(
                    "    rope {:<14} {} -> {} [{kind}] rest {d:.4} len{}",
                    r.name, r.a, r.b.map_or_else(|| r.rigid.clone().unwrap_or_default(), |b| b.to_string()),
                    if r.invisible { " invisible" } else { "" }
                );
            }
            // The pin offsets: how far each pinned cloth particle sits from its rigid target.
            for (i, rg) in it.pinned() {
                let (name, part) = rg.rsplit_once('.').unwrap();
                let rigid = l.rigid(name).unwrap();
                let a = it.particles[i].p;
                let b = rigid.particles[part].p;
                println!(
                    "    pin {i:>3} -> {rg}: d ({:.4}, {:.4}, {:.4}) |d| {:.4}",
                    b[0] - a[0], b[1] - a[1], b[2] - a[2],
                    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
                );
            }
            println!("    rope rest length {} .. {}", min_r, max_r);
            // Grid shape: how the (u, v) pairs are laid out.
            let mut rows: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for (i, p) in &it.particles {
                let t = p.t.unwrap_or([0.0, 0.0]);
                rows.entry((t[0] * 1000.0).round() as u32).or_default().push(*i);
            }
            println!("    u columns: {}", rows.iter().map(|(k, v)| format!("{:.3}x{}", *k as f32 / 1000.0, v.len())).collect::<Vec<_>>().join(" "));
        }
    }

    println!("\n== flag textures");
    for p in vfs.list(r"rigidmodels\flags\textures\") {
        println!("  {p}");
    }
}

/// `man_animation_type` x `unit_class` and x `drill_set`: does the men's animation table key
/// itself split trained from irregular?
fn manclass(vfs: &Vfs) {
    let db = GameDatabase::from_vfs(vfs).unwrap();
    let mut by_table: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut by_drill: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut by_cat: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut by_level: BTreeMap<(String, String), usize> = BTreeMap::new();
    for s in db.unit_stats_land.iter() {
        let cls = db.units.get(&s.key).map(|u| u.unit_class.clone()).unwrap_or_default();
        *by_table.entry(s.man_animation_type.clone()).or_default().entry(cls.clone()).or_default() += 1;
        *by_drill.entry((s.man_animation_type.clone(), s.drill_set.clone())).or_default() += 1;
        *by_cat.entry((s.man_animation_type.clone(), s.melee_animation_category.clone())).or_default() += 1;
        *by_level.entry((s.man_animation_type.clone(), s.training_level.clone())).or_default() += 1;
    }
    println!("== man_animation_type x unit_class");
    for (t, m) in &by_table {
        println!("{t}:");
        for (c, n) in m {
            println!("   {n:5}  {c}");
        }
    }
    println!("\n== man_animation_type x drill_set");
    for ((t, d), n) in &by_drill {
        println!("{n:5}  {t:<24} {d}");
    }
    println!("\n== man_animation_type x melee_animation_category");
    for ((t, d), n) in &by_cat {
        println!("{n:5}  {t:<24} {d}");
    }
    println!("\n== man_animation_type x training_level");
    for ((t, d), n) in &by_level {
        println!("{n:5}  {t:<24} {d}");
    }
}

/// Which `unit_stats_land` flag / column splits the irregular classes (infantry_militia,
/// infantry_mob, infantry_irregulars, infantry_skirmishers) off the drilled ones?
fn flagsplit(vfs: &Vfs) {
    let db = GameDatabase::from_vfs(vfs).unwrap();
    let irregular = ["infantry_militia", "infantry_mob", "infantry_irregulars", "infantry_skirmishers", "infantry_light"];
    let cols: Vec<Col<'_>> = vec![
        ("unknown_204", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_204))),
        ("unknown_205 militia", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_205))),
        ("unknown_206", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_206))),
        ("unknown_207", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_207))),
        ("unknown_208", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_208))),
        ("unknown_209", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_209))),
        ("unknown_20a", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_20a))),
        ("unknown_20b", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_20b))),
        ("unknown_20c", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_20c))),
        ("unknown_20d", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_20d))),
        ("unknown_20e", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_20e))),
        ("unknown_20f", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_20f))),
        ("unknown_210", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_210))),
        ("unknown_211", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_211))),
        ("unknown_212", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_212))),
        ("unknown_213", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_213))),
        ("unknown_1e8", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1e8))),
        ("unknown_1e9", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1e9))),
        ("unknown_1ea", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1ea))),
        ("unknown_1eb", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1eb))),
        ("unknown_1ec", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1ec))),
        ("unknown_1ed", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1ed))),
        ("unknown_1ee", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1ee))),
        ("unknown_1ef", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1ef))),
        ("unknown_1f0", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1f0))),
        ("unknown_1f1", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1f1))),
        ("unknown_1f2", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1f2))),
        ("unknown_1f3", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1f3))),
        ("unknown_1f4", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1f4))),
        ("unknown_1f5", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1f5))),
        ("unknown_7c", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_7c))),
        ("unknown_188", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_188))),
        ("unknown_1e4", Box::new(|s: &ntw_data::UnitStatsLand| format!("{}", s.unknown_1e4))),
    ];
    for (name, get) in cols {
        let mut hit = BTreeMap::<String, (usize, usize)>::new();
        for s in db.unit_stats_land.iter() {
            let cls = db.units.get(&s.key).map(|u| u.unit_class.clone()).unwrap_or_default();
            let irr = irregular.contains(&cls.as_str());
            let e = hit.entry(get(s)).or_default();
            if irr { e.1 += 1 } else { e.0 += 1 }
        }
        let perfect = hit.values().any(|(a, b)| *a == 0 || *b == 0);
        println!("{name:<22} {hit:?}{}", if perfect { "   <- SPLITS CLEANLY" } else { "" });
    }
    println!("\n== the irregular-ish units, by key");
    for s in db.unit_stats_land.iter() {
        let cls = db.units.get(&s.key).map(|u| u.unit_class.clone()).unwrap_or_default();
        if irregular.contains(&cls.as_str()) {
            println!(
                "{:<34} {:<24} {:<15} {:<24} lvl {:<14} militia {} 1e8 {} 1e9 {} 1ea {} 1eb {} 1ec {} 1f0 {}",
                s.key, cls, s.man_animation_type, s.drill_set, s.training_level,
                s.unknown_205 as u8, s.unknown_1e8 as u8, s.unknown_1e9 as u8, s.unknown_1ea as u8,
                s.unknown_1eb as u8, s.unknown_1ec as u8, s.unknown_1f0 as u8
            );
        }
    }
}

/// Per-map, per-list and per-species histograms of the non-zero scale bytes, and the I32 flags.
fn treepermap(vfs: &Vfs) {
    let names = battle_terrain::list_presets(vfs);
    for name in &names {
        let Ok(map) = BattleMap::load(vfs, name) else { continue };
        let mut h = [0usize; 256];
        let mut n = 0usize;
        let mut lists = 0usize;
        for list in &map.trees {
            let mut lh = [0usize; 256];
            for g in &list.groups {
                for i in &g.instances {
                    h[i.variation as usize] += 1;
                    lh[i.variation as usize] += 1;
                    if i.variation != 0 { n += 1 }
                    
                }
            }
            let tot: usize = lh.iter().sum();
            if tot == 0 { continue }
            lists += 1;
            let nz: Vec<usize> = (1..256).filter(|&v| lh[v] > 0).collect();
            let lo = *nz.first().unwrap_or(&0);
            let hi = *nz.last().unwrap_or(&0);
            let spark = |lo: usize, hi: usize| -> String {
                (lo..=hi).map(|v| format!("{v}:{}", lh[v])).collect::<Vec<_>>().join(" ")
            };
            println!("{name:<30} list{tot:>8} nz {n:>7} min {lo:>3} max {hi:>3}  {}", if hi > 0 { spark(lo, hi.min(lo + 40)) } else { String::new() });
        }
        if n == 0 { println!("{name:<30} no scaled list"); continue }
        println!("{name:<30} == {lists} lists, {n} scaled instances");
        println!("   total {}", (1..256).map(|v| format!("{v}:{}", h[v])).filter(|s| !s.ends_with(":0")).collect::<Vec<_>>().join(" "));
    }
}

/// One map, one scaled list: every species group's own byte range and histogram. A uniform
/// spread inside one group means a random scale in an artist-chosen band; spiky groups mean
/// quantised values.
fn treegroup(vfs: &Vfs) {
    let name = std::env::args().nth(2).unwrap_or_else(|| "hb_waterloo".into());
    let map = BattleMap::load(vfs, &name).expect("map");
    for (li, list) in map.trees.iter().enumerate() {
        let tot: usize = list.groups.iter().map(|g| g.instances.len()).sum();
        if list.groups.iter().all(|g| g.instances.iter().all(|i| i.variation == 0)) { continue }
        println!("== {name} list {li} flag {} groups {} instances {tot}", list.flag, list.groups.len());
        for g in &list.groups {
            let mut h = [0usize; 256];
            for i in &g.instances { h[i.variation as usize] += 1 }
            let nz: Vec<usize> = (1..256).filter(|&v| h[v] > 0).collect();
            if nz.is_empty() { continue }
            let lo = nz[0]; let hi = *nz.last().unwrap();
            let mut top: Vec<(usize, usize)> = nz.iter().map(|&v| (h[v], v)).collect();
            top.sort_by_key(|a| std::cmp::Reverse(a.0));
            let top: Vec<String> = top.iter().take(6).map(|(c, v)| format!("{v}x{c}")).collect();
            println!("  {:<46} n {:>6} {lo:>3}..{hi:>3} distinct {:>3} mean {:6.1}  {}",
                g.species, g.instances.len(), nz.len(),
                g.instances.iter().map(|i| i.variation as f32).sum::<f32>() / g.instances.len() as f32,
                top.join(" "));
            println!("      {}", nz.iter().map(|&v| format!("{v}:{}", h[v])).collect::<Vec<_>>().join(" "));
        }
    }
}

/// The decisive test for the tree scale byte: MIN_TREE_SCALE 0.5 and MAX_TREE_SCALE 1.4 with
/// `scale = u8 / 128` put the clamps at byte 64 (0.5) and byte 179 (1.398). If that is the
/// decode, the shipped bytes must stop dead at 179/180 and at 63/64.
fn treescale_clamps(vfs: &Vfs) {
    let names = battle_terrain::list_presets(vfs);
    let mut h = [0usize; 256];
    let mut per_list: Vec<(String, usize, usize, u32, u32)> = Vec::new();
    let mut at_or_above_180 = 0usize;
    let mut below_64 = 0usize;
    let mut total = 0usize;
    for name in &names {
        let Ok(map) = BattleMap::load(vfs, name) else { continue };
        for (li, list) in map.trees.iter().enumerate() {
            if !list.flag { continue }
            let mut n = 0;
            let (mut lo, mut hi) = (255u32, 0u32);
            for g in &list.groups {
                for i in &g.instances {
                    h[i.variation as usize] += 1;
                    total += 1;
                    n += 1;
                    lo = lo.min(i.variation as u32);
                    hi = hi.max(i.variation as u32);
                    if i.variation >= 180 { at_or_above_180 += 1 }
                    if i.variation > 0 && i.variation < 64 { below_64 += 1 }
                }
            }
            if n > 0 { per_list.push((format!("{name}#{li}"), n, 0, lo, hi)) }
        }
    }
    println!("scaled lists {} instances {total}", per_list.len());
    println!("below 64 (scale < 0.5 under /128): {below_64}");
    println!("at or above 180 (scale > 1.4 under /128): {at_or_above_180}");
    println!("\n== the cliff: bytes 168..200");
    for (v, n) in h.iter().enumerate().take(201).skip(168) {
        println!("  {v:3}  {n:6}  {}", if v as f32 / 128.0 > 1.4 { "<-- above MAX_TREE_SCALE 1.4" } else { "" });
    }
    println!("\n== the floor: bytes 58..72");
    for (v, n) in h.iter().enumerate().take(73).skip(58) {
        println!("  {v:3}  {n:6}  {}", if v as f32 / 128.0 < 0.5 { "<-- below MIN_TREE_SCALE 0.5" } else { "" });
    }
    // Where does the mass sit relative to the two clamps?
    // The histogram's counts, not the byte values (round 7 summed `0..64` itself; fixed in review
    // 0-D, so any "share below/above the clamps" quoted from this printout before was wrong).
    let below: usize = h[..64].iter().sum();
    let mid: usize = h[64..180].iter().sum();
    let above: usize = h[180..256].iter().sum();
    println!("\nbytes   0..63 (scale 0.00..0.49): {below:8}  {:.2}%", 100.0 * below as f64 / total as f64);
    println!("bytes 64..179 (scale 0.50..1.40): {mid:8}  {:.2}%", 100.0 * mid as f64 / total as f64);
    println!("bytes 180..255 (scale 1.41..1.99): {above:8}  {:.2}%", 100.0 * above as f64 / total as f64);
    let nz = (1..256).filter(|&v| h[v] > 0);
    println!("\nsmallest non-zero byte {}  largest {}", nz.clone().min().unwrap(), nz.max().unwrap());
    // Per-list ranges: does any scaled list reach past 179?
    let past: Vec<&(String, usize, usize, u32, u32)> = per_list.iter().filter(|l| l.4 > 179).collect();
    println!("{} of {} scaled lists have any byte > 179", past.len(), per_list.len());
    for l in past.iter().take(20) {
        println!("   {:<28} n {:>7} range {}..{}", l.0, l.1, l.3, l.4);
    }
}
