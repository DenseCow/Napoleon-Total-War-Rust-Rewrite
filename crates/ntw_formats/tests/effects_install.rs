//! The effect database (`effects\landbattle.xml`, `effects\unit_dust_parameters.txt`) against a
//! real install (read-only). `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_formats --test effects_install -- --ignored --nocapture
//! ```
//! See `analysis/graphics/BATTLE_EFFECTS.md`.
use std::path::PathBuf;

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::effects::{CAMPAIGN_MAP_EFFECTS, EffectLibrary, LAND_BATTLE_EFFECTS, ModifierType, NAVAL_BATTLE_EFFECTS};
use ntw_formats::pack::Vfs;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

/// The three dust groups the draw slot uploads are real groups of the shipped `landbattle.xml`, and
/// each one has emitters.
///
/// **Round 4.** `napoleon::battle::fx_draw::every_dust_group_has_emitters` used to check these three
/// names against the two-emitter `SAMPLE_DOC` in the `napoleon` crate's own test module, so it was
/// a statement about a fixture rather than about the game. That is what this is for: the names are
/// CONFIRMED group names of the shipped file, which is the only thing that makes
/// `fx::dust_puff`'s return value drawable.
#[test]
#[ignore]
fn every_dust_group_is_a_shipped_group() {
    let vfs = vfs();
    let lib = EffectLibrary::from_vfs(&vfs).expect("landbattle.xml");
    let dust: [&str; 3] = ["infantry_walk_dust", "cavalry_walk_dust", "infantry_combat_dust"];
    for g in dust {
        assert!(lib.has_group(g), "{g} is not a landbattle group, so the slot uploads nothing for it");
        let n = lib.group_effects(g).len();
        println!("{g}: {n} emitters");
        assert!(n > 0, "{g} has no emitters");
        // Every entry resolves, so the group cannot be half-read.
        for e in lib.groups[g].entries.iter() {
            assert!(lib.effects.contains_key(e), "{g}: unknown effect {e}");
        }
    }
    // Clean negative: they are landbattle groups, not naval ones, so nothing about a ship is drawn
    // from them. (There is no `ship_*` row in the dust table either.)
    let naval = EffectLibrary::from_vfs_path(&vfs, NAVAL_BATTLE_EFFECTS).expect("navalbattle");
    for g in dust {
        assert!(!naval.has_group(g), "{g} is in navalbattle.xml too");
    }
}

/// All 283 emitters and all 152 groups of `landbattle.xml` read, every group entry names an
/// emitter that exists, and the three firing groups are the shipped ones.
#[test]
#[ignore]
fn land_battle_effects_parse() {
    let lib = EffectLibrary::from_vfs(&vfs()).expect("landbattle.xml");
    assert_eq!(lib.effects.len(), 283, "SCRIPTED_EFFECT_INFO count");
    assert_eq!(lib.groups.len(), 152, "SCRIPTED_EFFECT_GROUP count");

    // Every group entry resolves: a clean check over all 514 entries.
    let mut entries = 0usize;
    for g in lib.groups.values() {
        for e in &g.entries {
            assert!(lib.effects.contains_key(e), "group {}: unknown effect {e}", g.name);
            entries += 1;
        }
    }
    assert_eq!(entries, 514, "SCRIPTED_EFFECT_GROUP_ENTRY count");

    // The firing groups, verbatim (CONFIRMED, BATTLE_EFFECTS.md §1.3).
    let want: [(&str, &[&str]); 4] = [
        ("MusketFire", &["musket_1", "musket_2", "musket_3", "flash_dir_tiny", "Uber_smoke_white"]),
        ("rifleFire", &["musket_1", "musket_2", "flash_dir_tiny"]),
        ("pistolFire", &["pistol_1", "pistol_2", "flash_dir_tiny"]),
        ("LandGunFire", &[
            "landgun_med_1", "landgun_med_3", "landgun_fastsmoke", "flash_dir_sml", "explodesmall_glow",
            "cannon_fire_distortion", "sparks_cannon", "cannon_fire_particle_ring_small",
            "landgun_med_2_whispy", "Uber_smoke_white", "landgun_ground_smoke",
        ]),
    ];
    for (name, expected) in want {
        let got: Vec<&str> = lib.groups[name].entries.iter().map(String::as_str).collect();
        assert_eq!(got, expected, "group {name}");
    }
    // CannonFire is LandGunFire with the animated smoke and the sea-coloured plume instead of
    // the sparks and the particle ring.
    assert_eq!(
        lib.groups["CannonFire"].entries,
        vec![
            "landgun_med_1", "landgun_med_3", "landgun_fastsmoke", "flash_dir_sml", "explodesmall_glow",
            "cannon_fire_distortion", "cannon_fire_anim_smoke", "landgun_med_2_whispy", "Uber_smoke_sea",
        ]
    );

    // Every emitter: a texture, a shader, a sane life and a non-zero particle count.
    for fx in lib.effects.values() {
        assert!(!fx.texture().is_empty(), "{}: no texture_1", fx.name);
        assert!(fx.fx.ends_with(".fx"), "{}: fx {:?}", fx.name, fx.fx);
        assert!(fx.num_particles_per_point >= 1, "{}: num_particles_per_point 0", fx.name);
        assert!(fx.life_range.base >= 0.0, "{}: life_range {:?}", fx.name, fx.life_range);
        assert!(fx.frames() >= 1, "{}: total_frames {:?}", fx.name, fx.total_frames);
    }

    // The emission modifiers: 3906 of the 3962 are NONE and inert, so most shipped effects keep
    // their own attribute values. The 37 ADD and 19 MUL rows are the only ones that change
    // anything (CONFIRMED counts, from `analysis/graphics/BATTLE_EFFECTS.md` §1.4).
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for fx in lib.effects.values() {
        for m in [Some(fx.velocity_modifier), Some(fx.lifetime_modifier), Some(fx.opacity_modifier)]
            .into_iter()
            .flatten()
            .chain(fx.scale_modifiers)
            .chain(fx.rotation_modifiers)
            .chain(fx.colour_modifiers.into_iter().flatten())
        {
            *kinds.entry(match m.kind {
                ModifierType::None => "NONE",
                ModifierType::Add => "ADD",
                ModifierType::Mul => "MUL",
            })
            .or_default() += 1;
        }
    }
    assert_eq!(kinds.get("ADD"), Some(&37), "{kinds:?}");
    assert_eq!(kinds.get("MUL"), Some(&19), "{kinds:?}");
    // 283 emitters x 14 modifier elements each = 3962, and 3962 - 37 - 19 = 3906 inert.
    assert_eq!(kinds.get("NONE"), Some(&3906), "{kinds:?}");

    // Every emitter still produces particles: a NONE lifetime modifier must leave `life_range`
    // alone, which is the bug that made the whole system emit nothing.
    for fx in lib.effects.values() {
        assert!(fx.life_seconds(fx.life_range.base) >= fx.life_range.base * 0.999 || fx.lifetime_modifier.kind != ModifierType::None);
    }
    let alive = lib.effects.values().filter(|f| f.life_seconds(f.life_range.base) > 0.0).count();
    println!("{alive} of {} emitters have a positive life", lib.effects.len());
    assert!(alive > 200, "only {alive} emitters would produce a particle");
}

