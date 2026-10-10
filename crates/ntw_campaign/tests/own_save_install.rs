//! Our own save format and the campaign source seam against the real install (read-only): a
//! campaign opened through `source::open`, played a few turns, saved in our own format and opened
//! again gives the same model. Each test **skips** when the install is not there.
//!
//! `cargo test -p ntw_campaign --release --test own_save_install -- --nocapture`

use std::path::PathBuf;
use std::sync::OnceLock;

use ntw_campaign::own_save::{self, SaveData};
use ntw_campaign::source::{self, SourceError, Start};
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::GameFiles;
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::CampaignModel;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

struct Fixture {
    vfs: Vfs,
    db: GameDatabase,
}

impl Fixture {
    fn files(&self) -> GameFiles<'_> {
        GameFiles { vfs: &self.vfs }
    }
}

/// The install's packs and DB, opened once (None without an install).
fn fixture() -> Option<&'static Fixture> {
    static F: OnceLock<Option<Fixture>> = OnceLock::new();
    F.get_or_init(|| {
        let dir = data_dir();
        if !dir.is_dir() {
            eprintln!("skipped: no install at {}", dir.display());
            return None;
        }
        let db = GameDatabase::from_install(&dir).expect("DB");
        let vfs = Vfs::open_install(&dir).expect("vfs");
        Some(Fixture { vfs, db })
    })
    .as_ref()
}

/// The model without the fields our save leaves out on purpose (own_save.rs "What is saved"):
/// what a save of `m` must give back.
fn saved_part(m: &CampaignModel) -> CampaignModel {
    let mut m = m.clone();
    m.terrain = None;
    m.last_autoresolve = None;
    m.script_rngs = Default::default();
    m.negotiations = Default::default();
    m.world.recruitment_sources.clear();
    m.world.agents_acted.clear();
    m.world.sabotaged.clear();
    m.world.network_sight = Default::default();
    for d in m.world.character_details.values_mut() {
        d.wounded = false;
    }
    m
}

/// A new campaign opened through the seam, played three turns as France, saved in our own format
/// and opened again through the seam: the same model (rules rebuilt from the DB, the movement grid
/// and region links from the map), header, script slots and restricted units.
#[test]
fn a_played_campaign_round_trips_through_our_own_save() {
    let Some(f) = fixture() else { return };
    let mut opened = source::open(f.files(), Start::New("eur_napoleon", None), &f.db).expect("open eur_napoleon");
    // A new campaign sets its caps (the default multiplier 0.75: 20 per army, 10 per navy), not the
    // start position's 20 / 14.
    assert_eq!(opened.loaded.model.force_caps, ntw_sim::campaign::rules::ForceCaps { army: 20, navy: 10 });
    assert!(opened.loaded.set_human("france"));
    let m = &mut opened.loaded.model;
    let at_start: std::collections::BTreeSet<_> = m.world.characters.keys().copied().collect();
    m.start_campaign();
    for _ in 0..3 {
        m.end_turn();
    }
    assert!(m.calendar.turn_number() >= 4, "three turns played");
    // A unit raised where France has no garrison comes with a new colonel.
    let france = m.faction_by_key("france").expect("france").id;
    let unit_key = m.world.forces.values().filter(|f| f.faction == france && !f.is_navy).flat_map(|f| &f.units).map(|u| u.unit_key.clone()).next().expect("a French land unit");
    let region = m.world.regions.values().find(|r| r.owner == france && r.garrison.is_none()).map(|r| r.id).expect("a French region without a garrison");
    let event = m.spawn_recruited_unit(region, unit_key);
    // His unit's officer carries his name.
    let ntw_sim::campaign::CampaignEvent::UnitTrained { force, unit } = event else { panic!("no unit: {event:?}") };
    let raised = m.world.forces[&force].units.iter().find(|u| u.id == unit).expect("the unit").clone();
    let colonel = raised.character.expect("raised with a colonel");
    let cd = &m.world.character_details[&colonel];
    assert_eq!(raised.officer_name, (cd.forename.clone(), cd.surname.clone()), "the unit carries the colonel's name");
    // Characters created in play (recruitment-pool candidates, colonels) are named when they are
    // created, from their faction's pools, and the names are in the save.
    let new: Vec<_> = m.world.characters.keys().filter(|c| !at_start.contains(c)).copied().collect();
    eprintln!("{} characters created in 3 turns", new.len());
    assert!(!new.is_empty(), "characters were created");
    for c in &new {
        let d = m.world.character_details.get(c).expect("details");
        assert!(d.forename.starts_with("names_name_") && d.surname.starts_with("names_name_"), "{c:?} unnamed: {:?} {:?}", d.forename, d.surname);
    }
    let info = own_save::save_header(&opened.loaded.info, m, "france", 1_790_000_000, &opened.map.theatre_pictures);
    let data = SaveData {
        human: "france".into(),
        model: m.clone(),
        rebel_faction: opened.loaded.rebel_faction,
        script_values: opened.loaded.script_values.clone(),
        restricted_units: vec!["made_up_restricted_unit".into()],
    };
    let t = std::time::Instant::now();
    let bytes = own_save::write(&info, &data).expect("write");
    let written = t.elapsed();
    let t = std::time::Instant::now();
    let back = source::open(f.files(), Start::Save(&bytes), &f.db).expect("open the save");
    eprintln!("own save of eur_napoleon after 3 turns: {} bytes, written in {written:?}, opened with its map in {:?}", bytes.len(), t.elapsed());
    let t = std::time::Instant::now();
    let parts = own_save::read_parts(&bytes).expect("read");
    eprintln!("  body decoded alone in {:?}; header alone {:?}", t.elapsed(), { let t = std::time::Instant::now(); own_save::read_info(&bytes).expect("header"); t.elapsed() });
    drop(parts);
    assert_eq!(back.loaded.info, info);
    assert_eq!(back.loaded.rebel_faction, data.rebel_faction);
    assert_eq!(back.loaded.script_values, data.script_values);
    assert_eq!(back.loaded.restricted_units, data.restricted_units);
    assert!(back.loaded.warnings.is_empty(), "{:?}", back.loaded.warnings);
    // Rebuilt, not saved: the same rules and the same grid.
    assert_eq!(back.loaded.model.rules, m.rules);
    let grid = |m: &CampaignModel| m.terrain.as_ref().map(|t| t.0.clone());
    assert!(grid(&back.loaded.model).is_some() && grid(&back.loaded.model) == grid(m), "the movement grid differs");
    assert!(saved_part(&back.loaded.model) == saved_part(m), "the loaded model differs from the saved one");
    for c in &new {
        assert_eq!(back.loaded.model.world.character_details[c].forename, m.world.character_details[c].forename, "{c:?} keeps his name");
    }
    assert!(!back.loaded.model.rules.names.pools.is_empty(), "the loaded campaign names new characters too");
    // A second save of the loaded campaign is the same bytes: nothing drifts on a load.
    let again = SaveData { model: back.loaded.model.clone(), ..data };
    assert!(own_save::write(&info, &again).expect("write again") == bytes, "a re-save differs");
}

