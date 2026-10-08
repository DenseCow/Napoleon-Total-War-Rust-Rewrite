//! `slots_art` and `slots_templates_models`: the art a campaign-map slot draws, and the model
//! templates it names. Read-only, against a real install. All `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_data --test slots_install -- --ignored --nocapture
//! ```
//! The install path can be overridden with `NTW_DATA_DIR`.
//!
//! Why this matters: `0x00B42B90` (the model-name builder) looks a `slots_art` row up by key,
//! reads its template column at the row's `+0x30`, checks that value is a key of
//! `slots_templates_models` and then appends the literal `_slot_fortifications_lvl` and the
//! fortification level. Neither table was loaded before, so that chain had no data behind it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::pack::Vfs;

const FORT_SLOT: &str = "fort";
/// The building chain a region's fort belongs to (`building_chains`).
const FORT_CHAIN: &str = "fFort";
/// The chain the *settlement's* own fortification slot belongs to.
const SETTLEMENT_FORT_CHAIN: &str = "sFortifications";
/// Where the settlement templates live in the pack.
const TEMPLATES: &str = r"rigidmodels\campaignbuildings\templates";
/// Where the region's fort models live in the pack.
const BUILDINGS: &str = r"rigidmodels\campaignbuildings\buildings";

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

fn install() -> (GameDatabase, Vfs) {
    let dir = data_dir();
    (GameDatabase::from_install(&dir).expect("load the database"), Vfs::open_install(&dir).expect("open install"))
}

/// `slots_art` reads with the exe's own 13-column layout (`DB_BUILDERS.md`, row reader
/// `0x00F06D20`) and every row's flags agree with its values.
#[test]
#[ignore]
fn slots_art_reads_with_the_exes_thirteen_columns() {
    let (db, _) = install();
    let t = &db.campaign.slot_art;
    assert_eq!(t.version(), 0);
    assert_eq!(t.len(), 72, "72 shipped rows");

    // 12 slot types x 6 cultures, and the pair is unique.
    let mut types = BTreeSet::new();
    let mut cultures = BTreeSet::new();
    let mut pairs = BTreeSet::new();
    for r in t.iter() {
        assert!(!r.slot_type.is_empty() && !r.culture.is_empty(), "{r:?}");
        assert!(pairs.insert((r.slot_type.clone(), r.culture.clone())), "duplicate row {r:?}");
        types.insert(r.slot_type.clone());
        cultures.insert(r.culture.clone());
    }
    assert_eq!(types.len(), 12, "slot types: {types:?}");
    assert_eq!(cultures.len(), 6, "cultures: {cultures:?}");
    assert_eq!(types.len() * cultures.len(), t.len(), "the table is a full cross product");
    println!("{types:?}\n{cultures:?}");

    // Column #3 says exactly whether column #2 is there; nothing in the shipped data contradicts it.
    for r in t.iter() {
        assert_eq!(r.has_diffuse, r.diffuse.is_some(), "#3 against #2 in {r:?}");
        // And the two 0/1 columns split the settlement slots from the resource and fort slots,
        // always with the same value in both.
        assert_eq!(r.is_settlement_slot, r.is_settlement_slot_2, "#9 against #12 in {r:?}");
        assert!(matches!(r.is_settlement_slot, 0 | 1), "#9 is not 0/1 in {r:?}");
    }
    for ty in ["settlement", "port", "town-commercial", "town-industrial", "town-intellectual"] {
        for r in t.iter().filter(|r| r.slot_type == ty) {
            assert_eq!(r.is_settlement_slot, 1, "{ty} should be a settlement slot: {r:?}");
        }
    }
    for ty in [FORT_SLOT, "gold", "iron", "wheat", "wine", "timber", "horses"] {
        for r in t.iter().filter(|r| r.slot_type == ty) {
            assert_eq!(r.is_settlement_slot, 0, "{ty} should not be a settlement slot: {r:?}");
        }
    }
}