/// The three effect files and the dust table all read, and the dust table is the shipped one.
#[test]
#[ignore]
fn all_effect_files_and_dust_parameters_read() {
    let vfs = vfs();
    let land = EffectLibrary::from_vfs_path(&vfs, LAND_BATTLE_EFFECTS).expect("land");
    let naval = EffectLibrary::from_vfs_path(&vfs, NAVAL_BATTLE_EFFECTS).expect("naval");
    let campaign = EffectLibrary::from_vfs_path(&vfs, CAMPAIGN_MAP_EFFECTS).expect("campaign");
    assert_eq!(land.effects.len(), 283);
    println!("naval: {} effects, {} groups", naval.effects.len(), naval.groups.len());
    println!("campaign: {} effects, {} groups", campaign.effects.len(), campaign.groups.len());
    assert!(naval.effects.len() > 50 && campaign.effects.len() > 10);

    let dust = EffectLibrary::dust_from_vfs(&vfs).expect("unit_dust_parameters.txt");
    // The whole shipped file, CONFIRMED and complete (BATTLE_EFFECTS.md §1.4): 15 rows, four
    // entity families (infantry, cavalry, elephants, artillery) and the walking / running /
    // charging / melee behaviours each artillery row set lacks one of.
    assert_eq!(dust.frequency.len(), 15);
    let want: [(&str, f32); 15] = [
        ("infantry_walking", 0.05), ("infantry_running", 0.10), ("infantry_charging", 0.15), ("infantry_melee", 0.10),
        ("cavalry_walking", 0.05), ("cavalry_running", 0.15), ("cavalry_charging", 0.30), ("cavalry_melee", 0.10),
        ("elephants_walking", 0.15), ("elephants_running", 0.30), ("elephants_charging", 0.45), ("elephants_melee", 0.10),
        ("artillery_walking", 0.1), ("artillery_running", 0.2), ("artillery_melee", 0.2),
    ];
    for (k, v) in want {
        assert_eq!(dust.frequency(k), Some(v), "{k}");
    }
    // Clean negatives: no melee row for walking entities beyond these, and nothing for
    // ships / aircraft families.
    assert_eq!(dust.frequency("artillery_charging"), None);
    assert_eq!(dust.frequency("infantry_idle"), None);
}

/// The 104 group names in `db\particle_effects` are all groups of `landbattle.xml`.
#[test]
#[ignore]
fn particle_effects_table_lists_land_battle_groups() {
    use ntw_formats::db::{DbTable, Schema};
    let vfs = vfs();
    let bytes = vfs.read("db\\particle_effects_tables\\particle_effects").expect("particle_effects");
    let table = DbTable::read(&bytes, &Schema::from_codes("s").expect("schema")).expect("read");
    assert_eq!(table.rows.len(), 104);
    let lib = EffectLibrary::from_vfs(&vfs).expect("landbattle.xml");
    let mut missing = Vec::new();
    for row in &table.rows {
        let Some(ntw_formats::db::DbValue::Str(name)) = row.first() else { continue };
        if !lib.has_group(name) {
            missing.push(name.clone());
        }
    }
    assert!(missing.is_empty(), "particle_effects names not in landbattle.xml: {missing:?}");
}

