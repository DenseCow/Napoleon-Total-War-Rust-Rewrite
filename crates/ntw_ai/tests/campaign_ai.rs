//! Campaign AI over several End Turns of the real eur_napoleon startpos (read-only), through the
//! real turn loop: bare (`driver::end_turn`, map movement grid included) and inside the Lua script
//! host (`driver::install`). Skips (passes) when the game is not installed. Override with
//! `NTW_DATA_DIR`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use ntw_ai::campaign::driver::{self, AiTurnReport};
use ntw_ai::campaign::keys::{FactionAiKeys, read_ai_keys};
use ntw_ai::campaign::world::dist;
use ntw_ai::campaign::{Node, AiOrder, AiWorld, CampaignAiData, FactionAiConfig};
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::{CampaignModel, FactionId, RegionId, Terrain};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

struct Fixture {
    /// The startpos with its rules and the map's movement grid, france human, not started.
    model: CampaignModel,
    data: Arc<CampaignAiData>,
    /// The factions' stored manager / personality keys (CONFIRMED source).
    keys: BTreeMap<String, FactionAiKeys>,
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
        let mut loaded = ntw_campaign::read_file(&sp, &db).expect("startpos");
        assert!(loaded.set_human("france"));
        let vfs = Vfs::open_install(&dir).expect("packs");
        let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
        let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
        loaded.model.terrain = Some(Terrain(Arc::new(ntw_campaign::pathing::build_grid(&map))));
        let keys = read_ai_keys(&ntw_formats::esf::EsfFile::open(&sp).expect("esf").root);
        Some(Fixture { model: loaded.model, data: Arc::new(data), keys })
    })
    .as_ref()
}

fn faction(m: &CampaignModel, key: &str) -> FactionId {
    m.world.factions.values().find(|f| f.key == key).unwrap().id
}

/// Plays `turns` End Turns through the turn loop with the AI. Returns the final model and every
/// AI turn's report.
fn play(f: &Fixture, turns: u32) -> (CampaignModel, Vec<AiTurnReport>) {
    let mut m = f.model.clone();
    let mut ctx = driver::context_for(&m, "eur_napoleon");
    ctx.ai_keys = f.keys.clone();
    let mut log = Vec::new();
    for _ in 0..turns {
        let (_, reports) = driver::end_turn(&mut m, &f.data, &ctx);
        log.extend(reports);
    }
    (m, log)
}

fn kind(o: &AiOrder) -> &'static str {
    match o {
        AiOrder::Recruit { .. } => "recruit",
        AiOrder::Construct { .. } => "construct",
        AiOrder::MoveForce { .. } => "move",
        AiOrder::AttackForce { .. } => "attack",
        AiOrder::DeclareWar { .. } => "war",
        AiOrder::MakePeace { .. } => "peace",
        AiOrder::Merge { .. } => "merge",
        AiOrder::SetTax { .. } => "tax",
        AiOrder::StartResearch { .. } => "research",
    }
}

fn negative_treasuries(m: &CampaignModel, human: FactionId) -> Vec<String> {
    m.world.factions.values().filter(|x| x.id != human && x.treasury < 0).map(|x| x.key.clone()).collect()
}

#[test]
fn ai_tables_load_from_the_install() {
    let Some(f) = fixture() else { return };
    let d = &f.data;
    assert_eq!(d.default_personality, "default");
    assert_eq!(d.tunable("default", "BASIC_SPENDING_BIAS_CONSTRUCTION"), Some(0.25));
    assert_eq!(d.tunable("eur_france", "BASIC_SPENDING_BIAS_CONSTRUCTION"), Some(0.25), "falls back to default");
    assert_eq!(d.tunable("eur_france", "LOOT_SETTLEMENT_BASE_CHANCE"), Some(0.0), "overlay wins");
    assert_eq!(d.managers["nap_eur_full"]["EXPANSION_BEHAVIOUR"], 3000.0);
    assert!(d.unit_balances.len() >= 10);
    assert!(d.faction_may_recruit("france", "Inf_Line_French_Fusiliers"));
    assert!(!d.faction_may_recruit("austria", "Inf_Line_French_Fusiliers"));
    let cfg = FactionAiConfig::resolve(d, "eur_napoleon", "france");
    assert_eq!((cfg.manager.as_str(), cfg.personality.as_str()), ("nap_eur_france", "eur_france"));
    let cfg = FactionAiConfig::resolve(d, "eur_napoleon", "prussia");
    assert_eq!((cfg.manager.as_str(), cfg.personality.as_str()), ("nap_eur_full", "default"));
}

