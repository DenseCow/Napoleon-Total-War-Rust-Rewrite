//! Integration tests against the real install and the user's own saves (read-only).
//!
//! Each test **skips** (passes without checking anything, printing a note) when the files are not
//! there, so `cargo test` also works on machines without the game.
//! Override the locations with `NTW_DATA_DIR` (the install's `data` folder) and `NTW_SAVE_DIR`.
//! See the counts with `cargo test -p ntw_campaign --release -- --nocapture`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use ntw_campaign::{FileKind, LoadWarning, LoadedCampaign};
use ntw_data::GameDatabase;
use ntw_formats::esf::EsfFile;
use ntw_sim::campaign::FactionId;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
        })
}

/// The game database, loaded once for all tests (None without an install).
fn db() -> Option<&'static GameDatabase> {
    static DB: OnceLock<Option<GameDatabase>> = OnceLock::new();
    DB.get_or_init(|| {
        let dir = data_dir();
        if !dir.is_dir() {
            return None;
        }
        Some(GameDatabase::from_install(&dir).expect("install present but DB does not load"))
    })
    .as_ref()
}

const CAMPAIGNS: [&str; 8] = [
    "eur_napoleon",
    "mp_eur_napoleon",
    "egy_napoleon",
    "mp_egy_napoleon",
    "ita_napoleon",
    "mp_ita_napoleon",
    "spa_napoleon",
    "tut_napoleon",
];

/// Checks that hold for every loaded campaign, startpos or save.
/// `units_must_exist`: true for the shipped start positions. The user's saves may contain units from
/// content that is not installed any more (DLC or a mod; their keys are not in the installed
/// `units` table at all), so for saves unknown unit keys are only counted.
/// The detail records (CAMPAIGN_DATA.md §3): every character has its details, every real faction
/// a government with a leader and a capital that is a loaded region, and every stored stance its
/// full relationship record.
fn check_details(name: &str, l: &LoadedCampaign) {
    let w = &l.model.world;
    assert_eq!(w.character_details.len(), w.characters.len(), "{name}: character details");
    for (id, d) in &w.character_details {
        assert!(!d.forename.is_empty() || !d.surname.is_empty() || w.characters[id].kind == ntw_sim::campaign::CharacterKind::Minister, "{name}: {id:?} has no name");
        // Rebels keep their original faction key (CONFIRMED in the saves).
        if Some(w.characters[id].faction) != l.rebel_faction {
            assert_eq!(d.faction_key, w.factions[&w.characters[id].faction].key, "{name}: {id:?} faction key");
        }
    }
    let mut leaders = 0;
    for f in w.factions.values().filter(|f| Some(f.id) != l.rebel_faction) {
        let d = &w.faction_details[&f.id];
        assert!(!d.posts.is_empty(), "{name}: {} has no government posts", f.key);
        if let Some(leader) = d.leader() {
            leaders += 1;
            assert!(w.characters.contains_key(&leader), "{name}: {} leader {leader:?} not loaded", f.key);
        }
        if let Some(c) = d.capital {
            assert!(w.regions.contains_key(&c), "{name}: {} capital {c:?} is not a region", f.key);
        }
        assert!(ntw_sim::campaign::details::TAX_LEVELS.contains(&f.tax_lower.as_str()), "{name}: {} tax", f.key);
    }
    assert!(leaders > 0, "{name}: no faction leaders");
    let stances: usize = w.factions.values().map(|f| f.diplomacy.len()).sum();
    assert_eq!(w.relationships.len(), stances, "{name}: relationships");
    // The named DIPLOMACY_RELATIONSHIP fields (S1_LEFTOVERS.md §1) hold their documented ranges.
    for ((owner, target), r) in &w.relationships {
        assert_eq!(r.attitudes.len(), 24, "{name}: {owner:?}->{target:?} attitude factors");
        assert!([-85, -45, 0, 45, 85].contains(&r.start_attitude), "{name}: start attitude {}", r.start_attitude);
        assert!(r.friendship_turns <= 10 && r.trade_embargo_turns <= 10, "{name}: countdowns");
        assert!(r.military_access_turns >= -1 && r.military_access_granted >= -1, "{name}: military access");
        match w.factions[owner].diplomacy.get(target).map(|s| s.esf_name()) {
            Some("patron") => assert!(r.protectorate_income > 0, "{name}: patron income"),
            Some("protectorate") => assert!(r.protectorate_tribute > 0, "{name}: protectorate tribute"),
            _ => {}
        }
    }
    let capitals = w.regions.keys().filter(|r| w.is_capital(**r)).count();
    println!("    details: {leaders} leaders, {capitals} capitals held by their owners, {} relationships", w.relationships.len());
}

