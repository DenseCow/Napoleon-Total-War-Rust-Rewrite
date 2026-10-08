//! Our own save writer against the real install: a start position written as a save reloads, and
//! each model change is written (CAMPAIGN_DATA.md §5). Loading the original game's saves is out of
//! scope, so nothing here reads the user's save folder. Run with
//! `cargo test -p ntw_campaign --test save_compat -- --nocapture` (skips itself without an install).

use std::path::PathBuf;

use ntw_campaign::save;
use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// A start position written as a save reloads to the same model, for all 8 campaigns, and
/// keeps every startpos block except the front-end one (W3 §4).
#[test]
fn startpos_saves_reload() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIPPED: no install");
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    for c in ["eur_napoleon", "mp_eur_napoleon", "egy_napoleon", "mp_egy_napoleon", "ita_napoleon", "mp_ita_napoleon", "spa_napoleon", "tut_napoleon"] {
        let bytes = std::fs::read(dir.join("campaigns").join(c).join("startpos.esf")).unwrap();
        let esf = EsfFile::from_bytes(&bytes).unwrap();
        let l = ntw_campaign::read_esf(&esf, &db).unwrap();
        let human = l.info.header.faction_key.clone();
        let out = save::write_save(&esf, &l.model, &human, 1).unwrap();
        assert_eq!(out.root.name, ntw_campaign::SAVE_ROOT);
        assert!(out.root.child("CAMPAIGN_PREOPEN_MAP_INFO").is_none());
        // The player is human (only him): without the flag the original plays every turn by
        // itself (SAVE_COMPAT.md §2, the user's test).
        let flags = save::player_flags(&out);
        assert!(save::player_flags(&esf).iter().all(|(_, h)| !h), "{c}: a startpos has no human");
        assert_eq!(flags.iter().filter(|(_, h)| *h).map(|(k, _)| k.as_str()).collect::<Vec<_>>(), [human.as_str()], "{c}");
        // Every link the original needs, and the AI block kept (nothing changed).
        let r = ntw_campaign::save_check::check(&out);
        assert!(r.violations.is_empty(), "{c}: {:#?}", r.violations);
        assert!(!r.ai_block_rebuilt, "{c}");
        let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap();
        let w = &back.model.world;
        assert_eq!(w.characters, l.model.world.characters, "{c}");
        assert_eq!(w.forces, l.model.world.forces, "{c}");
        assert_eq!(w.character_details, l.model.world.character_details, "{c}");
        assert_eq!(w.faction_details, l.model.world.faction_details, "{c}");
        assert_eq!(w.relationships, l.model.world.relationships, "{c}");
        for (id, f) in &l.model.world.factions {
            let g = &w.factions[id];
            assert_eq!((&g.tax_lower, &g.tax_upper, &g.diplomacy), (&f.tax_lower, &f.tax_upper, &f.diplomacy), "{c} {}", f.key);
        }
        println!("{c}: startpos -> save -> reload OK");
    }
}

/// Changes the model makes are written where the original keeps them: taxes, stances and the
/// script slots.
#[test]
fn taxes_stances_and_script_values_are_written() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let bytes = std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap();
    let esf = EsfFile::from_bytes(&bytes).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    let austria = m.faction_by_key("austria").unwrap().id;
    // As the player sets it: `SetTaxLevel` also updates the governorship (SAVE_COMPAT.md §26), and
    // the writer writes the governorship's levels.

    m.apply(ntw_sim::campaign::CampaignCommand::SetTaxLevel { faction: france, class: ntw_sim::campaign::TaxClass::Lower, level: "tax_high".into() })
        .unwrap_or_else(|e| panic!("SetTaxLevel: {e}"));
    let _ = m.world.set_stance(france, austria, ntw_sim::campaign::Stance::War);
    use ntw_campaign::script_values::ScriptSaveValue as V;
    let vals = [V::Int(3), V::Bool(true), V::Int(-1)];
    let out = save::write_save_with(&esf, &m, "france", 1, Some(&vals)).unwrap();
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap();
    let w = &back.model.world;
    assert_eq!(w.factions[&france].tax_lower, "tax_high");
    assert_eq!(w.faction_details[&france].governorship().unwrap().taxes.lower_rate, 20);
    assert_eq!(w.stance(france, austria), ntw_sim::campaign::Stance::War);
    assert_eq!(w.stance(austria, france), ntw_sim::campaign::Stance::War);
    assert_eq!(w.relationships[&(france, austria)].previous_stance, l.model.world.factions[&france].diplomacy.get(&austria).copied().unwrap_or_default().esf_name());
    assert_eq!(back.script_values, vals);
}

/// Every place outside the AI block where `v` appears as an integer (or array element).
fn int_sites(nodes: &[EsfNode], v: i64, path: &str, out: &mut Vec<String>) {
    for (i, n) in nodes.iter().enumerate() {
        match n {
            EsfNode::Record(r) if r.name == "CAI_INTERFACE" => {}
            EsfNode::Record(r) => int_sites(&r.children, v, &format!("{path}/{}", r.name), out),
            EsfNode::RecordArray(a) => a.items.iter().for_each(|it| int_sites(it, v, &format!("{path}/{}[]", a.name), out)),
            other => {
                let hit = other.as_int() == Some(v)
                    || other.as_u32_array().is_some_and(|a| a.iter().any(|&x| i64::from(x) == v))
                    || other.as_i32_array().is_some_and(|a| a.iter().any(|&x| i64::from(x) == v));
                if hit {
                    out.push(format!("{path} #{i}"));
                }
            }
        }
    }
}