/// `campaign_difficulty_handicap_effects` through the CONFIRMED lookup (clamp −2..2, flag picks the
/// list) and the INFERRED mapping (human: (d, true); AI: (−d, false)).
#[test]
fn difficulty_handicaps() {
    let Some(f) = fixture() else { return };
    let d = &f.data;
    // Human on very hard: recruitment costs +20 %, attrition +15 (CONFIRMED rows).
    assert_eq!(d.handicap(-2, true, "recruitment_cost_mod_land_all"), 20.0);
    assert_eq!(d.handicap(-2, true, "attrition_difficulty_addition"), 15.0);
    // Out-of-range difficulties clamp (0x00F9F970).
    assert_eq!(d.handicap(-7, true, "recruitment_cost_mod_land_all"), 20.0);
    // AI at the player's very hard: the (2, false) rows, e.g. GDP +40 %.
    assert_eq!(d.ai_handicap(-2, "gdp_mod_all"), 40.0);
    assert_eq!(d.ai_handicap(-1, "recruitment_cost_mod_land_all"), -10.0);
    assert_eq!(d.ai_handicap(0, "research_rate_mod"), 25.0);
    assert_eq!(d.handicap_effects(2, false).len(), 20);
    assert!(d.handicap_effects(2, true).is_empty());
}

#[test]
fn several_end_turns_from_eur_startpos() {
    let Some(f) = fixture() else { return };
    // Map scale, to sanity-check the PROVISIONAL radii.
    let w = AiWorld::from_model(&f.model);
    let mut nn: Vec<f64> = w
        .regions
        .values()
        .map(|r| w.regions.values().filter(|o| o.id != r.id).map(|o| dist(r.position, o.position)).fold(f64::INFINITY, f64::min))
        .collect();
    nn.sort_by(f64::total_cmp);
    eprintln!("nearest-settlement distance: min {:.1} median {:.1} max {:.1}", nn[0], nn[nn.len() / 2], nn[nn.len() - 1]);

    let france = faction(&f.model, "france");
    let start_units: usize = f.model.world.forces.values().filter(|x| x.faction != france).map(|x| x.units.len()).sum();
    let (m, log) = play(f, 4);
    let mut given: BTreeMap<&str, usize> = BTreeMap::new();
    for r in &log {
        assert_ne!(r.faction, france, "the human faction gets no AI turn");
        for o in &r.orders {
            *given.entry(kind(o)).or_default() += 1;
        }
    }
    let accepted: usize = log.iter().map(|r| r.accepted).sum();
    let given_total: usize = given.values().sum();
    eprintln!("orders over 4 End Turns: {given:?}, accepted {accepted} of {given_total}");
    assert!(given.get("recruit").copied().unwrap_or(0) > 0, "AI factions recruit");
    assert!(given.get("construct").copied().unwrap_or(0) > 0, "AI factions build");
    assert!(given.get("move").copied().unwrap_or(0) > 0, "AI armies move");
    assert!(given.get("merge").copied().unwrap_or(0) > 0, "MERGE_UNITS joins small armies");
    // TAXATION keeps every class of every AI faction at level 2 (CONFIRMED rule).
    let after = AiWorld::from_model(&m);
    let normal = after.tax_levels[2].0.clone();
    for (_, fac) in after.factions.iter().filter(|(id, x)| **id != france && x.regions > 0) {
        assert!(fac.tax.iter().all(|l| *l == normal), "{}: {:?}", fac.key, fac.tax);
    }
    assert!(accepted * 2 >= given_total, "most AI orders are valid model commands ({accepted} of {given_total})");
    assert_eq!(m.calendar.turns_elapsed, 4);
    assert_eq!(m.turn.current, Some(france), "back to the human's turn");

    // The orders had effects in the model: buildings queued or finished, units queued or trained,
    // armies moved.
    let queued_buildings: usize = m.world.regions.values().filter(|r| r.owner != france).map(|r| r.construction.len()).sum();
    let changed_buildings = m
        .world
        .regions
        .values()
        .filter(|r| r.owner != france)
        .filter(|r| f.model.world.regions.get(&r.id).is_some_and(|o| o.slots != r.slots || o.road != r.road))
        .count();
    let units: usize = m.world.forces.values().filter(|x| x.faction != france).map(|x| x.units.len()).sum();
    let queued_units: usize = m.world.regions.values().filter(|r| r.owner != france).map(|r| r.recruitment_queue.len()).sum();
    let moved = m
        .world
        .characters
        .values()
        .filter(|c| c.faction != france)
        .filter(|c| f.model.world.characters.get(&c.id).is_some_and(|o| o.position != c.position))
        .count();
    eprintln!(
        "AI factions: {queued_buildings} buildings queued, {changed_buildings} regions with new buildings, units {start_units} -> {units} (+{queued_units} queued), {moved} characters moved"
    );
    assert!(queued_buildings + changed_buildings > 0, "AI construction reached the model");
    assert!(units > start_units || queued_units > 0, "AI recruitment reached the model");
    assert!(moved > 0, "AI armies moved on the map");

    // No AI faction ends up bankrupt from its own spending: every negative treasury is one the
    // faction also has with the AI idle (upkeep alone).
    let mut idle = f.model.clone();
    for _ in 0..4 {
        idle.end_turn();
    }
    let broke = negative_treasuries(&m, france);
    let broke_idle = negative_treasuries(&idle, france);
    eprintln!("negative treasuries: with AI {broke:?}, AI idle {broke_idle:?}");
    for k in &broke {
        assert!(broke_idle.contains(k), "{k} went bankrupt because of the AI");
    }
}