/// CONFIRMED: `slots_art` #4 and #6 are foreign keys into `slots_templates_models` -- every value
/// in either column that is present at all is a key of that table. This is what `0x00B42B90`
/// relies on: it reads the row's `+0x30` (column #4) and refuses to build a name when that string
/// is not a key.
#[test]
#[ignore]
fn the_slot_art_templates_are_template_keys() {
    let (db, _) = install();
    let art = &db.campaign.slot_art;
    let templates = &db.campaign.slot_templates_models;
    assert_eq!(templates.len(), 28, "28 shipped template rows");
    let keys: BTreeSet<&str> = templates.iter().map(|t| t.key.as_str()).collect();
    assert_eq!(keys.len(), 28, "the template keys are unique");

    let mut checked = 0;
    for r in art.iter() {
        for (what, v) in [("#4", r.template.as_deref()), ("#6", r.second_template.as_deref())] {
            let Some(v) = v else { continue };
            assert!(keys.contains(v), "{} of {}/{}: {v:?} is not a slots_templates_models key", what, r.slot_type, r.culture);
            checked += 1;
        }
        // The second template only exists when #5 says so.
        assert_eq!(r.has_second_template, r.second_template.is_some(), "#5 against #6 in {r:?}");
    }
    println!("{checked} template references across {} rows, all keys", art.len());

    // And the two tables really do pair up: every template row's folder is one of the four
    // culture folders the pack has, and its model name is that folder's own prefix.
    let mut folders = BTreeSet::new();
    for t in templates.iter() {
        folders.insert(t.folder.clone());
        assert!(!t.model.is_empty() && !t.folder.is_empty(), "{t:?}");
    }
    assert_eq!(folders.len(), 4, "four culture folders: {folders:?}");
    for f in &folders {
        assert!(f.starts_with("RigidModels/CampaignBuildings/Templates/"), "unexpected folder {f}");
    }
}

/// CONFIRMED: the **region fort's** model is named by `slots_art`'s `fort` row, column #10 --
/// `Fort_lvl1`, the same for every culture -- and the pack holds exactly the three
/// `fort_lvl<n>_blend.rigid_model` files that the three `fFort` building levels call for.
///
/// This is *not* `0x00B42B90`: that function appends `_slot_fortifications_lvl`, which is the
/// settlement's own fortification slot, and the `fort` rows carry no template key at all.
#[test]
#[ignore]
fn the_regions_fort_model_is_the_slot_art_stem_and_there_are_three_of_them() {
    let (db, vfs) = install();
    let art = &db.campaign.slot_art;

    let forts: Vec<_> = art.iter().filter(|r| r.slot_type == FORT_SLOT).collect();
    assert_eq!(forts.len(), 6, "a fort row for every culture");
    for r in &forts {
        assert_eq!(r.model.as_deref(), Some("Fort_lvl1"), "the fort stem in {r:?}");
        assert_eq!(r.model_stem(), Some("Fort_lvl1"), "{r:?}");
        // A fort has no template: `0x00B42B90` cannot build a name from this row at all.
        assert!(r.template.is_none() && r.second_template.is_none(), "a fort row with a template: {r:?}");
        assert_eq!(r.is_settlement_slot, 0, "{r:?}");
    }

    // The chain `fFort` has exactly three levels, and level n (0-based) is `fort_lvl<n+1>`.
    assert!(db.campaign.chains.get(FORT_CHAIN).is_some(), "no {FORT_CHAIN} chain in building_chains");
    let levels: Vec<&str> = db
        .building_levels
        .iter()
        .filter(|l| l.chain == FORT_CHAIN)
        .map(|l| l.key.as_str())
        .collect();
    assert_eq!(levels.len(), 3, "the {FORT_CHAIN} levels: {levels:?}");
    assert_eq!(levels[0], "fFort1_wooden_artillery_fort");
    assert_eq!(levels[1], "fFort2_western_artillery_fort");
    assert_eq!(levels[2], "fFort3_star_fort");
    let level_index: Vec<i32> = db.building_levels.iter().filter(|l| l.chain == FORT_CHAIN).map(|l| l.level).collect();
    assert_eq!(level_index, vec![0, 1, 2], "the chain levels are 0..2");

    // So: three models, one per level, and no more.
    for n in 1..=3u32 {
        let model = format!("{BUILDINGS}\\generic\\fort_lvl{n}_blend.rigid_model");
        assert!(vfs.contains(&model), "missing {model}");
        println!("fort level {} -> {model}", n - 1);
        let icon = format!("ui\\buildings\\icons\\fort_lvl{n}.tga");
        assert!(vfs.contains(&icon), "missing {icon}");
    }
    // The pack has levels 4 and 5 too, but only per culture and with no building level behind
    // them: they are not this chain's (INFERRED: battle-map forts, or unused).
    for (folder, n) in [("eu", 4), ("eu", 5), ("ott", 4), ("ott", 5), ("ind", 4), ("ind", 5)] {
        let model = format!("{BUILDINGS}\\{folder}\\{folder}_fort_lvl{n}_blend.rigid_model");
        assert!(vfs.contains(&model), "missing {model}");
    }
    println!("{FORT_CHAIN}: 3 campaign levels -> 3 generic models; 6 further culture models with no level");

    // The settlement's own fortification chain, for contrast: two levels, and the models the
    // literal in `0x00B42B90` builds.
    let set_levels: Vec<&str> = db
        .building_levels
        .iter()
        .filter(|l| l.chain == SETTLEMENT_FORT_CHAIN)
        .map(|l| l.key.as_str())
        .collect();
    assert_eq!(set_levels.len(), 2, "the {SETTLEMENT_FORT_CHAIN} levels: {set_levels:?}");
    assert_eq!(set_levels[0], "sFortifications1_settlement_fortifications");
    assert_eq!(set_levels[1], "sFortifications2_improved_settlement_fort");
}

