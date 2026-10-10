//! Unit tests on a tiny hand-built start position (MADE-UP values, no game data).
//! Tests against the real install are in `tests/real_install.rs`.


use ntw_data::campaign::{BuildingEffect, BuildingUnitAllowed};
use ntw_data::effects::EffectBonusBasic;
use ntw_data::{GameDatabase, Table};
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord, EsfRecordArray};
use ntw_sim::campaign::{
    BuildingRef, CampaignCommand, CharacterId, FactionId, FortId, ForceId, GovernmentType, RegionId,
    Stance, economy,
};

use super::*;

fn rec(name: &str, version: u8, children: Vec<EsfNode>) -> EsfNode {
    let mut r = EsfRecord::new(name, version);
    r.children = children;
    EsfNode::Record(Box::new(r))
}

fn arr(name: &str, items: Vec<Vec<EsfNode>>) -> EsfNode {
    let mut a = EsfRecordArray::new(name, 0);
    a.items = items;
    EsfNode::RecordArray(Box::new(a))
}

fn s(v: &str) -> EsfNode {
    EsfNode::Utf16String(v.into())
}

fn date() -> EsfNode {
    rec(
        "DATE",
        2,
        vec![
            EsfNode::U32(1805),
            EsfNode::U32(1),
            EsfNode::U32(0),
            EsfNode::U32(0),
        ],
    )
}

fn unit(id: i32, key: &str) -> Vec<EsfNode> {
    let empty = || rec("X", 0, vec![]);
    vec![rec(
        "LAND_UNIT",
        1,
        vec![rec(
            "UNIT",
            3,
            vec![
                rec("UNIT_RECORD_KEY", 0, vec![s(key)]),
                empty(),
                empty(),
                empty(),
                EsfNode::I32(id),
                EsfNode::U32(60),
                EsfNode::U32(80),
            ],
        )],
    )]
}

fn character(id: i32, kind: &str, x: i32) -> Vec<EsfNode> {
    let mut loco = vec![EsfNode::I32(x << 20), EsfNode::I32(-(2 << 20))];
    loco.extend((2..8).map(|_| EsfNode::I32(0)));
    loco.extend([EsfNode::I32(26), EsfNode::I32(30)]);
    vec![rec(
        "CHARACTER",
        12,
        vec![
            rec("LOCOMOTABLE", 2, loco),
            rec("CHARACTER_DETAILS", 3, vec![]),
            EsfNode::I32(id),
            s(kind),
        ],
    )]
}

fn faction(
    id: i32,
    key: &str,
    gov: &str,
    other: i32,
    stance: &str,
    chars: Vec<Vec<EsfNode>>,
    armies: Vec<Vec<EsfNode>>,
) -> Vec<EsfNode> {
    vec![rec(
        "FACTION",
        18,
        vec![
            rec(
                "FACTION_ECONOMICS",
                6,
                vec![arr("history", vec![]), EsfNode::I32(id / 100)],
            ),
            rec(
                "DIPLOMACY_MANAGER",
                3,
                vec![arr(
                    "DIPLOMACY_RELATIONSHIPS_ARRAY",
                    vec![vec![rec(
                        "DIPLOMACY_RELATIONSHIP",
                        14,
                        vec![
                            EsfNode::I32(other),
                            arr("ATTITUDES", vec![]),
                            EsfNode::Bool(false),
                            EsfNode::I32(0),
                            s(stance),
                        ],
                    )]],
                )],
            ),
            rec(
                "GOVERNMENT",
                1,
                vec![
                    EsfNode::I32(0),
                    arr("GOV_IMP", vec![vec![rec(gov, 1, vec![])]]),
                ],
            ),
            EsfNode::I32(id),
            s(key),
            s("Display Name"),
            arr("CHARACTER_ARRAY", chars),
            arr("ARMY_ARRAY", armies),
        ],
    )]
}

/// A `REGION` item. `forts` are its `FORT_ARRAY` items, each a flat node list (the ESF gives an
/// array's items no name of their own).
fn region(id: i32, key: &str, owner: i32, forts: Vec<Vec<EsfNode>>) -> Vec<EsfNode> {
    let mut garrison = vec![rec(
        "GARRISON_RESIDENCE",
        1,
        vec![EsfNode::U32(owner as u32)],
    )];
    garrison.extend((1..10).map(|_| EsfNode::U32(0)));
    garrison.extend([EsfNode::I32(5 << 20), EsfNode::I32(7 << 20)]);
    let mut children = vec![
        s(key),
        rec(
            "POPULATION",
            1,
            vec![rec("REGION_FACTORS", 1, vec![]), EsfNode::U32(123_456)],
        ),
        rec("TRAITS", 1, vec![]),
        rec(
            "REGION_SLOT_MANAGER",
            1,
            vec![arr(
                "REGION_SLOT_ARRAY",
                vec![
                    vec![rec(
                        "REGION_SLOT",
                        3,
                        vec![rec(
                            "BUILDING_MANAGER",
                            1,
                            vec![
                                EsfNode::Bool(true),
                                rec("BUILDING", 1, vec![EsfNode::U32(100), s("test_building")]),
                                EsfNode::Bool(false),
                            ],
                        )],
                    )],
                    vec![rec(
                        "REGION_SLOT",
                        3,
                        vec![rec(
                            "BUILDING_MANAGER",
                            1,
                            vec![EsfNode::Bool(false), EsfNode::Bool(false)],
                        )],
                    )],
                ],
            )],
        ),
        EsfNode::I32(id),
        rec(
            "SETTLEMENT",
            2,
            vec![
                rec("SIEGEABLE_GARRISON_RESIDENCE", 1, garrison),
                rec("CAMPAIGN_LOCALISATION", 1, vec![]),
                EsfNode::I32(3),
                s("settlement:test:town"),
            ],
        ),
    ];
    children.extend((6..20).map(|_| EsfNode::U32(0)));
    children.push(EsfNode::U32(owner as u32));
    children.push(arr("FORT_ARRAY", forts));
    vec![rec("REGION", 5, children)]
}

