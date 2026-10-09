//! Campaign gameplay against the real eur_napoleon start position (read-only): the turn loop,
//! movement on the real map grid, province management with DB values, autoresolve and the save
//! round trip. Each test **skips** when the install is not there.
//!
//! See the numbers with `cargo test -p ntw_campaign --release --test campaign_play -- --nocapture`.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use ntw_campaign::{pathing, save, LoadedCampaign};
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::{
    economy, CampaignCommand, CampaignEvent, CampaignModel, CommandError, SlotRef, Terrain, TaxClass,
};
use ntw_sim::campaign::polypath::Mover;
use ntw_sim::fixed::Fixed20;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

struct Fixture {
    db: GameDatabase,
    source: EsfFile,
    loaded: LoadedCampaign,
    terrain: Terrain,
}

/// The eur_napoleon startpos, its map grid and the DB, loaded once (None without an install).
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
        let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
        let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
        let source = EsfFile::from_bytes(&bytes).expect("esf");
        let mut loaded = ntw_campaign::read_esf(&source, &db).expect("load");
        let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
        ntw_campaign::trade::attach_map(&mut loaded.model, &map.regions);
        let terrain = Terrain(Arc::new(pathing::build_grid(&map)));
        Some(Fixture { db, source, loaded, terrain })
    })
    .as_ref()
}

/// A fresh model: France human, with the map grid.
fn model(f: &Fixture) -> CampaignModel {
    let mut l = f.loaded.clone();
    assert!(l.set_human("france"));
    l.model.terrain = Some(f.terrain.clone());
    l.model
}

fn faction(m: &CampaignModel, key: &str) -> ntw_sim::campaign::FactionId {
    m.faction_by_key(key).unwrap().id
}

#[test]
fn turn_loop_from_the_startpos() {
    let Some(f) = fixture() else { return };
    let mut m = model(f);
    let france = faction(&m, "france");
    // France is first in FACTION_ARRAY, so turn 1 starts with France straight after round start.
    assert_eq!(m.world.turn_order[0], france);
    let ev = m.start_campaign();
    assert_eq!(m.turn.current, Some(france));
    assert!(m.turn.in_turn);
    let rounds = ev.iter().filter(|e| matches!(e, CampaignEvent::FactionRoundStart { .. })).count();
    assert_eq!(rounds, m.world.factions.len());
    // Then France's characters and regions, and its FactionTurnStart last (0x008F2620, CONFIRMED order).
    assert_eq!(ev.last(), Some(&CampaignEvent::FactionTurnStart { faction: france }));
    // DB values reached the rules.
    assert_eq!(m.rules.var("road_level_0_action_point_cost", 0.0), 0.67);
    assert_eq!(m.rules.agent_action_points.get("General"), Some(&26));
    assert_eq!(m.rules.tax_rate("tax_normal"), 15);

    let income = economy::faction_income(&m, france);
    println!("france income {income:?}, treasury {}", m.world.factions[&france].treasury);
    assert!(income.taxes > 0 && income.upkeep > 0);

    let mut expected = m.world.factions[&france].treasury;
    let mut seen_end = 0;
    for _ in 0..5 {
        // Each round end settles France's economy once (town wealth grows every round, so the
        // income is taken anew each time; France can pay its upkeep in the startpos).
        expected += economy::faction_income(&m, france).net();
        let ev = m.end_turn();
        seen_end += ev.iter().filter(|e| matches!(e, CampaignEvent::FactionTurnEnd { .. })).count();
        assert_eq!(m.turn.current, Some(france));
    }
    assert_eq!(m.calendar.turns_elapsed, 5);
    assert_eq!(m.calendar.date.year, 1805);
    assert_eq!(m.calendar.date.month, 2); // Early Jan + 5 half-months = Late Mar
    assert_eq!(seen_end, 5 * m.world.factions.len());
    // France was paid five times, once per round end.
    assert_eq!(m.world.factions[&france].treasury, expected);

    // Deterministic: a second run from the same startpos gives the same state.
    let mut m2 = model(f);
    m2.start_campaign();
    for _ in 0..5 {
        m2.end_turn();
    }
    assert_eq!(m.state_hash(), m2.state_hash());
}