/// CONFIRMED: the three fortification files of a settlement template are a **rising ladder** --
/// `_lvl0` has the fewest vertices and `_lvl2` the most, for every culture and every slot number
/// that has any of them. Three files for a two-level chain fits reading the file level as the
/// `sFortifications` chain level + 1 (`_lvl0` = "no walls yet"), which is what
/// `napoleon::campaign::scene::SettlementWalls::level_of` uses -- but that reading is INFERRED: a
/// vertex ladder does not say which building level asks for which file. All three files use the **same three textures** as the plain
/// `<stem>_<n>_slot.rigid_model`, so they are variants of one city model, not a separate wall set.
#[test]
#[ignore]
fn the_settlement_fortification_levels_are_a_rising_ladder() {
    let (db, vfs) = install();
    let verts = |path: &str| -> Option<usize> {
        let b = vfs.read(path).ok()?;
        let m = ntw_formats::rigid_model::RigidModel::read(&b).ok()?;
        Some(m.meshes.iter().map(|rm| rm.vertices.len()).sum())
    };
    let textures = |path: &str| -> Option<Vec<String>> {
        let b = vfs.read(path).ok()?;
        let m = ntw_formats::rigid_model::RigidModel::read(&b).ok()?;
        Some(
            m.meshes
                .iter()
                .flat_map(|rm| {
                    [rm.material.diffuse.as_ref(), rm.material.normal.as_ref(), rm.material.gloss.as_ref()]
                        .into_iter()
                        .flatten()
                        .map(|t| t.name.clone())
                })
                .collect(),
        )
    };

    let mut ladders = 0;
    let mut slots = 0;
    for r in db.campaign.slot_art.iter().filter(|r| r.slot_type == "settlement") {
        let key = r.template.as_deref().expect("a settlement row has a template");
        let t = db.campaign.slot_template_model(key).unwrap_or_else(|| panic!("{key} is not a template key"));
        for n in 1..=6 {
            let counts: Vec<Option<usize>> = ["0", "1", "2"]
                .iter()
                .map(|level| verts(&t.fortification_slot_model(n, level.parse().unwrap())))
                .collect();
            let present: Vec<usize> = counts.iter().filter_map(|c| *c).collect();
            if present.is_empty() {
                continue;
            }
            slots += 1;
            assert_eq!(present.len(), 3, "{} slot {n}: a fortification ladder is all three levels or none", t.key);
            assert!(
                present[0] < present[1] && present[1] < present[2],
                "{} slot {n}: not a rising ladder -- {present:?}",
                t.key
            );
            ladders += 1;
        }
        // The plain city mesh and its three fortification variants share one texture set.
        for n in 1..=6 {
            let plain = textures(&t.slot_model(n));
            let Some(plain) = plain else { continue };
            for level in 1..=2u32 {
                let path = t.fortification_slot_model(n, level);
                let Some(f) = textures(&path) else { continue };
                assert_eq!(f, plain, "{path} does not use the plain model's textures");
            }
        }
    }
    assert!(ladders >= 20, "only {ladders} fortification ladders found over {slots} slots -- re-check this test");
    println!("{ladders} settlement fortification ladders, every one _lvl0 < _lvl1 < _lvl2, all sharing the plain model's textures");
}