/// A character the model no longer has leaves no reference behind (SAVE_COMPAT.md §4: the
/// original resolves every id through its global map): here a candidate of a recruitment pool,
/// the reference site a startpos has besides the character records.
#[test]
fn removed_characters_leave_no_references() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let mut pool = None;
    esf.root.walk(&mut |r| {
        if pool.is_none() && r.name == "GENERAL_RECRUITMENT" {
            pool = r.get(0).and_then(EsfNode::as_u32_array).and_then(|a| a.first().copied());
        }
    });
    let id = pool.expect("a pool candidate");
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    assert!(m.world.characters.remove(&ntw_sim::campaign::CharacterId(id as i32)).is_some());
    let human = l.info.header.faction_key.clone();
    let out = save::write_save(&esf, &m, &human, 1).unwrap();
    let mut sites = Vec::new();
    int_sites(&out.root.children, i64::from(id), &out.root.name, &mut sites);
    assert!(sites.is_empty(), "{id} still referenced: {sites:#?}");
    let r = ntw_campaign::save_check::check(&out);
    assert!(r.violations.is_empty(), "{:#?}", r.violations);
}

/// The region economy the model changes each round is written where the original keeps it and
/// reads back (REGION #9/#10/#12/#14/#15/#17/#18/#19, CAMPAIGN_FIDELITY.md "Economy").
#[test]
fn region_economy_is_written() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    let ids: Vec<_> = m.world.regions.keys().copied().take(3).collect();
    for (k, id) in ids.iter().enumerate() {
        let r = m.world.regions.get_mut(id).unwrap();
        r.base_gdp += 11 + k as u32;
        r.gdp += 101 + k as u32;
        r.town_wealth += 7 + k as u32;
        r.town_wealth_growth = -5 + k as i32;
        r.wealth_growth_offset = 2 * k as i32;
        r.discontent_growth = -(k as i32);
        r.tax_exempt = k == 1;
    }
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap();
    for id in &ids {
        let (a, b) = (&m.world.regions[id], &back.model.world.regions[id]);
        assert_eq!(
            (a.base_gdp, a.gdp, a.town_wealth, a.town_wealth_growth, a.wealth_growth_offset, a.discontent_growth, a.tax_exempt),
            (b.base_gdp, b.gdp, b.town_wealth, b.town_wealth_growth, b.wealth_growth_offset, b.discontent_growth, b.tax_exempt),
            "{}",
            a.key
        );
    }
    // #14 follows #12 (CONFIRMED in every original save).
    let rm = out.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER").unwrap();
    for r in rm.record_array("REGIONS_ARRAY").unwrap().records() {
        assert_eq!(r.get_u32(12), r.get_u32(14));
    }
    let r = ntw_campaign::save_check::check(&out);
    assert!(r.violations.is_empty(), "{:#?}", r.violations);
}

/// The trade routes' accumulated values and the bankrupt-turn counters are read from a save and
/// written back where the original keeps them (SAVE_COMPAT.md §12).
#[test]
fn trade_accumulation_and_bankruptcy_are_written() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    let pairs: Vec<_> = ntw_campaign::trade::read_accumulated(&esf, &m).keys().copied().take(3).collect();
    assert!(!pairs.is_empty(), "the startpos has trade routes");
    for (k, p) in pairs.iter().enumerate() {
        m.world.trade_accumulated.insert(*p, 100 * (k as i32 + 1));
    }
    let france = m.faction_by_key("france").unwrap().id;
    m.world.bankrupt_turns.insert(france, 3);
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap();
    for p in &pairs {
        assert_eq!(back.model.world.trade_accumulated.get(p), m.world.trade_accumulated.get(p));
    }
    assert_eq!(back.model.world.bankrupt_turns.get(&france), Some(&3));
}

/// An embarked army is written as the two-way link ARMY #7 / NAVY #4 (CONFIRMED storage, the
/// ports worker), every other force has 0 there, and `save_check` accepts it.
#[test]
fn embarked_armies_are_linked() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    let army = m.world.forces.values().find(|f| f.faction == france && !f.is_navy && f.commander.is_some()).unwrap().id;
    let navy = m.world.forces.values().find(|f| f.faction == france && f.is_navy && f.commander.is_some()).unwrap().id;
    // The commander stands at the navy (our placement, PROVISIONAL).
    let navy_pos = m.world.characters[&m.world.forces[&navy].commander.unwrap()].position;
    let cmd = m.world.forces[&army].commander.unwrap();
    m.world.characters.get_mut(&cmd).unwrap().position = navy_pos;
    m.world.characters.get_mut(&cmd).unwrap().garrisoned_in = None;
    for r in m.world.regions.values_mut() {
        if r.garrison == Some(army) {
            r.garrison = None;
        }
    }
    m.world.embarked.insert(army, navy);
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let mut seen = (0, 0);
    for f in out.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap().record_array("FACTION_ARRAY").unwrap().records() {
        for a in f.record_array("ARMY_ARRAY").into_iter().flat_map(|x| x.records()) {
            let id = a.child("MILITARY_FORCE").and_then(|x| x.get_u32(0)).unwrap();
            let link = if a.name == "NAVY" { a.get_u32(4) } else { a.get_u32(7) };
            let want = if id == army.raw() {
                seen.0 += 1;
                navy.raw()
            } else if id == navy.raw() {
                seen.1 += 1;
                army.raw()
            } else {
                0
            };
            assert_eq!(link, Some(want), "force {id} ({})", a.name);
        }
    }
    assert_eq!(seen, (1, 1));
    let r = ntw_campaign::save_check::check(&out);
    assert!(r.violations.is_empty(), "{:#?}", r.violations);
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap();
    assert_eq!(back.model.world.embarked.get(&army), Some(&navy));
}