fn check_sane(name: &str, l: &LoadedCampaign, db: &GameDatabase, units_must_exist: bool) {
    check_details(name, l);
    let w = &l.model.world;
    let n_armies = w.forces.values().filter(|f| !f.is_navy).count();
    let n_navies = w.forces.values().filter(|f| f.is_navy).count();
    let n_units: usize = w.forces.values().map(|f| f.units.len()).sum();
    println!(
        "{name:<40} {:?} factions {:>2} (+rebels) regions {:>2} characters {:>4} armies {:>3} navies {:>2} units {:>4} date {}/{}/{} turn {} warnings {}",
        l.info.kind,
        w.factions.len() - usize::from(l.rebel_faction.is_some()),
        w.regions.len(),
        w.characters.len(),
        n_armies,
        n_navies,
        n_units,
        l.model.calendar.date.year,
        l.model.calendar.date.month,
        l.model.calendar.date.half,
        l.model.calendar.turn_number(),
        l.warnings.len(),
    );
    let unknown_units = l
        .warnings
        .iter()
        .filter(|w| matches!(w, LoadWarning::UnknownUnitKey { .. }))
        .count();
    for warning in l
        .warnings
        .iter()
        .filter(|w| !matches!(w, LoadWarning::UnknownUnitKey { .. }))
    {
        println!("    warning: {warning}");
    }
    if unknown_units > 0 {
        println!("    {unknown_units} units have keys that are not in the installed units table");
    }

    assert!(l.rebel_faction.is_some(), "{name}: no rebel faction");
    assert!(w.factions.len() >= 3, "{name}");
    assert!(!w.regions.is_empty(), "{name}");
    assert!(!w.characters.is_empty(), "{name}");

    // Header and calendar agree (CONFIRMED rule, W3 §3.1).
    assert_eq!(
        l.info.header.turn_number,
        l.model.calendar.turn_number(),
        "{name}"
    );
    assert_eq!(
        l.model.calendar.turn_in_year,
        l.model.calendar.date.turn_in_year(),
        "{name}"
    );
    assert_eq!(l.model.calendar.turns_per_year, 24, "{name}");

    // Every faction except the rebels is a real DB faction.
    for f in w.factions.values() {
        if Some(f.id) != l.rebel_faction {
            assert!(db.faction(&f.key).is_some(), "{name}: faction {}", f.key);
        }
    }
    // Every region is a DB region, has a settlement, and is owned by a loaded faction.
    for r in w.regions.values() {
        assert!(db.region(&r.key).is_some(), "{name}: region {}", r.key);
        assert!(
            r.settlement.key.starts_with("settlement:"),
            "{name}: {}",
            r.settlement.key
        );
        assert!(
            w.factions.contains_key(&r.owner),
            "{name}: owner of {}",
            r.key
        );
    }
    // Every army or navy has units, every unit key is in the units table.
    for f in w.forces.values() {
        assert!(
            !f.units.is_empty(),
            "{name}: force {} has no units",
            f.id.raw()
        );
        assert!(w.factions.contains_key(&f.faction));
        for u in &f.units {
            if units_must_exist {
                assert!(
                    db.unit(&u.unit_key).is_some(),
                    "{name}: unit key {}",
                    u.unit_key
                );
            }
            assert!(u.max_men > 0, "{name}: unit {} has max_men 0", u.unit_key);
        }
        // Every force in the shipped files and the saves has a commander (CONFIRMED by this check).
        let c = f
            .commander
            .unwrap_or_else(|| panic!("{name}: force {} has no commander", f.id.raw()));
        assert!(w.characters.contains_key(&c), "{name}: commander");
        assert_eq!(
            w.characters[&c].faction, f.faction,
            "{name}: commander's faction"
        );
    }
    // Diplomacy only points at loaded factions.
    for f in w.factions.values() {
        for other in f.diplomacy.keys() {
            assert!(w.factions.contains_key(other), "{name}");
        }
    }
    // No unknown-key warnings against the real database.
    assert!(
        !l.warnings.iter().any(|w| matches!(
            w,
            LoadWarning::UnknownRegionKey(_)
                | LoadWarning::UnknownFactionKey(_)
                | LoadWarning::UnknownCharacterType { .. }
                | LoadWarning::UnknownStance { .. }
        )),
        "{name}: unexpected warnings"
    );
}

