//! The effects system against the install (slot 0-F, `analysis/fidelity/EFFECTS_FIDELITY.md`): the
//! effect tables load, the eur_napoleon start position's sources reach the sums, the campaign start
//! builds faction +0x8D4 (base + difficulty handicap) and a save keeps it. Skipped without an install.

use std::path::PathBuf;

use ntw_campaign::save;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::GameFiles;
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::effects::{apply_start_handicaps, saved_set, Effects, TECH_RESEARCHED};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

fn load() -> Option<(GameDatabase, EsfFile)> {
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return None;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let bytes = GameFiles { vfs: &vfs, data_dir: Some(&dir) }.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    Some((db, EsfFile::from_bytes(&bytes).expect("esf")))
}

#[test]
fn effect_tables_and_start_sums() {
    let Some((db, source)) = load() else { return };
    let t = &db.campaign.effects;
    // Row counts of the shipped tables (EFFECTS_FIDELITY.md §1 / ntw_data::effects).
    assert_eq!(t.bonus_basic.len(), 143);
    assert_eq!(t.difficulty.len(), 88);
    assert!(t.technology.len() > 100 && t.trait_level.len() > 500 && t.ancillary.len() > 300);

    let mut loaded = ntw_campaign::read_esf(&source, &db).expect("load");
    assert!(loaded.set_human("france"));
    let m = &loaded.model;
    let rules = &m.rules.effects;
    assert!(rules.difficulty.contains_key(&(0, true)) && rules.difficulty.contains_key(&(0, false)));
    let france = m.faction_by_key("france").expect("france").id;
    let d = &m.world.faction_details[&france];
    // The startpos base holds the agent caps only (type 0), and #55 equals it before the start.
    assert!(!d.bonus_base.is_empty() && d.bonus_base.iter().all(|b| b.kind == 0));
    assert_eq!(d.bonus_base, d.bonus_with_difficulty);
    assert!(d.technologies.iter().any(|(_, s)| *s == TECH_RESEARCHED));

    // Faction sum: researched technologies reach it (each one's compiled set).
    let fx = Effects::compute(m);
    let tech: f32 = d
        .technologies
        .iter()
        .filter(|(_, s)| *s == TECH_RESEARCHED)
        .filter_map(|(k, _)| rules.technology.get(k))
        .map(|s| s.get("tax_bonus_technology"))
        .sum();
    assert!(fx.faction(france, "tax_bonus_technology") >= tech);
    // One governorship per faction (the home theatre), so the region sum never takes the governor part
    // that the exe adds outside the home theatre (`0x008AFB50`).
    for d in m.world.faction_details.values() {
        assert!(d.posts.iter().filter(|p| p.governorship.is_some()).count() <= 1);
    }
    // Characters with traits get trait effects.
    assert!(fx.character.values().any(|s| !s.is_empty()));
}

#[test]
fn campaign_start_applies_the_handicap_and_saves_keep_it() {
    let Some((db, source)) = load() else { return };
    let mut loaded = ntw_campaign::read_esf(&source, &db).expect("load");
    assert!(loaded.set_human("france"));
    let mut m = loaded.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    let austria = m.faction_by_key("austria").unwrap().id;
    let before = Effects::compute(&m);
    apply_start_handicaps(&mut m);
    let rules = &m.rules.effects;
    let human_rows = &rules.difficulty[&(0, true)];
    let ai_rows = &rules.difficulty[&(0, false)];
    let after = Effects::compute(&m);
    for (f, rows) in [(france, human_rows), (austria, ai_rows)] {
        let d = &m.world.faction_details[&f];
        let mut want = saved_set(&d.bonus_base);
        want.merge(rows);
        assert_eq!(saved_set(&d.bonus_with_difficulty), want, "faction {f:?}");
        for b in ["upkeep_cost_mod_land_all", "recruitment_mod_cost_land_all", "gdp_mod_all"] {
            assert_eq!(after.faction(f, b), before.faction(f, b) + rows.get(b), "{b}");
        }
    }
    // The shipped normal rows: human land upkeep +10 %, AI −10 %.
    assert_eq!(human_rows.get("upkeep_cost_mod_land_all"), 10.0);
    assert_eq!(ai_rows.get("upkeep_cost_mod_land_all"), -10.0);

    // The campaign start runs it; a save keeps #55 and a reload reads it back unchanged.
    let mut started = loaded.model.clone();
    started.begin_start_campaign();
    assert_eq!(started.world.faction_details[&france].bonus_with_difficulty, m.world.faction_details[&france].bonus_with_difficulty);
    let tree = save::write_save(&source, &m, "france", 1_000_000).expect("write");
    let back = ntw_campaign::read_esf(&tree, &db).expect("reload");
    for f in [france, austria] {
        assert_eq!(back.model.world.faction_details[&f].bonus_with_difficulty, m.world.faction_details[&f].bonus_with_difficulty);
        assert_eq!(back.model.world.faction_details[&f].bonus_base, m.world.faction_details[&f].bonus_base);
    }
}