/// A new campaign's human gets the start position's first victory option in its three records
/// (the default would make the original declare a campaign victory, SAVE_COMPAT.md §17), and a
/// captured region hands every residence of the old owner to the new one.
#[test]
fn victory_conditions_and_captured_regions_are_written() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let option = ntw_campaign::victory::options(&esf.root, "france").unwrap().remove(0);
    assert_eq!(ntw_campaign::victory::kind(&option), (1, 5));
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    // Austria takes a french region (its garrison leaves).
    let (france, austria) = (m.faction_by_key("france").unwrap().id, m.faction_by_key("austria").unwrap().id);
    let rid = m.world.regions.values().find(|r| r.owner == france && r.garrison.is_some()).unwrap().id;
    let g = m.world.regions.get_mut(&rid).unwrap().garrison.take().unwrap();
    let c = m.world.forces[&g].commander.unwrap();
    m.world.characters.get_mut(&c).unwrap().garrisoned_in = None;
    m.world.regions.get_mut(&rid).unwrap().owner = austria;
    let before = ntw_campaign::victory::met_at_once(&{
        let mut e = esf.clone();
        save::mark_human(&mut e, "france");
        e
    });
    assert!(before.is_empty() || before == ["france"], "{before:?}");
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    assert!(ntw_campaign::victory::met_at_once(&out).is_empty());
    let mut found = 0;
    out.root.walk(&mut |r: &EsfRecord| {
        if r.name == ntw_campaign::victory::RECORD && *r == option {
            found += 1;
        }
    });
    assert_eq!(found, 3, "setup, faction player setup and faction records");
    let world = out.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let region = world
        .child("REGION_MANAGER")
        .and_then(|x| x.record_array("REGIONS_ARRAY"))
        .unwrap()
        .records()
        .find(|r| r.get_u32(20) == Some(austria.raw() as u32) && r.get_str(0) == Some(m.world.regions[&rid].key.as_str()))
        .unwrap();
    let mut owners = Vec::new();
    region.walk(&mut |r: &EsfRecord| {
        if r.name == "GARRISON_RESIDENCE" {
            owners.push(r.get_u32(0));
        }
    });
    assert!(owners.len() > 1 && owners.iter().all(|o| *o == Some(austria.raw() as u32)), "{owners:?}");
    let r = ntw_campaign::save_check::check(&out);
    assert!(r.violations.is_empty(), "{:#?}", r.violations);
}

/// The list-1 rows of every grid node: (node first u32, pair list, x). Rows are matched by their
/// pair list, not by their version index: a removal renumbers the versions (SAVE_COMPAT.md §29),
/// and no node of an original save has two rows with one pair list.
fn grid_rows(esf: &EsfFile) -> Vec<(u32, Vec<u32>, u32)> {
    let grid = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").unwrap().record_array("PATHFINDING_GRID").unwrap().items[0].clone();
    let nodes = grid.iter().find_map(|n| n.as_record_array().filter(|a| a.name == "OBSTACLE_BASE_GRID_NODE")).unwrap();
    let mut out = Vec::new();
    for it in &nodes.items {
        let key = it.first().and_then(EsfNode::as_u32).unwrap();
        if let Some(l) = it.iter().find_map(EsfNode::as_record_array) {
            for row in &l.items {
                let pairs = row.iter().find_map(EsfNode::as_u32_array).unwrap().to_vec();
                out.push((key, pairs, row[1].as_u32().unwrap()));
            }
        }
    }
    out
}

/// Removing commanders takes their obstacles out of the grid as the original's saves show it:
/// no grid node left empty (the node and its cell entry go), the rows' other fields untouched,
/// a moved commander's core cleared (SAVE_COMPAT.md §18: our earlier writer left empty nodes
/// and rewrote the rows' second u32, and the original crashed loading the pathfinder), and every
/// cell version naming a removed layer gone whole, so that the loader replay (`grid_load_check`,
/// part of `save_check`) finds no row repeating another's pair list and no version without a
/// row (§29: the 0x00B1D3E0 crash of NR-5..NR-10).
#[test]
fn removed_obstacles_leave_a_valid_grid() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    // Three field armies go (commander and force); one more field army moves a little.
    let garrisons: std::collections::BTreeSet<_> = m.world.regions.values().filter_map(|r| r.garrison).collect();
    let field: Vec<_> = m.world.forces.values().filter(|f| !f.is_navy && !garrisons.contains(&f.id) && f.commander.is_some()).map(|f| (f.id, f.commander.unwrap())).take(4).collect();
    for &(f, c) in &field[..3] {
        m.world.forces.remove(&f);
        m.world.characters.remove(&c);
        m.world.character_details.remove(&c);
    }
    let mover = field[3].1;
    let p = &mut m.world.characters.get_mut(&mover).unwrap().position;
    p.0 = ntw_sim::fixed::Fixed20::from_raw(p.0.raw() + (1 << 20));
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let r = ntw_campaign::save_check::check(&out);
    assert!(r.violations.is_empty(), "{:#?}", r.violations);
    // Rows that survive keep their fields.
    let before: std::collections::BTreeMap<(u32, Vec<u32>), u32> = grid_rows(&esf).into_iter().map(|(k, p, x)| ((k, p), x)).collect();
    let after = grid_rows(&out);
    assert!(!after.is_empty() && after.len() < before.len());
    for (k, p, x) in after {
        assert_eq!(before.get(&(k, p.clone())), Some(&x), "row ({k}, {p:?})");
    }
    // The pair lists that name a removed commander are gone with their versions, the others stay.
    let gone: std::collections::BTreeSet<u32> = field[..3].iter().map(|&(_, c)| c.raw() as u32 | 2).collect();
    let grid = |e: &EsfFile| e.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").unwrap().record_array("PATHFINDING_GRID").unwrap().items[0].clone();
    let g = grid(&out);
    let rep = ntw_campaign::grid_load_check::check(&g);
    assert!(rep.faults.is_empty() && rep.rule_breaks.is_empty(), "{rep:?}");
    let versions_before = ntw_campaign::grid_load_check::check(&grid(&esf)).counts[0];
    let dropped_rows = before.keys().filter(|(_, p)| p.chunks(2).any(|q| gone.contains(&q[0]))).count();
    assert!(dropped_rows > 0);
    assert!(rep.counts[0] + dropped_rows <= versions_before, "the removed commanders' versions are gone");
    assert_eq!(rep.counts[0], rep.counts[1], "every version has a row");
}

