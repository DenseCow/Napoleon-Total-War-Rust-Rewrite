//! Mods seen by the game data layer: synthetic packs and folders in the OS temp folder
//! (nothing is written into the install). The `#[ignore]` tests also need the real install:
//! `cargo test -p ntw_data --test mods -- --ignored --nocapture`.

use std::path::{Path, PathBuf};

use ntw_data::{DbRecord, GameDatabase, UnitRecord, load_table};
use ntw_formats::db::{DbTable, DbValue, FieldType};
use ntw_formats::loc::Localisation;
use ntw_formats::pack::{LayerKind, ModOptions, PackFile, UserScriptSetting, Vfs};

fn temp_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ntw_data_mods_{}_{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A PFH0 pack with the given type and files.
fn pack(pack_type: u32, files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut index = Vec::new();
    for (path, data) in files {
        index.extend_from_slice(&(data.len() as u32).to_le_bytes());
        index.extend_from_slice(path.as_bytes());
        index.push(0);
    }
    let mut b = b"PFH0".to_vec();
    for v in [pack_type, 0, 0, files.len() as u32, index.len() as u32] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&index);
    for (_, data) in files {
        b.extend_from_slice(data);
    }
    b
}

/// A `.loc` file with the given (key, text) entries.
fn loc(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut b = vec![0xFF, 0xFE, b'L', b'O', b'C', 0];
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    let s = |b: &mut Vec<u8>, t: &str| {
        let u: Vec<u16> = t.encode_utf16().collect();
        b.extend_from_slice(&(u.len() as u16).to_le_bytes());
        u.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
    };
    for (k, t) in entries {
        s(&mut b, k);
        s(&mut b, t);
        b.push(0);
    }
    b
}

/// A `units` table (version 4) whose rows have the given (key, dev_name); other columns default.
fn units(rows: &[(&str, &str)]) -> Vec<u8> {
    let schema = UnitRecord::schema();
    let rows = rows
        .iter()
        .map(|(key, name)| {
            schema
                .fields
                .iter()
                .enumerate()
                .map(|(i, f)| match (i, f.ty) {
                    (0, _) => DbValue::Str((*key).into()),
                    (1, _) => DbValue::Str((*name).into()),
                    (_, FieldType::Str) => DbValue::Str(String::new()),
                    (_, FieldType::OptStr) => DbValue::OptStr(None),
                    (_, FieldType::Bool) => DbValue::Bool(false),
                    (_, FieldType::I32) => DbValue::I32(0),
                    (_, FieldType::F32) => DbValue::F32(0.0),
                    (_, FieldType::U16) => DbValue::U16(0),
                })
                .collect()
        })
        .collect();
    DbTable { version: 4, has_version_marker: true, flag: 1, rows }.to_bytes(&schema).unwrap()
}

fn script(text: &str, mods_dir: Option<&Path>) -> ModOptions {
    ModOptions { user_script: UserScriptSetting::Text(text.into()), mods_dir: mods_dir.map(Path::to_path_buf) }
}

fn dev_name(t: &ntw_data::Table<UnitRecord>, key: &str) -> String {
    t.get(key).map(|u| u.dev_name.clone()).unwrap_or_default()
}