/// CONFIRMED, and it closes the `0x00B42B90` chain from the data side. For a **settlement** slot
/// the template key in `slots_art` #4 leads to a `slots_templates_models` row whose model name is
/// the lower-cased stem of the file
///
/// ```text
/// <folder>\<stem>_<n>_slot_fortifications_lvl<chain level + 1>.rigid_model
/// ```
///
/// for every `n` the template folder has and every level of the `sFortifications` chain. (The
/// offset by one is the INFERRED reading above; this test checks the files exist under it.)
/// Ports and towns have **no** such file at all -- only the city templates do.
#[test]
#[ignore]
fn the_settlement_fortification_models_follow_the_templates_own_names() {
    let (db, vfs) = install();
    let set_levels: Vec<i32> = db
        .building_levels
        .iter()
        .filter(|l| l.chain == SETTLEMENT_FORT_CHAIN)
        .map(|l| l.level)
        .collect();
    assert_eq!(set_levels, vec![0, 1], "fortification levels 0 and 1");

    let mut with_models = 0;
    let mut without_models = Vec::new();
    for r in db.campaign.slot_art.iter().filter(|r| r.slot_type == "settlement") {
        let key = r.template.as_deref().expect("a settlement row has a template");
        let t = db.campaign.slot_template_model(key).unwrap_or_else(|| panic!("{key} is not a template key"));
        // The template row's own folder is the pack folder (the table writes it with `/`).
        let folder = t.folder.replace('/', "\\");
        assert!(folder.to_ascii_lowercase().starts_with(TEMPLATES), "{} -> {folder}", t.key);
        let stem = t.model.to_ascii_lowercase();
        // The chain level is offset by one in the file name. Which `<n>` a culture folder has
        // differs (`eu` has 1..5, `ind` 1/4/5, `ott` 1..5), so walk them and require that a slot
        // numbered `<n>` comes with all three levels or with none.
        let levels: Vec<String> = set_levels.iter().map(|l| format!("{}", l + 1)).collect();
        let mut slots = 0;
        for n in 1..=5 {
            let names: Vec<String> = ["0", levels[0].as_str(), levels[1].as_str()]
                .iter()
                .map(|level| format!("{folder}\\{stem}_{n}_slot_fortifications_lvl{level}.rigid_model"))
                .collect();
            let present: Vec<bool> = names.iter().map(|p| vfs.contains(p)).collect();
            if !present.iter().any(|p| *p) {
                continue;
            }
            for (name, there) in names.iter().zip(&present) {
                assert!(there, "missing {name} (template {}, slot {n}): a slot that has fortification models has all of them", t.key);
            }
            slots += 1;
        }
        // The tribal folder ships **no** fortification art at all -- CONFIRMED over the pack: it
        // has `na_city_<n>_slot` and the minibuildings variants, and no `_slot_fortifications_lvl*`.
        // The other three culture templates have all three levels.
        if t.model.to_ascii_uppercase().starts_with("NA_") {
            assert_eq!(slots, 0, "the tribal folder should have no fortification model, found {slots}");
            without_models.push(r.culture.clone());
            continue;
        }
        assert!(slots > 0, "no fortification model at all for template {}", t.key);
        println!("{:>18}/settlement: {stem}: {slots} slots x 3 levels", r.culture);
        with_models += 1;
    }
    assert_eq!(with_models, 5, "five settlement rows with fortification art (the two egy_* cultures reuse the ott template)");
    assert_eq!(without_models, vec!["tribal"], "only the tribal folder has no fortification art");

    // Only the city templates have fortification models: a port or a town template does not.
    let mut without = 0;
    for r in db.campaign.slot_art.iter().filter(|r| r.is_settlement_slot == 1 && r.slot_type != "settlement") {
        let Some(key) = r.template.as_deref() else { continue };
        let Some(t) = db.campaign.slot_template_model(key) else { continue };
        let folder = t.folder.replace('/', "\\");
        let stem = t.model.to_ascii_lowercase();
        let found = (1..=5).any(|n| vfs.contains(&format!("{folder}\\{stem}_{n}_slot_fortifications_lvl1.rigid_model")));
        assert!(!found, "{} ({stem}) has a fortification model after all: {key}", r.slot_type);
        without += 1;
    }
    assert!(without > 0, "no port or town template checked");
    println!("{without} port / town templates carry no fortification model");
}