/// The commodity market (prices, the two previous prices, trends, factors) is written back and
/// reloads as the model holds it; untouched, the trade manager is written back byte for byte.
#[test]
fn commodity_market_is_written() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let tm = |e: &EsfFile| e.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_TRADE_MANAGER").unwrap().clone();
    let same = save::write_save(&esf, &l.model, "france", 1).unwrap();
    assert!(tm(&same) == tm(&esf), "an unchanged market is written back as stored");
    let mut m = l.model.clone();
    assert!(!m.world.commodity_prices.is_empty());
    m.update_commodity_prices();
    for (i, p) in m.world.commodity_prices.iter_mut().enumerate() {
        *p += i as u32 + 1;
    }
    m.world.commodity_market.trend.iter_mut().for_each(|t| *t = 4);
    m.world.commodity_market.factors.iter_mut().for_each(|f| *f *= 1.5);
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap();
    assert_eq!(back.model.world.commodity_prices, m.world.commodity_prices);
    assert_eq!(back.model.world.commodity_market, m.world.commodity_market);
}

/// Unit id -> (faction key, name key), and every name list entry (faction key, name key, flag).
type UnitNames = (std::collections::BTreeMap<i32, (String, String)>, Vec<(String, String, bool)>);

fn unit_names_and_flags(esf: &EsfFile) -> UnitNames {
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let mut names = std::collections::BTreeMap::new();
    let mut flags = Vec::new();
    for f in w.record_array("FACTION_ARRAY").unwrap().records() {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        f.walk(&mut |r: &EsfRecord| {
            if r.name == "UNIT"
                && let Some(id) = r.get_i32(4)
            {
                let n = r.child("CAMPAIGN_LOCALISATION").and_then(|c| c.get_str(0)).unwrap_or("").to_string();
                names.insert(id, (key.clone(), n));
            }
            if r.name == "UNIT_CLASS_NAME_ALLOCATOR" {
                for it in r.record_array("UNIT_CLASS_NAMES_LIST").into_iter().flat_map(|a| a.items.iter()) {
                    let n = it[0].as_record().and_then(|c| c.get_str(0)).unwrap_or("").to_string();
                    flags.push((key.clone(), n, it[1].as_bool().unwrap_or(false)));
                }
            }
        });
    }
    (names, flags)
}

/// New units take the lowest free name of their class's list, their names are flagged, the names
/// of removed units are freed, and no name is carried twice (SAVE_COMPAT.md §20).
#[test]
fn new_units_get_regiment_names() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    // Two line infantry units join a french garrison; one french unit is disbanded.
    let region = m.world.regions.values().find(|r| r.owner == france && r.garrison.is_some_and(|g| m.world.forces[&g].units.len() < 18)).unwrap().id;
    let key = m.rules.units.iter().find(|(k, u)| u.unit_class == "infantry_line" && k.contains("French")).map(|(k, _)| k.clone()).expect("a line unit");
    let a = m.spawn_recruited_unit(region, key.clone());
    let b = m.spawn_recruited_unit(region, key.clone());
    let ids: Vec<i32> = m.world.forces.values().flat_map(|f| f.units.iter()).filter(|u| u.unit_key == key).map(|u| u.id.raw()).collect();
    let _ = (a, b);
    let (before, _) = unit_names_and_flags(&esf);
    let gone = m.world.forces.values().filter(|f| f.faction == france && f.units.len() > 2).flat_map(|f| f.units.iter()).find(|u| u.character.is_none() && before.get(&u.id.raw()).is_some_and(|(_, n)| n.contains("unit_regiment_names"))).map(|u| u.id).unwrap();
    let gone_name = before[&gone.raw()].1.clone();
    for f in m.world.forces.values_mut() {
        f.units.retain(|u| u.id != gone);
    }
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let (names, flags) = unit_names_and_flags(&out);
    // Every flagged name is carried by exactly one unit of the faction, every carried listed name
    // is flagged.
    let mut carried: std::collections::BTreeMap<(String, String), usize> = std::collections::BTreeMap::new();
    for (f, n) in names.values() {
        *carried.entry((f.clone(), n.clone())).or_default() += 1;
    }
    for (f, n, b) in &flags {
        let c = carried.get(&(f.clone(), n.clone())).copied().unwrap_or(0);
        assert_eq!((*b, c.min(2)), (c > 0, c.min(1)), "{f} {n}");
    }
    assert!(!flags.iter().any(|(f, n, b)| f == "france" && *n == gone_name && *b), "the removed unit's name is freed");
    // The new units' names: distinct, line infantry names, the lowest free ones of the source list.
    let new: Vec<&String> = ids.iter().filter(|id| !before.contains_key(id)).map(|id| &names[id].1).collect();
    assert_eq!(new.len(), 2);
    assert_ne!(new[0], new[1]);
    let (_, src_flags) = unit_names_and_flags(&esf);
    let line_list: Vec<&(String, String, bool)> = src_flags.iter().filter(|(f, n, _)| f == "france" && n.contains("euro_infantry_units_") && {
        let num: u32 = n.rsplit('_').next().unwrap().parse().unwrap_or(999);
        (1..=160).contains(&num)
    }).collect();
    let free: Vec<&String> = line_list.iter().filter(|(_, _, b)| !b).map(|(_, n, _)| n).take(2).collect();
    let mut got = new.clone();
    got.sort();
    let mut want = free.clone();
    want.sort();
    assert_eq!(got, want);
    let r = ntw_campaign::save_check::check(&out);
    assert!(r.violations.is_empty(), "{:#?}", r.violations);
}

