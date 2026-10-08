//! Research helper (read-only): three scans over the shipped install that back the battle-effects
//! research in `analysis/graphics/BATTLE_EFFECTS.md` §2 and §5.
//!
//! ```text
//! cargo run -p ntw_data --example fx_table_probe -- files [filter]   # 1: what is in data.pack
//! cargo run -p ntw_data --example fx_table_probe -- scan <needle>     # 2: which table holds a string
//! cargo run -p ntw_data --example fx_table_probe -- rows <path>       # 3: the string/float shape of a table
//! cargo run -p ntw_data --example fx_table_probe -- projectiles       # 4: the last columns of every projectiles row
//! cargo run -p ntw_data --example fx_table_probe -- trails            # 5: the trail bridge, both sides
//! cargo run -p ntw_data --example fx_table_probe -- artillery         # 6: the gun model table, raw
//! cargo run -p ntw_data --example fx_table_probe -- gunmodels        # 7: do the gun models have nodes?
//! cargo run -p ntw_data --example fx_table_probe -- budgets          # 8: per-emitter quad budgets
//! cargo run -p ntw_data --example fx_table_probe -- facing           # 9: which emitters are not camera facing
//! cargo run -p ntw_data --example fx_table_probe -- billboard         # 10: does any attribute separate BILLBOARD?
//! cargo run -p ntw_data --example fx_table_probe -- shader <path>    # 11: a shipped .fx, which is plain HLSL source
//! cargo run -p ntw_data --example fx_table_probe -- shaderfind <s>    # 12: which shipped shaders mention a token
//! ```
//!
//! Everything it prints comes from the install's own bytes; it writes nothing.
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    Vfs::open_install(dir).expect("open install")
}