fn army(force: u32, commander: u32, units: Vec<Vec<EsfNode>>) -> Vec<EsfNode> {
    vec![rec(
        "ARMY",
        2,
        vec![
            rec(
                "MILITARY_FORCE",
                1,
                vec![
                    EsfNode::U32(force),
                    EsfNode::U32(commander),
                    EsfNode::U32Array(vec![]),
                ],
            ),
            arr("UNITS_ARRAY", units),
        ],
    )]
}

/// A tiny, MADE-UP start position: 2 factions at war, 1 region, 2 characters, 1 army.
fn tiny_world() -> EsfNode {
    tiny_world_with_region(region(77, "test_region", 2000, vec![]))
}

/// [`tiny_world`] with its one region replaced.
fn tiny_world_with_region(r: Vec<EsfNode>) -> EsfNode {
    tiny_world_with_regions(vec![r])
}

/// [`tiny_world`] with these regions instead of its one.
fn tiny_world_with_regions(regions: Vec<Vec<EsfNode>>) -> EsfNode {
    rec(
        "WORLD",
        4,
        vec![
            arr(
                "FACTION_ARRAY",
                vec![
                    faction(
                        1000,
                        "test_a",
                        "GOVERNMENT::REPUBLIC",
                        2000,
                        "war",
                        vec![character(11, "General", 3), character(12, "minister", 0)],
                        vec![army(
                            500,
                            11,
                            vec![unit(1, "test_unit"), unit(2, "test_unit")],
                        )],
                    ),
                    faction(
                        2000,
                        "test_b",
                        "GOVERNMENT::ABSOLUTE_MONARCHY",
                        1000,
                        "war",
                        vec![],
                        vec![],
                    ),
                ],
            ),
            rec(
                "REBEL_FACTION",
                0,
                vec![rec(
                    "FACTION",
                    18,
                    vec![
                        rec(
                            "FACTION_ECONOMICS",
                            6,
                            vec![arr("history", vec![]), EsfNode::I32(0)],
                        ),
                        EsfNode::I32(9),
                        s(""),
                        s("REBELS"),
                    ],
                )],
            ),
            rec(
                "REGION_MANAGER",
                1,
                vec![arr("REGIONS_ARRAY", regions)],
            ),
        ],
    )
}

fn tiny(root_name: &str, world: EsfNode) -> Vec<u8> {
    let root = EsfRecord {
        name: root_name.into(),
        version: 5,
        children: vec![
            rec("BUILD", 0, vec![s("build"), s("v0")]),
            rec(
                "SAVE_GAME_HEADER",
                2,
                vec![
                    s("test_a"),
                    s("portrait"),
                    EsfNode::U32(1),
                    EsfNode::U32(1805),
                    s("Winter"),
                    s("flag"),
                    date(),
                ],
            ),
            rec(
                "CAMPAIGN_ENV",
                2,
                vec![
                    EsfNode::Bool(true),
                    EsfNode::Bool(false),
                    rec(
                        "CAMPAIGN_SETUP",
                        3,
                        vec![
                            s("test_campaign"),
                            rec(
                                "CAMPAIGN_PLAYERS_SETUP",
                                1,
                                vec![arr(
                                    "PLAYERS_ARRAY",
                                    vec![vec![rec(
                                        "CAMPAIGN_PLAYER_SETUP",
                                        3,
                                        vec![
                                            rec("V", 5, vec![]),
                                            rec("M", 2, vec![]),
                                            s("test_a"),
                                            EsfNode::Bool(false),
                                            EsfNode::Bool(true),
                                            EsfNode::Bool(false),
                                        ],
                                    )]],
                                )],
                            ),
                        ],
                    ),
                    rec(
                        "CAMPAIGN_MODEL",
                        10,
                        vec![
                            rec(
                                "CAMPAIGN_MAP_DATA",
                                2,
                                vec![s("a"), s("b"), s("test_map"), EsfNode::Bool(false)],
                            ),
                            EsfNode::U32(0),
                            rec("RandSeed", 0, vec![EsfNode::U32(12345)]),
                            rec(
                                "CAMPAIGN_CALENDAR",
                                2,
                                vec![EsfNode::U32(24), EsfNode::U32(0), date(), EsfNode::U32(0)],
                            ),
                            world,
                        ],
                    ),
                ],
            ),
        ],
    };
    EsfFile::new(root).to_bytes().unwrap()
}