/// New colonels and unit officers are named from their faction's pools by its allocators (the
/// deck draws), the colonel and his unit share the name, names are distinct from each other and
/// from historical characters, and the advanced allocators are written back and read again
/// (SAVE_COMPAT.md §21).
#[test]
fn new_characters_and_officers_get_names() {
    use ntw_campaign::names::{self, Allocator, NameData};
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let nd = NameData::load(&vfs, &db).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    // A new colonel-led army (a region without a garrison) and two units joining a garrison.
    let empty = m.world.regions.values().find(|r| r.owner == france && r.garrison.is_none()).unwrap().id;
    let garrisoned = m.world.regions.values().find(|r| r.owner == france && r.garrison.is_some_and(|g| m.world.forces[&g].units.len() < 18)).unwrap().id;
    let key = m.rules.units.iter().find(|(k, u)| u.unit_class == "infantry_line" && k.contains("French")).map(|(k, _)| k.clone()).unwrap();
    let before_chars: std::collections::BTreeSet<_> = m.world.characters.keys().copied().collect();
    let before_units: std::collections::BTreeSet<_> = m.world.forces.values().flat_map(|f| f.units.iter().map(|u| u.id)).collect();
    m.spawn_recruited_unit(empty, key.clone());
    m.spawn_recruited_unit(garrisoned, key.clone());
    m.spawn_recruited_unit(garrisoned, key);
    let new_char = *m.world.characters.keys().find(|c| !before_chars.contains(c)).unwrap();
    let out = save::write_save_named(&esf, &m, "france", 1, None, Some(&nd)).unwrap();
    let w = out.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let f = w.record_array("FACTION_ARRAY").unwrap().records().find(|f| f.values().filter_map(EsfNode::as_str).next() == Some("france")).unwrap();
    let group = nd.groups["france"].clone();
    let fore: std::collections::BTreeSet<String> = names::pool_rows(&nd.rows, &group, names::POOL_MALE_FORENAME).unwrap().iter().map(|r| r.loc_key()).collect();
    let sur: std::collections::BTreeSet<String> = names::pool_rows(&nd.rows, &group, names::POOL_SURNAME).unwrap().iter().map(|r| r.loc_key()).collect();
    let loc = |n: Option<&EsfNode>| n.and_then(EsfNode::as_record).and_then(|r| r.get_str(0)).unwrap_or("").to_string();
    // The colonel.
    let ch = f.record_array("CHARACTER_ARRAY").unwrap().records().find(|c| c.get_i32(2) == Some(new_char.raw())).unwrap();
    let d = ch.children[1].as_record().unwrap();
    let colonel = (loc(d.children.get(1)), loc(d.children.get(2)));
    assert!(fore.contains(&colonel.0) && sur.contains(&colonel.1), "{colonel:?}");
    // The new units' officers.
    let mut officers = Vec::new();
    f.walk(&mut |r: &EsfRecord| {
        if r.name == "UNIT"
            && let Some(id) = r.get_i32(4)
            && !before_units.contains(&ntw_sim::campaign::UnitId(id))
        {
            let cd = r.child("COMMANDER_DETAILS").unwrap();
            officers.push((r.get_u32(10).unwrap_or(0) as i32, (loc(cd.children.first()), loc(cd.children.get(1)))));
        }
    });
    assert_eq!(officers.len(), 3);
    for (attached, name) in &officers {
        assert!(fore.contains(&name.0) && sur.contains(&name.1), "{name:?}");
        if *attached == new_char.raw() {
            assert_eq!(name, &colonel, "the colonel's unit carries his name");
        }
    }
    let distinct: std::collections::BTreeSet<_> = officers.iter().map(|o| o.1.clone()).collect();
    assert_eq!(distinct.len(), 3);
    // The allocators: exactly three draws each (one per name), written back and read again.
    let before: Vec<Allocator> = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap().record_array("FACTION_ARRAY").unwrap().records()
        .find(|f| f.values().filter_map(EsfNode::as_str).next() == Some("france")).unwrap()
        .children_named("NAME_ALLOCATION_DETAILS").filter_map(Allocator::read).collect();
    let after: Vec<Allocator> = f.children_named("NAME_ALLOCATION_DETAILS").filter_map(Allocator::read).collect();
    for p in [names::POOL_MALE_FORENAME, names::POOL_SURNAME] {
        let pool = names::pool_rows(&nd.rows, &group, p).unwrap().len() as u32;
        let mut sim = before[p].clone();
        if sim.size != pool {
            sim.size = pool;
            sim.deck.clear();
        }
        for _ in 0..3 {
            sim.draw();
        }
        assert_eq!(after[p], sim, "pool {p}");
    }
    let back = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    assert!(back.to_bytes().unwrap() == out.to_bytes().unwrap());
    let r = ntw_campaign::save_check::check(&out);
    assert!(r.violations.is_empty(), "{:#?}", r.violations);
}