/// `CAMPAIGN_PREOPEN_MAP_INFO/REGION_OWNERSHIPS_BY_THEATRE` (startpos only) lists
/// (region key, owner faction key) pairs. Our owner field (REGION #20) must agree: this is the
/// cross-check that makes REGION #20 = owner CONFIRMED.
fn preopen_ownership(esf: &EsfFile) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(info) = esf.find_path("CAMPAIGN_PREOPEN_MAP_INFO") else {
        return out;
    };
    let Some(theatres) = info.record_array("REGION_OWNERSHIPS_BY_THEATRE") else {
        return out;
    };
    // Walk every record below and pick (string, string) pairs that look like region → faction.
    for item in &theatres.items {
        for node in item {
            if let Some(r) = node.as_record() {
                r.walk(&mut |rec| {
                    if let (Some(region), Some(owner)) = (rec.get_str(0), rec.get_str(1)) {
                        out.insert(region.to_string(), owner.to_string());
                    }
                });
            }
            if let Some(a) = node.as_record_array() {
                for it in &a.items {
                    if let (Some(region), Some(owner)) = (
                        it.first().and_then(|n| n.as_str()),
                        it.get(1).and_then(|n| n.as_str()),
                    ) {
                        out.insert(region.to_string(), owner.to_string());
                    }
                    for n in it {
                        if let Some(r) = n.as_record() {
                            r.walk(&mut |rec| {
                                if let (Some(region), Some(owner)) =
                                    (rec.get_str(0), rec.get_str(1))
                                {
                                    out.insert(region.to_string(), owner.to_string());
                                }
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
fn loads_all_eight_startpos() {
    let Some(db) = db() else {
        println!("SKIPPED: no install at {}", data_dir().display());
        return;
    };
    for c in CAMPAIGNS {
        let path = data_dir().join("campaigns").join(c).join("startpos.esf");
        if !path.is_file() {
            println!("SKIPPED {c}: {} missing", path.display());
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let esf = EsfFile::from_bytes(&bytes).unwrap();
        let l = ntw_campaign::read_esf(&esf, db).unwrap_or_else(|e| panic!("{c}: {e}"));
        assert_eq!(l.info.kind, FileKind::Startpos);
        assert_eq!(l.info.campaign_key, c);
        assert_eq!(l.model.calendar.turns_elapsed, 0, "{c}: startpos is turn 1");
        check_sane(c, &l, db, true);

        // Region owners agree with the front-end ownership table.
        let preopen = preopen_ownership(&esf);
        let w = &l.model.world;
        let mut checked = 0;
        for r in w.regions.values() {
            if let Some(owner_key) = preopen.get(&r.key) {
                assert_eq!(
                    &w.factions[&r.owner].key, owner_key,
                    "{c}: owner of {}",
                    r.key
                );
                checked += 1;
            }
        }
        println!("    {checked} region owners cross-checked with CAMPAIGN_PREOPEN_MAP_INFO");
        assert!(checked > 0, "{c}: ownership cross-check found nothing");
    }
}

/// Exact counts for the Europe start position (W3 §3.3-§3.8: 41 factions, 72 regions,
/// 531 characters, 69 armies, 18 navies, 441 units, Early January 1805).
/// Note: the `regions` DB table has 159 rows because it lists the regions of *all* theatres;
/// the Europe campaign itself has 72.
#[test]
fn europe_startpos_exact_counts() {
    let Some(db) = db() else { return };
    let path = data_dir().join(r"campaigns\eur_napoleon\startpos.esf");
    if !path.is_file() {
        return;
    }
    let l = ntw_campaign::read_file(&path, db).unwrap();
    let w = &l.model.world;
    assert_eq!(w.factions.len(), 41 + 1);
    assert_eq!(w.regions.len(), 72);
    assert_eq!(w.characters.len(), 531);
    assert_eq!(w.forces.values().filter(|f| !f.is_navy).count(), 69);
    assert_eq!(w.forces.values().filter(|f| f.is_navy).count(), 18);
    assert_eq!(w.forces.values().map(|f| f.units.len()).sum::<usize>(), 441);
    assert_eq!(l.model.calendar.date.year, 1805);
    assert_eq!(
        (l.model.calendar.date.month, l.model.calendar.date.half),
        (0, 0)
    );
    assert_eq!(l.info.map_key, "nap_europe");
    let playable: BTreeSet<_> = l
        .info
        .players
        .iter()
        .filter(|p| p.is_playable)
        .map(|p| p.faction_key.as_str())
        .collect();
    assert_eq!(
        playable,
        BTreeSet::from(["austria", "britain", "france", "prussia", "russia"])
    );

    // Spot checks from W3 §3.3 / §3.4 / §2.4.
    let by_key = |k: &str| w.factions.values().find(|f| f.key == k).unwrap();
    let france = by_key("france");
    assert_eq!(france.id, FactionId(749_327_284));
    assert_eq!(france.treasury, 6500);
    assert_eq!(by_key("austria").treasury, 7500);
    assert_eq!(
        by_key("britain").government,
        ntw_sim::campaign::GovernmentType::ConstitutionalMonarchy
    );
    assert_eq!(
        w.stance(france.id, by_key("britain").id),
        ntw_sim::campaign::Stance::War
    );
    assert_eq!(
        w.stance(france.id, by_key("spain").id),
        ntw_sim::campaign::Stance::Allied
    );
    let paris = w.regions.values().find(|r| r.key == "eur_france").unwrap();
    assert_eq!(paris.owner, france.id);
    assert_eq!(paris.population, 17_386_000);
    assert_eq!(paris.settlement.position.0.raw(), -222_517_056);
    assert_eq!(paris.settlement.position.1.raw(), 2_406_541);
    let french_forces = w.forces.values().filter(|f| f.faction == france.id);
    assert_eq!(french_forces.count(), 14);
    // Details (CAMPAIGN_DATA.md §3): France's government, capital and its navy minister.
    let fd = &w.faction_details[&france.id];
    assert_eq!(fd.display_name, "France");
    assert_eq!(fd.capital, Some(paris.id));
    assert!(w.is_capital(paris.id));
    assert_eq!(fd.religion, "rel_catholic");
    assert_eq!(fd.posts.len(), 8);
    let leader = fd.leader().expect("france has a leader");
    let ld = &w.character_details[&leader];
    println!("france leader {:?}: {} {} ({:?})", leader, ld.forename, ld.surname, ld.portrait.card);
    let navy = w.character_details[&ntw_sim::campaign::CharacterId(748_905_244)].clone();
    assert_eq!((navy.forename.as_str(), navy.surname.as_str()), ("names_name_names_frenchDenis", "names_name_names_frenchDecrès"));
    assert_eq!(navy.portrait.index, 62);
    assert_eq!(navy.traits.len(), 2);
    assert_eq!(fd.holder_of("navy"), Some(ntw_sim::campaign::CharacterId(748_905_244)));
    assert_eq!(navy.post as i32, fd.posts.iter().find(|p| p.key == "navy").unwrap().id);
    let gov = fd.governorship().expect("governor_europe");
    assert_eq!(gov.regions.len(), w.regions.values().filter(|r| r.owner == france.id).count());
    assert_eq!((france.tax_lower.as_str(), gov.taxes.lower_rate), ("tax_normal", 15));
    // Stances are stored on both sides; report (not assert) whether they all mirror.
    println!("eur diplomacy symmetric: {}", w.diplomacy_is_symmetric());

    // The loaded campaign can be played: one end-turn changes the date deterministically.
    let mut a = l.model.clone();
    let mut b = l.model.clone();
    a.end_turn();
    b.end_turn();
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.calendar.turns_elapsed, 1);
}

#[test]
fn missing_file_is_an_io_error() {
    let db = GameDatabase::test_fixture();
    let e = ntw_campaign::load_startpos_file("this/file/does/not/exist.esf", &db).unwrap_err();
    assert!(matches!(e, ntw_campaign::LoadError::Io { .. }), "{e}");
}

/// Every `CAMPAIGN_MISSION_MANAGER` in the 8 start positions reads with `ntw_campaign::missions`,
/// and writing it back gives the identical record (so the manager and `MISSIONS[]` versions match
/// the exe's writer) (analysis/campaign/S1_MISSIONS_UI.md).
#[test]
fn mission_managers_read_and_write_back() {
    use ntw_campaign::missions;
    let files: Vec<PathBuf> =
        CAMPAIGNS.iter().map(|c| data_dir().join("campaigns").join(c).join("startpos.esf")).collect();
    let (mut seen_files, mut managers, mut missions_total) = (0, 0, 0);
    for path in files.iter().filter(|p| p.is_file()) {
        let esf = EsfFile::open(path).unwrap();
        for (faction, rec) in missions::find_managers(&esf.root) {
            let m = missions::read_manager(rec)
                .unwrap_or_else(|e| panic!("{}: {faction}: {e}", path.display()));
            assert_eq!(&missions::write_manager(&m), rec, "{}: {faction}", path.display());
            managers += 1;
            missions_total += m.missions.len();
            for x in &m.missions {
                println!("{}: {faction}: {} ({:?})", path.display(), x.script_key, x.objectives.kind);
                assert!(!x.script_key.is_empty() && x.objectives.kind != missions::MissionKind::Unset);
            }
        }
        seen_files += 1;
    }
    if seen_files == 0 {
        println!("SKIPPED: no start positions found");
        return;
    }
    println!("{seen_files} files, {managers} mission managers, {missions_total} missions");
    assert!(managers > 0, "no CAMPAIGN_MISSION_MANAGER found");
}

/// The model's recruit permission (`units_to_exclusive_faction_permissions`, `CampaignRules::faction_may_recruit`)
/// on the real data: France's own line infantry is France's alone. (Ported from the AI's deleted copy of the
/// rule, 0b-recruit review round 2.)
#[test]
fn recruit_permissions_from_the_real_data() {
    let Some(db) = db() else { return };
    let rules = ntw_campaign::rules_from_db(db, "eur_napoleon");
    assert!(rules.faction_may_recruit("france", "Inf_Line_French_Fusiliers"));
    assert!(!rules.faction_may_recruit("austria", "Inf_Line_French_Fusiliers"));
}