#[test]
fn loads_tiny_startpos() {
    let db = GameDatabase::test_fixture();
    let bytes = tiny(STARTPOS_ROOT, tiny_world());
    let loaded = read(&bytes, &db).unwrap();
    let m = &loaded.model;
    assert_eq!(loaded.info.kind, FileKind::Startpos);
    assert_eq!(loaded.info.campaign_key, "test_campaign");
    assert_eq!(loaded.info.map_key, "test_map");
    assert_eq!(loaded.info.header.turn_number, 1);
    assert_eq!(loaded.info.players[0].faction_key, "test_a");
    assert!(loaded.info.players[0].is_playable);
    assert_eq!(m.rng.state, 12345);
    assert_eq!(m.calendar.date.year, 1805);
    assert_eq!(m.calendar.turn_number(), 1);

    let w = &m.world;
    assert_eq!(w.factions.len(), 3, "2 factions + rebels");
    assert_eq!(loaded.rebel_faction, Some(FactionId(9)));
    let a = &w.factions[&FactionId(1000)];
    assert_eq!(
        (a.key.as_str(), a.treasury, a.government),
        ("test_a", 10, GovernmentType::Republic)
    );
    assert_eq!(w.stance(FactionId(1000), FactionId(2000)), Stance::War);
    assert!(w.diplomacy_is_symmetric());

    assert_eq!(w.characters.len(), 2);
    let g = &w.characters[&CharacterId(11)];
    assert_eq!(
        (g.position.0.raw(), g.position.1.raw()),
        (3 << 20, -(2 << 20))
    );
    // LOCOMOTABLE #8 = base (max), #9 = current.
    assert_eq!((g.max_movement_points, g.movement_points), (26, 30));

    let f = &w.forces[&ForceId(500)];
    assert_eq!(f.commander, Some(CharacterId(11)));
    assert_eq!(f.faction, FactionId(1000));
    assert_eq!(f.units.len(), 2);
    assert_eq!((f.units[0].men, f.units[0].max_men), (60, 80));

    let r = &w.regions[&RegionId(77)];
    assert_eq!(r.owner, FactionId(2000));
    assert_eq!(r.population, 123_456);
    assert_eq!(r.settlement.key, "settlement:test:town");
    assert_eq!(r.settlement.position.0.raw(), 5 << 20);
    assert_eq!(r.slots.len(), 2);
    assert_eq!(
        r.slots[0].building.as_ref().unwrap().level_key,
        "test_building"
    );
    assert!(r.slots[1].building.is_none());

    // The same bytes through the narrow API.
    assert_eq!(load_startpos(&bytes, &db).unwrap(), loaded.model);
    // The loaded model can play a turn.
    let mut m2 = loaded.model.clone();
    m2.end_turn();
    assert_eq!(m2.calendar.turns_elapsed, 1);
}

/// `REGION/FORT_ARRAY` items become map objects in `World::forts`, each carrying the region that
/// held it. **The array is empty in every shipped file** (all eight `startpos.esf` and all ten
/// vanilla saves), so this test builds one: the field meanings are the exe's
/// (`read_forts`), and only the shape -- id, position, key, owning region -- can be checked here.
#[test]
fn fort_array_loads_as_map_objects() {
    let db = GameDatabase::test_fixture();
    let forts = vec![
        vec![EsfNode::U32(4242), EsfNode::Coord2d(-3.5, 2.25), s("fort_alpha")],
        // No position: a fort whose record gave none still loads, with `position: None`.
        vec![EsfNode::U32(4243), s("")],
        // No leading integer at all: skipped rather than misread.
        vec![s("no_id_here")],
    ];
    let bytes = tiny(
        STARTPOS_ROOT,
        tiny_world_with_region(region(77, "test_region", 2000, forts)),
    );
    let m = read(&bytes, &db).unwrap().model;
    assert_eq!(m.world.forts.len(), 2, "the id-less item is skipped");
    let a = &m.world.forts[&FortId(4242)];
    assert_eq!(a.region, RegionId(77));
    assert_eq!(a.key, "fort_alpha");
    let (x, z) = a.position.expect("the item's coordinate pair");
    assert!((x.to_f64() - -3.5).abs() < 1e-3, "x = {}", x.to_f64());
    assert!((z.to_f64() - 2.25).abs() < 1e-3, "z = {}", z.to_f64());
    assert_eq!(m.world.forts[&FortId(4243)].position, None);
}