/// Every UTF-16 string the three projectile effect tables write, checked against the shipped
/// library. This is the kept evidence for BATTLE_EFFECTS.md §2: it does **not** need the tables'
/// column layouts (which are still UNKNOWN), only that the names they carry are real effect groups.
///
/// The two things it pins down:
/// - every name in `projectiles_explosions` / `projectile_impacts` that is an effect at all is a
///   `SCRIPTED_EFFECT_GROUP` of `effects\landbattle.xml` (the rest are `projectiles` keys such as
///   `shell_12lb`, `fuse`, `shockwave`, and column-ish words such as `medium`);
/// - the 22 `gun_type_to_projectiles.muzzle_flash` names are **none** of them a group, a group
///   entry or an emitter, in any of the three shipped effect files (a clean negative).
#[test]
#[ignore]
fn projectile_effect_names_are_land_battle_groups() {
    use std::collections::BTreeSet;

    let vfs = vfs();
    let lib = EffectLibrary::from_vfs(&vfs).expect("landbattle.xml");
    let naval = EffectLibrary::from_vfs_path(&vfs, NAVAL_BATTLE_EFFECTS).expect("navalbattle.xml");
    let entries: BTreeSet<String> = lib.groups.values().flat_map(|g| g.entries.iter()).cloned().collect();

    // Every UTF-16 string in a DB file, found by scanning the bytes for a length-prefixed run.
    let strings = |bytes: &[u8]| -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut i = 0usize;
        while i + 2 <= bytes.len() {
            let n = u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
            if (2..=64).contains(&n) && i + 2 + n * 2 <= bytes.len() {
                let units: Vec<u16> = bytes[i + 2..i + 2 + n * 2]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                if let Ok(s) = String::from_utf16(&units)
                    && s.chars().all(|c| c.is_ascii_graphic() || c == ' ')
                {
                    out.insert(s.trim().to_string());
                }
            }
            i += 1;
        }
        out
    };

    let explosions = strings(&vfs.read("db\\projectiles_explosions_tables\\projectiles_explosions").expect("explosions"));
    let impacts = strings(&vfs.read("db\\projectile_impacts_tables\\projectile_impacts").expect("impacts"));
    let both: BTreeSet<&str> = explosions.iter().map(String::as_str).chain(impacts.iter().map(String::as_str)).collect();
    println!("projectiles_explosions: {} distinct strings, projectile_impacts: {}", explosions.len(), impacts.len());
    assert!(explosions.len() >= 20 && impacts.len() >= 20, "{explosions:?} {impacts:?}");
    // The air bursts and ground scorches we play are named in the shipped bytes, and every one of
    // them is a `SCRIPTED_EFFECT_GROUP` of `effects\landbattle.xml`.
    for want in [
        "AirExplosion_sml", "AirExplosion_med", "AirExplosion_lrg", "Cannon_Groundimpact_gen_sml",
        "Cannon_Groundimpact_gen_med", "Cannon_Groundimpact_explosive", "blood_spray", "blood_gen",
        "Musket_impact_hard", "Musket_impact_soft",
    ] {
        assert!(both.contains(want), "{want} not in the shipped tables");
        assert!(lib.has_group(want), "{want} is not a landbattle group");
    }
    // Clean negative: the tables never name a single *emitter*, only groups, so the engine plays
    // groups here too. (The other strings are `projectiles` keys: `shell_12lb`, `fuse`,
    // `shockwave`, `percussive`, `medium`, ...)
    let emitters: Vec<&&str> = both.iter().filter(|n| lib.effects.contains_key(**n)).collect();
    assert!(emitters.is_empty(), "the tables name emitters: {emitters:?}");
    println!("{} strings scanned, {} of them landbattle groups", both.len(), both.iter().filter(|n| lib.has_group(n)).count());

    // Clean negative: the muzzle-flash column never names anything we can play.
    let gtp = strings(&vfs.read("db\\gun_type_to_projectiles_tables\\gun_type_to_projectiles").expect("gtp"));
    let flashes: Vec<&str> = gtp.iter().map(String::as_str).filter(|n| n.contains("muzzle_flash")).collect();
    assert_eq!(flashes.len(), 22, "22 distinct muzzle_flash names over the 153 rows");
    for f in flashes {
        assert!(!lib.has_group(f) && !lib.effects.contains_key(f) && !entries.contains(f), "{f} resolves after all");
        assert!(!naval.has_group(f) && !naval.effects.contains_key(f), "{f} is in navalbattle.xml");
    }
    // CONFIRMED NEGATIVE, whole pack: `muzzle_flash` is in two shipped files only — the
    // `gun_type_to_projectiles` column itself, and one unrelated `warscape_rigid` node called
    // `muzzle_flash`. It is in no effect file, no gun model and no animation file, so the flash
    // the column names is built in the exe (BATTLE_EFFECTS.md §2 item 1).
    let needle: Vec<u8> = "muzzle_flash".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut files_with: Vec<String> = Vec::new();
    let mut paths: Vec<String> = vfs.list("").into_iter().map(str::to_string).collect();
    paths.sort();
    for p in &paths {
        let Ok(bytes) = vfs.read(p) else { continue };
        if bytes.windows(needle.len()).any(|w| w == needle.as_slice()) {
            files_with.push(p.clone());
        }
    }
    assert_eq!(
        files_with,
        vec![
            "db\\gun_type_to_projectiles_tables\\gun_type_to_projectiles".to_string(),
            "db\\warscape_rigid_tables\\warscape_rigid".to_string(),
        ],
        "the muzzle_flash name is in {files_with:?}"
    );
}

