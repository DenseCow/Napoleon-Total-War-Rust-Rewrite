//! A campaign run on the real eur_napoleon start position (read-only): a French army boards a
//! French fleet, the fleet sails and lands it (`ntw_sim::campaign::embark`,
//! `analysis/campaign/PATHFINDING_PORTS.md`). Skips without an install.
//! See the story with `cargo test -p ntw_campaign --release --test embark_campaign -- --nocapture`.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use ntw_campaign::{pathing, save, LoadedCampaign};
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::embark::{army_stands_on, embark_points, is_coast, polygon_point, LANDING_RADIUS};
use ntw_sim::campaign::polypath::Mover;
use ntw_sim::campaign::{CampaignCommand, CampaignEvent, CampaignModel, ForceId, Terrain};
use ntw_sim::fixed::Fixed20;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

struct Fixture {
    db: GameDatabase,
    source: EsfFile,
    loaded: LoadedCampaign,
    terrain: Terrain,
}

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
        let files = GameFiles { vfs: &vfs };
        let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
        let source = EsfFile::from_bytes(&bytes).expect("esf");
        let loaded = ntw_campaign::read_esf(&source, &db).expect("load");
        let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
        let terrain = Terrain(Arc::new(pathing::build_grid(&map)));
        Some(Fixture { db, source, loaded, terrain })
    })
    .as_ref()
}

fn model(f: &Fixture) -> CampaignModel {
    let mut l = f.loaded.clone();
    assert!(l.set_human("france"));
    l.model.terrain = Some(f.terrain.clone());
    l.model
}