/// A UTF-16 needle as UTF-16LE bytes.
fn needle_bytes(needle: &str) -> Vec<u8> {
    needle.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// Every length-prefixed, printable UTF-16 string in `bytes`, with its byte offset.
fn strings(bytes: &[u8]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 2 <= bytes.len() {
        let n = u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
        if (1..=96).contains(&n) && i + 2 + n * 2 <= bytes.len() {
            let units: Vec<u16> = bytes[i + 2..i + 2 + n * 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            if let Ok(s) = String::from_utf16(&units)
                && s.chars().all(|c| c.is_ascii_graphic() || c == ' ')
                && !s.is_empty()
            {
                out.push((i, s));
            }
        }
        i += 1;
    }
    out
}

fn files(vfs: &Vfs, filter: &str) {
    let all = vfs.list("");
    println!("{} entries in data.pack", all.len());
    let mut hits: Vec<&str> = all
        .iter()
        .copied()
        .filter(|p| {
            filter.is_empty()
                || p.to_ascii_lowercase().contains(&filter.to_ascii_lowercase())
        })
        .collect();
    hits.sort_unstable();
    for h in hits {
        println!("  {h}");
    }
}

fn scan(vfs: &Vfs, needle: &str) {
    let nb = needle_bytes(needle);
    let paths: Vec<String> = vfs
        .list("db\\")
        .into_iter()
        .chain(vfs.list("effects\\"))
        .map(str::to_string)
        .collect();
    for p in paths {
        let Ok(bytes) = vfs.read(&p) else { continue };
        let mut at = Vec::new();
        let mut i = 0usize;
        while i + nb.len() <= bytes.len() {
            if bytes[i..i + nb.len()] == nb[..] {
                at.push(i);
            }
            i += 1;
        }
        if !at.is_empty() {
            println!("{p}: {} hits at {:?}", at.len(), &at[..at.len().min(8)]);
        }
    }
}

/// The shape of one DB table: its header, every string with its offset, and the 4-byte floats in
/// the gaps between strings (only the plausible ones: finite, |v| < 1e9, not a denormal).
fn rows(vfs: &Vfs, path: &str) {
    let bytes = vfs.read(path).expect("read table");
    let mut header = 0usize;
    if bytes.len() > 8 && bytes[0..4] == [0xFC, 0xFD, 0xFE, 0xFF] {
        header = 8;
    }
    println!("{path}: {} bytes, header {header}", bytes.len());
    let found = strings(&bytes);
    println!("{} strings", found.len());
    let mut prev_end = header;
    for (i, (at, s)) in found.iter().enumerate() {
        // The floats in the gap since the last string ended.
        let gap_end = *at;
        if gap_end > prev_end {
            let gap = &bytes[prev_end..gap_end];
            let mut floats = Vec::new();
            let mut o = 0usize;
            while o + 4 <= gap.len() {
                let v = f32::from_le_bytes([gap[o], gap[o + 1], gap[o + 2], gap[o + 3]]);
                if v.is_finite() && v != 0.0 && v.abs() < 1e9 && v.abs() > 1e-6 {
                    floats.push(format!("{v:.5}"));
                } else {
                    floats.push("-".into());
                }
                o += 4;
            }
            if !floats.iter().all(|f| f == "-") {
                println!("  gap {} bytes before string {i}: {}", gap.len(), floats.join(" "));
            }
        }
        println!("  {:#08x} {s}", at);
        prev_end = *at + 2 + s.encode_utf16().count() * 2;
    }
}

/// Every `gun_type_to_projectiles` row: the gun type, the projectile, and the `muzzle_flash`
/// column, so its shape can be seen next to the two it joins to.
fn guns() {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let db = ntw_data::GameDatabase::from_install(&dir).expect("database");
    println!("{} gun_type_to_projectiles rows", db.gun_type_to_projectiles.len());
    for g in db.gun_type_to_projectiles.iter() {
        let derived = format!("{}_muzzle_flash", g.gun_type);
        println!(
            "{:34} {:34} {:44} {}",
            g.gun_type,
            g.projectile,
            g.muzzle_flash,
            if g.muzzle_flash == derived { "<= gun_type + _muzzle_flash" } else { "" }
        );
    }
}

/// Which pack files hold a needle, in UTF-16LE or ASCII, filtered by a substring of their path.
fn packscan(vfs: &Vfs, needle: &str, path_filter: &str) {
    let forms: [(&str, Vec<u8>); 2] =
        [("utf16", needle_bytes(needle)), ("ascii", needle.as_bytes().to_vec())];
    let mut paths: Vec<String> =
        vfs.list("").into_iter().filter(|p| path_filter.is_empty() || p.contains(path_filter)).map(str::to_string).collect();
    paths.sort();
    println!("{} candidate files", paths.len());
    let mut hits = 0usize;
    for p in paths {
        let Ok(bytes) = vfs.read(&p) else { continue };
        for (kind, nb) in &forms {
            if let Some(at) = bytes.windows(nb.len()).position(|w| w == nb.as_slice()) {
                println!("HIT {kind} {p} at {at:#x} (first hit)");
                hits += 1;
            }
        }
    }
    println!("{hits} hits for \"{needle}\"");
}

/// Every distinct value of an XML attribute in an effect file, with counts.
fn attrs(vfs: &Vfs, path: &str, attr: &str) {
    let bytes = vfs.read(path).expect("effect file");
    let text = String::from_utf8_lossy(&bytes);
    let open = format!("{attr}='");
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut rest = text.as_ref();
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        let Some(end) = after.find('\'') else { break };
        *counts.entry(after[..end].to_string()).or_default() += 1;
        rest = &after[end + 1..];
    }
    println!("{path}: {} distinct {attr} values", counts.len());
    for (v, n) in counts {
        println!("  {n:4} {v}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("projectiles") => projectiles(args.get(1).map(String::as_str).unwrap_or("")),
        Some("trails") => trails(&vfs()),
        Some("artillery") => artillery(&vfs(), args.get(1).map(String::as_str).unwrap_or("")),
        Some("gunmodels") => gun_models(&vfs()),
        Some("budgets") => budgets(&vfs()),
        Some("facing") => facing(&vfs()),
        Some("billboard") => billboard(&vfs()),
        Some("shader") => shader(&vfs(), args.get(1).map(String::as_str).unwrap_or("fx\\particle.fx")),
        Some("shaderfind") => shader_find(&vfs(), args.get(1).map(String::as_str).unwrap_or("particle_vertex_30")),
        Some("guns") => guns(),
        _ => {
            let vfs = vfs();
            match args.first().map(String::as_str) {
                Some("files") => files(&vfs, args.get(1).map(String::as_str).unwrap_or("")),
                Some("scan") => scan(&vfs, args.get(1).map(String::as_str).unwrap_or("")),
                Some("rows") => rows(&vfs, args.get(1).map(String::as_str).unwrap_or("")),
                // Every distinct value of an XML attribute in an effect file, with counts.
                Some("attrs") => attrs(
                    &vfs,
                    args.get(1).map(String::as_str).unwrap_or(ntw_formats::effects::LAND_BATTLE_EFFECTS),
                    args.get(2).map(String::as_str).unwrap_or("render_method"),
                ),
                // Every group / emitter name of an effect file, or just those matching a filter.
                Some("groups") => {
                    let lib = ntw_formats::effects::EffectLibrary::from_vfs_path(
                        &vfs,
                        args.get(1).map(String::as_str).unwrap_or(ntw_formats::effects::LAND_BATTLE_EFFECTS),
                    )
                    .expect("effect file");
                    let filter = args.get(2).map(String::as_str).unwrap_or("");
                    println!("{} groups, {} emitters", lib.groups.len(), lib.effects.len());
                    for g in lib.groups.keys() {
                        if filter.is_empty() || g.contains(filter) {
                            println!("  group {g}");
                        }
                    }
                    for e in lib.effects.keys() {
                        if !filter.is_empty() && e.contains(filter) {
                            println!("  EMITTER {e}");
                        }
                    }
                }
                Some("packscan") => {
                    packscan(&vfs, args.get(1).map(String::as_str).unwrap_or(""), args.get(2).map(String::as_str).unwrap_or(""))
                }
                other => {
                    eprintln!("unknown mode {other:?}");
                    std::process::exit(2);
                }
            }
        }
    }
}

/// The trail bridge, both sides of it. Left: every `projectiles.trail` value (column 32) and what
/// it resolves to. Right: the `projectile_trails` table's own five rows with their ten floats, and
/// what the seven trail projectiles have in their decoded columns to correlate those floats against.
fn trails(vfs: &Vfs) {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let db = ntw_data::GameDatabase::from_install(&dir).expect("database");
    let land = ntw_formats::effects::EffectLibrary::from_vfs_path(
        vfs,
        ntw_formats::effects::LAND_BATTLE_EFFECTS,
    )
    .expect("landbattle.xml");

    // Left side: every distinct trail value, and whether it is a group, an emitter, or nothing.
    let mut seen: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for p in db.projectiles.iter() {
        let Some(t) = p.trail.as_deref() else { continue };
        seen.entry(t).or_default().push(p.key.as_str());
    }
    println!("--- projectiles.trail: {} distinct values", seen.len());
    for (t, keys) in &seen {
        let kind = if land.has_group(t) {
            "landbattle GROUP"
        } else if land.effects.contains_key(*t) {
            "landbattle EMITTER"
        } else {
            "NOT IN landbattle.xml"
        };
        println!("  {t:34} {kind:22} on {} rows: {}", keys.len(), keys.join(", "));
    }

    // The properties of those rows, to correlate the trail table's floats against.
    println!("\n--- the trail projectiles' decoded columns");
    for p in db.projectiles.iter().filter(|p| p.trail.is_some()) {
        println!(
            "  {:34} {:9} {:9} {:6} {:6} {:6} range {:5} min {:5} traj {:22} tex {:24}",
            p.key,
            p.trail.as_deref().unwrap_or("-"),
            p.calibre,
            p.muzzle_velocity,
            p.effective_range,
            p.minimum_range,
            p.damage,
            0,
            p.trajectory_class,
            p.trail_texture.as_deref().unwrap_or("-"),
        );
    }

    // Right side: the `projectile_trails` table, raw. The two strings and the ten floats, per row.
    println!("\n--- db\\projectile_trails_tables\\projectile_trails");
    let bytes = vfs
        .read("db\\projectile_trails_tables\\projectile_trails")
        .expect("projectile_trails");
    rows(vfs, "db\\projectile_trails_tables\\projectile_trails");
    println!("  {} bytes total", bytes.len());

    // The other side of the bridge: column 6, `trail_texture`, over **all** 144 rows, against the
    // five `projectile_trails` keys. If every value is one of them, column 6 is the lookup.
    let keys: std::collections::BTreeSet<String> =
        strings(&bytes).into_iter().map(|(_, s)| s).collect();
    println!(
        "\n--- projectiles.trail_texture (column 6) over all {} rows; {} projectile_trails keys",
        db.projectiles.len(),
        keys.len()
    );
    let mut tally: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for p in db.projectiles.iter() {
        let v = p.trail_texture.as_deref().unwrap_or("<unset>");
        let n = tally.entry(v).or_default();
        n.0 += 1;
        if p.trail.is_some() {
            n.1 += 1;
        }
    }
    for (v, (n, with_trail)) in &tally {
        let in_table = keys.contains(*v);
        println!(
            "  {v:24} on {n:3} rows ({with_trail:2} of them also name a trail group)  a projectile_trails key: {in_table}"
        );
    }

    // Can any of the ten floats encode the *projectile's* own properties? One `projectile_trails`
    // row serves many projectiles, so if a row spans a wide spread of muzzle velocity or range then
    // no float in it can be a per-shot value. This is the test that decides it either way.
    println!("\n--- the spread of projectile properties inside each projectile_trails row");
    for key in ["alpha", "alpha_bullet", "alpha_shrapnel", "e3_rocket", "none"] {
        let rows: Vec<&ntw_data::schemas::Projectile> =
            db.projectiles.iter().filter(|p| p.trail_texture.as_deref() == Some(key)).collect();
        if rows.is_empty() {
            continue;
        }
        let vmin = rows.iter().map(|p| p.muzzle_velocity).fold(f32::MAX, f32::min);
        let vmax = rows.iter().map(|p| p.muzzle_velocity).fold(f32::MIN, f32::max);
        let rmin = rows.iter().map(|p| p.effective_range).min().unwrap();
        let rmax = rows.iter().map(|p| p.effective_range).max().unwrap();
        let cals: std::collections::BTreeSet<&str> = rows.iter().map(|p| p.calibre.as_str()).collect();
        let trajs: std::collections::BTreeSet<&str> = rows.iter().map(|p| p.trajectory_class.as_str()).collect();
        println!(
            "  {key:14} {:3} rows  muzzle_velocity {vmin:6.1}..{vmax:6.1}  effective_range {rmin:5}..{rmax:5}  calibre {:?}  trajectory {:?}",
            rows.len(),
            cals.iter().collect::<Vec<_>>(),
            trajs.iter().collect::<Vec<_>>(),
        );
    }
}

/// `db\models_artilleries_tables\models_artillery`: one row per gun model, each followed by a large
/// numeric block. The question is whether that block carries an **attachment point** — a muzzle —
/// because that is where the original's muzzle node would live if the gun models hold one.
fn artillery(vfs: &Vfs, filter: &str) {
    let bytes = vfs.read("db\\models_artilleries_tables\\models_artillery").expect("models_artillery");
    println!("{} bytes, header {}", bytes.len(), bytes[0]);
    // Rows start at a length-prefixed string that begins the row (the `model_artillery_*` keys).
    let mut starts = Vec::new();
    let mut i = 0usize;
    while i + 2 <= bytes.len() {
        let n = u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
        if (10..=64).contains(&n) && i + 2 + n * 2 <= bytes.len() {
            let units: Vec<u16> = bytes[i + 2..i + 2 + n * 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            if let Ok(s) = String::from_utf16(&units)
                && (s.starts_with("model_artillery") || s.starts_with("model_ship") || s.starts_with("model_"))
            {
                starts.push(i);
            }
        }
        i += 1;
    }
    println!("{} row keys found", starts.len());
    let mut blocks: Vec<(String, usize, usize)> = Vec::new();
    for (n, &at) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(bytes.len());
        let len = u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
        let key = String::from_utf16(
            &bytes[at + 2..at + 2 + len * 2].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect::<Vec<_>>(),
        )
        .unwrap_or_default();
        if !filter.is_empty() && !key.contains(filter) {
            // Still record the block: the comparison below needs every row.
            let inner_all = strings(&bytes[at..end]);
            let last_all = inner_all
                .last()
                .map(|(o, s)| at + o + 2 + s.encode_utf16().count() * 2)
                .unwrap_or(at);
            blocks.push((key.clone(), last_all, end));
            continue;
        }
        // The strings inside the row, then every non-zero 4-byte run in its numeric block.
        let inner = strings(&bytes[at..end]);
        let paths: Vec<&str> = inner.iter().map(|(_, s)| s.as_str()).filter(|s| s.contains('/') || s.contains('\\')).collect();
        println!(
            "\n{key}  row {n}: {len} bytes  strings: {}",
            inner.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(" | ")
        );
        // Where the numeric block starts: after the last string in the row.
        let last = inner.last().map(|(o, s)| at + o + 2 + s.encode_utf16().count() * 2).unwrap_or(at);
        let mut floats = Vec::new();
        let mut o = last;
        while o + 4 <= end {
            let v = f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
            if v.is_finite() && v.abs() < 1e6 && v != 0.0 {
                floats.push((o - at, v));
            }
            o += 4;
        }
        println!("  numeric block from {last:#x} to {end:#x} ({} bytes), non-zero floats:", end - last);
        for (off, v) in &floats {
            println!("    +{off:#06x} {v:.4}");
        }
        blocks.push((key.clone(), last, end));
        let _ = paths;
    }

    // The decisive test: is that block the same for every gun? If it is, it is a shared rig or
    // collision definition and not a per-gun muzzle attachment.
    println!("\n--- is the numeric block the same for every row?");
    let mut shapes: std::collections::BTreeMap<String, Vec<&str>> = Default::default();
    for (key, from, to) in &blocks {
        let mut h: u64 = 1469598103934665603;
        for b in &bytes[*from..*to] {
            h ^= *b as u64;
            h = h.wrapping_mul(1099511628211);
        }
        shapes.entry(format!("{h:016x} ({} bytes)", to - from)).or_default().push(key);
    }
    for (shape, keys) in &shapes {
        println!("  {shape}: {} rows: {}", keys.len(), keys.join(", "));
    }
    println!("{} distinct blocks over {} rows", shapes.len(), blocks.len());
}

/// Do the gun models themselves carry an attachment point? A model with a named node would have
/// that name as a string in the file; a model that is only a mesh has only texture and shader
/// constant names. This lists every string in every gun model and flags any that is not one of
/// the shipped shader constants or a texture name.
fn gun_models(vfs: &Vfs) {
    /// The named shader constants a `.animatable_rigid_model` carries, from the shipped files.
    const CONSTANTS: &[&str] = &[
        "light_scale", "offsetu0", "offsetv0", "bumpfactor", "specpower", "specbrightness",
        "specularfresnelpower", "glossfactor", "fresnelpower", "reflect_factor", "ambientfactor",
        "colourmapfactor", "specfactor", "rimcolor", "dirtfactor", "alpha", "light_scale0",
    ];
    let paths: Vec<String> = vfs
        .list("enginemodels\\")
        .into_iter()
        .filter(|p| {
            p.contains("cannon")
                || p.contains("howitzer")
                || p.contains("mortar")
                || p.contains("carronade")
                || p.contains("rocket")
        })
        .map(str::to_string)
        .collect();
    println!("{} gun model files", paths.len());
    // A model that carries a named attachment point would hold that name as a string. These are
    // the words a muzzle / socket / node attachment could plausibly be called.
    let words = ["muzzle", "attach", "socket", "node", "barrel", "bore", "trunnion", "flash"];
    let mut total_strings = 0usize;
    let mut names: std::collections::BTreeSet<String> = Default::default();
    let mut hits: Vec<String> = Vec::new();
    for p in &paths {
        let Ok(bytes) = vfs.read(p) else { continue };
        for (_, s) in strings(&bytes) {
            total_strings += 1;
            let lower = s.to_ascii_lowercase();
            // Every distinct string, reduced to its shape so the tally is readable.
            names.insert(if lower.ends_with("_diffuse") || lower.ends_with("_diffuse0") {
                "<texture diffuse>".into()
            } else if lower.ends_with("_normal") || lower.ends_with("_normal0") {
                "<texture normal>".into()
            } else if lower.ends_with("_gloss_map") || lower.ends_with("_gloss_map0") {
                "<texture gloss>".into()
            } else if s.starts_with("building_") {
                "<destruction clip>".into()
            } else {
                s.clone()
            });
            if words.iter().any(|w| lower.contains(w)) {
                hits.push(format!("{p}: {s}"));
            }
        }
    }
    println!("{total_strings} strings over the {} gun model files", paths.len());
    println!("distinct string kinds: {}: {:?}", names.len(), names);
    println!("{} strings naming an attachment point:", hits.len());
    for h in hits.iter().take(20) {
        println!("  {h}");
    }
    let _ = CONSTANTS;
}

/// The per-emitter quad budget. `max_effects` is a constant 50 on all 283 land-battle emitters, so
/// it cannot discriminate; `max_num_quads` has nine values. The question this answers is whether a
/// single release of a group can exceed its own emitter's budget — if it can, the shipped budget is
/// a cap the draw layer should enforce, and one global cap is the wrong shape.
fn budgets(vfs: &Vfs) {
    let lib = ntw_formats::effects::EffectLibrary::from_vfs_path(
        vfs,
        ntw_formats::effects::LAND_BATTLE_EFFECTS,
    )
    .expect("landbattle.xml");

    println!("--- max_effects (the per-instance budget)");
    let mut me: std::collections::BTreeMap<u32, usize> = Default::default();
    for fx in lib.effects.values() {
        *me.entry(fx.max_effects).or_default() += 1;
    }
    println!("  {me:?}  (one value over every emitter = it is not tuned per effect)");

    println!("\n--- max_num_quads");
    let mut q: std::collections::BTreeMap<u32, usize> = Default::default();
    for fx in lib.effects.values() {
        *q.entry(fx.max_num_quads).or_default() += 1;
    }
    println!("  {q:?}");

    // How big is one release of each group, against the smallest budget in it?
    println!("\n--- one release of every group vs the smallest budget in it");
    let mut over: Vec<(String, usize, u32, Vec<&str>)> = Vec::new();
    let mut groups: Vec<(&String, &ntw_formats::effects::EffectGroup)> = lib.groups.iter().collect();
    groups.sort_by_key(|(k, _)| (*k).clone());
    for (name, group) in &groups {
        let entries = &group.entries;
        let total: usize =
            entries.iter().filter_map(|e| lib.effects.get(e)).map(|f| f.num_particles_per_point as usize).sum();
        let budget = entries
            .iter()
            .filter_map(|e| lib.effects.get(e))
            .map(|f| f.max_num_quads)
            .min()
            .unwrap_or(0);
        if (total as u32) > budget {
            let culprits: Vec<&str> = entries
                .iter()
                .filter(|e| {
                    lib.effects
                        .get(*e)
                        .is_some_and(|f| f.num_particles_per_point > f.max_num_quads)
                })
                .map(String::as_str)
                .collect();
            over.push((name.to_string(), total, budget, culprits));
        }
    }
    println!("{} of {} groups release more particles than their own emitters allow", over.len(), groups.len());
    for (name, total, budget, culprits) in &over {
        println!("  {name:44} releases {total:5}  smallest budget {budget:6}  over on: {culprits:?}");
    }

    // The firing groups specifically, since those are the ones that matter at the cap.
    println!("\n--- the firing and dust groups");
    for name in [
        "MusketFire", "rifleFire", "pistolFire", "LandGunFire", "CannonFire", "LandGunFire_small",
        "LandGunFire_large", "LandGunFire_howitzer", "LandGunFire_mortar", "LandGunFire_canister",
        "fougasse_default", "ArtilleryDust", "CavalryDust", "InfantryDust", "dirt_kick",
    ] {
        let Some(group) = lib.groups.get(name) else { continue };
        let entries = &group.entries;
        let total: usize = entries.iter().filter_map(|e| lib.effects.get(e)).map(|f| f.num_particles_per_point as usize).sum();
        let budget = entries.iter().filter_map(|e| lib.effects.get(e)).map(|f| f.max_num_quads).min().unwrap_or(0);
        println!("  {name:22} {total:5} particles, smallest budget {budget:6}, over: {}", (total as u32) > budget);
    }
}

/// Which shipped shader/header files mention `needle`. The particle vertex shader that would
/// handle a facing mode lives in an included file, and the effect file never names it.
fn shader_find(vfs: &Vfs, needle: &str) {
    let mut paths: Vec<String> = vfs
        .list("fx\\")
        .into_iter()
        .map(str::to_string)
        .collect();
    paths.sort();
    for p in &paths {
        let Ok(bytes) = vfs.read(p) else { continue };
        if bytes
            .windows(needle.len())
            .any(|w| w == needle.as_bytes())
        {
            println!("{p} ({}) mentions {needle}", bytes.len());
        }
    }
}

/// A shipped `.fx` shader's own strings, ASCII and UTF-16. The effect file only says which shader an
/// emitter uses, so if a shader names a constant or a semantic that only one facing mode needs, that
/// is where the difference between the modes has to live.
fn shader(vfs: &Vfs, path: &str) {
    let bytes = vfs.read(path).expect("shader");
    println!("{path}: {} bytes", bytes.len());
    // These ship as HLSL source, so print the readable text of the file rather than only its
    // strings: the vertex shader is where a facing mode would be handled.
    let text = String::from_utf8_lossy(&bytes);
    if text.is_ascii() {
        let out = std::path::PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into()))
            .join("tmp")
            .join(format!("{}.txt", path.replace('\\', "_")));
        let _ = std::fs::create_dir_all(out.parent().unwrap());
        let _ = std::fs::write(&out, &bytes);
        println!("wrote {}", out.display());
        for (n, line) in text.lines().enumerate() {
            println!("{:4}| {line}", n + 1);
        }
        return;
    }
    // ASCII runs of 4+ printable characters.
    let mut ascii: std::collections::BTreeSet<String> = Default::default();
    let mut run = String::new();
    for &b in &bytes {
        if b.is_ascii_graphic() || b == b' ' {
            run.push(b as char);
        } else {
            if run.trim().len() >= 4 {
                ascii.insert(run.trim().to_string());
            }
            run.clear();
        }
    }
    if run.trim().len() >= 4 {
        ascii.insert(run.trim().to_string());
    }
    println!("--- {} ASCII strings:", ascii.len());
    for s in &ascii {
        println!("  {s}");
    }
    // UTF-16 runs.
    let mut utf16: std::collections::BTreeSet<String> = Default::default();
    for (_, s) in strings(&bytes) {
        if s.len() >= 4 {
            utf16.insert(s);
        }
    }
    println!("--- {} UTF-16 strings:", utf16.len());
    for s in &utf16 {
        println!("  {s}");
    }
}