/// **`db\projectile_trails`, read whole.** The table is `ssffffffffff` and exactly regular — key,
/// blend mode, ten floats, next key — so `TrailTable` cuts it with no schema guess and checks the
/// row count against the header.
///
/// This is the right-hand side of the trail bridge that round 2 left UNKNOWN. Column 32 of
/// `projectiles` (`trail`) is **not** what reaches this table: it names the trail's *effect group*
/// in `effects\landbattle.xml` (`shrapnel_trail`, `congreve_rocket`, ...). `projectiles`' **column
/// 6** (`trail_texture`) is the foreign key. The other half of that proof, over the 144
/// `projectiles` rows, is `ntw_data/tests/effects_data.rs::the_trail_group_and_the_trail_table_are_joined_by_column_six`.
///
/// What it pins down here, on the shipped bytes:
/// - 5 rows, the second string is the **blend mode** (`alpha` / `add` / `none`);
/// - floats **4-7 are an 8-bit RGBA quadruple**: white at alpha 128 on three rows, grey at
///   alpha 100 on the rocket, all zero on `none` — the only four values in the table that share an
///   8-bit range, and the only run of three equal values;
/// - float 10 is the constant 50 on all four live rows.
///
/// The other six floats stay **UNKNOWN**, and the reason is in the `ntw_data` half: one row serves
/// projectiles spanning a 62x muzzle-velocity range, so no float in a row can be a per-shot
/// duration or length.
#[test]
#[ignore]
fn the_projectile_trails_table_reads_whole() {
    use ntw_formats::projectile_fx::{TrailTable, PROJECTILE_TRAILS};

    let table =
        TrailTable::read(&vfs().read(PROJECTILE_TRAILS).expect("projectile_trails")).expect("read");
    assert_eq!(table.len(), 5, "the shipped table has 5 rows");

    // The ten floats of every row, verbatim: this is the whole table.
    let want: [(&str, &str, [f32; 10]); 5] = [
        ("alpha", "alpha", [0.2, 0.1, 50.0, 255.0, 255.0, 255.0, 128.0, 300.0, 30.0, 50.0]),
        ("alpha_bullet", "alpha", [0.1, 0.05, 50.0, 255.0, 255.0, 255.0, 128.0, 300.0, 25.0, 50.0]),
        ("alpha_shrapnel", "add", [0.05, 0.05, 25.0, 255.0, 255.0, 255.0, 128.0, 300.0, 25.0, 50.0]),
        ("e3_rocket", "alpha", [2.0, 2.0, 500.0, 125.0, 125.0, 125.0, 100.0, 1000.0, 100.0, 50.0]),
        ("none", "none", [0.0; 10]),
    ];
    for (key, blend, floats) in want {
        let row = table.get(key).unwrap_or_else(|| panic!("no {key} row"));
        assert_eq!(row.floats, floats, "{key}: the ten floats, verbatim");
        assert_eq!(row.blend, blend, "{key}: the second string is the blend mode");
    }
    for (key, colour) in [
        ("alpha", [255u8, 255, 255, 128]),
        ("alpha_bullet", [255, 255, 255, 128]),
        ("alpha_shrapnel", [255, 255, 255, 128]),
        ("e3_rocket", [125, 125, 125, 100]),
        ("none", [0, 0, 0, 0]),
    ] {
        assert_eq!(table.get(key).unwrap().colour(), colour, "{key}: floats 4-7 are RGBA");
    }
    assert!(!table.get("none").unwrap().is_visible());
    // Float 10 is 50 on every live row and 0 on `none`, so it discriminates nothing.
    for key in ["alpha", "alpha_bullet", "alpha_shrapnel", "e3_rocket"] {
        assert_eq!(table.get(key).unwrap().floats[9], 50.0, "{key}");
    }
    // And the six unnamed floats come back in file order.
    assert_eq!(
        table.get("e3_rocket").unwrap().numbers(),
        [2.0, 2.0, 500.0, 1000.0, 100.0, 50.0]
    );
}