#[test]
fn rejects_wrong_root() {
    let db = GameDatabase::test_fixture();
    let bytes = tiny(STARTPOS_ROOT, tiny_world());
    assert!(matches!(
        load_save(&bytes, &db),
        Err(LoadError::WrongRoot { .. })
    ));
    let save = tiny(SAVE_ROOT, tiny_world());
    assert!(load_save(&save, &db).is_ok());
    assert!(matches!(
        load_startpos(&save, &db),
        Err(LoadError::WrongRoot { .. })
    ));
    let other = tiny("SOMETHING_ELSE", tiny_world());
    assert!(matches!(
        read(&other, &db),
        Err(LoadError::WrongRoot { .. })
    ));
}

#[test]
fn rejects_garbage_and_truncation_without_panicking() {
    let db = GameDatabase::test_fixture();
    assert!(matches!(read(b"not an esf", &db), Err(LoadError::Esf(_))));
    let bytes = tiny(STARTPOS_ROOT, tiny_world());
    for len in (0..bytes.len()).step_by(7) {
        assert!(read(&bytes[..len], &db).is_err());
    }
    // Flipping bytes may or may not parse, but never panics.
    for i in (0..bytes.len()).step_by(3) {
        let mut b = bytes.clone();
        b[i] ^= 0x5A;
        let _ = read(&b, &db);
    }
}

#[test]
fn missing_world_is_an_error() {
    let db = GameDatabase::test_fixture();
    let bytes = tiny(STARTPOS_ROOT, rec("NOT_WORLD", 0, vec![]));
    let e = read(&bytes, &db).unwrap_err();
    assert!(
        matches!(&e, LoadError::MissingRecord { path } if path.ends_with("CAMPAIGN_MODEL/WORLD")),
        "{e}"
    );
}

#[test]
fn bad_field_type_is_an_error() {
    let db = GameDatabase::test_fixture();
    let mut world = tiny_world();
    // Replace the region id (REGION #4, an i32) with a string.
    if let EsfNode::Record(w) = &mut world
        && let Some(EsfNode::Record(rm)) = w.children.get_mut(2)
        && let Some(EsfNode::RecordArray(a)) = rm.children.get_mut(0)
        && let Some(EsfNode::Record(r)) = a.items[0].get_mut(0)
    {
        r.children[4] = s("oops");
    }
    let e = read(&tiny(STARTPOS_ROOT, world), &db).unwrap_err();
    assert!(
        matches!(
            e,
            LoadError::BadField {
                index: 4,
                expected: "i32",
                found: Some("utf16"),
                ..
            }
        ),
        "{e}"
    );
}

#[test]
fn oddities_become_warnings() {
    let db = GameDatabase::test_fixture();
    let world = rec(
        "WORLD",
        4,
        vec![
            arr(
                "FACTION_ARRAY",
                vec![
                    faction(
                        1000,
                        "test_a",
                        "GOVERNMENT::NEW_KIND",
                        4242,
                        "war",
                        vec![character(11, "spy", 0)],
                        vec![army(500, 99, vec![])],
                    ),
                    faction(
                        2000,
                        "test_b",
                        "GOVERNMENT::REPUBLIC",
                        1000,
                        "frenemies",
                        vec![],
                        vec![],
                    ),
                ],
            ),
            rec(
                "REGION_MANAGER",
                1,
                vec![arr("REGIONS_ARRAY", vec![region(77, "test_region", 31337, vec![])])],
            ),
        ],
    );
    let loaded = read(&tiny(STARTPOS_ROOT, world), &db).unwrap();
    let w = &loaded.warnings;
    let has = |p: &dyn Fn(&LoadWarning) -> bool| w.iter().any(p);
    assert!(has(
        &|x| matches!(x, LoadWarning::UnknownGovernment { name, .. } if name == "GOVERNMENT::NEW_KIND")
    ));
    assert!(has(&|x| matches!(
        x,
        LoadWarning::DanglingDiplomacy { target: 4242, .. }
    )));
    assert!(has(
        &|x| matches!(x, LoadWarning::UnknownStance { stance, .. } if stance == "frenemies")
    ));
    assert!(has(&|x| matches!(
        x,
        LoadWarning::UnknownCharacterType { id: 11, .. }
    )));
    assert!(has(&|x| matches!(
        x,
        LoadWarning::DanglingCommander {
            force: 500,
            commander: 99
        }
    )));
    assert!(has(&|x| matches!(
        x,
        LoadWarning::DanglingRegionOwner { owner: 31337, .. }
    )));
    // Every warning prints a message.
    assert!(w.iter().all(|x| !x.to_string().is_empty()));
    // Skipped items are really skipped.
    assert!(loaded.model.world.characters.is_empty());
    assert_eq!(loaded.model.world.forces[&ForceId(500)].commander, None);
    assert_eq!(loaded.rebel_faction, None);
}

