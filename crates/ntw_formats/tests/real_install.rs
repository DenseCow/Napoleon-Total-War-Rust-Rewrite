//! Integration tests against a real Napoleon: Total War install.
//!
//! All of them are `#[ignore]`d because they need the game installed. Run with:
//! ```text
//! cargo test -p ntw_formats -- --ignored
//! ```
//! Everything here only **reads** the install. Round-trips are done in memory.
//! The install path can be overridden with the `NTW_DATA_DIR` environment variable.

use std::path::{Path, PathBuf};

use ntw_formats::db::{DbHeader, DbTable, Schema};
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_formats::loc::{LocFile, Localisation};
use ntw_formats::pack::{PackFile, PackType, Vfs};
use ntw_formats::unit_variant::{UnitVariant, VariantPartMesh, VariantPartMeshBody, VariantVertexFormat};

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR))
}

/// Recursively collects files with the given extension.
fn find_files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            find_files(&path, ext, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            out.push(path);
        }
    }
}

fn pack_paths() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(data_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "pack"))
        .collect();
    v.sort();
    v
}

/// Every loose `.esf` parses, its root ends exactly at `names_offset` (checked by the
/// reader), and writing it back gives identical bytes.
#[test]
#[ignore]
fn every_loose_esf_parses_and_round_trips() {
    let mut files = Vec::new();
    find_files(&data_dir(), "esf", &mut files);
    assert!(files.len() >= 33, "expected the 33 shipped .esf files, found {}", files.len());
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        let esf = EsfFile::from_bytes(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let names_offset = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        assert_eq!(esf.header.names_offset, names_offset);
        let written = esf.to_bytes().unwrap();
        assert!(written == bytes, "{}: round-trip differs", path.display());
    }
    println!("{} loose .esf files parsed and round-tripped", files.len());
}

/// The European startpos: structure checks from the Worker 3 report, plus a round trip.
#[test]
#[ignore]
fn eur_startpos_structure_and_round_trip() {
    let path = data_dir().join(r"campaigns\eur_napoleon\startpos.esf");
    let bytes = std::fs::read(path).unwrap();
    let esf = EsfFile::from_bytes(&bytes).unwrap();
    assert_eq!((esf.root.name.as_str(), esf.root.version), ("CAMPAIGN_STARTPOS", 5));
    let model = esf.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL").unwrap();
    assert_eq!(model.version, 10);
    let world = esf.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let factions = world.record_array("FACTION_ARRAY").unwrap();
    assert_eq!(factions.items.len(), 41, "W3 §3.3: 41 FACTION records");
    let first = esf.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY[0]/FACTION").unwrap();
    assert_eq!(first.version, 18);

    // The u32 after CAMPAIGN_MAP_DATA is the regions.esf timestamp (W3 §3).
    assert!(matches!(model.get(1), Some(EsfNode::U32(1_260_299_822))));
    let regions = EsfFile::open(data_dir().join(r"campaign_maps\nap_europe\regions.esf")).unwrap();
    assert_eq!(regions.header.timestamp, 1_260_299_822);

    // Count characters and regions anywhere in the tree.
    let (mut characters, mut regions_n) = (0, 0);
    esf.root.walk(&mut |r| match r.name.as_str() {
        "CHARACTER" => characters += 1,
        "REGION" => regions_n += 1,
        _ => {}
    });
    println!("eur startpos: {characters} CHARACTER records, {regions_n} REGION records");
    assert_eq!(esf.to_bytes().unwrap(), bytes);
}

/// Every pack index opens and the last entry ends exactly at the end of the file.
#[test]
#[ignore]
fn every_pack_index_opens() {
    let paths = pack_paths();
    assert_eq!(paths.len(), 11, "W2 lists 11 shipped packs");
    let mut total = 0;
    for path in &paths {
        let pack = PackFile::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let end = pack.entries().last().map_or(pack.header().data_start, |e| e.offset + u64::from(e.size));
        assert_eq!(end, pack.file_len(), "{}", path.display());
        assert!(pack.header().dependencies.is_empty());
        total += pack.entries().len();
        println!("{:<24} {:?} {} entries", path.file_name().unwrap().to_string_lossy(), pack.pack_type(), pack.entries().len());
    }
    assert_eq!(total, 3627 + 19 + 11058 + 19235 + 1852 + 112 + 74 + 4756 + 40635 + 5551 + 76);
}

/// A known entry: `units` is version 4 with 442 rows (W2 DB_FORMAT_NOTES).
#[test]
#[ignore]
fn read_known_entry_units_table() {
    let pack = PackFile::open(data_dir().join("data.pack")).unwrap();
    assert_eq!(pack.pack_type(), PackType::Release);
    let entry = pack.find("db/units_tables/units").unwrap();
    let bytes = pack.read_entry(entry).unwrap();
    assert_eq!(bytes.len(), entry.size as usize);
    assert_eq!(&bytes[..13], &[0xFC, 0xFD, 0xFE, 0xFF, 4, 0, 0, 0, 1, 0xBA, 0x01, 0, 0]);
    let h = DbHeader::read(&bytes).unwrap();
    assert_eq!((h.version, h.flag, h.row_count), (4, 1, 442));
}

/// The VFS mounts the install in load order, and the patch pack wins for text.
#[test]
#[ignore]
fn vfs_over_install() {
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let order: Vec<PackType> = vfs.packs().iter().map(|p| p.pack_type()).collect();
    assert_eq!(order.first(), Some(&PackType::Boot));
    let mut sorted = order.clone();
    sorted.sort_by_key(|t| match t {
        PackType::Boot => 0,
        PackType::Release => 1,
        PackType::Patch => 2,
        PackType::Movie => 3,
        _ => 4,
    });
    assert_eq!(order, sorted);
    let (pack, _) = vfs.find("text/localisation.loc").unwrap();
    assert!(pack.path().ends_with("local_en_patch.pack"));
    assert!(!vfs.read("db/units_tables/units").unwrap().is_empty());
    let tables = vfs.list("db/");
    assert_eq!(tables.len(), 310, "W2: 310 DB tables");
    println!("VFS: {} packs, {} visible paths", vfs.packs().len(), vfs.len());
}