/// Traits, trait points and ancillaries of existing characters are written from the model; a
/// commander who dies of natural causes leaves no reference (his force goes to a new colonel named
/// after the unit officer he takes over); the recruitment pools are written from the model
/// (SAVE_COMPAT.md §23).
#[test]
fn character_changes_and_deaths_are_written() {
    use ntw_sim::campaign::details::CharacterTrait;
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let nd = ntw_campaign::names::NameData::load(&vfs, &db).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    let (fid, dead) = m.world.forces.values().filter(|f| f.faction == france && !f.is_navy && f.units.len() > 1).map(|f| (f.id, f.commander.unwrap())).next().unwrap();
    let pool_dead = m.world.faction_details[&france].general_pool.0.first().copied();
    // Two French characters change: one gains a trait, raises another and gains an ancillary;
    // the other loses everything.
    let mut ids = m.world.character_details.iter().filter(|(id, d)| **id != dead && Some(**id) != pool_dead && m.world.characters.get(id).is_some_and(|c| c.faction == france) && !d.traits.is_empty()).map(|(id, _)| *id);
    let (a, b) = (ids.next().unwrap(), ids.next().unwrap());
    let anc_key = l.model.world.character_details.values().flat_map(|d| d.ancillaries.iter()).next().cloned().unwrap_or_else(|| "anc_test".into());
    {
        let d = m.world.character_details.get_mut(&a).unwrap();
        d.traits[0].points += 3;
        d.traits.push(CharacterTrait { key: "C_Gen_Test_Trait".into(), points: 2 });
        d.ancillaries.retain(|x| *x != anc_key);
        d.ancillaries.push(anc_key.clone());
        let d = m.world.character_details.get_mut(&b).unwrap();
        d.traits.clear();
        d.ancillaries.clear();
    }
    // A commander with units dies; a general-pool candidate dies too.
    m.character_dies(dead);
    if let Some(p) = pool_dead {
        m.character_dies(p);
    }
    let colonel = m.world.forces[&fid].commander.unwrap();
    // The colonel joins the unit the original picks (`CampaignModel::commander_unit`), which has
    // no character of its own here; the save names him after its officer.
    let pick = m.world.forces[&fid].units.iter().position(|u| u.character == Some(colonel)).expect("colonel on a unit");
    assert_eq!(m.commander_unit(fid), Some(pick));
    let first_unit = m.world.forces[&fid].units[pick].id;
    assert!(!l.model.world.characters.contains_key(&colonel));
    let out = save::write_save_named(&esf, &m, "france", 1, None, Some(&nd)).unwrap();
    let bytes = out.to_bytes().unwrap();
    let back_esf = EsfFile::from_bytes(&bytes).unwrap();
    let r = ntw_campaign::save_check::check(&back_esf);
    assert!(r.violations.is_empty(), "{:?}", r.violations);
    let back = ntw_campaign::read_esf(&back_esf, &db).unwrap();
    let bw = &back.model.world;
    for id in [a, b] {
        assert_eq!(bw.character_details[&id].traits, m.world.character_details[&id].traits);
        assert_eq!(bw.character_details[&id].ancillaries, m.world.character_details[&id].ancillaries);
    }
    // Every character's traits and ancillaries read back as the model has them.
    for (id, d) in m.world.character_details.iter().filter(|(id, _)| m.world.characters.contains_key(id)) {
        assert_eq!((&bw.character_details[id].traits, &bw.character_details[id].ancillaries), (&d.traits, &d.ancillaries), "{id:?}");
    }
    // The dead are gone everywhere; the colonel commands the force from the picked unit.
    assert!(!bw.characters.contains_key(&dead));
    let mut refs = Vec::new();
    int_sites(&back_esf.root.children, dead.raw() as i64, "", &mut refs);
    assert!(refs.is_empty(), "{refs:?}");
    assert_eq!(bw.forces[&fid].commander, Some(colonel));
    assert_eq!(bw.forces[&fid].units[pick].character, Some(colonel));
    assert_eq!(bw.forces[&fid].units[pick].id, first_unit);
    // He carries the name of the unit's officer.
    let w = back_esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let loc = |n: Option<&EsfNode>| n.and_then(EsfNode::as_record).and_then(|r| r.get_str(0)).unwrap_or("").to_string();
    let mut officer = None;
    let mut name = None;
    w.walk(&mut |r: &EsfRecord| {
        if r.name == "UNIT" && r.get_i32(4) == Some(first_unit.0) {
            let cd = r.child("COMMANDER_DETAILS").unwrap();
            officer = Some((loc(cd.children.first()), loc(cd.children.get(1))));
        }
        if r.name == "CHARACTER" && r.get_i32(2) == Some(colonel.raw()) {
            let d = r.children[1].as_record().unwrap();
            name = Some((loc(d.children.get(1)), loc(d.children.get(2))));
        }
    });
    assert!(officer.is_some() && officer == name, "{officer:?} {name:?}");
    // The pools as the model has them.
    let pools = |w: &ntw_sim::campaign::World| w.faction_details[&france].general_pool.clone();
    assert_eq!(pools(bw), pools(&m.world));
    if let Some(p) = pool_dead {
        assert!(!pools(bw).0.contains(&p));
    }
}

/// A force whose commander dies goes to a character already with it when the original's pick
/// (`CampaignModel::commander_unit`) lands on a unit with a character of its own (SAVE_COMPAT.md
/// §23): the written save has him commanding the force (`CHARACTER` #4, `MILITARY_FORCE` #1),
/// still attached to his unit (#5), the dead man nowhere, and every rule holds.
#[test]
fn an_existing_character_takes_over_a_dead_commanders_force() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let mut m = l.model.clone();
    // A land force with a second character riding on one of its units: make that character the
    // original's pick (a General on the first unit in pick order) by giving the pick's unit to a
    // General of the same faction who commands nothing, when the startpos has no such force.
    let found = m.world.forces.values().filter(|f| !f.is_navy && f.units.len() > 1).find_map(|f| {
        let c = f.commander?;
        let pick = m.commander_unit(f.id)?;
        let next = f.units[pick].character.filter(|n| *n != c)?;
        Some((f.id, c, next))
    });
    let (fid, dead, next) = match found {
        Some(x) => x,
        None => {
            use ntw_sim::campaign::CharacterKind;
            let busy: std::collections::BTreeSet<_> = m.world.forces.values().flat_map(|f| f.commander.into_iter().chain(f.units.iter().filter_map(|u| u.character))).collect();
            let (fid, dead, faction) = m.world.forces.values().filter(|f| !f.is_navy && f.units.len() > 1).find_map(|f| Some((f.id, f.commander?, f.faction))).unwrap();
            let spare = m.world.characters.values().find(|c| c.faction == faction && c.kind == CharacterKind::General && !busy.contains(&c.id)).map(|c| c.id).expect("a spare General");
            let pick = m.commander_unit(fid).unwrap();
            m.world.forces.get_mut(&fid).unwrap().units[pick].character = Some(spare);
            assert_eq!(m.commander_unit(fid), Some(pick), "a General's unit leads the pick");
            (fid, dead, spare)
        }
    };
    let chars_before = m.world.characters.len();
    m.character_dies(dead);
    assert_eq!(m.world.forces[&fid].commander, Some(next), "the existing character takes over");
    assert_eq!(m.world.characters.len(), chars_before - 1, "no new colonel");
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let bytes = out.to_bytes().unwrap();
    let back_esf = EsfFile::from_bytes(&bytes).unwrap();
    let r = ntw_campaign::save_check::check(&back_esf);
    assert!(r.violations.is_empty(), "{:?}", r.violations);
    let mut refs = Vec::new();
    int_sites(&back_esf.root.children, dead.raw() as i64, "", &mut refs);
    assert!(refs.is_empty(), "{refs:?}");
    // The links as written.
    let w = back_esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let unit_of_next = m.world.forces[&fid].units.iter().find(|u| u.character == Some(next)).map(|u| u.id.0 as u32).unwrap();
    let mut seen = (false, false);
    w.walk(&mut |rec: &EsfRecord| {
        if rec.name == "CHARACTER" && rec.get_i32(2) == Some(next.raw()) {
            assert_eq!((rec.get_u32(4), rec.get_u32(5)), (Some(fid.0), Some(unit_of_next)));
            seen.0 = true;
        }
        if rec.name == "MILITARY_FORCE" && rec.get_u32(0) == Some(fid.0) {
            assert_eq!(rec.get_u32(1), Some(next.raw() as u32));
            seen.1 = true;
        }
    });
    assert_eq!(seen, (true, true));
    let back = ntw_campaign::read_esf(&back_esf, &db).unwrap();
    assert_eq!(back.model.world.forces[&fid].commander, Some(next));
}