/// The fixture plus the campaign rows that turn the fixture's barracks level into somewhere the
/// fixture infantry can be raised, and the effect junction row that gives a building a recruitment
/// cost effect. Everything stays MADE-UP — the shapes are the shipped ones.
fn db_with_a_barracks() -> GameDatabase {
    let mut db = GameDatabase::test_fixture();
    db.campaign.building_units = Table::from_rows(
        0,
        vec![BuildingUnitAllowed { building: BARRACKS.into(), unit: UNIT.into(), ..Default::default() }],
    );
    db.campaign.building_effects = Table::from_rows(
        0,
        vec![
            BuildingEffect { building: BARRACKS.into(), effect: "recruitment_points".into(), value: 2.0 },
            BuildingEffect { building: BARRACKS.into(), effect: "fixture_cheaper_recruits".into(), value: -10.0 },
        ],
    );
    // The effect → bonus junction every source is compiled through, which the fixture leaves empty.
    db.campaign.effects.bonus_basic = Table::from_rows(
        0,
        vec![
            EffectBonusBasic { effect: "recruitment_points".into(), bonus: "recruitment_points".into() },
            EffectBonusBasic { effect: "fixture_cheaper_recruits".into(), bonus: "recruitment_mod_cost_land_all".into() },
        ],
    );
    db
}

/// The fixture's building level and the unit the test recruits; both are fixture keys.
const BARRACKS: &str = "fixture_barracks_1";
const UNIT: &str = "fixture_line_infantry";

/// The loader copies `units` #7 into the campaign cost and keeps #4 apart: the campaign charges #7
/// (`UNIT_RECORD` +0x38, read by `0x00B0D220`), #4 is the battle army-setup price. It also copies the
/// unit cap #15 (`UNIT_RECORD` +0x68, read by `0x008F68B0`).
#[test]
fn the_loader_takes_the_campaign_cost_from_units_7() {
    let mut db = db_with_a_barracks();
    let units = db.units.rows().iter().cloned().map(|mut u| {
        if u.key == UNIT {
            u.unit_cap = 4;
        }
        u
    });
    db.units = Table::from_rows(0, units.collect());
    let rules = rules_from_db(&db, "test_campaign");
    let land = &rules.units[UNIT];
    assert_eq!((land.cost, land.campaign_cost, land.unit_cap), (111, 99, 4));
}

/// All the way through a **loaded campaign**: `read` runs `rules_from_db`, and the recruitment command
/// then charges the region's entry cost — `units` #7 with the region's building effects (the
/// barracks' −10 `recruitment_mod_cost_land_all`, compiled through the junction) — keeps it on the
/// item, and a cancel refunds it.
#[test]
fn recruiting_from_a_loaded_campaign_charges_units_7_with_the_region_effects() {
    let db = db_with_a_barracks();
    let mut m = read(&tiny(STARTPOS_ROOT, tiny_world()), &db).unwrap().model;
    // The map layout is not what this test is about: point the region's slot at the fixture's
    // barracks level and hand the region to the player faction, whose turn it is.
    let region = m.world.regions.get_mut(&RegionId(77)).expect("the tiny region is there");
    region.owner = FactionId(1000);
    region.slots[0].building = Some(BuildingRef { level_key: BARRACKS.into(), health: 100 });
    m.world.factions.get_mut(&FactionId(1000)).expect("the player is there").treasury = 10_000;
    assert_eq!(m.recruitment_points(RegionId(77), false), 2);
    assert!(m.recruitable_units(RegionId(77)).iter().any(|e| e.unit_key == UNIT && e.flags == 0));

    // FISTP(90 × 99 × 0.01) = FISTP(89.1) = 89.
    let cost = economy::recruitment_cost(&m, &m.world.regions[&RegionId(77)], UNIT, &m.rules.units[UNIT]);
    assert_eq!(cost, 89);
    m.apply(CampaignCommand::Recruit { region: RegionId(77), unit_key: UNIT.into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&FactionId(1000)].treasury, 10_000 - cost);
    let item = &m.world.regions[&RegionId(77)].recruitment_queue[0];
    assert_eq!(item.cost, cost);
    let id = item.id;
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(77), item: id }).unwrap();
    assert_eq!(m.world.factions[&FactionId(1000)].treasury, 10_000);
}

/// `region` with a `REGION_RECRUITMENT_MANAGER` queueing one land item per `(id, unit key)`.
fn region_recruiting(id: i32, key: &str, owner: i32, items: &[(i32, &str)]) -> Vec<EsfNode> {
    let item = |id: i32, unit: &str| {
        let inner = rec("RECRUITMENT_ITEM", 2, vec![EsfNode::I32(id), EsfNode::I32(1), EsfNode::I32(0), EsfNode::U32(2), EsfNode::U32(300), EsfNode::Bool(true), s(unit)]);
        vec![rec("RECRUITMENT_ITEM", 2, vec![rec("LAND_UNIT_RECRUITMENT_ITEM", 1, vec![inner, EsfNode::U32(0)])])]
    };
    let manager = rec("REGION_RECRUITMENT_MANAGER", 0, vec![arr("REGION_RECRUITMENT_ITEM_ARRAY", items.iter().map(|&(i, u)| item(i, u)).collect())]);
    let mut r = region(id, key, owner, vec![]);
    let EsfNode::Record(b) = &mut r[0] else { unreachable!("region() gives a REGION record") };
    b.children.push(manager);
    r
}

