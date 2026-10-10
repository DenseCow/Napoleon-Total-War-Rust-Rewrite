//! The two projectile effect tables (`db\projectiles_explosions`, `db\projectile_impacts`) and the
//! `projectiles` columns that point at them, against a real install (read-only). `#[ignore]`d. Run
//! with:
//! ```text
//! cargo test -p ntw_data --test effects_data -- --ignored --nocapture
//! ```
//! See `analysis/graphics/BATTLE_EFFECTS.md` §2 and §5. The companion test in
//! `ntw_formats/tests/effects_install.rs` checks the names against `effects\landbattle.xml`.
use std::collections::BTreeMap;
use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::effects::EffectLibrary;
use ntw_formats::pack::Vfs;
use ntw_formats::projectile_fx::{ExplosionTable, ImpactTable, TrailTable};

// The vanilla files themselves (these tests check what the install ships).
const PROJECTILES_EXPLOSIONS: &str = "db/projectiles_explosions_tables/projectiles_explosions";
const PROJECTILE_IMPACTS: &str = "db/projectile_impacts_tables/projectile_impacts";
const PROJECTILE_TRAILS: &str = "db/projectile_trails_tables/projectile_trails";

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR))
}

fn vfs() -> Vfs {
    Vfs::open_install(data_dir()).expect("open install")
}