#[test]
fn armies_move_on_the_map_grid() {
    let Some(f) = fixture() else { return };
    let mut m = model(f);
    m.start_campaign();
    let france = faction(&m, "france");
    // The French army nearest to Paris.
    let paris = m.world.regions.values().find(|r| r.key == "eur_france").unwrap().settlement.position;
    let force = m
        .world
        .forces
        .values()
        .filter(|x| x.faction == france && !x.is_navy && x.commander.is_some())
        .min_by_key(|x| {
            let p = m.force_position(x.id).unwrap();
            ((p.0.raw() as i64 - paris.0.raw() as i64).pow(2) + (p.1.raw() as i64 - paris.1.raw() as i64).pow(2)) as u64
        })
        .unwrap()
        .id;
    let general = m.world.forces[&force].commander.unwrap();
    let vienna = m.world.regions.values().find(|r| r.key == "eur_austria").unwrap().settlement.position;
    let plan = m.plan_path(general, vienna).expect("a land path to Vienna");
    let ap = m.world.characters[&general].movement_points;
    println!(
        "path to Vienna: {} points, cost {:.1} AP, reachable {} with {ap} AP",
        plan.path.points.len(),
        plan.path.total_cost(),
        plan.reachable
    );
    assert!(plan.path.total_cost() > ap as f32, "Vienna is more than one turn away");
    assert!(plan.reachable > 0);
    // The path comes from the original's polygon search, and every polygon it passes is one an
    // army may enter (land, road, kind 7).
    let grid = &f.terrain.0;
    let pm = grid.poly.as_ref().expect("eur_napoleon has pathfinding.esf");
    assert_eq!(plan.path.polys.len(), plan.path.points.len());
    for &q in &plan.path.polys {
        assert!(Mover::Land.may_enter(pm.kind[q as usize]), "polygon {q} kind {}", pm.kind[q as usize]);
    }
    // And every point lies inside (or on) its polygon.
    for (p, &q) in plan.path.points.iter().zip(&plan.path.polys).skip(1) {
        let poly = pm.polygon_at(p.0, p.1).expect("on the map");
        assert!(Mover::Land.may_enter(pm.kind[poly]), "point {p:?} of polygon {q} lies in kind {}", pm.kind[poly]);
    }
    // Moving spends the action points and stops part way.
    let ev = m.apply(CampaignCommand::MoveForce { force, to: vienna }).unwrap();
    assert!(matches!(&ev[0], CampaignEvent::CharacterMoved { .. }));
    assert!(m.world.characters[&general].movement_points < ap.max(1));
    // A navy cannot sail to Vienna.
    let navy = m.world.forces.values().find(|x| x.faction == france && x.is_navy).unwrap();
    let admiral = navy.commander.unwrap();
    let p = m.plan_path(admiral, vienna);
    assert!(p.is_none_or(|p| p.path.polys.iter().all(|&q| Mover::Sea.may_enter(pm.kind[q as usize]))));
}