/// Review (round 12): recruitment item ids were not checked for uniqueness on load, and the UI
/// finds a cancelled item's region from its id alone, so with one id in two regions cancelling
/// the second region's item removed and refunded the first one's. The loader now gives the later
/// item a new id, with a warning.
#[test]
fn a_recruitment_id_repeated_across_regions_gets_a_new_id() {
    use ntw_sim::campaign::RecruitmentItemId;
    let db = GameDatabase::test_fixture();
    let world = tiny_world_with_regions(vec![
        region_recruiting(77, "test_region", 1000, &[(500, "test_unit")]),
        region_recruiting(78, "test_region_2", 1000, &[(500, "test_unit"), (0, "test_unit")]),
    ]);
    let loaded = read(&tiny(STARTPOS_ROOT, world), &db).unwrap();
    assert!(loaded.warnings.contains(&LoadWarning::DuplicateRecruitmentItemId { region: "test_region_2".into(), unit: "test_unit".into(), id: 500 }));
    let mut m = loaded.model;
    let id_of = |m: &ntw_sim::campaign::CampaignModel, r: u32| m.world.regions[&RegionId(r)].recruitment_queue.iter().map(|i| i.id).collect::<Vec<_>>();
    assert_eq!(id_of(&m, 77), vec![RecruitmentItemId(500)]);
    let b = id_of(&m, 78);
    assert!(b.iter().all(|&i| i.raw() != 0 && i.raw() != 500) && b[0] != b[1], "new, distinct ids: {b:?}");
    // Every item is linked to the record it was read from, renumbered ones too.
    let src = |i: usize| ntw_sim::campaign::RecruitmentSource { port_slot: None, index: i };
    assert_eq!(m.world.recruitment_sources[&RecruitmentItemId(500)], src(0));
    assert_eq!((m.world.recruitment_sources[&b[0]], m.world.recruitment_sources[&b[1]]), (src(0), src(1)));
    assert_eq!(m.world.recruitment_sources.len(), 3);

    // Each id names its own region, so cancelling region 78's item leaves region 77's queued.
    assert_eq!(m.recruitment_item_region(b[0]), Some(RegionId(78)));
    let treasury = m.world.factions[&FactionId(1000)].treasury;
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(78), item: b[0] }).unwrap();
    assert_eq!(id_of(&m, 77), vec![RecruitmentItemId(500)]);
    assert_eq!(id_of(&m, 78), vec![b[1]]);
    assert_eq!(m.world.factions[&FactionId(1000)].treasury, treasury + 300);
}