/// **`BILLBOARD` against every attribute that might tell it apart from `CAMERA_FACING`.** The draw
/// layer draws the two alike, and round 3 left the reading INFERRED. The question this asks is
/// whether the shipped data separates them at all: if `BILLBOARD` is the *only* mode on
/// `particle.fx`, and every other attribute correlates with the mode rather than with it, then no
/// attribute tells the renderer what to do differently and the drawing is undecidable from the file.
fn billboard(vfs: &Vfs) {
    use ntw_formats::effects::FacingMode;
    let _ = FacingMode::Velocity;
    for (path, label) in [
        (ntw_formats::effects::LAND_BATTLE_EFFECTS, "landbattle"),
        (ntw_formats::effects::NAVAL_BATTLE_EFFECTS, "navalbattle"),
        (ntw_formats::effects::CAMPAIGN_MAP_EFFECTS, "campaignmap"),
    ] {
        let lib = ntw_formats::effects::EffectLibrary::from_vfs_path(vfs, path).expect(path);
        println!("\n=== {label}");
        // Cross-tabulate the mode against every attribute the reader keeps.
        let tab = |label: &str, key: &dyn Fn(&ntw_formats::effects::Effect) -> String| {
            let mut by_mode: std::collections::BTreeMap<String, std::collections::BTreeMap<String, usize>> = Default::default();
            for fx in lib.effects.values() {
                *by_mode
                    .entry(fx.sprite_facing.as_shipped().to_string())
                    .or_default()
                    .entry(key(fx))
                    .or_default() += 1;
            }
            // Does any value of this attribute appear under two different modes? If yes, the
            // attribute does not separate the modes and cannot be what the renderer keys on.
            let mut shared: Vec<(String, Vec<String>)> = Vec::new();
            let mut seen: std::collections::BTreeMap<String, Vec<String>> = Default::default();
            for (mode, values) in &by_mode {
                for v in values.keys() {
                    seen.entry(v.clone()).or_default().push(mode.clone());
                }
            }
            for (v, modes) in &seen {
                if modes.len() > 1 {
                    shared.push((v.clone(), modes.clone()));
                }
            }
            println!("  {label:34} {} distinct, shared across modes: {}", seen.len(), shared.len());
            for (m, values) in &by_mode {
                let listed: Vec<String> = values
                    .iter()
                    .filter(|(v, _)| shared.iter().any(|(sv, _)| sv == *v))
                    .map(|(v, n)| format!("{v} x{n}"))
                    .collect();
                println!("    {m:16} {} values; shared: {}", values.len(), if listed.is_empty() { "-".to_string() } else { listed.join(", ") });
            }
        };
        tab("sprite_facing_mode", &|f: &ntw_formats::effects::Effect| f.sprite_facing.as_shipped().to_string());
        tab("fx (shader)", &|f| f.fx.clone());
        tab("render_method", &|f| format!("{:?}", f.render_method));
        tab("adjust_direction_by_offset", &|f| f.adjust_direction_by_offset.to_string());
        tab("align_to_velocity", &|f| f.align_to_velocity.to_string());
        tab("clamp_sea_level", &|f| f.clamp_sea_level.to_string());
        tab("thickness", &|f| format!("{}", f.thickness.base));
        tab("lighting", &|f| format!("{}", f.lighting.base));
        tab("quality_level", &|f| f.quality_level.to_string());
        tab("emission_type", &|f| format!("{:?}", f.emission_type));
        tab("release_type", &|f| f.release_type.clone());
        tab("start_channels_linked", &|f| format!("{:?}", f.start_channels_linked));
        // The full attribute line of every BILLBOARD emitter, so a hand read can check for anything
        // the struct does not keep.
        let text = String::from_utf8_lossy(&vfs.read(path).expect(path)).into_owned();
        println!("  --- every BILLBOARD emitter, its full RENDERING_VARS line:");
        let mut n = 0;
        for fx in lib.effects.values().filter(|f| f.sprite_facing == FacingMode::Billboard) {
            n += 1;
            let Some(at) = text.find(&format!("name='{}'", fx.name)) else { continue };
            let window = &text[at..(at + 8000).min(text.len())];
            let Some(i) = window.find("SCRIPTED_EFFECT_RENDERING_VARS") else { continue };
            let end = window[i..].find("/>").map(|j| i + j + 2).unwrap_or(window.len());
            println!("    {}", window[i..end].replace('\n', " ").replace('\r', ""));
        }
        println!("  ({n} BILLBOARD emitters in {label})");
    }
}