/// The seam's front-end facts are the same whichever way they are read: the source's `info`, the
/// UI's `campaign_info` (through its own file reader) and every listed campaign opens.
#[test]
fn the_original_campaigns_are_found_through_the_seam() {
    let Some(f) = fixture() else { return };
    let files = f.files();
    let keys = source::original_campaigns(&files);
    for key in ["eur_napoleon", "tut_napoleon", "spa_napoleon", "mp_eur_napoleon"] {
        assert!(keys.iter().any(|k| k == key), "{key} not in {keys:?}");
    }
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(keys, sorted, "sorted, no duplicates");
    let read = |p: &str| files.read(p).ok();
    for key in &keys {
        let s = source::find(files, key).unwrap_or_else(|| panic!("{key}: no source"));
        assert_eq!(s.key(), key);
        let info = s.info().unwrap_or_else(|e| panic!("{key}: {e}"));
        assert_eq!(&info.campaign_key, key);
        assert_eq!(source::campaign_info(&read, key), Some(info), "{key}");
    }
}

/// An unknown campaign is an error naming it, not a panic or an empty campaign.
#[test]
fn an_unknown_campaign_is_not_found() {
    let Some(f) = fixture() else { return };
    assert!(source::find(f.files(), "made_up_campaign").is_none());
    match source::open(f.files(), Start::New("made_up_campaign", None), &f.db) {
        Err(SourceError::NotFound(k)) => assert_eq!(k, "made_up_campaign"),
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("opened a campaign that does not exist"),
    }
}


/// The importer's display data for the European map draws what the map files give: the ground
/// texture's tiles (read by range through the game's files) are the supertexture's pixels, the
/// lines are the spline folders in logic units, the heights are the heightmap's, and the river
/// texture and arrow model are handed over.
#[test]
fn the_original_map_display_is_the_map_files() {
    use ntw_campaign::map_display::{level_rgba, LineKind};
    use ntw_formats::campaign_map::{CampaignMap, DISPLAY_TO_LOGIC};
    let Some(f) = fixture() else { return };
    let files = f.files();
    let map = CampaignMap::load(&files, "nap_europe").expect("nap_europe");
    let d = source::original_display(&files, map.clone());
    assert_eq!(d.key, "nap_europe");
    let st = map.supertexture.as_ref().expect("supertexture");
    let ground = d.ground.as_deref().expect("ground texture");
    assert_eq!(ground.tile_size(), st.tile_size);
    assert_eq!(ground.levels().len(), st.levels.len());
    // The coarsest level, whole, against the old whole-file decoder.
    let level = st.levels.len() - 1;
    assert_eq!(level_rgba(ground, level).expect("level"), map.supertexture_rgba(&files, level).expect("old level"));
    assert!(ground.window_rgba(level, 0, 0, st.levels[level].tiles_x + 1, 1).is_err(), "a window past the level");
    for (kind, folder) in [(LineKind::Border, "borders"), (LineKind::River, "rivers"), (LineKind::Road, "roads"), (LineKind::TradeRoute, "traderoutes")] {
        let old: Vec<Vec<(f32, f32)>> = map
            .splines
            .iter()
            .filter(|(f, _)| f == folder)
            .map(|(_, s)| s.points.iter().map(|p| (p[0] * DISPLAY_TO_LOGIC, p[2] * DISPLAY_TO_LOGIC)).collect())
            .collect();
        let new: Vec<Vec<(f32, f32)>> = d.lines(kind).map(|l| l.points.clone()).collect();
        assert!(!new.is_empty() || folder == "traderoutes", "{folder}: none");
        assert_eq!(new, old, "{folder}");
    }
    for (x, z) in [(0.0, 0.0), (-123.4, 56.7), (300.0, -150.0)] {
        assert_eq!(d.height_at(x, z), map.height_at(x, z));
    }
    assert_eq!(d.coast.len(), map.coast.len());
    assert!(d.trees.is_some());
    assert!(d.river_texture.as_ref().is_ok_and(|b| b.starts_with(b"DDS ")), "river texture");
    assert!(d.arrow_model.as_ref().is_ok_and(|b| !b.is_empty()), "arrow model");
}