/// Technology research written from the model (`FACTION_TECHNOLOGY_MANAGER` `techs[]` #1 state,
/// #2 progress, #3 researcher; CAMPAIGN_FIDELITY.md §Research): changed entries read back as the
/// model has them, and an unchanged model leaves every faction's technology record as stored.
#[test]
fn technology_research_is_written() {
    use ntw_sim::campaign::details::TechResearch;
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let techs = |e: &EsfFile| -> Vec<EsfRecord> {
        let mut v = Vec::new();
        e.root.walk(&mut |r: &EsfRecord| {
            if r.name == "FACTION_TECHNOLOGY_MANAGER" {
                v.push(r.clone());
            }
        });
        v
    };
    // Unchanged: every technology record as stored.
    let same = save::write_save(&esf, &l.model, "france", 1).unwrap();
    assert_eq!(techs(&same), techs(&esf));
    // France researches one tech to the end and has another under way at a school.
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    let slot = m.world.regions.values().filter(|r| r.owner == france).flat_map(|r| r.slots.iter()).map(|s| s.id).find(|&id| id != 0).unwrap();
    let d = m.world.faction_details.get_mut(&france).unwrap();
    let avail: Vec<String> = d.technologies.iter().filter(|t| t.1 == 2).map(|t| t.0.clone()).take(2).collect();
    assert_eq!(avail.len(), 2, "two available techs");
    for t in d.technologies.iter_mut().filter(|t| t.0 == avail[0]) {
        t.1 = 0;
    }
    d.research.insert(avail[0].clone(), TechResearch { progress: 1500.0, researcher: 0 });
    d.research.insert(avail[1].clone(), TechResearch { progress: 37.5, researcher: slot });
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let back_esf = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    let r = ntw_campaign::save_check::check(&back_esf);
    assert!(r.violations.is_empty(), "{:?}", r.violations);
    let back = ntw_campaign::read_esf(&back_esf, &db).unwrap();
    let (bd, md) = (&back.model.world.faction_details[&france], &m.world.faction_details[&france]);
    assert_eq!(bd.technologies, md.technologies);
    assert_eq!(bd.research, md.research);
    // Only France's technology record changed.
    let changed = techs(&back_esf).iter().zip(techs(&esf).iter()).filter(|(a, b)| a != b).count();
    assert_eq!(changed, 1);
}

/// The save header's territory picture follows the region owners (SAVE_COMPAT.md §25): written
/// from the startpos model it equals the stored picture; after France takes a region, that
/// region's pixels are tinted and France's lost region's are plain; the rule's pixels are the
/// original's on its own saves (`header_map_check`).
#[test]
fn header_territory_map_follows_the_owners() {
    use ntw_campaign::header_map::{header_maps, update_maps, TheatrePictures};
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(&dir) };
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let pics: Vec<TheatrePictures> = l.info.header.maps.iter().filter_map(|m| TheatrePictures::load(&files, &db, &l.info.map_key, &m.theatre)).collect();
    assert_eq!(pics.len(), 1);
    let mut same = save::write_save(&esf, &l.model, "france", 1).unwrap();
    assert_eq!(update_maps(&mut same, &l.model, "france", &pics), 0, "unchanged owners: picture as stored");
    assert_eq!(header_maps(&same.root), header_maps(&esf.root));
    // France takes Bavaria and loses Corsica.
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    let other = m.world.regions.values().find(|r| r.owner != france).map(|r| r.owner).unwrap();
    for r in m.world.regions.values_mut() {
        if r.key == "eur_bavaria" {
            r.owner = france;
        } else if r.key == "eur_corsica" {
            r.owner = other;
        }
    }
    let mut out = save::write_save(&esf, &m, "france", 1).unwrap();
    assert_eq!(update_maps(&mut out, &m, "france", &pics), 1);
    let back = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    let (old, new) = (&header_maps(&esf.root)[0].1, &header_maps(&back.root)[0].1);
    let p = &pics[0];
    let colour = |k: &str| p.region_colours[k];
    let region_at = |i: usize| {
        let ix = p.lookup.indices[i] as usize;
        let c = p.lookup.palette[ix];
        [c[0], c[1], c[2]]
    };
    let green = |px: u32| ((px >> 8) & 0xFF) > 150 && (px >> 16 & 0xFF) < 100;
    let (mut bav, mut cor) = (0, 0);
    for i in 0..new.len() {
        if region_at(i) == colour("eur_bavaria") {
            assert!(green(new[i]) && !green(old[i]));
            bav += 1;
        } else if region_at(i) == colour("eur_corsica") {
            assert!(!green(new[i]) && green(old[i]));
            cor += 1;
        } else {
            assert_eq!(new[i], old[i]);
        }
    }
    assert!(bav > 100 && cor > 10, "{bav} {cor}");
    assert!(ntw_campaign::save_check::check(&back).violations.is_empty());
}