/// The `projectiles` tail columns: the **fire effect group** (column 31), the **trail** (32), the
/// impact ball class (33) and the weapon family (34).
///
/// Column 31 is the big one: it is the firing muzzle flash and smoke group, it is set on most of
/// the 144 shipped rows, and every value is a `SCRIPTED_EFFECT_GROUP` of `effects\landbattle.xml`.
/// That is what replaces the PROVISIONAL `weapon_family` -> group table in `battle::fx::fire_group`.
#[test]
#[ignore]
fn projectiles_name_their_own_fire_group_and_trail() {
    let db = GameDatabase::from_install(data_dir()).expect("database");
    let vfs = vfs();
    let lib = EffectLibrary::from_vfs(&vfs).expect("landbattle.xml");
    assert_eq!(db.projectiles.len(), 144);

    let mut groups: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut with_group = 0usize;
    for p in db.projectiles.iter() {
        let Some(g) = db.fire_effect(p) else { continue };
        assert!(lib.has_group(g), "{}: fire_effect {g} is not a landbattle group", p.key);
        with_group += 1;
        *groups.entry(g).or_default() += 1;
    }
    // **Round 4.** This was `assert!(with_group >= 60, ...)`. A floor of 60 passes if half the
    // column stops being read — which is exactly the failure this table exists to catch, and it is
    // the bug that was live in `battle::fx::fire_group` (it discarded `db.fire_effect` and guessed
    // from the weapon family instead). The floor is replaced by the **exact** tally, which is also
    // what `battle::fx::fire_group`'s doc comment quotes, so the comment cannot drift from the data
    // without one of the two failing.
    assert_eq!(with_group, 120, "the fire_effect column is set on 120 of the 144 rows");
    assert_eq!(db.projectiles.len() - with_group, 24, "the rows that fire no group");
    let want: BTreeMap<&str, usize> = [
        ("CannonFire", 56),
        ("LandGunFire_canister", 13),
        ("LandGunFire_large", 9),
        ("LandGunFire_mortar", 8),
        ("MusketFire", 8),
        ("LandGunFire", 7),
        ("LandGunFire_howitzer", 7),
        ("LandGunFire_small", 7),
        ("fougasse_default", 2),
        ("rifleFire", 2),
        ("pistolFire", 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(groups, want, "the fire group tally, verbatim");
    println!("fire groups over {with_group} rows: {groups:?}");

    // And the group is a *group*, not an emitter, for every row that sets it — so the draw slot can
    // ask for it by name. (Clean negative, kept here because it is the same column.)
    let emitters: Vec<&&str> = groups.keys().filter(|g| lib.effects.contains_key(**g)).collect();
    assert!(emitters.is_empty(), "fire_effect names emitters: {emitters:?}");

    // Column 32 is the trail: the trail's **effect group** in `landbattle.xml`. Round 3 closed the
    // bridge — the trail *table* is reached by column 6, not by this one — so the clean negative
    // below is expected and is now explained. See
    // `the_trail_group_and_the_trail_table_are_joined_by_column_six` below.
    let mut trails: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for p in db.projectiles.iter() {
        if let Some(t) = p.trail.as_deref() {
            *trails.entry(t).or_default() += 1;
        }
    }
    for t in ["shrapnel_trail", "congreve_rocket", "carcass_trail", "quicklime_trail"] {
        assert!(trails.contains_key(t), "{t} missing: {trails:?}");
    }
    assert!(
        !trails.keys().any(|t| ["alpha", "alpha_bullet", "alpha_shrapnel", "e3_rocket", "none"].contains(t)),
        "a trail value is a `projectile_trails` key: {trails:?}"
    );
    println!("trails: {trails:?}");

    // Column 33 is the impact ball class, and every value is a `projectile_impacts` row key.
    let impacts = ImpactTable::read(&vfs.read(PROJECTILE_IMPACTS).expect("impacts")).expect("projectile_impacts");
    assert_eq!(impacts.len(), 8);
    let mut balls: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for p in db.projectiles.iter() {
        if let Some(row) = db.impact_effects(p) {
            *balls.entry(row.key.as_str()).or_default() += 1;
        } else if let Some(c) = p.impact_ball.as_deref() {
            panic!("{}: impact_ball {c} is not a row key", p.key);
        }
    }
    println!("impact ball classes: {balls:?}");

    // Every group either table names is a real group of the shipped library.
    for row in impacts.iter() {
        for g in row.groups.iter().flatten() {
            assert!(lib.has_group(g), "{}: {g} is not a landbattle group", row.key);
        }
    }
}

/// The `projectiles_explosions` rows: every row's three effect groups are `landbattle.xml` groups,
/// and **the air burst is chosen per row in the data, not by a calibre rule**.
///
/// This closes BATTLE_EFFECTS.md §5 item 16 (the PROVISIONAL 6/12/24-pound air-burst split). The
/// rows say `shell_12lb -> AirExplosion_med` and `shrapnel_12lb -> AirExplosion_sml`: a pure
/// calibre rule cannot produce both, which is why the split used to be a stand-in.
#[test]
#[ignore]
fn explosion_rows_name_the_effect_groups_they_play() {
    let vfs = vfs();
    let lib = EffectLibrary::from_vfs(&vfs).expect("landbattle.xml");
    let bytes = vfs.read(PROJECTILES_EXPLOSIONS).expect("explosions");
    let db = GameDatabase::from_install(data_dir()).expect("database");

    let table = ExplosionTable::read(&bytes).expect("projectiles_explosions");
    assert_eq!(table.len(), 35);

    // Every distinct `projectiles.explosion` foreign key reaches a row. 18 of the 35 rows are named
    // by a projectile; the other 17 (`carcass_24lb`, `rocket_naval`, the `shrapnel_*lb` series,
    // ...) are in the table for another caller, so the FK list is a subset, not the key set.
    let fks: std::collections::BTreeSet<&str> = db.projectiles.iter().filter_map(|p| p.explosion.as_deref()).collect();
    assert_eq!(fks.len(), 18, "18 distinct `projectiles.explosion` keys");
    for k in &fks {
        assert!(table.get(k).is_some(), "no explosion row for {k}");
    }

    for row in table.iter() {
        for g in [row.air.as_deref(), row.ground.as_deref()].into_iter().flatten() {
            assert!(lib.has_group(g), "{}: {g} is not a landbattle group", row.key);
        }
        assert!(!row.fuse.is_empty() && !row.shockwave.is_empty(), "{}: empty fuse/shockwave", row.key);
    }

    // CONFIRMED NEGATIVE: the third string column is a fragment **projectile**, not an effect
    // group. Every one of its distinct values is a `projectiles` key, and none of them is a group.
    let mut fragments: std::collections::BTreeSet<&str> =
        table.iter().filter_map(|r| r.fragment_projectile.as_deref()).collect();
    assert!(fragments.len() >= 6, "only {} fragment projectiles: {fragments:?}", fragments.len());
    for f in &fragments {
        assert!(db.projectile(f).is_some(), "{f} is not a `projectiles` key");
        assert!(!lib.has_group(f), "{f} is also an effect group");
    }
    println!("fragment projectiles: {fragments:?}");
    fragments.clear();

    // Every projectile's `explosion` foreign key reaches a row, and the air burst is what that row
    // names — this is the lookup `battle::fx` now uses instead of a calibre threshold.
    let mut resolved = 0usize;
    for p in db.projectiles.iter() {
        let Some(row) = db.explosion_effects(p) else { continue };
        assert!(lib.has_group(row.air.as_deref().unwrap_or("")), "{}: {}", p.key, row.key);
        resolved += 1;
    }
    println!("{resolved} of 144 projectiles resolve an explosion row");

    // The shipped air-burst choices, verbatim.
    let want: [(&str, &str); 10] = [
        ("shell_12lb", "AirExplosion_med"),
        ("shell_18lb", "AirExplosion_med"),
        ("shell_24lb", "AirExplosion_lrg"),
        ("shell_32lb", "AirExplosion_lrg"),
        ("shell_64lb", "AirExplosion_lrg"),
        ("shell_percussive_12lb", "AirExplosion_sml"),
        ("shell_percussive_24lb", "AirExplosion_med"),
        ("shell_percussive_32lb", "AirExplosion_lrg"),
        ("shrapnel_12lb", "AirExplosion_sml"),
        ("shrapnel_24lb", "AirExplosion_med"),
    ];
    for (key, air) in want {
        assert_eq!(table.get(key).unwrap().air.as_deref(), Some(air), "{key}");
    }
    // The ground scorch: ten of the 35 rows have one (the shell, mortar, rocket and percussive
    // families), and it is the same group on every one.
    let ground_rows: Vec<&str> = table.iter().filter(|r| r.ground.is_some()).map(|r| r.key.as_str()).collect();
    assert_eq!(ground_rows.len(), 10, "the rows with a ground scorch: {ground_rows:?}");
    assert!(
        table.iter().filter(|r| r.ground.is_some()).all(|r| r.ground.as_deref() == Some("Cannon_Groundimpact_explosive")),
        "a ground group other than Cannon_Groundimpact_explosive"
    );
    assert_eq!(table.get("carcass_12lb").unwrap().ground, None);
    assert_eq!(table.get("carcass_12lb").unwrap().air.as_deref(), Some("explode_cone"));

    // The numeric neighbours of the air-burst string, kept as the evidence that they are there:
    // the fourth value of each row's first numeric block. It rises with the pound count over the
    // shrapnel series (2, 3, 4, 6, 8, 10, 12, 14 m), so it is INFERRED to be a burst radius in
    // metres — but it does NOT pick the group (`shell_percussive_12lb` has 20 and plays `_sml`),
    // so the group choice stays "authored per row".
    let fourth = |key: &str| table.get(key).unwrap().numbers[3];
    assert_eq!(
        ["shrapnel_3lb", "shrapnel_6lb", "shrapnel_9lb", "shrapnel_12lb", "shrapnel_18lb", "shrapnel_24lb", "shrapnel_32lb", "shrapnel_64lb"]
            .map(fourth),
        [2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0]
    );
    assert_eq!(fourth("shell_percussive_12lb"), 20.0);
}

/// **The trail bridge, closed.** This is the `projectiles`-side proof; the `projectile_trails`-side
/// proof (that the table reads whole) is `ntw_formats/tests/effects_install.rs::the_projectile_trails_table_reads_whole`.
///
/// Round 2 left this UNKNOWN. It is now a plain foreign key, on a column that already existed:
///
/// - `projectiles` **column 32** (`trail`) is **not** a `projectile_trails` key — it names the
///   trail's **effect group** in `effects\landbattle.xml` (all 7 values, over 9 rows).
/// - `projectiles` **column 6** (`trail_texture`) **is** a `projectile_trails` key — all 5 values,
///   over all 144 rows, and they are exactly the table's five rows.
///
/// So a shot's trail is a pair: the group in `landbattle.xml` draws the particles, and the
/// `projectile_trails` row gives the trail's colour and geometry. `carcass_fragment_trail` (group)
/// + `alpha` (trail row) is the whole of a carcass fragment's trail.
#[test]
#[ignore]
fn the_trail_group_and_the_trail_table_are_joined_by_column_six() {
    let db = GameDatabase::from_install(data_dir()).expect("database");
    let vfs = vfs();
    let lib = EffectLibrary::from_vfs(&vfs).expect("landbattle.xml");
    let table = TrailTable::read(&vfs.read(PROJECTILE_TRAILS).expect("trails")).expect("projectile_trails");

    // Every column-32 value is a landbattle GROUP (7 values, 9 rows) — it names what plays.
    let mut groups: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for p in db.projectiles.iter() {
        let Some(t) = p.trail.as_deref() else { continue };
        assert!(lib.has_group(t), "{}: trail {t} is not a landbattle group", p.key);
        groups.entry(t).or_default().push(p.key.as_str());
    }
    assert_eq!(
        groups.keys().copied().collect::<Vec<_>>(),
        vec![
            "carcass_fragment_trail",
            "carcass_trail",
            "congreve_rocket",
            "quicklime_fragment_trail",
            "quicklime_trail",
            "ship_explosion_fragment_trail",
            "shrapnel_trail",
        ],
        "the 7 trail groups, verbatim"
    );
    assert_eq!(groups.values().map(Vec::len).sum::<usize>(), 9, "9 rows name a trail group");

    // Every column-6 value is a `projectile_trails` key (5 values, 144 rows) — it says how it draws.
    let mut textures: std::collections::BTreeMap<&str, usize> = Default::default();
    for p in db.projectiles.iter() {
        let t = p.trail_texture.as_deref().expect("every row sets column 6");
        assert!(table.get(t).is_some(), "{}: trail_texture {t} is not a projectile_trails key", p.key);
        *textures.entry(t).or_default() += 1;
    }
    assert_eq!(
        textures.keys().copied().collect::<Vec<_>>(),
        vec!["alpha", "alpha_bullet", "alpha_shrapnel", "e3_rocket", "none"],
        "column 6 covers exactly the table's five rows"
    );
    assert_eq!(textures.values().sum::<usize>(), 144, "144 of 144 rows");
    // And the negative that stays clean: the two column sets are disjoint.
    for g in groups.keys() {
        assert!(table.get(g).is_none(), "{g} is a projectile_trails key after all");
    }

    // The two together: the trail each of the 9 rows plays, and the trail row it draws with.
    for p in db.projectiles.iter().filter(|p| p.trail.is_some()) {
        let row = table.get(p.trail_texture.as_deref().unwrap()).expect("row");
        println!(
            "  {:34} group {:34} trail row {:14} blend {:6} rgba {:?}",
            p.key, p.trail.as_deref().unwrap(), row.key, row.blend, row.colour(),
        );
    }

    // The negative that keeps the six unnamed floats UNKNOWN: one `projectile_trails` row serves
    // projectiles spanning a very wide range of their own properties, so none of its floats can be
    // a per-shot duration, length or rate. `alpha` alone: 89 rows, 4..250 m/s, 50..750 m.
    let alpha: Vec<_> = db.projectiles.iter().filter(|p| p.trail_texture.as_deref() == Some("alpha")).collect();
    assert_eq!(alpha.len(), 89);
    assert_eq!(alpha.iter().map(|p| p.muzzle_velocity).fold(f32::MAX, f32::min), 4.0);
    assert_eq!(alpha.iter().map(|p| p.muzzle_velocity).fold(f32::MIN, f32::max), 250.0);
    assert_eq!(alpha.iter().map(|p| p.effective_range).min(), Some(50));
    assert_eq!(alpha.iter().map(|p| p.effective_range).max(), Some(750));
    for key in ["alpha", "alpha_bullet", "alpha_shrapnel", "e3_rocket", "none"] {
        let rows: Vec<_> =
            db.projectiles.iter().filter(|p| p.trail_texture.as_deref() == Some(key)).collect();
        let vmin = rows.iter().map(|p| p.muzzle_velocity).fold(f32::MAX, f32::min);
        let vmax = rows.iter().map(|p| p.muzzle_velocity).fold(f32::MIN, f32::max);
        println!(
            "  {key:14} {:3} rows  muzzle_velocity {vmin:6.1}..{vmax:6.1}  effective_range {:5}..{:<5} calibre {:?}",
            rows.len(),
            rows.iter().map(|p| p.effective_range).min().unwrap(),
            rows.iter().map(|p| p.effective_range).max().unwrap(),
            rows.iter().map(|p| p.calibre.as_str()).collect::<std::collections::BTreeSet<_>>(),
        );
    }
}