/// **The muzzle attachment point does not exist in the shipped gun models.** This is the negative
/// that keeps `battle::fx_draw::MUZZLE_HEIGHT` PROVISIONAL, and it is exhaustive over three
/// independent places the attachment could have been:
///
/// 1. **The whole pack.** `muzzle` is a substring of only **5** of the 86,977 entries, and none of
///    them is a gun model: `db\gun_type_to_projectiles`, `db\warscape_rigid_lod`,
///    `db\warscape_rigid`, `rigidmodels\projectile\cannon_muzzle01.rigid_model` (the shot's own
///    impact model) and `text\localisation.loc`.
/// 2. **The gun models themselves.** All **158** files under `enginemodels\` whose name is a gun
///    (cannon / howitzer / mortar / carronade / rocket) hold **8,588** strings between them, and
///    every one is a texture name, a texture path, a `building_NNN` destruction clip or one of the
///    15 named shader constants. There is no node name, so there is no attachment point.
/// 3. **The gun table.** `db\models_artilleries_tables\models_artillery` has one row per gun model
///    with a 683-byte numeric block after it — but the block is **byte-identical across all 34
///    rows**, so it is the shared field-cannon rig, not a per-gun muzzle.
///
/// Together these say the original computes the muzzle, it does not read it off a shipped file,
/// which is a 0-A / exe-side question. `MUZZLE_HEIGHT` therefore stays PROVISIONAL with a named
/// target, and this test is what a future round should re-run before looking for it again.
#[test]
#[ignore]
fn no_shipped_gun_model_carries_a_muzzle_attachment_point() {
    let vfs = vfs();

    // (1) The whole pack, `muzzle` in UTF-16.
    let needle: Vec<u8> = "muzzle".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut paths: Vec<String> = vfs.list("").into_iter().map(str::to_string).collect();
    paths.sort();
    assert_eq!(paths.len(), 86_977, "the shipped data.pack entry count");
    let mut files_with: Vec<String> = Vec::new();
    for p in &paths {
        let Ok(bytes) = vfs.read(p) else { continue };
        if bytes.windows(needle.len()).any(|w| w == needle.as_slice()) {
            files_with.push(p.clone());
        }
    }
    assert_eq!(
        files_with,
        vec![
            "db\\gun_type_to_projectiles_tables\\gun_type_to_projectiles".to_string(),
            "db\\warscape_rigid_lod_tables\\warscape_rigid_lod".to_string(),
            "db\\warscape_rigid_tables\\warscape_rigid".to_string(),
            "rigidmodels\\projectile\\cannon_muzzle01.rigid_model".to_string(),
            "text\\localisation.loc".to_string(),
        ],
        "`muzzle` is in {files_with:?}"
    );
    // Not one of them is a gun model file.
    for p in &files_with {
        assert!(
            !p.starts_with("enginemodels\\"),
            "a gun model names a muzzle after all: {p}"
        );
    }

    // (2) Every string in every gun model file, classified. A named attachment point would be a
    // string that is none of these.
    let gun_files: Vec<String> = paths
        .iter()
        .filter(|p| {
            p.starts_with("enginemodels\\")
                && ["cannon", "howitzer", "mortar", "carronade", "rocket"]
                    .iter()
                    .any(|w| p.contains(w))
        })
        .cloned()
        .collect();
    assert_eq!(gun_files.len(), 158, "the shipped gun model files");
    let strings = |bytes: &[u8]| -> Vec<String> {
        let mut out = Vec::new();
        let mut i = 0usize;
        while i + 2 <= bytes.len() {
            let n = u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
            if (4..=128).contains(&n) && i + 2 + n * 2 <= bytes.len() {
                let units: Vec<u16> = bytes[i + 2..i + 2 + n * 2]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                if let Ok(s) = String::from_utf16(&units)
                    && s.chars().all(|c| c.is_ascii_graphic() || c == ' ')
                {
                    out.push(s.trim().to_string());
                }
            }
            i += 1;
        }
        out
    };
    let mut total = 0usize;
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for p in &gun_files {
        let bytes = vfs.read(p).expect("gun model");
        for s in strings(&bytes) {
            total += 1;
            let lower = s.to_ascii_lowercase();
            let is_texture = ["_diffuse", "_diffuse0", "_normal", "_normal0", "_gloss_map", "_gloss_map0"]
                .iter()
                .any(|suffix| lower.ends_with(suffix));
            let kind: String = if is_texture {
                "texture".into()
            } else if s.starts_with("building_") {
                "destruction clip".into()
            } else if s.contains('\\') || s.contains('/') {
                "texture path".into()
            } else if s.len() >= 4
                && s.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
            {
                "shader constant".into()
            } else {
                // Anything left is the thing this test is looking for.
                format!("UNCLASSIFIED {s}")
            };
            kinds.insert(kind);
        }
    }
    println!("{total} strings over {} gun model files", gun_files.len());
    let unclassified: Vec<&String> = kinds.iter().filter(|k| k.starts_with("UNCLASSIFIED")).collect();
    println!("kinds: {kinds:?}");
    assert_eq!(
        unclassified.len(),
        0,
        "gun model strings that are not a texture, a path, a clip or a shader constant: {unclassified:?}"
    );

    // (3) The gun table's numeric block is the same on every row, so it is not a per-gun muzzle.
    let bytes = vfs.read("db\\models_artilleries_tables\\models_artillery").expect("models_artillery");
    let starts: Vec<usize> = (0..bytes.len().saturating_sub(2))
        .filter(|i| {
            let n = u16::from_le_bytes([bytes[*i], bytes[*i + 1]]) as usize;
            (10..=64).contains(&n) && *i + 2 + n * 2 <= bytes.len() && {
                let units: Vec<u16> = bytes[*i + 2..*i + 2 + n * 2]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                String::from_utf16(&units).is_ok_and(|s| s.starts_with("model_artillery"))
            }
        })
        .collect();
    assert_eq!(starts.len(), 34, "the 34 shipped gun model rows");
    let mut hashes: BTreeSet<u64> = BTreeSet::new();
    // The block is the 683 bytes at the end of each row, after that row's key and three model
    // paths (the row keys and paths differ, the block does not).
    const BLOCK: usize = 683;
    for (n, &at) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(bytes.len());
        assert!(end - at > BLOCK, "row {n} is shorter than the block");
        let mut h: u64 = 1469598103934665603;
        for b in &bytes[end - BLOCK..end] {
            h ^= *b as u64;
            h = h.wrapping_mul(1099511628211);
        }
        hashes.insert(h);
    }
    assert_eq!(hashes.len(), 1, "the gun rows do NOT share one numeric block after all");
}