/// Review (polish rounds): the save writer matched queued items to source records by id and unit
/// key, so a renumbered item, a new recruit or an id shared by two items could take another
/// item's record (a port's ship and a foot item sharing an id: the ship took the foot record).
/// Each loaded item now keeps the record it was read from. Load → save → load: every kept record
/// is the source's own (fields and all) apart from the id of a renumbered item, which the next
/// load reads back unchanged; a new recruit is built fresh.
#[test]
fn recruitment_items_keep_their_own_records_over_a_save_and_load() {
    use ntw_formats::esf::EsfFile;
    use ntw_sim::campaign::RecruitmentItemId;
    let db = GameDatabase::test_fixture();
    // A source item whose trailing u32 is `mark`, naming the record, and whose #2 is `target` (the
    // commander it was recruited through: 11 is the fixture's general, 12345 no character).
    let item_to = |id: i32, unit: &str, mark: u32, target: i32| {
        let inner = rec("RECRUITMENT_ITEM", 2, vec![EsfNode::I32(id), EsfNode::I32(1), EsfNode::I32(target), EsfNode::U32(2), EsfNode::U32(300), EsfNode::Bool(true), s(unit)]);
        vec![rec("RECRUITMENT_ITEM", 2, vec![rec("UNIT_RECRUITMENT_ITEM", 1, vec![inner, EsfNode::U32(mark)])])]
    };
    let item = |id: i32, unit: &str, mark: u32| item_to(id, unit, mark, 0);
    let manager = |items: Vec<Vec<EsfNode>>| rec("REGION_RECRUITMENT_MANAGER", 0, vec![arr("REGION_RECRUITMENT_ITEM_ARRAY", items)]);
    let mut region = region(77, "test_region", 1000, vec![]);
    let EsfNode::Record(b) = &mut region[0] else { unreachable!("region() gives a REGION record") };
    // Slot 1 is a port whose ship shares id 600 with two foot items (ports are read first, so the
    // ship keeps 600); then an id-less item and an item cancelled below.
    if let Some(EsfNode::Record(sm)) = b.children.iter_mut().find(|c| matches!(c, EsfNode::Record(r) if r.name == "REGION_SLOT_MANAGER")) {
        let EsfNode::RecordArray(slots) = &mut sm.children[0] else { unreachable!() };
        let EsfNode::Record(port) = &mut slots.items[1][0] else { unreachable!() };
        port.children.push(manager(vec![item(600, "ship", 9)]));
    }
    b.children.push(manager(vec![item(600, "unit_a", 1), item(600, "unit_b", 2), item_to(0, "unit_c", 3, 12345), item_to(800, "unit_d", 4, 11)]));
    let source = EsfFile::from_bytes(&tiny(STARTPOS_ROOT, tiny_world_with_regions(vec![region]))).unwrap();
    let first = read(&source.to_bytes().unwrap(), &db).unwrap();
    let mut m = first.model;
    let queue = |m: &ntw_sim::campaign::CampaignModel| m.world.regions[&RegionId(77)].recruitment_queue.clone();
    let loaded = queue(&m);
    let units: Vec<&str> = loaded.iter().map(|i| i.unit_key.as_str()).collect();
    assert_eq!(units, ["ship", "unit_a", "unit_b", "unit_c", "unit_d"]);
    assert_eq!(loaded[0].id, RecruitmentItemId(600), "the ship, read first, keeps 600");
    // #2: unit_d's target is the general; unit_c's names no character, dropped with a warning.
    let general = ntw_sim::campaign::CharacterId(11);
    assert_eq!(loaded[4].target, Some(general));
    assert_eq!(loaded[3].target, None);
    assert!(first.warnings.iter().any(|w| matches!(w, LoadWarning::DanglingRecruitmentTarget { target: 12345, .. })), "{:?}", first.warnings);
    // Cancel unit_a, which was renumbered: unit_b must still get its own record. Recruit a new unit_d.
    m.world.regions.get_mut(&RegionId(77)).unwrap().recruitment_queue.remove(1);
    let new_id = RecruitmentItemId(m.world.alloc_id() as i32);
    m.world.regions.get_mut(&RegionId(77)).unwrap().recruitment_queue.push(ntw_sim::campaign::RecruitmentItem {
        id: new_id,
        unit_key: "unit_d".into(),
        turns_remaining: 3,
        cost: 77,
        target: Some(general),
    });
    let written = crate::save::write_save(&source, &m, "test_faction", 1).unwrap();
    let back = read(&written.to_bytes().unwrap(), &db).unwrap();
    assert!(
        back.warnings.iter().all(|w| !matches!(
            w,
            LoadWarning::DuplicateRecruitmentItemId { .. } | LoadWarning::RecruitmentItemWithoutId { .. } | LoadWarning::DanglingRecruitmentTarget { .. }
        )),
        "{:?}",
        back.warnings
    );
    assert_eq!(queue(&back.model), queue(&m), "ids, units, turns, costs and targets come back");
    // Every recruitment record of a tree (ports first, then the region's own), as (id, unit, mark).
    let records = |f: &EsfFile| -> Vec<(EsfNode, String, Option<u32>)> {
        let mut out = Vec::new();
        f.root.walk(&mut |r| {
            if let Some(inner) = r.child("RECRUITMENT_ITEM") {
                out.push((inner.children[0].clone(), inner.get_str(6).unwrap().to_owned(), r.get_u32(1)));
            }
        });
        out
    };
    let after = records(&written);
    let id = |i: usize| EsfNode::I32(loaded[i].id.raw());
    assert_eq!(after[0], (EsfNode::I32(600), "ship".into(), Some(9)), "the ship keeps its own record");
    assert_eq!(after[1], (id(2), "unit_b".into(), Some(2)), "unit_b: its own record, renumbered");
    assert_eq!(after[2], (id(3), "unit_c".into(), Some(3)), "unit_c: its own record, given its id");
    assert_eq!(after[3], (EsfNode::I32(800), "unit_d".into(), Some(4)), "unit_d: its own record");
    assert_eq!(after.len(), 5);
    assert_ne!(after[4].2, Some(4), "the new unit_d recruit is built fresh, not a copy of unit_d's record");
    // Stable: saving the reloaded model again gives the same tree.
    let again = crate::save::write_save(&written, &back.model, "test_faction", 1).unwrap();
    assert_eq!(again.to_bytes().unwrap(), written.to_bytes().unwrap());
}