/// A synthetic install plus original-style mod packs: one overrides a DB row and one loc
/// string, and the game data layer (`load_table`, `Localisation`) sees it.
#[test]
fn synthetic_mod_overrides_row_and_string() {
    let root = temp_dir("synthetic");
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("data.pack"),
        pack(1, &[("db\\units_tables\\units", units(&[("unit_a", "Vanilla A"), ("unit_b", "Vanilla B")]))]),
    )
    .unwrap();
    std::fs::write(
        data.join("local_en.pack"),
        pack(1, &[("text\\localisation.loc", loc(&[("greeting", "Hello"), ("bye", "Goodbye")]))]),
    )
    .unwrap();
    // Original style 1: the whole table / loc file replaced at the same path.
    std::fs::write(
        data.join("full_mod.pack"),
        pack(
            3,
            &[
                ("db\\units_tables\\units", units(&[("unit_a", "Full A"), ("unit_b", "Full B")])),
                ("text\\localisation.loc", loc(&[("greeting", "Bonjour"), ("bye", "Goodbye")])),
                // The original only opens localisation.loc and ui.loc: this one is ignored.
                ("text\\full_mod.loc", loc(&[("bye", "IGNORED")])),
            ],
        ),
    )
    .unwrap();
    // Style 2: a differently named table file that only carries the changed row (merged).
    std::fs::write(data.join("row_mod.pack"), pack(3, &[("db\\units_tables\\!row_mod", units(&[("unit_b", "Row B")]))]))
        .unwrap();
    // The same, named bob_*: on equal priority its rows replace earlier ones.
    std::fs::write(data.join("bob_mod.pack"), pack(3, &[("db\\units_tables\\bob_rows", units(&[("unit_b", "Bob B")]))]))
        .unwrap();

    // Vanilla: no mod lines, nothing changes.
    let (vfs, _) = Vfs::open_with_mods(&data, &script("", None)).unwrap();
    let t = load_table::<UnitRecord>(&vfs, &mut Vec::new()).unwrap();
    assert_eq!((dev_name(&t, "unit_a"), dev_name(&t, "unit_b")), ("Vanilla A".into(), "Vanilla B".into()));
    assert_eq!(Localisation::from_vfs(&vfs).unwrap().get("greeting"), Some("Hello"));

    // Row replacement: a mod file's row beats the vanilla row with the same key.
    let (vfs, _) = Vfs::open_with_mods(&data, &script("mod row_mod.pack;", None)).unwrap();
    let t = load_table::<UnitRecord>(&vfs, &mut Vec::new()).unwrap();
    assert_eq!((dev_name(&t, "unit_a"), dev_name(&t, "unit_b")), ("Vanilla A".into(), "Row B".into()));
    assert_eq!(t.len(), 2, "the overridden row is not duplicated");

    // Both mods active: same pack type, so the row read first (full_mod's `units`) stays.
    let (vfs, report) = Vfs::open_with_mods(&data, &script("mod \"full_mod.pack\";\nmod \"row_mod.pack\";", None)).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let mut warnings = Vec::new();
    let t = load_table::<UnitRecord>(&vfs, &mut warnings).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(dev_name(&t, "unit_a"), "Full A", "whole-table replacement");
    assert_eq!(dev_name(&t, "unit_b"), "Full B", "equal priority: the first row stays");
    assert_eq!(t.len(), 2);
    // ... unless the later file is named bob_*.
    let (vfs, _) = Vfs::open_with_mods(&data, &script("mod full_mod.pack;\nmod bob_mod.pack;", None)).unwrap();
    let t = load_table::<UnitRecord>(&vfs, &mut Vec::new()).unwrap();
    assert_eq!(dev_name(&t, "unit_b"), "Bob B");
    let text = Localisation::from_vfs(&vfs).unwrap();
    assert_eq!((text.get("greeting"), text.get("bye")), (Some("Bonjour"), Some("Goodbye")));

    // Our mods folder: a loose .loc with only the changed string, and a broken table that is skipped.
    let mods = root.join("mods");
    std::fs::create_dir_all(mods.join("easy").join("text")).unwrap();
    std::fs::create_dir_all(mods.join("easy").join("db").join("units_tables")).unwrap();
    std::fs::write(mods.join("easy").join("text").join("easy.loc"), loc(&[("bye", "Ciao")])).unwrap();
    std::fs::write(mods.join("easy").join("db").join("units_tables").join("broken"), b"not a table").unwrap();
    let (vfs, _) = Vfs::open_with_mods(&data, &script("mod full_mod.pack", Some(&mods))).unwrap();
    assert_eq!(vfs.origin("text/easy.loc").unwrap().kind, LayerKind::ModsFolder);
    let text = Localisation::from_vfs(&vfs).unwrap();
    assert_eq!((text.get("greeting"), text.get("bye")), (Some("Bonjour"), Some("Ciao")));
    let mut warnings = Vec::new();
    let t = load_table::<UnitRecord>(&vfs, &mut warnings).unwrap();
    assert_eq!(dev_name(&t, "unit_a"), "Full A");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("broken"));
}