/// **Every `sprite_facing_mode` in the three shipped effect files is one the reader names**, and the
/// tally is the whole of what the data says about sprite facing.
///
/// Two things this pins down for `battle::fx_draw`:
/// - `VELOCITY_FACING` is a **clean negative**: no shipped effect file writes it, over all 435
///   emitters, so nothing should draw with it.
/// - The three non-camera modes are all in use: **22 of the 283 land-battle emitters** are not
///   camera-facing (13 `BILLBOARD`, 8 `LOCAL_Y_AXIS`, 1 `WORLD_Y_AXIS`). They are the earth, debris,
///   water and distortion sprites, and every cannon ground-impact group the slice plays carries at
///   least one of them (asserted at the end). Only the `*_Y_AXIS` ones are drawn upright;
///   `BILLBOARD` is drawn like `CAMERA_FACING` (see the next test).
#[test]
#[ignore]
fn every_sprite_facing_mode_is_a_named_one() {
    use ntw_formats::effects::{EffectLibrary, FacingMode, CAMPAIGN_MAP_EFFECTS, LAND_BATTLE_EFFECTS, NAVAL_BATTLE_EFFECTS};

    /// One effect file's expected facing tally: (path, label, [(mode, emitters)]).
    type FacingTally = (&'static str, &'static str, [(FacingMode, usize); 4]);

    let vfs = vfs();
    // CONFIRMED over the three files: 435 emitters, four distinct modes, none of them unnamed.
    let want: [FacingTally; 3] = [
        (LAND_BATTLE_EFFECTS, "landbattle", [
            (FacingMode::Camera, 261), (FacingMode::Billboard, 13),
            (FacingMode::LocalYAxis, 8), (FacingMode::WorldYAxis, 1),
        ]),
        (NAVAL_BATTLE_EFFECTS, "navalbattle", [
            (FacingMode::Camera, 121), (FacingMode::Billboard, 5),
            (FacingMode::LocalYAxis, 6), (FacingMode::WorldYAxis, 1),
        ]),
        (CAMPAIGN_MAP_EFFECTS, "campaignmap", [
            (FacingMode::Camera, 19), (FacingMode::Billboard, 0),
            (FacingMode::LocalYAxis, 0), (FacingMode::WorldYAxis, 0),
        ]),
    ];
    for (path, label, modes) in want {
        let lib = EffectLibrary::from_vfs_path(&vfs, path).expect(path);
        for (mode, n) in modes {
            let got = lib.effects.values().filter(|f| f.sprite_facing == mode).count();
            assert_eq!(got, n, "{label}: {} emitters", mode.as_shipped());
        }
        // Nothing slipped through to `Other`, and nothing uses velocity facing.
        assert_eq!(
            lib.effects.values().filter(|f| f.sprite_facing == FacingMode::Other).count(),
            0,
            "{label}: an unnamed sprite_facing_mode"
        );
        assert_eq!(
            lib.effects.values().filter(|f| f.sprite_facing == FacingMode::Velocity).count(),
            0,
            "{label}: VELOCITY_FACING is a clean negative, it is never shipped"
        );
        // And `align_to_velocity` is false on every one of them, so the sprite never turns with its
        // own movement either.
        assert_eq!(
            lib.effects.values().filter(|f| f.align_to_velocity).count(),
            0,
            "{label}: an emitter aligns to velocity"
        );
    }

    // The non-camera emitters are the ground and water ones, and the ground-impact groups the slice
    // plays are among the groups that reach them.
    let lib = EffectLibrary::from_vfs_path(&vfs, LAND_BATTLE_EFFECTS).expect("landbattle");
    for want_emitter in [
        "cannon_hit_smoke", "cannon_hit_smoke_small", "explosion", "dust_burst", "fougasse_earth",
        "impact_cannon_earth_sml1", "impact_cannon_earth_med1", "impact_cannon_earth_lrg1",
        "impact_water_sml2", "impact_water_med2", "impact_water_lrg2",
        "building_destroydust2", "building_destroydust_med2",
    ] {
        let fx = lib.effects.get(want_emitter).unwrap_or_else(|| panic!("no {want_emitter}"));
        assert_eq!(fx.sprite_facing, FacingMode::Billboard, "{want_emitter}");
        assert!(!fx.sprite_facing.is_y_axis(), "{want_emitter}");
    }
    for want_emitter in [
        "ground_impact_distortion", "ground_impact_distortion_small", "ripple_distortion",
        "ripple_distortion_small", "ripple_distortion_large", "ripple_distortion_sinking",
        "ripple_distortion_drops_small", "shock_distortion",
    ] {
        let fx = lib.effects.get(want_emitter).unwrap_or_else(|| panic!("no {want_emitter}"));
        assert_eq!(fx.sprite_facing, FacingMode::LocalYAxis, "{want_emitter}");
        assert!(fx.sprite_facing.is_y_axis(), "{want_emitter} should stand upright");
    }
    assert_eq!(lib.effects["shockwave_large"].sprite_facing, FacingMode::WorldYAxis);
    assert!(lib.effects["shockwave_large"].sprite_facing.is_y_axis());

    // Every ground-impact group the slice plays on a shell's landing carries at least one of them.
    for group in [
        "Cannon_Groundimpact_explosive",
        "Cannon_Groundimpact_gen_sml",
        "Cannon_Groundimpact_gen_med",
        "Cannon_Groundimpact_gen_lrg",
    ] {
        let g = lib.groups.get(group).unwrap_or_else(|| panic!("no {group}"));
        let odd = g.entries.iter().filter(|e| {
            lib.effects.get(*e).is_some_and(|f| f.sprite_facing != FacingMode::Camera)
        });
        println!("{group}: {} of {} entries are not camera facing", odd.clone().count(), g.entries.len());
        assert!(odd.count() > 0, "{group} has no upright emitter at all");
    }
}