#[test]
fn province_management_with_db_values() {
    let Some(f) = fixture() else { return };
    let mut m = model(f);
    m.start_campaign();
    let france = faction(&m, "france");
    let paris = m.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    let units = m.recruitable_units(paris);
    println!("Paris can recruit {} units: {:?}", units.len(), &units[..units.len().min(6)]);
    assert!(!units.is_empty());
    assert!(m.recruitment_points(paris, false) > 0);
    let unit = units.iter().find(|u| !m.rules.units[*u].is_naval).unwrap().clone();
    // The region's entry cost: `units` #7 with the region's cost effects (`0x00B0D220`).
    let cost = ntw_sim::campaign::economy::recruitment_cost(&m, &m.world.regions[&paris], &unit, &m.rules.units[&unit]);
    assert!(cost > 0 && cost != m.rules.units[&unit].cost);
    let t0 = m.world.factions[&france].treasury;
    m.apply(CampaignCommand::Recruit { region: paris, unit_key: unit.clone() }).unwrap();
    assert_eq!(m.world.factions[&france].treasury, t0 - cost);
    // Austrian-only units are refused.
    let austrian = m.rules.unit_factions.iter().find(|(_, f)| f.iter().all(|f| f == "austria") && !f.is_empty()).map(|(u, _)| u.clone());
    if let Some(a) = austrian {
        assert!(matches!(m.apply(CampaignCommand::Recruit { region: paris, unit_key: a }), Err(CommandError::UnitNotAvailable(_))));
    }
    // Upgrade the first building that has an upgrade, at the DB cost.
    let (slot, level) = m.world.regions[&paris]
        .slots
        .iter()
        .enumerate()
        .find_map(|(i, s)| {
            let b = s.building.as_ref()?;
            let up = m.rules.buildings.get(&b.level_key)?.upgrades_to.first()?.clone();
            m.can_build(paris, SlotRef::Slot(i), &up).ok().map(|_| (i, up))
        })
        .expect("an upgradable building in Paris");
    let bcost = m.rules.buildings[&level].cost;
    let turns = m.rules.buildings[&level].turns;
    m.world.factions.get_mut(&france).unwrap().treasury += bcost; // make sure it is affordable
    m.apply(CampaignCommand::ConstructBuilding { region: paris, slot: SlotRef::Slot(slot), level_key: level.clone() }).unwrap();
    // Taxes.
    m.apply(CampaignCommand::SetTaxLevel { faction: france, class: TaxClass::Lower, level: "tax_high".into() }).unwrap();
    let po = economy::public_order(&m, paris);
    println!("Paris public order with high lower-class taxes: {po:?}");
    let mut trained = false;
    for _ in 0..turns {
        let ev = m.end_turn();
        trained |= ev.iter().any(|e| matches!(e, CampaignEvent::UnitTrained { .. }));
    }
    assert!(trained);
    assert_eq!(m.world.regions[&paris].slots[slot].building.as_ref().unwrap().level_key, level);
}