/// ABCE-magic entries inside packs (`.tree_list`, `.building_list`, ...) parse and round-trip.
#[test]
#[ignore]
fn esf_entries_inside_packs() {
    let mut count = 0;
    for name in ["data.pack", "battleterrain.pack"] {
        let pack = PackFile::open(data_dir().join(name)).unwrap();
        for entry in pack.entries() {
            if entry.size < 16 || pack.read_entry_prefix(entry, 4).unwrap() != [0xCE, 0xAB, 0, 0] {
                continue;
            }
            let bytes = pack.read_entry(entry).unwrap();
            let esf = EsfFile::from_bytes(&bytes).unwrap_or_else(|e| panic!("{}: {e}", entry.path));
            assert!(esf.to_bytes().unwrap() == bytes, "{}: round-trip differs", entry.path);
            count += 1;
        }
    }
    assert_eq!(count, 440, "W3: 440 ABCE entries (4 in data.pack, 436 in battleterrain.pack)");
}

/// Every DB table header parses, and three simple tables decode exactly to EOF with
/// schemas taken from Worker 1's DB_BUILDERS.md §0 (test fixtures only).
#[test]
#[ignore]
fn db_tables() {
    let vfs = Vfs::open_install(data_dir()).unwrap();
    for path in vfs.list("db/") {
        DbHeader::read(&vfs.read(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
    }
    for (table, codes, rows_min) in [
        ("character_traits", "s i b i s", 1),
        ("ancillaries", "s s s b b b i i i", 1),
        ("government_types", "s b b i s s", 1),
    ] {
        let bytes = vfs.read(&format!("db/{table}_tables/{table}")).unwrap();
        let t = DbTable::read(&bytes, &Schema::from_codes(codes).unwrap())
            .unwrap_or_else(|e| panic!("{table}: {e}"));
        assert!(t.rows.len() >= rows_min);
        println!("{table}: v{} {} rows, first key {:?}", t.version, t.rows.len(), t.rows[0][0].as_str());
    }
}

/// All four shipped .loc files parse to EOF with the entry counts from W3, and the
/// merged lookup resolves a key.
#[test]
#[ignore]
fn loc_files() {
    let expected = [
        ("local_en.pack", "text/localisation.loc", 32_582),
        ("local_en.pack", "text/ui.loc", 2_787),
        ("local_en_patch.pack", "text/localisation.loc", 33_030),
        ("local_en_patch.pack", "text/ui.loc", 2_820),
    ];
    for (pack, path, n) in expected {
        let pack = PackFile::open(data_dir().join(pack)).unwrap();
        let bytes = pack.read_entry(pack.find(path).unwrap()).unwrap();
        let loc = LocFile::read(&bytes).unwrap();
        assert_eq!((loc.version, loc.entries.len()), (1, n), "{path}");
    }
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    assert!(loc.len() >= 33_030);
    println!("Localisation: {} keys", loc.len());
}

/// Every soldier variant and part mesh parses exactly to EOF. VMPF vertex and attachment
/// payloads intentionally remain raw because their field maps are not yet confirmed.
#[test]
#[ignore]
fn every_unit_variant_and_part_mesh_parses() {
    let pack = PackFile::open(data_dir().join("variantmodels.pack")).unwrap();
    let (mut variants, mut bytes_40, mut bytes_64, mut equipment_containers) =
        (0usize, 0usize, 0usize, 0usize);
    for entry in pack.entries() {
        let path = entry.path.to_ascii_lowercase();
        let bytes = pack.read_entry(entry).unwrap();
        if path.ends_with(".unit_variant") {
            let variant =
                UnitVariant::read(&bytes).unwrap_or_else(|error| panic!("{}: {error}", entry.path));
            assert_eq!(variant.version, 0, "{}", entry.path);
            variants += 1;
        } else if path.ends_with(".variant_part_mesh") {
            let mesh = VariantPartMesh::read(&bytes)
                .unwrap_or_else(|error| panic!("{}: {error}", entry.path));
            match (&mesh.header.vertex_format, &mesh.body) {
                (VariantVertexFormat::Bytes40, VariantPartMeshBody::Part { .. }) => bytes_40 += 1,
                (VariantVertexFormat::Bytes64, VariantPartMeshBody::Part { .. }) => bytes_64 += 1,
                (
                    VariantVertexFormat::EquipmentContainer,
                    VariantPartMeshBody::EquipmentContainer { .. },
                ) => equipment_containers += 1,
                (format, _) => panic!("{}: inconsistent VMPF format/body {format:?}", entry.path),
            }
        }
    }
    println!(
        "unit variants: {variants}; VMPF: {bytes_40} 40-byte, {bytes_64} 64-byte, {equipment_containers} containers"
    );
    assert_eq!(variants, 3_126, "Worker 2 full-pack survey");
    assert_eq!((bytes_40, bytes_64, equipment_containers), (267, 30, 2));
}

/// Every `.rigid_model` in every pack (not only the VFS winners) parses exactly to EOF.
/// Also checks every `.rigid_model_header` against its model's vertex/index totals.
/// Prints a per-pack and per-version summary; run with `--nocapture` to see it.
#[test]
#[ignore]
fn every_rigid_model_parses() {
    use ntw_formats::rigid_model::{RigidModel, RigidModelHeader};
    use std::collections::BTreeMap;

    let mut ok = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut per_pack: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut per_version: BTreeMap<u32, (usize, usize)> = BTreeMap::new(); // meshes, vertices
    let mut flags: BTreeMap<u8, usize> = BTreeMap::new();
    let mut nonzero_color = 0usize;
    let mut nonzero_uv2 = 0usize;
    let mut headers_checked = 0usize;
    let mut headers_exact = 0usize;
    let mut header_examples: Vec<String> = Vec::new();
    for path in pack_paths() {
        let pack = PackFile::open(&path).unwrap();
        let pack_name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for e in pack.entries() {
            let lower = e.path.to_ascii_lowercase();
            if !lower.ends_with(".rigid_model") {
                continue;
            }
            let slot = per_pack.entry(pack_name.clone()).or_default();
            slot.0 += 1;
            let bytes = pack.read_entry(e).unwrap();
            match RigidModel::read(&bytes) {
                Ok(m) => {
                    ok += 1;
                    slot.1 += 1;
                    for mesh in &m.meshes {
                        let v = per_version.entry(mesh.version).or_default();
                        v.0 += 1;
                        v.1 += mesh.vertices.len();
                        for t in [&mesh.material.diffuse, &mesh.material.normal, &mesh.material.gloss].into_iter().flatten() {
                            *flags.entry(t.flag).or_default() += 1;
                        }
                        nonzero_color += mesh.vertices.iter().filter(|v| v.color != [0.0; 4]).count();
                        nonzero_uv2 += mesh.vertices.iter().filter(|v| v.uv2 != [0.0; 2]).count();
                    }
                    counts.insert(lower.clone(), (m.vertex_count(), m.index_count()));
                }
                Err(err) => failures.push(format!("{pack_name}: {} ({} bytes): {err}", e.path, bytes.len())),
            }
        }
        // Header side files in the same pack.
        for e in pack.entries() {
            let lower = e.path.to_ascii_lowercase();
            let Some(stem) = lower.strip_suffix(".rigid_model_header") else { continue };
            let h = RigidModelHeader::read(&pack.read_entry(e).unwrap())
                .unwrap_or_else(|err| panic!("{}: {err}", e.path));
            if let Some(&(v, i)) = counts.get(&format!("{stem}.rigid_model")) {
                assert!(!h.animated);
                headers_checked += 1;
                if (h.vertex_count as usize, h.index_count as usize) == (v, i) {
                    headers_exact += 1;
                } else if header_examples.len() < 5 {
                    header_examples.push(format!("{}: header {:?} model {:?}", e.path, (h.vertex_count, h.index_count), (v, i)));
                }
            }
        }
    }
    println!("rigid_model: {ok} parsed, {} failed", failures.len());
    for (p, (n, k)) in &per_pack {
        println!("  {p}: {k}/{n}");
    }
    for (v, (m, n)) in &per_version {
        println!("  mesh version {v}: {m} meshes, {n} vertices");
    }
    println!("  texture flag bytes: {flags:?}");
    println!("  vertices with non-zero colour: {nonzero_color}, non-zero uv2: {nonzero_uv2}");
    println!("  headers: {headers_checked} checked, {headers_exact} equal to the model totals");
    for x in &header_examples {
        println!("    {x}");
    }
    for f in failures.iter().take(40) {
        println!("  FAIL {f}");
    }
    assert!(ok >= 4_700, "expected about 4,796 models");
    assert!(failures.is_empty(), "{} failures", failures.len());
}

/// Every `.dds` in every pack parses; the last mip of each decodes. Prints a format histogram.
#[test]
#[ignore]
fn every_dds_parses() {
    use ntw_formats::dds::Dds;
    use std::collections::BTreeMap;
    let mut formats: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut n = 0;
    for path in pack_paths() {
        let pack = PackFile::open(&path).unwrap();
        for e in pack.entries() {
            if !e.path.to_ascii_lowercase().ends_with(".dds") {
                continue;
            }
            n += 1;
            let bytes = pack.read_entry(e).unwrap();
            match Dds::parse(&bytes) {
                Ok(d) => {
                    let last = d.mip_count - 1;
                    let (w, h) = d.level_dims(last);
                    assert_eq!(d.decode_rgba8(last).len(), (w * h * 4) as usize);
                    *formats.entry(format!("{:?}", d.format)).or_default() += 1;
                }
                Err(err) => failures.push(format!("{}: {err}", e.path)),
            }
        }
    }
    println!("dds: {n} files, {} failed", failures.len());
    for (k, v) in &formats {
        println!("  {v:>6} {k}");
    }
    for f in &failures {
        println!("  FAIL {f}");
    }
    // Two oddities are known: testdata\gadgets.dds (fourCC 36 = A16B16G16R16)
    // and ui\cinematicicons4.dds (fourCC 63). Neither is a model texture.
    assert!(failures.is_empty(), "{failures:?}");
}

/// Every `.anim` in data.pack parses; prints a survey of skeleton sizes, frame rates and
/// trailing-byte counts. Run with `--nocapture` to see it.
#[test]
#[ignore]
fn every_anim_parses() {
    use ntw_formats::anim::Anim;
    use std::collections::BTreeMap;
    let pack = PackFile::open(data_dir().join("data.pack")).unwrap();
    let (mut n, mut failed) = (0usize, Vec::new());
    let mut testdata_failed = 0usize;
    let mut bones: BTreeMap<usize, usize> = BTreeMap::new();
    let mut rates: BTreeMap<String, usize> = BTreeMap::new();
    let mut events: BTreeMap<String, usize> = BTreeMap::new();
    let mut duration_mismatch = 0usize;
    for entry in pack.entries() {
        if !entry.path.to_ascii_lowercase().ends_with(".anim") {
            continue;
        }
        let bytes = pack.read_entry(entry).unwrap();
        match Anim::read(&bytes) {
            Ok(a) => {
                n += 1;
                *bones.entry(a.bones.len()).or_default() += 1;
                *rates.entry(format!("{}", a.frame_rate)).or_default() += 1;
                for e in &a.events { *events.entry(e[0].clone()).or_default() += 1; }
                let expect = (a.frames.len().max(1) - 1) as f32 / a.frame_rate;
                // The 4 oldest testdata clips store another duration (3.333 s for 66 frames).
                if (expect - a.duration).abs() > 1e-3 && !entry.path.to_ascii_lowercase().starts_with("testdata") {
                    duration_mismatch += 1;
                }
            }
            // testdatanimations holds 31 prototype clips in an older layout (not used by the game: INFERRED).
            Err(e) if entry.path.to_ascii_lowercase().starts_with("testdata") => {
                println!("  testdata failed: {}: {e} ({} bytes)", entry.path, bytes.len());
                testdata_failed += 1
            }
            Err(e) => failed.push(format!("{}: {e}", entry.path)),
        }
    }
    println!("anims parsed {n}, failed {}", failed.len());
    for f in failed.iter().take(10) {
        println!("  {f}");
    }
    println!("bone counts {bones:?}\nframe rates {rates:?}\nevents {events:?}\nduration != (frames-1)/rate: {duration_mismatch}");
    println!("testdata clips that failed: {testdata_failed}");
    assert_eq!(testdata_failed, 0);
    assert!(failed.is_empty());
    assert_eq!(duration_mismatch, 0);
    assert_eq!(n, 3_814, "parsed {n}");
}

/// Every `uniforms` row resolves: its soldier variant file exists and parses, and every
/// entry of every category resolves to a part mesh or an equipment piece. Then the French
/// line infantry man is assembled and posed with frame 0 of `mus_standt.anim`, and the
/// posed figure must stand on the ground with the hat above the head.
#[test]
#[ignore]
fn every_unit_resolves_to_parts() {
    use ntw_formats::anim::Anim;
    use ntw_formats::unit_model::{
        EquipmentLibrary, EquipmentThemes, SoldierModel, UnitModelIndex, VariantRole, unit_variant_path,
    };
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let index = UnitModelIndex::from_vfs(&vfs).unwrap();
    let equipment = EquipmentLibrary::from_vfs(&vfs).unwrap();
    assert_eq!(index.uniforms.len(), 1_120);
    assert_eq!(equipment.len(), 150 + 34);
    let (mut variants, mut part_refs, mut piece_refs, mut missing) = (0, 0, 0, Vec::new());
    let mut stale_pieces = std::collections::BTreeSet::new();
    let mut no_colours = 0;
    for u in &index.uniforms {
        if index.colours(u).is_none() {
            no_colours += 1;
        }
        let path = unit_variant_path(&u.variant, VariantRole::Soldier);
        let variant = UnitVariant::read(&vfs.read(&path).unwrap()).unwrap();
        variants += 1;
        for r in &variant.mesh_references {
            if r.kind == 1 {
                piece_refs += 1;
                if equipment.find(&r.mesh).is_none() {
                    stale_pieces.insert(r.mesh.to_ascii_lowercase());
                }
            } else {
                part_refs += 1;
                if !vfs.contains(&format!("{}.variant_part_mesh", r.mesh)) {
                    missing.push(r.mesh.clone());
                }
            }
        }
    }
    missing.sort();
    missing.dedup();
    println!(
        "{variants} soldier variants, {part_refs} part refs, {piece_refs} piece refs, missing parts {missing:?}, rows without colours {no_colours}"
    );
    // Kind-1 names that match no container piece (Empire-era names such as
    // rigid_equip_euro_bag01); the DB equipment themes are used instead.
    println!("{} kind-1 names without a piece: {stale_pieces:?}", stale_pieces.len());
    assert!(missing.is_empty());

    let themes = EquipmentThemes::from_vfs(&vfs).unwrap();
    let mut unresolved_items = std::collections::BTreeSet::new();
    for set in ["euro_musket", "french_infantry_equipment", "euro_drum_kit", "euro_cavalry_sabre", "lance", "pistol"] {
        for item in themes.items(set) {
            if equipment.pieces_for_item(item).is_empty() {
                unresolved_items.insert(item.clone());
            }
        }
    }
    println!("theme items without pieces: {unresolved_items:?}");

    let rows = index.uniforms_for_unit("Inf_Line_French_Fusiliers", Some("france"));
    assert_eq!(rows[0].faction, "france");
    let variant = UnitVariant::read(&vfs.read(&unit_variant_path(&rows[0].variant, VariantRole::Soldier)).unwrap()).unwrap();
    let (mut man, problems) = SoldierModel::assemble(&vfs, &variant, &equipment, 0);
    println!("assemble notes: {problems:?}");
    let theme = themes.theme("france_musket").unwrap();
    let problems = man.equip_from_theme(&vfs, &themes, theme, &equipment, &[], 0);
    assert!(problems.is_empty(), "{problems:?}");
    assert!(man.parts.iter().any(|p| p.source.to_ascii_lowercase().contains("euro_musket")));
    let anim = Anim::read(&vfs.read("animations/men/musket/mus_stand_trained/mus_standt.anim").unwrap()).unwrap();
    let bones = anim.world_matrices(0);
    let span = |cat: &str| {
        let mut y = (f32::MAX, f32::MIN);
        for p in man.parts.iter().filter(|p| p.category == cat) {
            for q in p.pose(&bones).positions {
                y = (y.0.min(q[1]), y.1.max(q[1]));
            }
        }
        y
    };
    let (legs, heads, hats) = (span("legs"), span("heads"), span("hats"));
    println!("legs {legs:?} heads {heads:?} hats {hats:?}");
    assert!(legs.0 > -0.15 && legs.0 < 0.15, "feet near the ground");
    assert!(heads.1 > 1.5 && heads.1 < 2.0, "head top at man height");
    assert!(hats.1 > heads.1, "hat above the head");
}

/// Survey: in which file position is the most detailed LOD of each part mesh?
#[test]
#[ignore]
fn part_mesh_lod_order_survey() {
    use std::collections::BTreeMap;
    let pack = PackFile::open(data_dir().join("variantmodels.pack")).unwrap();
    let mut order: BTreeMap<String, usize> = BTreeMap::new();
    for entry in pack.entries() {
        if !entry.path.to_ascii_lowercase().ends_with(".variant_part_mesh") {
            continue;
        }
        let mesh = VariantPartMesh::read(&pack.read_entry(entry).unwrap()).unwrap();
        if let VariantPartMeshBody::Part { lods, .. } = &mesh.body {
            let counts: Vec<u32> = lods.iter().map(|l| l.vertex_count).collect();
            let desc = counts.windows(2).all(|w| w[0] >= w[1]);
            let asc = counts.windows(2).all(|w| w[0] <= w[1]);
            let key = match (desc, asc) {
                (true, true) => "single/equal",
                (true, false) => "descending (first = most detailed)",
                (false, true) => "ascending (last = most detailed)",
                _ => "mixed",
            };
            *order.entry(key.into()).or_default() += 1;
        }
    }
    println!("{order:?}");
}

#[test]
#[ignore]
fn every_ui_layout_parses() {
    use ntw_formats::ui_layout::{is_layout, UiLayout};
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let (mut ok, mut failed) = (0, Vec::new());
    for p in vfs.list("ui/") {
        // Layouts are the extensionless files; `fontcategories.fc` also starts with "Version" but is another format.
        if p.rsplit(['\\', '/']).next().unwrap().contains('.') {
            continue;
        }
        let b = vfs.read(p).unwrap();
        if !is_layout(&b) {
            continue;
        }
        match UiLayout::read(&b) {
            Ok(_) => ok += 1,
            Err(e) => failed.push(format!("{p}: {e}")),
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
    assert!(ok >= 170, "only {ok} layouts found");
    let main = UiLayout::read(&vfs.read("ui/frontend ui/main").unwrap()).unwrap();
    assert_eq!(main.version, 39);
    let b = main.root.find("continue_campaign").unwrap();
    assert_eq!(b.event("OnMouseLClickUp"), Some("OnLeftClickUp"));
    assert_eq!(b.find("button_txt").unwrap().initial_state().unwrap().font, "Frontend 22, Normal");
}

#[test]
#[ignore]
fn every_cuf_font_and_font_categories_parse() {
    use ntw_formats::font::{font_file_for, CufFont, FontCategories};
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let fonts = vfs.list("font/");
    let mut n = 0;
    for p in fonts.iter().filter(|p| p.ends_with(".cuf")) {
        let f = CufFont::read(&vfs.read(p).unwrap()).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert!(f.glyph('A').is_some(), "{p}");
        n += 1;
    }
    assert!(n >= 70, "only {n} fonts");
    let fc = FontCategories::read(&vfs.read("ui/fontcategories.fc").unwrap()).unwrap();
    assert!(!fc.entries.is_empty());
    for e in &fc.entries {
        let file = font_file_for(&e.font).unwrap();
        assert!(vfs.contains(&file), "{} -> {file}", e.font);
    }
    let f = CufFont::read(&vfs.read("font/frontend_22.cuf").unwrap()).unwrap();
    assert!(f.text_width("Single Player") > 100);
}

#[test]
#[ignore]
fn every_ui_tga_decodes() {
    use ntw_formats::tga::Tga;
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let mut failed = Vec::new();
    let mut n = 0;
    for p in vfs.list("ui/").into_iter().filter(|p| p.ends_with(".tga")) {
        match Tga::decode(&vfs.read(p).unwrap()) {
            Ok(t) => {
                assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize, "{p}");
                n += 1;
            }
            Err(e) => failed.push(format!("{p}: {e}")),
        }
    }
    // Two shipped flag frames are broken: punjab 0011 is truncated, quebec 0000 is really a JPEG. Not UI-menu art.
    failed.retain(|f| !f.contains("punjab") && !f.contains("quebec"));
    assert!(failed.is_empty(), "{} of {} failed: {:#?}", failed.len(), n + failed.len(), &failed[..failed.len().min(20)]);
    assert!(n > 1000, "only {n}");
}

/// Every battle mount mesh named by `warscape_animated_lod` (models of kind `animation`)
/// parses with the strict `.variant_weighted_mesh` reader, and every `mount_variants` row
/// names such a model.
#[test]
#[ignore]
fn every_weighted_mesh_parses() {
    use ntw_formats::mount::MountIndex;
    use ntw_formats::weighted_mesh::WeightedMesh;
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let index = MountIndex::from_vfs(&vfs).unwrap();
    let (mut meshes, mut bad, mut other_kinds) = (0, Vec::new(), 0);
    let mounts: Vec<String> = index.mounts().map(str::to_owned).collect();
    let mut models = std::collections::BTreeSet::new();
    for m in &mounts {
        for (key, _) in index.variants(m) {
            let model = index.model(key).unwrap_or_else(|| panic!("{m}: model {key} missing"));
            models.insert(model.key.to_ascii_lowercase());
        }
    }
    for key in &models {
        let model = index.model(key).unwrap();
        if model.kind != "animation" {
            other_kinds += 1;
        }
        assert!(!model.lods.is_empty(), "{key}: no LODs");
        for (path, _) in &model.lods {
            meshes += 1;
            match vfs.read(path).map_err(|e| e.to_string()).and_then(|b| WeightedMesh::read(&b).map_err(|e| e.to_string())) {
                Ok(m) => assert!(!m.pieces.is_empty(), "{path}: no pieces"),
                Err(e) => bad.push(format!("{path}: {e}")),
            }
        }
    }
    println!("{} mounts, {} models ({other_kinds} not of kind animation), {meshes} LOD meshes, bad {bad:?}", mounts.len(), models.len());
    assert!(bad.is_empty());
}

/// Every `.variant_weighted_mesh` in the packs reads to its last byte, including the
/// attachments (Napoleon's outfit, `euro_equipment`) and the older headerless layouts (campaign
/// agents, testdata). See `weighted_mesh` module docs.
#[test]
#[ignore]
fn all_286_weighted_meshes_parse() {
    use ntw_formats::weighted_mesh::WeightedMesh;
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let paths: Vec<String> = vfs.list("").into_iter().filter(|p| p.ends_with(".variant_weighted_mesh")).map(str::to_owned).collect();
    let mut bad = Vec::new();
    let (mut attachments, mut headerless) = (0, 0);
    for p in &paths {
        match WeightedMesh::read(&vfs.read(p).unwrap()) {
            Ok(m) => {
                attachments += m.attachments.len();
                headerless += usize::from(m.version == 0);
                assert!(!m.pieces.is_empty() || !m.attachments.is_empty(), "{p}: empty");
            }
            Err(e) => bad.push(format!("{p}: {e}")),
        }
    }
    println!("{} weighted meshes, {headerless} headerless, {attachments} attachments, bad {bad:?}", paths.len());
    assert_eq!(paths.len(), 286);
    assert!(bad.is_empty());
}

/// `data\UI\Templates\uied.templates`: all 126 entries read (including the two oldest,
/// `InputWindow` v1/2 and `BattleEditor` v4) and each entry's offset field is its own file offset.
#[test]
#[ignore]
fn uied_templates_read_completely() {
    use ntw_formats::ui_templates::UiTemplateLibrary;
    let b = std::fs::read(data_dir().join(r"UI\Templates\uied.templates")).unwrap();
    let lib = UiTemplateLibrary::read(&b).unwrap();
    assert_eq!(lib.templates.len(), 126);
    assert!(lib.unreadable.is_empty(), "{:?}", lib.unreadable);
    assert_eq!(lib.templates[0].offset, 4);
    assert!(lib.templates.windows(2).all(|w| w[0].offset < w[1].offset));
    assert_eq!(lib.get("BattleEditor").unwrap().version, 4);
    assert!(lib.get("InputWindow").unwrap().component.script.contains("CharacterInput"));
}

/// The diplomacy negotiation screen's layout contract (0-E diplomacy work, 2026-10-04):
/// `ui\campaign ui\diplomacy_panel` (Version039) plus its four `ui\templates\diplomacy_*`
/// templates. Field meanings per `analysis/frontend/UI_LAYOUT_FORMAT.md`:
/// - `unknown_da` = ClipChildren (CONFIRMED `0x01027D20`): 1 on exactly 21 components of the
///   panel (the offer/demand `list_clip`s, the four radar-map `Blank` masks, the declare-war
///   and war-declared `list_box`/`bg` pairs, the technology and join-war `list_clip`s and the
///   diplomat portrait's `clip_box`); 0 on every template component.
/// - `unknown_e5` = UseGlobalClicks: 0 on every component (as in every shipped layout).
/// - `unknown_140` = DrawMode (name INFERRED, inheritance CONFIRMED `0x01027D20`): 0 on every
///   component, so the whole screen draws in mode 0 (normal, UI-scaled) by inheritance.
///
/// Also locks the negotiation button rows and subpopup ids the panel script drives through
/// `LuaCall` (CONFIRMED inline layout Lua, e.g. `parent:LuaCall("SendOffer")`).
#[test]
#[ignore]
fn diplomacy_negotiation_layout_fields() {
    use ntw_formats::ui_layout::{UiComponent, UiLayout, is_layout};
    fn walk(c: &UiComponent, out: &mut Vec<(String, u8, u8, u32, String)>) {
        out.push((c.id.clone(), c.unknown_da, c.unknown_e5, c.unknown_140, c.script.clone()));
        for ch in &c.children {
            walk(ch, out);
        }
    }
    fn find<'a>(root: &'a UiComponent, id: &str) -> Option<&'a UiComponent> {
        if root.id == id {
            return Some(root);
        }
        root.children.iter().find_map(|c| find(c, id))
    }
    let vfs = Vfs::open_install(data_dir()).unwrap();
    // The panel script files the buttons LuaCall into ship next to the layout.
    for luac in [
        r"ui\campaign ui\diplomacy_panel_scripts\diplomacy_panel.luac",
        r"ui\campaign ui\diplomacy_panel_scripts\diplomacy_tech_offer.luac",
        r"ui\campaign ui\diplomacy_panel_scripts\offers.luac",
    ] {
        assert!(vfs.contains(luac), "missing {luac}");
    }
    let b = vfs.read(r"ui\campaign ui\diplomacy_panel").unwrap();
    assert!(is_layout(&b));
    let panel = UiLayout::read(&b).unwrap();
    assert_eq!(panel.version, 39);
    let mut comps = Vec::new();
    walk(&panel.root, &mut comps);
    assert_eq!(comps.len(), 351, "diplomacy_panel component count");
    // ClipChildren: exactly the 21 list/map/portrait clips.
    let clipped: Vec<&str> = comps.iter().filter(|(_, da, _, _, _)| *da == 1).map(|(id, _, _, _, _)| id.as_str()).collect();
    assert_eq!(clipped.len(), 21, "{clipped:?}");
    assert!(
        clipped.iter().all(|id| ["list_clip", "Blank", "list_box", "bg", "clip_box"].contains(id)),
        "{clipped:?}"
    );
    // UseGlobalClicks / DrawMode: 0 everywhere (DrawMode 0 = normal by inheritance).
    assert!(comps.iter().all(|(_, _, e5, _, _)| *e5 == 0));
    assert!(comps.iter().all(|(_, _, _, m, _)| *m == 0));
    // The negotiation rows: offer buttons LuaCall the panel script, which drives `negotiation:*`.
    let root = &panel.root;
    assert!(find(root, "diplomacy_panel").is_some());
    for (button, call) in [
        ("button_threat_of_force", "ThreatOfForce"),
        ("button_send", "SendOffer"),
        ("button_cancel", "CancelOffer"),
        ("button_accept", "AcceptOffer"),
        ("button_counteroffer", "CounterOffer"),
    ] {
        let c = find(root, button).unwrap_or_else(|| panic!("missing {button}"));
        assert!(c.script.contains(&format!("LuaCall(\"{call}\")")), "{button}: no LuaCall(\"{call}\")");
    }
    // One subpopup per deal kind, each closed with ClearSubPopups.
    for popup in ["regions", "declare_war", "war_declared", "access", "state_gift", "end_alliance", "end_trade", "technology", "payments", "request_join_war"] {
        assert!(find(root, popup).is_some(), "missing subpopup {popup}");
    }
    for (button, call) in [
        ("ok_regions", "OkRegions"),
        ("button_ok_declare", "OkDeclareWar"),
        ("button_ok", "OkWarDeclared"),
        ("ok_access", "AcceptMilitaryAccess"),
        ("ok_stategift", "OkStateGift"),
        ("ok_technology", "OkTechnologies"),
        ("ok_payments", "OkPayments"),
        ("ok_request", "OkStanceDeclaration"),
    ] {
        let c = find(root, button).unwrap_or_else(|| panic!("missing {button}"));
        assert!(c.script.contains(&format!("LuaCall(\"{call}\"")), "{button}: no LuaCall(\"{call}\")");
    }
    // The diplomat portrait is clipped (MinisterPortraitPath picture) and the radar maps ship.
    let clip = find(root, "clip_box").expect("diplomat clip_box");
    assert_eq!(clip.unknown_da, 1);
    assert!(find(clip, "portrait").is_some());
    for theatre in ["egypt", "europe", "italy", "spain"] {
        assert!(find(root, theatre).is_some(), "missing radar map {theatre}");
    }
    // The four templates parse (Version039) with no clip/global-click/draw-mode flags.
    for (path, top) in [
        (r"ui\templates\diplomacy_button", "diplomacy_button"),
        (r"ui\templates\diplomacy_item_regions", "diplomacy_item_regions"),
        (r"ui\templates\diplomacy_item_text", "diplomacy_item_text"),
        (r"ui\templates\diplomacy_region_tooltip", "Diplomacy_region_tooltip"),
    ] {
        let b = vfs.read(path).unwrap();
        assert!(is_layout(&b), "{path}");
        let l = UiLayout::read(&b).unwrap();
        assert_eq!(l.version, 39, "{path}");
        let mut t = Vec::new();
        walk(&l.root, &mut t);
        assert!(t.iter().all(|(_, da, e5, m, _)| *da == 0 && *e5 == 0 && *m == 0), "{path}");
        assert!(find(&l.root, top).is_some(), "{path}: missing {top}");
    }
}

/// The campaign save-naming screen's layout contract (0-E save naming, 2026-10-04):
/// `ui\campaign ui\load-save_game` (Version039, 57 components) serves both Load and Save
/// (title states `Load` = "Load Game" / `Save` = "Save Game"); in Save it shows the
/// `filename_panel` with the `input_name` text field and the `button_ok` / `button_cancel`
/// pair. Field meanings per `analysis/frontend/UI_LAYOUT_FORMAT.md`:
/// - `input_name` carries the single `NewState` with state `unknown_d4` = 2 (the text-entry
///   focus flag the host's `accepts_focus` tests) and sample text "Testtext", the
///   `input_name_label` child ("Filename:"), and the `OnKey` = `OnKeyEvent` binding the
///   text-entry path (`CharacterInput` / RETURN) runs through — the same contract as the
///   front-end `file_requester`'s field, which the `typed_file_name_saves_the_army` test
///   already drives.
/// - `button_ok` / `button_cancel` fire `call Parent.LuaCall, OnAccept` / `OnCancel` on
///   `OnMouseLClickUp` into the panel driver `load-save_game.load-save_game.luac`.
/// - `unknown_da` = ClipChildren: 1 on exactly `Flags` (the territory-map card) and
///   `list_clip` (the save list); `unknown_e5` = UseGlobalClicks and `unknown_140` =
///   DrawMode are 0 everywhere, so the screen draws in mode 0 by inheritance.
///
/// **The naming rules are CONFIRMED (0-E, read on the install 2026-10-06 with `luac_dump`).** The
/// driver sets `input_name:SetGlobal("CharacterValidator", ValidateFilename)` (line 264) and
/// `ValidateFilename` (the same file, the function at line 33) walks a nine-character table
/// `/ \ * ? " < > | :` — a character in it is refused — and otherwise accepts only while
/// `string.length(field) < 100`. So a typed save name holds **none of those characters and is at
/// most 100 characters long**. It is a per-keystroke filter, not a whole-name check: the field's
/// own `CharacterInput` (`ui\templates\template.text_input.luac`, line 30) calls the validator
/// before inserting and drops the character unless it answers exactly `true`.
///
/// The save writer itself (`ntw_campaign::save`) takes no file name — it rewrites the ESF
/// tree — so no validation lives there (CONFIRMED by the `write_save*` signatures); the name
/// becomes `<save_games>\<name>.save` through `FileExtenstionAndPathForWriteClass` (class
/// `save_game`, CONFIRMED `0x0046EE50`). Still UNKNOWN: `ConfirmSave` / `SaveCampaign` /
/// `DefaultSaveName` in the driver.
#[test]
#[ignore]
fn campaign_save_naming_layout_fields() {
    use ntw_formats::ui_layout::{UiComponent, UiLayout, is_layout};
    fn walk(c: &UiComponent, out: &mut Vec<(String, u8, u8, u32, String)>) {
        out.push((c.id.clone(), c.unknown_da, c.unknown_e5, c.unknown_140, c.script.clone()));
        for ch in &c.children {
            walk(ch, out);
        }
    }
    fn find<'a>(root: &'a UiComponent, id: &str) -> Option<&'a UiComponent> {
        if root.id == id {
            return Some(root);
        }
        root.children.iter().find_map(|c| find(c, id))
    }
    let vfs = Vfs::open_install(data_dir()).unwrap();
    // The panel driver, the escape-menu Save entry and the save-row template ship with it.
    for luac in [
        r"ui\campaign ui\load-save_game.load-save_game.luac",
        r"ui\campaign ui\campaign_escape_menu_scripts\menu_save_game_button.luac",
        r"ui\templates\template.campaign_save_game.luac",
    ] {
        assert!(vfs.contains(luac), "missing {luac}");
    }
    let b = vfs.read(r"ui\campaign ui\load-save_game").unwrap();
    assert!(is_layout(&b));
    let panel = UiLayout::read(&b).unwrap();
    assert_eq!(panel.version, 39);
    let mut comps = Vec::new();
    walk(&panel.root, &mut comps);
    assert_eq!(comps.len(), 57, "load-save_game component count");
    // ClipChildren: exactly the territory-map card and the save-list clip.
    let clipped: Vec<&str> = comps.iter().filter(|(_, da, _, _, _)| *da == 1).map(|(id, _, _, _, _)| id.as_str()).collect();
    assert_eq!(clipped.len(), 2, "{clipped:?}");
    assert!(clipped.contains(&"Flags") && clipped.contains(&"list_clip"), "{clipped:?}");
    // UseGlobalClicks / DrawMode: 0 everywhere (DrawMode 0 = normal by inheritance).
    assert!(comps.iter().all(|(_, _, e5, _, _)| *e5 == 0));
    assert!(comps.iter().all(|(_, _, _, m, _)| *m == 0));
    let root = &panel.root;
    assert!(find(root, "load-save_game").is_some());
    // The title doubles for Load and Save.
    let title = find(root, "TX_save_game").expect("missing TX_save_game");
    let state_text = |name: &str| title.states.iter().find(|s| s.name == name).map(|s| s.text.as_str());
    assert_eq!(state_text("Load"), Some("Load Game"));
    assert_eq!(state_text("Save"), Some("Save Game"));
    // The name field: text-entry focus flag, sample text, Filename label, OnKey binding.
    let filename = find(root, "filename_panel").expect("missing filename_panel");
    assert!(find(filename, "input_name").is_some());
    let input = find(root, "input_name").expect("missing input_name");
    assert_eq!(input.states.len(), 1);
    assert_eq!(input.states[0].unknown_d4, 2, "input_name focus flag");
    assert_eq!(input.states[0].text, "Testtext");
    assert!(find(input, "input_name_label").is_some());
    assert!(input.events.iter().any(|(e, f)| e == "OnKey" && f == "OnKeyEvent"), "input_name OnKey");
    // Ok / Cancel LuaCall the panel script (a mechanism the host already implements).
    for (button, call) in [("button_ok", "OnAccept"), ("button_cancel", "OnCancel")] {
        let c = find(root, button).unwrap_or_else(|| panic!("missing {button}"));
        assert_eq!(c.states.len(), 5, "{button}: five button states");
        let fire = c.events.iter().find(|(e, _)| e == "OnMouseLClickUp").unwrap_or_else(|| panic!("{button}: no click event"));
        assert!(fire.1.contains("LuaCall") && fire.1.contains(call), "{button}: no LuaCall(\"{call}\")");
    }
    // The save list: sortable headers, one clipped list, rows with name / turns / date.
    let headers = find(root, "headers").expect("missing headers");
    for header in ["name", "turns", "date"] {
        let h = find(headers, header).unwrap_or_else(|| panic!("missing header {header}"));
        assert!(h.events.iter().any(|(e, f)| e == "OnMouseLClickUp" && f == "SortList"), "{header}: no SortList");
    }
    let row = find(root, "row_example").expect("missing row_example");
    for cell in ["game_name", "time_played", "date"] {
        assert!(find(row, cell).is_some(), "row_example: missing {cell}");
    }
    // The shared text-entry contract on the front-end file requester (driven by
    // `typed_file_name_saves_the_army`): same version, same d4=2 Testtext field, and the
    // Ok / Cancel pair LuaCalls Accept / Decline.
    let b = vfs.read(r"ui\frontend ui\file_requester").unwrap();
    assert!(is_layout(&b));
    let req = UiLayout::read(&b).unwrap();
    assert_eq!(req.version, 39);
    let req_input = find(&req.root, "input_name").expect("file_requester: missing input_name");
    assert_eq!(req_input.states.len(), 1);
    assert_eq!(req_input.states[0].unknown_d4, 2);
    assert_eq!(req_input.states[0].text, "Testtext");
    for (button, call) in [("button_ok", "Accept"), (" button_cancel", "Decline")] {
        let c = find(&req.root, button).unwrap_or_else(|| panic!("file_requester: missing {button}"));
        assert!(c.script.contains(&format!("LuaCall(\"{call}\")")), "file_requester {button}: no LuaCall(\"{call}\")");
    }
    // Data fact: the requester's Cancel id carries a leading space (" button_cancel").
}