/// Governorship tax levels written from the model (SAVE_COMPAT.md §26): both classes set with
/// `SetTaxLevel` read back in the governorship (level index and rate) and as the faction's levels;
/// an unchanged model leaves every `GOVERNORSHIP_TAXES` as stored.
#[test]
fn governorship_tax_levels_are_written() {
    use ntw_sim::campaign::{CampaignCommand, TaxClass};
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let taxes = |e: &EsfFile| {
        let mut v = Vec::new();
        e.root.walk(&mut |r: &EsfRecord| {
            if r.name == "GOVERNORSHIP_TAXES" {
                v.push(r.clone());
            }
        });
        v
    };
    let same = save::write_save(&esf, &l.model, "france", 1).unwrap();
    assert_eq!(taxes(&same), taxes(&esf));
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    m.apply(CampaignCommand::SetTaxLevel { faction: france, class: TaxClass::Lower, level: "tax_extortionate".into() }).unwrap();
    m.apply(CampaignCommand::SetTaxLevel { faction: france, class: TaxClass::Upper, level: "tax_low".into() }).unwrap();
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let back_esf = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    assert!(ntw_campaign::save_check::check(&back_esf).violations.is_empty());
    let back = ntw_campaign::read_esf(&back_esf, &db).unwrap();
    let g = back.model.world.faction_details[&france].governorship().unwrap().taxes;
    assert_eq!((g.lower, g.lower_rate, g.upper, g.upper_rate), (4, 25, 1, 10));
    let f = &back.model.world.factions[&france];
    assert_eq!((f.tax_lower.as_str(), f.tax_upper.as_str()), ("tax_extortionate", "tax_low"));
    assert_eq!(taxes(&back_esf).iter().zip(taxes(&esf).iter()).filter(|(a, b)| a != b).count(), 1, "only France's governorship");
}

/// Duel counters written from the model (`CHARACTER` #36 lost, #37 won; SAVE_COMPAT.md §27):
/// changed values read back, an unchanged model leaves every character's counters as stored.
#[test]
fn duel_counters_are_written() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let counters = |e: &EsfFile| {
        let mut v = Vec::new();
        e.root.walk(&mut |r: &EsfRecord| {
            if r.name == "CHARACTER" {
                v.push((r.get_i32(2), r.get_u32(36), r.get_u32(37)));
            }
        });
        v
    };
    let same = save::write_save(&esf, &l.model, "france", 1).unwrap();
    assert_eq!(counters(&same), counters(&esf));
    let mut m = l.model.clone();
    let id = *m.world.character_details.keys().find(|c| m.world.characters.contains_key(c)).unwrap();
    let d = m.world.character_details.get_mut(&id).unwrap();
    d.duels_lost = 2;
    d.duels_won = 5;
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let back_esf = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    assert!(ntw_campaign::save_check::check(&back_esf).violations.is_empty());
    let back = ntw_campaign::read_esf(&back_esf, &db).unwrap();
    let bd = &back.model.world.character_details[&id];
    assert_eq!((bd.duels_lost, bd.duels_won), (2, 5));
    assert_eq!(counters(&back_esf).iter().zip(counters(&esf).iter()).filter(|(a, b)| a != b).count(), 1);
}

/// The sight state (SAVE_COMPAT.md, shroud and sight): France's explored and visible cells
/// (`CAMPAIGN_SHROUD` #0 / #1), a character's sight radius (#17) and hidden flag (#22) and the
/// faction's `EXPOSED_CHARACTERS` are written from the model and read back; an unchanged model
/// leaves them as stored.
#[test]
fn sight_state_is_written() {
    let dir = data_dir();
    if !dir.is_dir() {
        return;
    }
    let db = GameDatabase::from_install(&dir).unwrap();
    let esf = EsfFile::from_bytes(&std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let same = save::write_save(&esf, &l.model, "france", 1).unwrap();
    let world = |e: &EsfFile| e.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap().clone();
    // France's shroud (the AI factions' are dropped from a new campaign, `trim_ai_faction_blocks`).
    let shrouds = |e: &EsfFile| {
        let w = world(e);
        w.record_array("FACTION_ARRAY").unwrap().records().find(|f| f.get_str(9) == Some("france")).and_then(|f| f.child("CAMPAIGN_SHROUD").cloned())
    };
    assert!(shrouds(&same).is_some() && shrouds(&same) == shrouds(&esf), "an unchanged shroud stays as stored");
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    assert!(m.world.shrouds.contains_key(&france), "France has a shroud in the start position");
    let grid = m.world.sight_grid.unwrap();
    let sh = m.world.shrouds.get_mut(&france).unwrap();
    let (x, z) = (grid.cols / 2, grid.rows / 3);
    let was = (sh.explored.get(x, z), sh.visible.get(x, z));
    sh.explored.set(x, z);
    sh.visible.clear();
    sh.visible.set(x, z);
    let general = *m.world.characters.iter().find(|(_, c)| c.faction == france && m.world.character_details.contains_key(&c.id)).unwrap().0;
    m.world.sight_radius.insert(general, 22.5);
    m.world.character_details.get_mut(&general).unwrap().hidden = true;
    m.world.faction_details.get_mut(&france).unwrap().exposed = vec![general];
    let out = save::write_save(&esf, &m, "france", 1).unwrap();
    let back_esf = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    assert!(ntw_campaign::save_check::check(&back_esf).violations.is_empty());
    let back = ntw_campaign::read_esf(&back_esf, &db).unwrap();
    let bs = &back.model.world.shrouds[&france];
    assert!(bs.explored.get(x, z) && bs.visible.get(x, z) && bs.visible.len() == 1, "was {was:?}");
    assert_eq!(bs.explored, m.world.shrouds[&france].explored);
    assert_eq!(back.model.world.sight_radius[&general], 22.5);
    assert!(back.model.world.character_details[&general].hidden);
    assert_eq!(back.model.world.faction_details[&france].exposed, vec![general]);
}