fn f32p(p: (Fixed20, Fixed20)) -> (f32, f32) {
    (p.0.to_f32(), p.1.to_f32())
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// A French army walks to a French fleet and boards it; the fleet sails and lands it on another
/// shore; landing ends the army's turn; the landing place is the original's (a polygon an army may
/// stand on, within 1.5 units of a coastal polygon's point); next turn the army walks on.
#[test]
fn an_army_boards_a_fleet_and_lands() {
    let Some(f) = fixture() else { return };
    let mut m = model(f);
    m.start_campaign();
    let france = m.faction_by_key("france").unwrap().id;
    let pm = f.terrain.0.poly.as_ref().expect("eur_napoleon has pathfinding.esf");
    // Both French fleets start out at sea, away from any coast: the one nearest to a French port
    // sails into it first (the port node).
    let navies: Vec<ForceId> =
        m.world.forces.values().filter(|x| x.faction == france && x.is_navy && x.commander.is_some()).map(|x| x.id).collect();
    let ports: Vec<(String, (Fixed20, Fixed20))> = m
        .world
        .regions
        .values()
        .filter(|r| r.owner == france)
        .flat_map(|r| r.slots.iter().filter(|s| s.port).filter_map(|s| s.position.map(|p| (s.key.clone(), p))))
        .collect();
    let armies: Vec<ForceId> = m
        .world
        .forces
        .values()
        .filter(|x| x.faction == france && !x.is_navy && x.commander.is_some() && x.units.len() <= 20)
        .map(|x| x.id)
        .collect();
    // A land component id of a point (armies reach each other only inside one).
    let comp = |p: (f32, f32)| pm.locate(p.0, p.1, Mover::Land, 2).map(|q| pm.component[0][q]);
    // The (fleet, port, army) with the shortest fleet + army distances, the army on the port's land.
    type Pick = (f32, ForceId, (String, (Fixed20, Fixed20)), ForceId);
    let mut best: Option<Pick> = None;
    for (key, p) in &ports {
        let Some(&(q, _)) = embark_points(pm, f32p(*p)).first() else { continue };
        for &n in &navies {
            for &a in &armies {
                let ap = f32p(m.force_position(a).unwrap());
                if comp(ap) != Some(pm.component[0][q]) {
                    continue;
                }
                let d = dist(f32p(m.force_position(n).unwrap()), f32p(*p)) + dist(ap, f32p(*p));
                if best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((d, n, (key.clone(), *p), a));
                }
            }
        }
    }
    let (_, navy, (port_key, port), army) = best.expect("a French fleet, port and army");
    let admiral = m.world.forces[&navy].commander.unwrap();
    let plan = m.plan_path(admiral, port).expect("a sea path into the port");
    println!("fleet {} -> {port_key}: {} points, {:.1} AP", navy.raw(), plan.path.points.len(), plan.path.total_cost());
    let mut turns = 0;
    while dist(f32p(m.world.characters[&admiral].position), f32p(port)) > 1e-3 {
        m.apply(CampaignCommand::MoveForce { force: navy, to: port }).expect("sail order");
        if dist(f32p(m.world.characters[&admiral].position), f32p(port)) <= 1e-3 {
            break;
        }
        m.end_turn();
        turns += 1;
        assert!(turns < 6, "the fleet never reached the port");
    }
    println!("fleet in {port_key} after {turns} turn(s)");
    assert!(!embark_points(pm, f32p(port)).is_empty(), "a fleet in port can be boarded");
    // The French army chosen above walks over and boards it.
    let general = m.world.forces[&army].commander.unwrap();
    println!("army {} is {:.1} units from the port", army.raw(), dist(f32p(m.force_position(army).unwrap()), f32p(port)));
    // Board (walking over as many turns as needed).
    let mut turns = 0;
    loop {
        let ev = m.apply(CampaignCommand::Embark { force: army, navy }).expect("embark order");
        if ev.iter().any(|e| matches!(e, CampaignEvent::CharacterEmbarksNavy { .. })) {
            break;
        }
        m.end_turn();
        turns += 1;
        assert!(turns < 8, "the army never reached the fleet");
    }
    assert_eq!(m.carrier_of(army), Some(navy));
    assert_eq!(m.world.characters[&general].position, m.world.characters[&admiral].position);
    // Boarding spends nothing itself; the walk to the fleet did, so the army cannot land before
    // next turn.
    let g = &m.world.characters[&general];
    assert!(g.movement_points < g.max_movement_points);
    println!("boarded after {turns} turn(s) at {:?}", f32p(m.world.characters[&admiral].position));
    // A save keeps the army aboard: it is written standing where the fleet is, and read back as
    // embarked.
    let tree = save::write_save(&f.source, &m, "france", 1_000_000).expect("write");
    let back = ntw_campaign::read_esf(&EsfFile::from_bytes(&tree.to_bytes().unwrap()).unwrap(), &f.db).expect("read back");
    assert_eq!(back.model.world.embarked.get(&army), Some(&navy));
    m.end_turn();
    // A landing target: the settlement of a region with a port 30..120 units from the fleet that
    // a transport path reaches.
    let fleet = f32p(m.force_position(navy).unwrap());
    let mut target = None;
    let mut regions: Vec<_> = m.world.regions.values().filter(|r| r.slots.iter().any(|s| s.port)).collect();
    regions.sort_by(|a, b| dist(f32p(a.settlement.position), fleet).total_cmp(&dist(f32p(b.settlement.position), fleet)));
    for r in regions {
        let p = f32p(r.settlement.position);
        if !(30.0..120.0).contains(&dist(p, fleet)) {
            continue;
        }
        if let Some(path) = m.plan_transport(army, r.settlement.position)
            && path.landing().is_some()
        {
            target = Some((r.key.clone(), r.settlement.position, path));
            break;
        }
    }
    let (key, to, path) = target.expect("a coastal settlement the fleet can land at");
    let l = path.landing().unwrap();
    println!("landing for {key}: {} points, landing at point {l} {:?}, turn costs to it {:?}", path.points.len(), path.points[l], &path.costs[..=l]);
    // The fleet's points are naval, the landing point is on land next to the coast.
    for i in 1..l {
        assert!(Mover::Sea.may_enter(pm.kind[path.polys[i] as usize]), "fleet point {i} kind {}", pm.kind[path.polys[i] as usize]);
    }
    assert!(army_stands_on(pm.kind[path.polys[l] as usize]));
    let near_coast = (0..pm.len()).filter(|&q| is_coast(pm.kind[q])).any(|q| dist(polygon_point(pm, q), path.points[l]) < LANDING_RADIUS);
    assert!(near_coast, "the landing place lies within 1.5 units of a coastal polygon's point");
    // Landing jumps to a whole turn (the army had not moved: + 0).
    assert!(path.costs[l] >= 1.0 && path.costs[l].fract() < 1e-4, "{}", path.costs[l]);
    // Sail and land over the next turns.
    let mut turns = 0;
    loop {
        let ev = m.apply(CampaignCommand::MoveForce { force: army, to }).expect("move order");
        if ev.iter().any(|e| matches!(e, CampaignEvent::CharacterDisembarksNavy { .. })) {
            break;
        }
        assert_eq!(m.world.characters[&general].position, m.world.characters[&admiral].position, "the army sails with the fleet");
        m.end_turn();
        turns += 1;
        assert!(turns < 10, "the fleet never landed the army");
    }
    let g = &m.world.characters[&general];
    assert_eq!(m.carrier_of(army), None);
    assert_eq!(g.movement_points, 0, "landing ends the army's turn");
    let at = f32p(g.position);
    let poly = pm.polygon_at(at.0, at.1).expect("on the map");
    assert!(Mover::Land.may_enter(pm.kind[poly]), "landed on kind {}", pm.kind[poly]);
    println!("landed at {at:?} after {turns} more turn(s); fleet at {:?}", f32p(m.world.characters[&admiral].position));
    // Next turn the army walks on towards the settlement.
    m.end_turn();
    let ev = m.apply(CampaignCommand::MoveForce { force: army, to }).expect("walk on");
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::CharacterMoved { .. })));
    println!("walked on to {:?}", f32p(m.world.characters[&general].position));
}