/// **`BILLBOARD` is indistinguishable from `CAMERA_FACING`, and this is why: the shipped shader
/// decides, and it is handed only one basis.** Round 3 drew the two alike and left the reading
/// INFERRED. This closes it as a **negative**, from the shader's own source, which ships as plain
/// HLSL text (`fx\particle.fx` is 4,246 bytes of readable source, not compiled bytecode).
///
/// What the shader is given, in `fx\particle_volumetric.fx_fragment` (the file that defines
/// `particle_vertex_30`, which both `particle.fx` and `particle_distortion.fx` compile):
///
/// ```text
/// // The following 3 vectors describe the axis of a billboard aligned with the camera
/// const float3  g_camera_aligned_x_axis;
/// const float3  g_camera_aligned_y_axis;
/// const float3  g_camera_aligned_z_axis;
/// ```
///
/// and the quad is built from two of them and nothing else:
///
/// ```text
/// current_pos += g_camera_aligned_x_axis * xy.x + g_camera_aligned_y_axis * xy.y;
/// ```
///
/// Four consequences, each one a check below:
///
/// 1. **The shader has no facing parameter.** No `sprite_facing_mode`, no `BILLBOARD`, no
///    `facing` token anywhere in the particle shaders. The basis is chosen **engine-side**, from the
///    particle's vertex stream, so the mode has to be interpreted by the exe and not by the shader.
/// 2. **There is only ever one basis.** A `LOCAL_Y_AXIS` sprite, which stands upright, would need a
///    second basis that the shader is never given. So the exe must be rewriting the vertex data, and
///    it cannot do that differently for two modes that both arrive as camera-aligned axes.
/// 3. **`align_to_velocity` is commented out in the shader**, which is the second, independent
///    confirmation of round 3's negative: the branch is present and disabled.
/// 4. **`particle_distortion.fx` — the shader every one of the 14 `*_Y_AXIS` emitters uses — compiles
///    the very same `particle_vertex_30()`.** So `LOCAL_Y_AXIS` and `BILLBOARD` run through identical
///    shader code, and any difference between them is entirely in what the exe puts in the buffer.
///
/// That is why `BILLBOARD` is drawn like `CAMERA_FACING`: not as an assumption about the name, but
/// because the file and the shader together leave no channel through which a difference could arrive.
/// It stays **UNKNOWN** what the original does differently, and the target for that is now named:
/// the exe's own basis choice for `sprite_facing_mode`.
#[test]
#[ignore]
fn billboard_is_indistinguishable_from_camera_facing_in_the_shipped_shader() {
    use ntw_formats::effects::FacingMode;
    let vfs = vfs();
    let lib = EffectLibrary::from_vfs_path(&vfs, LAND_BATTLE_EFFECTS).expect("landbattle");

    // (0) The emitters exist and the split is the shipped one, so the rest of this test is about
    // real emitters.
    let billboards: Vec<&str> = lib
        .effects
        .values()
        .filter(|f| f.sprite_facing == FacingMode::Billboard)
        .map(|f| f.name.as_str())
        .collect();
    let at = |name: &str| -> &ntw_formats::effects::Effect { &lib.effects[name] };
    assert_eq!(billboards.len(), 13, "the 13 landbattle BILLBOARD emitters");
    let camera: Vec<&str> = lib
        .effects
        .values()
        .filter(|f| f.sprite_facing == FacingMode::Camera)
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(camera.len(), 261, "the 261 landbattle CAMERA_FACING emitters");

    // (1) No attribute the reader keeps separates the two modes. Every value a BILLBOARD emitter
    // carries also occurs on some CAMERA_FACING emitter, so none of them can be what the renderer
    // keys on. `align_to_velocity` is `false` on all of them, which is the clean negative that
    // removes the one attribute that sounds as if it would.
    //
    // Printed as a table rather than asserted pairwise, so a failure says which attribute is the
    // one that broke the tie.
    // (1) No attribute the reader keeps **that could select a basis** separates the two modes. The
    // basis is a property of how the quad is built, so only the geometry/emission-side attributes
    // can carry it; `lighting` and `thickness` are per-vertex shading terms (the shader reads them
    // out of `fade_info_thickness_and_lighting` and passes them to the pixel shader) and are
    // recorded below rather than asserted, so that a new one showing up is visible without failing
    // on a term that provably cannot change a vertex's position.
    //
    // Printed as a table so a failure says which attribute broke the tie.
    let basis_attribute = |f: &ntw_formats::effects::Effect| -> Vec<String> {
        vec![
            format!("fx={}", f.fx),
            format!("render_method={:?}", f.render_method),
            format!("adjust_direction_by_offset={}", f.adjust_direction_by_offset),
            format!("align_to_velocity={}", f.align_to_velocity),
            format!("release_type={}", f.release_type),
            format!("quality_level={}", f.quality_level),
            format!("start_channels_linked={:?}", f.start_channels_linked),
        ]
    };
    let camera_attributes: std::collections::BTreeSet<String> = camera
        .iter()
        .flat_map(|n| basis_attribute(at(n)))
        .collect();
    for name in &billboards {
        for value in basis_attribute(at(name)) {
            assert!(
                camera_attributes.contains(&value),
                "{name}: {value} occurs under no CAMERA_FACING emitter, so it could carry the difference"
            );
        }
    }
    println!(
        "{} basis-side attributes, none of which separates the 13 BILLBOARD from the 261 CAMERA_FACING emitters",
        basis_attribute(at(billboards[0])).len()
    );
    // The shading terms, for the record: which of them a BILLBOARD emitter shares and which it does
    // not. `cannon_hit_smoke` has a `lighting` of its own, which is a shading choice and not a
    // basis, so it is expected to appear here and is the reason the assertion above leaves
    // `lighting` out.
    let mut shading_only: Vec<String> = Vec::new();
    for name in &billboards {
        let f = at(name);
        for value in [format!("lighting={}", f.lighting.base), format!("thickness={}", f.thickness.base)] {
            let under_camera = camera.iter().any(|n| {
                let g = at(n);
                value == format!("lighting={}", g.lighting.base) || value == format!("thickness={}", g.thickness.base)
            });
            if !under_camera {
                shading_only.push(format!("{name}: {value}"));
            }
        }
    }
    println!("shading terms unique to a BILLBOARD emitter (not a basis, so not asserted): {shading_only:?}");

    // (2) The shader is handed one camera-aligned basis and no facing parameter.
    let src = String::from_utf8(vfs.read("fx\\particle_volumetric.fx_fragment").expect("shader"))
        .expect("fx\\particle_volumetric.fx_fragment is ASCII");
    assert!(src.contains("g_camera_aligned_x_axis") && src.contains("g_camera_aligned_y_axis"));
    // The quad corner offset uses those two and nothing else.
    assert!(
        src.contains("g_camera_aligned_x_axis * xy.x + g_camera_aligned_y_axis * xy.y"),
        "the quad is not built from the two camera-aligned axes"
    );
    // No facing parameter reaches the shader, in any of the particle shaders.
    for path in [
        "fx\\particle.fx",
        "fx\\particle_distortion.fx",
        "fx\\particle_volumetric.fx_fragment",
        "fx\\ribbon.fx",
    ] {
        let text = String::from_utf8(vfs.read(path).expect(path)).expect(path);
        for token in ["sprite_facing", "SPRITE_FACING", "BILLBOARD", "facing_mode"] {
            assert!(!text.contains(token), "{path} mentions {token}, so the shader is told the mode");
        }
    }

    // (3) The `align_to_velocity` branch is present and commented out — the second, independent
    // confirmation of round 3's negative, this time from the shader rather than from the data. The
    // shipped text opens with `/*if (g_align_to_velocity)` and closes with `}*/` on the line before
    // the unconditional offset, so the whole branch is inert.
    let align_at = src.find("/*if (g_align_to_velocity)").unwrap_or_else(|| panic!("the shader has no g_align_to_velocity branch at all"));
    // The file is CRLF, so the block closes on `}else*/` rather than a bare `}*/`.
    let close_at = src[align_at..].find("}else*/").map(|i| align_at + i).unwrap_or_else(|| panic!("the g_align_to_velocity branch is not commented out"));
    let inert = &src[align_at..close_at];
    assert!(inert.contains("g_camera_aligned_x_axis,velocity_vector"), "unexpected branch text");
    assert!(
        src[close_at..].trim_start().trim_start_matches("}else*/").trim_start().starts_with("current_pos"),
        "the commented block does not end where the live offset begins"
    );
    // And the live path really is unconditional: the only offset left is the two camera-aligned axes.
    let live_at = src.rfind("current_pos\t+= g_camera_aligned_x_axis").expect("the live offset");
    assert!(src[live_at..].contains("xy.x + g_camera_aligned_y_axis * xy.y"));

    // (4) The shader the 14 `*_Y_AXIS` emitters use compiles the same vertex shader, so the upright
    // modes and `BILLBOARD` run through identical code.
    let distortion = String::from_utf8(vfs.read("fx\\particle_distortion.fx").expect("distortion")).expect("distortion");
    assert!(
        distortion.contains("particle_vertex_30()"),
        "particle_distortion.fx does not use particle_vertex_30"
    );
    let particle = String::from_utf8(vfs.read("fx\\particle.fx").expect("particle")).expect("particle");
    assert!(particle.contains("particle_vertex_30()"), "particle.fx does not use particle_vertex_30");
    // And every BILLBOARD emitter uses `particle.fx`, so both modes reach the same vertex shader.
    for name in &billboards {
        assert_eq!(at(name).fx, "particle.fx", "{name} does not use particle.fx");
    }
    println!("particle.fx and particle_distortion.fx both compile particle_vertex_30(): one basis, one shader");
}
