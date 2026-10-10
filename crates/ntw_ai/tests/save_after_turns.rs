//! Saves written after End Turns played by the campaign AI keep every link the original game needs
//! (`ntw_campaign::save_check`, analysis/campaign/SAVE_COMPAT.md): the real eur_napoleon startpos
//! (read-only), france human, the AI playing everyone else through the real turn loop; the save is
//! written, checked, loaded back, played on and saved again. Skips (passes) without an install.
//! Override the install with `NTW_DATA_DIR`. Written saves stay in memory (or in the temp folder).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use ntw_ai::campaign::CampaignAiData;
use ntw_ai::campaign::driver;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::{CampaignModel, Terrain};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

struct Fixture {
    db: GameDatabase,
    startpos: EsfFile,
    model: CampaignModel,
    terrain: Terrain,
    data: Arc<CampaignAiData>,
}

fn fixture() -> Option<&'static Fixture> {
    static F: OnceLock<Option<Fixture>> = OnceLock::new();
    F.get_or_init(|| {
        let dir = data_dir();
        let sp = dir.join(r"campaigns\eur_napoleon\startpos.esf");
        if !sp.is_file() {
            eprintln!("skipped: no install");
            return None;
        }
        let db = GameDatabase::from_install(&dir).expect("db");
        let data = CampaignAiData::from_install(&dir, &db).expect("AI tables");
        let startpos = EsfFile::open(&sp).expect("esf");
        let mut loaded = ntw_campaign::read_esf(&startpos, &db).expect("startpos");
        assert!(loaded.set_human("france"));
        let vfs = Vfs::open_install(&dir).expect("packs");
        let files = GameFiles { vfs: &vfs };
        let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
        let terrain = Terrain(Arc::new(ntw_campaign::pathing::build_grid(&map)));
        Some(Fixture { db, startpos, model: loaded.model, terrain, data: Arc::new(data) })
    })
    .as_ref()
}

fn play(f: &Fixture, m: &mut CampaignModel, turns: u32) {
    m.terrain = Some(f.terrain.clone());
    let ctx = driver::context_for(m, "eur_napoleon");
    for _ in 0..turns {
        driver::end_turn(m, &f.data, &ctx);
    }
}

fn assert_clean(name: &str, esf: &EsfFile) -> ntw_campaign::save_check::Report {
    let r = ntw_campaign::save_check::check(esf);
    assert!(r.violations.is_empty(), "{name}: {} violations, first: {:#?}", r.violations.len(), &r.violations[..r.violations.len().min(12)]);
    r
}

/// Model-side rules the original keeps (SAVE_COMPAT.md §4): every force has a commander of its
/// own faction and at least one unit; one garrison per settlement, commanded by someone inside.
fn assert_model_sane(name: &str, m: &CampaignModel) {
    for f in m.world.forces.values() {
        let c = f.commander.unwrap_or_else(|| panic!("{name}: force {} has no commander", f.id.raw()));
        assert_eq!(m.world.characters.get(&c).map(|c| c.faction), Some(f.faction), "{name}: force {} commander", f.id.raw());
        assert!(!f.units.is_empty(), "{name}: force {} has no units", f.id.raw());
    }
    for r in m.world.regions.values() {
        if let Some(g) = r.garrison {
            let f = &m.world.forces[&g];
            let c = f.commander.unwrap();
            assert_eq!(m.world.characters[&c].garrisoned_in, Some(r.id), "{name}: garrison of {}", r.key);
        }
    }
}

fn round(f: &Fixture, turns: u32, extra: u32) {
    let mut m = f.model.clone();
    play(f, &mut m, turns);
    assert_model_sane("after play", &m);
    let save = ntw_campaign::save::write_save(&f.startpos, &m, "france", 1).expect("write");
    let r = assert_clean(&format!("after {turns} turns"), &save);
    assert!(!r.ai_block_rebuilt, "the AI block stays loadable (version 13), kept in step with the world");
    println!("after {turns} turns: {} forces, {} commanders without obstacle", m.world.forces.len(), r.commanders_without_obstacle.len());
    // Load it back: the same world.
    let bytes = save.to_bytes().expect("bytes");
    let back = ntw_campaign::read(&bytes, &f.db).expect("reload");
    assert_eq!(back.model.world.forces.len(), m.world.forces.len());
    assert_eq!(back.model.world.characters.len(), m.world.characters.len());
    for (id, fo) in &m.world.forces {
        let b = &back.model.world.forces[id];
        assert_eq!((b.commander, b.units.len()), (fo.commander, fo.units.len()), "force {}", id.raw());
        for (u, v) in fo.units.iter().zip(&b.units) {
            assert_eq!((u.id, u.character), (v.id, v.character));
        }
    }
    for (id, r) in &m.world.regions {
        assert_eq!(back.model.world.regions[id].garrison, r.garrison, "garrison of {}", r.key);
    }
    assert_model_sane("reloaded", &back.model);
    // Play on from the written save and save again from it.
    let mut m2 = back.model;
    play(f, &mut m2, extra);
    assert_model_sane("after more play", &m2);
    let esf2 = EsfFile::from_bytes(&bytes).unwrap();
    let save2 = ntw_campaign::save::write_save(&esf2, &m2, "france", 2).expect("write 2");
    assert_clean(&format!("after {} turns, saved twice", turns + extra), &save2);
}

#[test]
fn saves_after_ai_turns_keep_the_originals_links() {
    let Some(f) = fixture() else { return };
    round(f, 3, 1);
}

/// The longer run behind the user's "after 10 turns" test save.
#[test]
#[ignore]
fn saves_after_ten_ai_turns_keep_the_originals_links() {
    let Some(f) = fixture() else { return };
    round(f, 10, 2);
}