#[test]
fn save_round_trip() {
    let Some(f) = fixture() else { return };
    let mut m = model(f);
    m.start_campaign();
    let france = faction(&m, "france");
    let paris = m.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    let unit = m.recruitable_units(paris).into_iter().find(|u| !m.rules.units[u].is_naval).unwrap();
    m.apply(CampaignCommand::Recruit { region: paris, unit_key: unit.clone() }).unwrap();
    m.end_turn();
    m.end_turn();
    m.end_turn(); // the unit is trained into a new garrison force
    m.apply(CampaignCommand::Recruit { region: paris, unit_key: unit }).unwrap();
    let force = m.world.forces.values().find(|x| x.faction == france && !x.is_navy && x.commander.is_some()).unwrap().id;
    let to = {
        let p = m.force_position(force).unwrap();
        (Fixed20::from_raw(p.0.raw() + (3 << 20)), p.1)
    };
    let _ = m.apply(CampaignCommand::MoveForce { force, to });
    // Walls under construction are saved in the region's FORTIFICATION_SLOT and read back.
    let walls = m.construction_options(paris, SlotRef::Walls).into_iter().find(|o| o.tech).expect("Paris can build walls").level_key;
    m.world.factions.get_mut(&france).unwrap().treasury += 100_000;
    m.apply(CampaignCommand::ConstructBuilding { region: paris, slot: SlotRef::Walls, level_key: walls }).unwrap();

    let tree = save::write_save(&f.source, &m, "france", 1_000_000).expect("write");
    let bytes = tree.to_bytes().expect("serialise");
    // Our own folder under the system temp dir; never the game's save folder.
    let dir = std::env::temp_dir().join("napoleonrust_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("round_trip.save");
    std::fs::write(&path, &bytes).unwrap();

    let back = ntw_campaign::read_file(&path, &f.db).expect("reload");
    assert_eq!(back.info.kind, ntw_campaign::FileKind::Save);
    assert_eq!(back.info.header.turn_number, m.calendar.turn_number());
    let mut r = back.model;
    r.terrain = m.terrain.clone();
    assert_eq!(r.calendar, m.calendar);
    assert_eq!(r.rng, m.rng);
    assert_eq!(r.turn.current, Some(france));
    assert!(r.turn.in_turn);
    for (id, fa) in &m.world.factions {
        assert_eq!(r.world.factions[id].treasury, fa.treasury, "{}", fa.key);
    }
    assert_eq!(r.world.characters, m.world.characters);
    assert_eq!(r.world.forces, m.world.forces);
    for (id, reg) in &m.world.regions {
        let b = &r.world.regions[id];
        assert_eq!((b.owner, &b.slots, &b.road, &b.fortification), (reg.owner, &reg.slots, &reg.road, &reg.fortification), "{}", reg.key);
        assert_eq!(b.recruitment_queue, reg.recruitment_queue, "{}", reg.key);
        assert_eq!(b.construction, reg.construction, "{}", reg.key);
    }
    // Writing the reloaded model again gives the same bytes (stable writer).
    let again = save::write_save(&EsfFile::from_bytes(&bytes).unwrap(), &r, "france", 1_000_000).unwrap().to_bytes().unwrap();
    assert_eq!(again, bytes);
    let _ = std::fs::remove_file(&path);
}

/// Zones of control on the real map (PATHFINDING.md §8): a French army heading past an enemy army
/// keeps out of the enemy's zone (the polygons within 12 units of walking from it), and a goal
/// inside the zone stops at its edge.
#[test]
fn paths_bend_round_enemy_zones() {
    use ntw_sim::campaign::Stance;
    let Some(f) = fixture() else { return };
    let mut m = model(f);
    let france = faction(&m, "france");
    // Every enemy seen: France's shroud would hide the far armies' zones (mode 5, CHARACTERS_FIDELITY.md §10).
    m.world.shrouds.remove(&france);
    let pm = f.terrain.0.poly.as_ref().expect("pathfinding.esf");
    let pos = |c: ntw_sim::campaign::CharacterId| {
        let p = m.world.characters[&c].position;
        (p.0.to_f32(), p.1.to_f32())
    };
    let armies: Vec<_> = m.world.forces.values().filter(|x| !x.is_navy && x.commander.is_some()).collect();
    let mut checked = 0;
    for mine in armies.iter().filter(|x| x.faction == france && m.world.characters[&x.commander.unwrap()].garrisoned_in.is_none()) {
        let me = mine.commander.unwrap();
        let a = pos(me);
        for enemy in armies.iter().filter(|x| x.faction != france && m.world.stance(france, x.faction) == Stance::War) {
            let e = pos(enemy.commander.unwrap());
            let d = ((e.0 - a.0).powi(2) + (e.1 - a.1).powi(2)).sqrt();
            if !(25.0..70.0).contains(&d) {
                continue;
            }
            // A goal 18 units beyond the enemy, on the line from us.
            let g = (e.0 + (e.0 - a.0) / d * 18.0, e.1 + (e.1 - a.1) / d * 18.0);
            let to = (Fixed20::from_f64(g.0 as f64), Fixed20::from_f64(g.1 as f64));
            let t0 = std::time::Instant::now();
            let Some(plan) = m.plan_path(me, to) else { continue };
            let ms = t0.elapsed().as_secs_f32() * 1000.0;
            let near = plan.path.points.iter().map(|p| ((p.0 - e.0).powi(2) + (p.1 - e.1).powi(2)).sqrt()).fold(f32::INFINITY, f32::min);
            let free = pm.find_path(a, g, Mover::Land, &vec![0.5; pm.region_sets.len()]);
            let free_near = free.map_or(f32::INFINITY, |p| p.points.iter().map(|q| ((q.0 - e.0).powi(2) + (q.1 - e.1).powi(2)).sqrt()).fold(f32::INFINITY, f32::min));
            println!("army {me:?} -> past enemy at {d:.0} units: closest {near:.1} (without zones {free_near:.1}), {} points, {ms:.1} ms", plan.path.points.len());
            let zone = ntw_sim::campaign::zoc::reach(pm, e, Mover::Land, ntw_sim::campaign::zoc::ARMY_ZONE);
            for q in &plan.path.points[1..] {
                let poly = pm.polygon_at(q.0, q.1).expect("on the map") as u32;
                assert!(zone.binary_search(&poly).is_err(), "the path enters the enemy's zone at {q:?} ({near:.1} from it)");
            }
            // A goal next to the enemy (inside its zone, not the enemy itself): the zone turns kind 10
            // for this search (mode 4, CONFIRMED), so the path walks in to the goal; once inside the
            // zone it stays inside (PATHFINDING_PORTS.md §12).
            let inside = (e.0 - (e.0 - a.0) / d * 4.0, e.1 - (e.1 - a.1) / d * 4.0);
            if let Some(p) = m.plan_path(me, (Fixed20::from_f64(inside.0 as f64), Fixed20::from_f64(inside.1 as f64))) {
                let in_zone: Vec<bool> = p.path.points[1..].iter().map(|q| zone.binary_search(&(pm.polygon_at(q.0, q.1).expect("on the map") as u32)).is_ok()).collect();
                if let Some(first) = in_zone.iter().position(|&z| z) {
                    assert!(in_zone[first..].iter().all(|&z| z), "left the zone again: {:?}", p.path.points);
                }
            }
            checked += 1;
            if checked >= 5 {
                return;
            }
        }
    }
    assert!(checked > 0, "no French army near an enemy army in the startpos");
}

/// Fog of war on the real map (CHARACTERS_FIDELITY.md §10): an enemy army France cannot see casts no
/// zone (mode 5), so the path is the one planned as if that army were not there.
#[test]
fn unseen_enemy_zones_are_ignored() {
    use ntw_sim::campaign::Stance;
    let Some(f) = fixture() else { return };
    let m = model(f);
    let france = faction(&m, "france");
    assert!(m.world.shrouds.contains_key(&france));
    let pos = |m: &ntw_sim::campaign::CampaignModel, c: ntw_sim::campaign::CharacterId| {
        let p = m.world.characters[&c].position;
        (p.0.to_f32(), p.1.to_f32())
    };
    let armies: Vec<_> = m.world.forces.values().filter(|x| !x.is_navy && x.commander.is_some()).cloned().collect();
    let mut checked = 0;
    for mine in armies.iter().filter(|x| x.faction == france && m.world.characters[&x.commander.unwrap()].garrisoned_in.is_none()) {
        let me = mine.commander.unwrap();
        let a = pos(&m, me);
        for enemy in armies.iter().filter(|x| x.faction != france && m.world.stance(france, x.faction) == Stance::War) {
            let e = pos(&m, enemy.commander.unwrap());
            let d = ((e.0 - a.0).powi(2) + (e.1 - a.1).powi(2)).sqrt();
            if !(25.0..70.0).contains(&d) || m.sees(france, e) {
                continue;
            }
            let g = (e.0 + (e.0 - a.0) / d * 18.0, e.1 + (e.1 - a.1) / d * 18.0);
            let to = (Fixed20::from_f64(g.0 as f64), Fixed20::from_f64(g.1 as f64));
            let Some(plan) = m.plan_path(me, to) else { continue };
            let mut without = m.clone();
            without.world.forces.remove(&enemy.id);
            let other = without.plan_path(me, to).expect("same search");
            assert_eq!(plan.path.points, other.path.points, "an unseen army changed the path");
            checked += 1;
            if checked >= 3 {
                return;
            }
        }
    }
    println!("checked {checked}");
}