/// Gait-change evidence (UNITS_TERRAIN_FIDELITY.md §1.9): which clips carry the
/// `*_FOOT_GEAR_UP_START/END` markers the exe turns into blend windows (`0x00E4F4D0`), and which
/// `blend_in_time` values and `WALK_TO_RUN` / `RUN_TO_WALK` lines the vanilla fragments give the
/// gait slots. Run with `--nocapture` to see it.
#[test]
#[ignore]
fn gait_blend_survey() {
    use ntw_formats::anim::Anim;
    use ntw_formats::battle_animation::Fragment;
    use std::collections::BTreeMap;
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let mut gear = Vec::new();
    for path in vfs.list("animations/") {
        if !path.ends_with(".anim") {
            continue;
        }
        let Ok(a) = Anim::read(&vfs.read(path).unwrap()) else { continue };
        let marks: Vec<_> = a.events.iter().filter(|e| e[0].contains("GEAR_UP")).cloned().collect();
        if !marks.is_empty() {
            gear.push(format!("{path} ({:.3} s): {marks:?}", a.duration));
        }
    }
    println!("clips with gear-up markers: {}", gear.len());
    for g in &gear {
        println!("  {g}");
    }
    let mut blends: BTreeMap<String, usize> = BTreeMap::new();
    let mut transitions: BTreeMap<String, usize> = BTreeMap::new();
    let mut fragments = 0;
    for path in vfs.list("animations/") {
        if !path.ends_with(".frg") && !path.contains("fragment") {
            continue;
        }
        let Ok(bytes) = vfs.read(path) else { continue };
        let frag = Fragment::parse(&String::from_utf8_lossy(&bytes));
        fragments += 1;
        for (slot, clips) in &frag.slots {
            let gait = slot.starts_with("WALK") || slot.starts_with("RUN") || slot.starts_with("RIDER_WALK") || slot.starts_with("RIDER_RUN");
            if !gait {
                continue;
            }
            if slot.contains("_TO_") {
                *transitions.entry(slot.clone()).or_default() += clips.len();
            }
            for c in clips {
                *blends.entry(format!("{slot} {:?}", c.blend_in_time)).or_default() += 1;
            }
        }
    }
    println!("fragments read {fragments}\ngait transitions {transitions:?}");
    for (k, n) in &blends {
        println!("  {k}: {n}");
    }
    assert!(fragments > 0);
}