/// Which emitters are **not** camera facing, and which groups reach them. The reader maps every
/// value that is not `CAMERA_FACING` or `VELOCITY_FACING` to `FacingMode::Other`, and the draw layer
/// draws `Other` as camera facing, so this is where a shipped effect would silently come out wrong.
fn facing(vfs: &Vfs) {
    use ntw_formats::effects::FacingMode;
    let _ = FacingMode::Velocity;
    for (path, label) in [
        (ntw_formats::effects::LAND_BATTLE_EFFECTS, "landbattle"),
        (ntw_formats::effects::NAVAL_BATTLE_EFFECTS, "navalbattle"),
        (ntw_formats::effects::CAMPAIGN_MAP_EFFECTS, "campaignmap"),
    ] {
        let lib = ntw_formats::effects::EffectLibrary::from_vfs_path(vfs, path).expect(path);
        // Read the mode straight out of the file, since the enum collapses what it does not name.
        let text = String::from_utf8_lossy(&vfs.read(path).expect(path)).into_owned();
        let mode_of = |emitter: &str| -> String {
            // `sprite_facing_mode` is on the emitter's `SCRIPTED_EFFECT_RENDERING_VARS` child, not
            // on the tag that names the emitter, so search forward from the name to the next one.
            let Some(at) = text.find(&format!("'{emitter}'")) else { return "?".into() };
            let window = &text[at..(at + 8000).min(text.len())];
            window
                .find("sprite_facing_mode='")
                .and_then(|i| {
                    let after = &window[i + "sprite_facing_mode='".len()..];
                    after.find('\'').map(|j| after[..j].to_string())
                })
                .unwrap_or_else(|| "?".into())
        };
        let mut tally: std::collections::BTreeMap<&str, usize> = Default::default();
        for fx in lib.effects.values() {
            *tally.entry(fx.sprite_facing.as_shipped()).or_default() += 1;
        }
        println!("{label}: {tally:?}");
        // Which emitters, and which groups reach them.
        let odd: Vec<&str> = lib
            .effects
            .values()
            .filter(|f| !matches!(f.sprite_facing, FacingMode::Camera))
            .map(|f| f.name.as_str())
            .collect();
        if odd.is_empty() {
            continue;
        }
        println!("  {} emitters are not camera facing:", odd.len());
        for e in &odd {
            let fx = &lib.effects[*e];
            let groups: Vec<&str> = lib
                .groups
                .values()
                .filter(|g| g.entries.iter().any(|x| x == e))
                .map(|g| g.name.as_str())
                .collect();
            println!(
                "    {e:34} {:14} fx {:28} in {} groups: {}",
                mode_of(e),
                fx.fx,
                groups.len(),
                groups.join(", ")
            );
        }
    }
}

/// Every `projectiles` row's tail columns, decoded with the shipped schema: the columns after
/// `calibre` are the ones the effects notes are about.
fn projectiles(filter: &str) {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let db = ntw_data::GameDatabase::from_install(&dir).expect("database");
    println!("{} projectiles", db.projectiles.len());
    for p in db.projectiles.iter().filter(|p| filter.is_empty() || p.key.contains(filter)) {
        println!(
            "{:34} class {:9} cal {:9} | 31 {:22} 32 {:14} 33 {:26} 34 {:16} | v {:6.1} dmg {:5.2}",
            p.key,
            p.weapon_class,
            p.calibre,
            p.fire_effect.as_deref().unwrap_or("-"),
            p.trail.as_deref().unwrap_or("-"),
            p.impact_ball.as_deref().unwrap_or("-"),
            p.weapon_family.as_deref().unwrap_or("-"),
            p.muzzle_velocity,
            p.damage,
        );
    }
}