/// Polish: a repeated `REGION` id record (malformed) was skipped by the loader for the region but
/// not for its forts, and the save left it unwritten, so after an owner change it kept the old owner
/// and residences. The loader now reads only the first record of an id, and the save drops the
/// others: the saved file holds what was loaded, and loads back the same with no warning.
#[test]
fn a_repeated_region_record_is_not_read_and_not_saved() {
    use ntw_formats::esf::EsfFile;
    let db = GameDatabase::test_fixture();
    let fort = vec![vec![EsfNode::U32(4242), s("fort_in_the_repeat")]];
    let world = tiny_world_with_regions(vec![
        region_recruiting(77, "test_region", 1000, &[]),
        region_recruiting(77, "test_region_repeat", 2000, &[(500, "test_unit")]),
    ]);
    let mut regions = world;
    // The repeated record holds a fort.
    if let EsfNode::Record(w) = &mut regions
        && let Some(EsfNode::Record(rm)) = w.children.iter_mut().find(|c| matches!(c, EsfNode::Record(r) if r.name == "REGION_MANAGER"))
        && let EsfNode::RecordArray(a) = &mut rm.children[0]
        && let EsfNode::Record(r) = &mut a.items[1][0]
    {
        let forts = r.children.iter_mut().find(|c| matches!(c, EsfNode::RecordArray(x) if x.name == "FORT_ARRAY")).unwrap();
        *forts = arr("FORT_ARRAY", fort);
    }
    let source = EsfFile::from_bytes(&tiny(STARTPOS_ROOT, regions)).unwrap();
    let loaded = read(&source.to_bytes().unwrap(), &db).unwrap();
    assert!(loaded.warnings.contains(&LoadWarning::DuplicateId { kind: "region", id: 77 }));
    let mut m = loaded.model;
    let r = &m.world.regions[&RegionId(77)];
    assert_eq!((r.key.as_str(), r.owner, r.recruitment_queue.len()), ("test_region", FactionId(1000), 0));
    assert!(m.world.forts.is_empty(), "the repeated record's fort is not read");
    // Region 77 changes hands, then the game saves.
    m.world.regions.get_mut(&RegionId(77)).unwrap().owner = FactionId(2000);
    let written = crate::save::write_save(&source, &m, "test_a", 1).unwrap();
    let region_records = |f: &EsfFile| {
        let mut out = Vec::new();
        f.root.walk(&mut |r| {
            if r.name == "REGION" {
                out.push((r.get_str(0).unwrap_or_default().to_owned(), r.get_u32(20)));
            }
        });
        out
    };
    assert_eq!(region_records(&written), [("test_region".to_owned(), Some(2000))], "one record, the one read, with the new owner");
    let back = read(&written.to_bytes().unwrap(), &db).unwrap();
    assert!(!back.warnings.iter().any(|w| matches!(w, LoadWarning::DuplicateId { .. })), "{:?}", back.warnings);
    assert_eq!(back.model.world.regions, m.world.regions);
}

/// Polish: a `REGION` without its own `REGION_RECRUITMENT_MANAGER` was read as an empty queue without
/// a word. It still is (nothing to read), with one warning for the region.
#[test]
fn a_region_without_a_recruitment_manager_warns_once() {
    let db = GameDatabase::test_fixture();
    let world = tiny_world_with_regions(vec![region(77, "test_region", 1000, vec![]), region_recruiting(78, "test_region_2", 1000, &[])]);
    let loaded = read(&tiny(STARTPOS_ROOT, world), &db).unwrap();
    let warned: Vec<&LoadWarning> = loaded.warnings.iter().filter(|w| matches!(w, LoadWarning::RegionWithoutRecruitmentManager { .. })).collect();
    assert_eq!(warned, [&LoadWarning::RegionWithoutRecruitmentManager { region: "test_region".into() }]);
    assert_eq!(warned[0].to_string(), "region test_region: no recruitment manager of its own (read as an empty queue)");
    assert!(loaded.model.world.regions[&RegionId(77)].recruitment_queue.is_empty());
}

/// Review (round 12): the scripts' restricted building levels of a save were copied into the
/// model by the game's scene setup, a second place for the rule; the loader now fills
/// `World::restricted_buildings` itself and hands only the restricted units on.
#[test]
fn the_loader_fills_the_restricted_building_levels() {
    let db = GameDatabase::test_fixture();
    let mut esf = EsfFile::from_bytes(&tiny(STARTPOS_ROOT, tiny_world())).unwrap();
    let episodic = rec(
        "EPISODIC_RESTRICTIONS",
        0,
        vec![
            arr("UNIT_RESTRICTIONS", vec![vec![s("unit_x")]]),
            arr("BUILDING_RESTRICTIONS", vec![vec![s("level_b")], vec![s("level_a")]]),
        ],
    );
    let model_rec = esf
        .root
        .children
        .iter_mut()
        .find_map(|n| match n {
            EsfNode::Record(r) if r.name == "CAMPAIGN_ENV" => Some(r),
            _ => None,
        })
        .and_then(|env| {
            env.children.iter_mut().find_map(|n| match n {
                EsfNode::Record(r) if r.name == "CAMPAIGN_MODEL" => Some(r),
                _ => None,
            })
        })
        .expect("the tiny file has a CAMPAIGN_MODEL");
    model_rec.children.push(episodic);
    let loaded = read(&esf.to_bytes().unwrap(), &db).unwrap();
    assert_eq!(loaded.model.world.restricted_buildings.iter().map(String::as_str).collect::<Vec<_>>(), vec!["level_a", "level_b"]);
    assert_eq!(loaded.restricted_units, vec!["unit_x".to_owned()]);
    // A file without the record restricts nothing.
    let plain = read(&tiny(STARTPOS_ROOT, tiny_world()), &db).unwrap();
    assert!(plain.model.world.restricted_buildings.is_empty() && plain.restricted_units.is_empty());
}