fn install_data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// With no mods active, the install loads exactly as before mod support: every file that the
/// old packs-only VFS showed comes from the same pack with the same bytes, the new loose layer
/// adds only files no pack has, and the database is identical.
#[test]
#[ignore]
fn vanilla_install_unchanged_without_mods() {
    let data = install_data_dir();
    let (vfs, report) = Vfs::open_with_mods(&data, &ModOptions::vanilla()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let plain = Vfs::open_install(&data).unwrap();
    assert_eq!(vfs.list(""), plain.list(""));

    // The pre-mod-support VFS: the vanilla packs only, in type + name order.
    let packs: Vec<PackFile> = plain.packs().iter().map(|p| PackFile::open(p.path()).unwrap()).collect();
    let old = Vfs::from_packs(packs);
    for path in old.list("") {
        let (a, _) = old.find(path).unwrap();
        let (b, _) = vfs.find(path).unwrap_or_else(|| panic!("{path} no longer comes from a pack"));
        assert_eq!(a.path(), b.path(), "{path}");
    }
    let loose_only = vfs.len() - old.len();
    println!("{} pack paths unchanged, {loose_only} loose-only files added", old.len());
    assert!(vfs.layers().iter().all(|l| !l.kind.is_mod()));

    let a = GameDatabase::from_vfs(&old).unwrap();
    let b = GameDatabase::from_vfs(&vfs).unwrap();
    assert_eq!(a.units, b.units);
    assert_eq!(a.unit_stats_land, b.unit_stats_land);
    assert_eq!(a.projectiles, b.projectiles);
    assert_eq!(a.factions, b.factions);
    assert_eq!(a.regions, b.regions);
    assert_eq!(a.technologies, b.technologies);
    assert_eq!(a.projectile_explosions, b.projectile_explosions);
    assert_eq!(a.projectile_impacts, b.projectile_impacts);
    assert_eq!(a.projectile_trails, b.projectile_trails);
    assert!(!b.projectile_trails.into_rows().is_empty(), "the fx tables load through the merged view");
    assert_eq!(a.kv_rules.table.entries(), b.kv_rules.table.entries());
    assert!(b.load_warnings.is_empty());
    assert_eq!(Localisation::from_vfs(&old).unwrap(), Localisation::from_vfs(&vfs).unwrap());
}

/// The real install plus a synthetic mod (an absolute-path `mod` line to a temp pack): one
/// unit row and one loc string change, and the full `GameDatabase` sees them.
#[test]
#[ignore]
fn install_with_synthetic_mod_pack() {
    let data = install_data_dir();
    let plain = Vfs::open_install(&data).unwrap();
    let vanilla = GameDatabase::from_vfs(&plain).unwrap();
    let key = "Inf_Line_Austrian_German_Fusiliers";

    // A one-row units file made from the vanilla row with a new dev_name.
    let schema = UnitRecord::schema();
    let raw = DbTable::read(&plain.read("db/units_tables/units").unwrap(), &schema).unwrap();
    let mut row = raw.rows.iter().find(|r| r[0].as_str() == Some(key)).unwrap().clone();
    row[1] = DbValue::Str("Modded Fusiliers".into());
    let one = DbTable { rows: vec![row], ..raw }.to_bytes(&schema).unwrap();
    // The full localisation.loc with one string changed (whole-file replacement, as in the original).
    let loc_bytes = plain.read("text/localisation.loc").unwrap();
    let mut file = ntw_formats::loc::LocFile::read(&loc_bytes).unwrap();
    let loc_key = file.entries[0].key.clone();
    file.entries[0].text = "MODDED TEXT".into();
    let entries: Vec<(&str, &str)> = file.entries.iter().map(|e| (e.key.as_str(), e.text.as_str())).collect();

    let root = temp_dir("install_mod");
    let mod_pack = root.join("test_mod.pack");
    std::fs::write(
        &mod_pack,
        pack(3, &[("db\\units_tables\\!test_mod_units", one), ("text\\localisation.loc", loc(&entries))]),
    )
    .unwrap();
    let opts = script(&format!("mod \"{}\";\nmod \"not_installed.pack\";", mod_pack.display()), None);
    let (vfs, report) = Vfs::open_with_mods(&data, &opts).unwrap();
    println!("{}", report.render(&vfs).lines().take(20).collect::<Vec<_>>().join("\n"));
    assert_eq!(report.warnings.len(), 1, "the missing pack is skipped with a warning");
    let modded = GameDatabase::from_vfs(&vfs).unwrap();
    assert_eq!(modded.unit(key).unwrap().dev_name, "Modded Fusiliers");
    assert_eq!(modded.units.len(), vanilla.units.len());
    assert_eq!(modded.unit_stats_land, vanilla.unit_stats_land);
    let text = Localisation::from_vfs(&vfs).unwrap();
    assert_eq!(text.get(&loc_key), Some("MODDED TEXT"));
}