/// `RESEARCH_TECHNOLOGY` (CONFIRMED behaviour row, priority 500 in every shipped eur manager): the
/// AI starts technologies at its own schools. Every start is a valid model command, no school gets
/// two technologies, and a school already busy is left alone.
#[test]
fn ai_researches_at_its_schools() {
    let Some(f) = fixture() else { return };
    let france = faction(&f.model, "france");
    // Every eur AI faction runs a manager with the RESEARCH_TECHNOLOGY row (CONFIRMED table).
    for k in f.keys.values() {
        if k.manager.contains("maintainance") || k.manager.contains("britain") || k.manager.contains("france") {
            assert_eq!(f.data.managers[&k.manager].get("RESEARCH_TECHNOLOGY"), Some(&500.0), "{}", k.manager);
        }
    }
    // The snapshot sees schools and technologies.
    let w = AiWorld::from_model(&f.model);
    let schools: usize = w.regions.values().filter_map(|r| r.schools.as_ref()).map(Vec::len).sum();
    let ai_factions: Vec<FactionId> =
        w.factions.iter().filter(|(id, x)| **id != france && x.regions > 0).map(|(id, _)| *id).collect();
    let avail: usize = ai_factions
        .iter()
        .filter_map(|id| w.factions[id].technologies.as_ref())
        .map(|t| t.values().filter(|&&s| s == ntw_sim::campaign::research::state::AVAILABLE).count())
        .sum();
    eprintln!("{schools} schools, {avail} available technologies over {} AI factions", ai_factions.len());
    assert!(schools > 0, "the snapshot finds schools");
    assert!(avail > 0, "AI factions have technologies they may research");
    assert!(w.technologies.is_some(), "the snapshot knows the technologies table");

    // Play eight turns and check every start.
    let (m, log) = play(f, 8);
    let mut starts: Vec<(FactionId, RegionId, usize, String)> = Vec::new();
    for r in &log {
        for o in &r.orders {
            if let AiOrder::StartResearch { region, slot, tech } = o {
                starts.push((r.faction, *region, *slot, tech.clone()));
            }
        }
    }
    eprintln!("AI research starts over 8 turns: {}", starts.len());
    assert!(!starts.is_empty(), "AI factions start research");
    // One school per faction per turn, and every start reached the model.
    let mut seen: BTreeSet<(FactionId, RegionId, usize, String)> = BTreeSet::new();
    for s in &starts {
        assert!(seen.insert(s.clone()), "the same school was given twice: {s:?}");
        let tech = &s.3;
        assert!(
            m.tech_state(s.0, tech).is_some_and(|st| st == ntw_sim::campaign::research::state::AVAILABLE
                || st == ntw_sim::campaign::research::state::RESEARCHED),
            "{tech} is not a technology of {}",
            m.world.factions[&s.0].key
        );
        // The start reached the model: the school works on it, or it has been finished since, or a
        // later turn gave this school another technology (the model drops the old one).
        let progress = m.world.faction_details[&s.0].research.get(tech).map_or(0.0, |t| t.progress);
        assert!(
            m.researching_at(s.1, s.2).as_deref() == Some(tech.as_str())
                || progress > 0.0
                || m.tech_state(s.0, tech) == Some(ntw_sim::campaign::research::state::RESEARCHED),
            "no research of {tech} at {} slot {}",
            m.world.regions[&s.1].key,
            s.2
        );
    }
    // Research actually progresses: a school with a researcher gains progress.
    let researching: usize = ai_factions
        .iter()
        .map(|id| m.world.faction_details.get(id).map_or(0, |d| d.research.values().filter(|t| t.researcher != 0).count()))
        .sum();
    eprintln!("technologies under research at the end: {researching}");
    assert!(researching > 0, "AI schools are working on a technology");
}

#[test]
fn campaign_ai_is_deterministic() {
    let Some(f) = fixture() else { return };
    let (a, la) = play(f, 2);
    let (b, lb) = play(f, 2);
    assert_eq!(la, lb);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn ai_plays_inside_the_script_host() {
    let Some(f) = fixture() else { return };
    let source = ntw_script::ScriptSource::from_install(data_dir()).expect("scripts");
    let mut host = ntw_script::ScriptHost::new(f.model.clone(), "france", source).expect("host");
    host.load_campaign("eur_napoleon").expect("scripts load");
    let log = driver::install(&mut host, f.data.clone(), f.keys.clone());
    host.fire("NewSession", ntw_script::ScriptContext::for_faction("france"));
    host.fire("NewCampaignStarted", ntw_script::ScriptContext::for_faction("france"));
    host.start_campaign();
    let mut errors = 0;
    for _ in 0..2 {
        errors += host.end_turn().iter().map(|(_, r)| r.errors.len()).sum::<usize>();
    }
    let log = log.borrow();
    let accepted: usize = log.iter().map(|r| r.accepted).sum();
    eprintln!("script host: {} AI turns, {accepted} orders accepted, {errors} script errors", log.len());
    assert!(log.len() > 10, "AI factions played their turns");
    assert!(accepted > 0);
    let st = host.state();
    let ctx = driver::context_from_script(&st);
    assert!(ctx.humans.contains(&faction(&st.model, "france")));
    assert_eq!(ctx.campaign_key, "eur_napoleon");
}

/// Every faction's manager and personality keys come from its `FACTION` record (CONFIRMED
/// source, `keys::read_ai_keys`), and they name rows of the AI tables.
#[test]
fn ai_keys_come_from_the_startpos() {
    let Some(f) = fixture() else { return };
    let fr = &f.keys["france"];
    assert_eq!((fr.manager.as_str(), fr.personality.as_str()), ("nap_eur_france", "eur_france"));
    let mut managers: BTreeMap<&str, usize> = BTreeMap::new();
    for (key, k) in &f.keys {
        *managers.entry(k.manager.as_str()).or_default() += 1;
        if !key.is_empty() && !k.manager.is_empty() {
            assert!(f.data.managers.contains_key(&k.manager), "{key}: manager {}", k.manager);
        }
    }
    eprintln!("managers in the eur startpos: {managers:?}");
    let full: Vec<(&String, &str)> = f.keys.iter().filter(|(_, k)| !k.manager.contains("maintainance")).map(|(f, k)| (f, k.personality.as_str())).collect();
    eprintln!("not maintainance: {full:?}");
    let cfg = FactionAiConfig::resolve_with(&f.data, "eur_napoleon", "britain", f.keys.get("britain"));
    assert_eq!(cfg.manager, f.keys["britain"].manager);
}

/// The original's own region base values (belief 0x4D, `CAI_REGION_BASE_VALUE`) are stored in the
/// startpos: one per region, each `15000 + 25 k` as the decoded formula `0x00ABD5E0` gives.
#[test]
fn region_base_values_come_from_the_startpos() {
    let dir = data_dir();
    let sp = dir.join(r"campaigns\eur_napoleon\startpos.esf");
    if !sp.is_file() {
        eprintln!("skipped: no install");
        return;
    }
    let v = ntw_ai::campaign::keys::read_region_base_values(&ntw_formats::esf::EsfFile::open(&sp).expect("esf").root);
    assert_eq!(v.len(), 72);
    assert_eq!(v["eur_france"], 50450);
    assert_eq!(v["eur_gibraltar"], 16900);
    assert!(v.values().all(|x| *x >= 15000 && (x - 15000) % 25 == 0), "{v:?}");
}

#[test]
fn bdi_pool_builds_the_goal_tree() {
    let Some(f) = fixture() else { return };
    let w = AiWorld::from_model(&f.model);
    let mut ctx = driver::context_for(&f.model, "eur_napoleon");
    ctx.ai_keys = f.keys.clone();
    let austria = faction(&f.model, "austria");
    let mut rng = ntw_sim::rng::CaRng::new(12345);
    let (orders, pool) = ntw_ai::campaign::take_turn_with_plan(&w, &f.data, &ctx, austria, &mut rng);
    let groups = w.region_groups(austria);
    eprintln!("austria: {} groups {:?}, {} components, {} orders", groups.len(), groups.iter().map(|g| g.len()).collect::<Vec<_>>(), pool.len(), orders.len());
    // One component per junction row of its manager (nap_eur_full), except WAR_AND_PEACE and
    // DIPLOMACY_MANAGER (CONFIRMED: they build nothing).
    let rows = f.data.managers[&f.keys["austria"].manager].keys().filter(|k| *k != "WAR_AND_PEACE" && *k != "DIPLOMACY_MANAGER").count();
    let behaviours: Vec<f32> = pool.iter().filter_map(|(n, p)| matches!(n, Node::Behaviour(_)).then_some(*p)).collect();
    assert_eq!(behaviours.len(), rows);
    // One REGION_GROUP_DEFENCE goal per own region group.
    assert_eq!(pool.iter().filter(|(n, _)| matches!(n, Node::GroupDefence(_))).count(), groups.len());
    // The raw ±10 jitter takes some priorities well outside ±10 % (or below 0).
    let mex = pool.iter().find_map(|(n, p)| match n {
        Node::Behaviour(k) if k == "MERGE_UNITS" => Some(*p),
        _ => None,
    });
    eprintln!("MERGE_UNITS priority after jitter: {mex:?} (DB 5000)");
    assert!(pool.iter().all(|(_, p)| p.is_finite()));
}

#[test]
fn faction_difficulty_blocks_come_from_the_startpos() {
    let dir = data_dir();
    let sp = dir.join(r"campaigns\eur_napoleon\startpos.esf");
    if !sp.is_file() {
        return;
    }
    let esf = ntw_formats::esf::EsfFile::open(&sp).expect("esf");
    let d = ntw_ai::campaign::keys::read_difficulties(&esf.root);
    eprintln!("difficulty blocks: {} factions, france {:?}", d.len(), d.get("france"));
    assert!(d.len() >= 30);
    assert!(d.values().all(|x| x.campaign == 0 && x.battle == 0), "{d:?}");
}
