//! Research helper for horses / `.variant_weighted_mesh` files. Read-only. Usage:
//!   cargo run -p ntw_formats --example mount_probe -- words <path> <offset> [count]
//!   cargo run -p ntw_formats --example mount_probe -- text <path>          (battleconfiguration fragments)
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    match args[0].as_str() {
        "words" => {
            let b = vfs.read(&args[1]).unwrap();
            let off: usize = parse(&args[2]);
            let n: usize = args.get(3).map_or(64, |s| parse(s));
            for i in 0..n {
                let o = off + i * 4;
                if o + 4 > b.len() {
                    break;
                }
                let w = [b[o], b[o + 1], b[o + 2], b[o + 3]];
                println!(
                    "{o:08x}  {:02x}{:02x}{:02x}{:02x}  u32 {:>11}  f32 {:>12.5}",
                    w[0],
                    w[1],
                    w[2],
                    w[3],
                    u32::from_le_bytes(w),
                    f32::from_le_bytes(w)
                );
            }
        }
        "text" => {
            let b = vfs.read(&args[1]).unwrap();
            print!("{}", String::from_utf8_lossy(&b));
        }
        "wsurvey" => {
            // Parse every .variant_weighted_mesh; report failures and piece names / bone ranges.
            let mut ok = 0;
            let mut bad = 0;
            for p in vfs.packs() {
                for e in p.entries() {
                    if !e.path.to_ascii_lowercase().ends_with(".variant_weighted_mesh") {
                        continue;
                    }
                    let b = vfs.read(&e.path).unwrap();
                    match ntw_formats::weighted_mesh::WeightedMesh::read(&b) {
                        Ok(m) => {
                            ok += 1;
                            if args.get(1).is_some_and(|f| e.path.to_ascii_lowercase().contains(f.as_str())) {
                                let mut maxb = 0;
                                let mut maxn = 0;
                                let mut extra_nonzero = 0;
                                for pc in &m.pieces {
                                    for v in &pc.vertices {
                                        maxn = maxn.max(v.influences.len());
                                        for i in &v.influences { maxb = maxb.max(i.bone); }
                                        if v.extra.iter().any(|x| *x != 0.0) { extra_nonzero += 1; }
                                    }
                                }
                                let names: Vec<String> = m.pieces.iter().map(|p| format!("{}({})", p.name, p.vertices.len())).collect();
                                println!("{} maxbone {maxb} maxinf {maxn} extra!=0 {extra_nonzero}: {}", e.path, names.join(" "));
                            }
                        }
                        Err(err) => {
                            bad += 1;
                            println!("FAIL {}: {err}", e.path);
                        }
                    }
                }
            }
            println!("ok {ok} failed {bad}");
        }
        "uvs" => {
            // UV bounding box per piece.
            let m = ntw_formats::weighted_mesh::WeightedMesh::read(&vfs.read(&args[1]).unwrap()).unwrap();
            for (k, v) in &m.scalars { print!("{k}={v} "); }
            println!();
            for (k, v) in &m.vectors { println!("{k} {v:?}"); }
            for p in &m.pieces {
                let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                let (mut plo, mut phi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for v in &p.vertices {
                    for k in 0..2 { lo[k] = lo[k].min(v.uv[k]); hi[k] = hi[k].max(v.uv[k]); }
                }
                let a = ntw_formats::anim::Anim::read(&vfs.read("animations/animals/horse/horse_stand.anim").unwrap()).unwrap();
                let posed = p.pose(&a.world_matrices(0));
                for q in &posed.positions { for k in 0..3 { plo[k] = plo[k].min(q[k]); phi[k] = phi[k].max(q[k]); } }
                println!("{:<24} uv {:.3?}-{:.3?} pos {:.2?}-{:.2?} first uv {:?}", p.name, lo, hi, plo, phi, p.vertices[0].uv);
            }
        }
        "slots" => {
            // Every slot of an animation table: winning fragment, alternatives, first clip.
            let t = ntw_formats::battle_animation::AnimationTables::from_vfs(&vfs).unwrap();
            let table = t.table(&args[1]).unwrap().clone();
            let mut seen = std::collections::BTreeSet::new();
            for f in &table.fragments {
                if let Some(frag) = t.fragment(&f.name) {
                    for (s, _) in &frag.slots { seen.insert(s.clone()); }
                }
            }
            let filt = args.get(2).map(|s| s.to_ascii_uppercase()).unwrap_or_default();
            for s in seen.iter().filter(|s| s.contains(&filt)) {
                let r = t.resolve(&args[1], s);
                let first = r.first().map(|c| c.clip.filename.as_str()).unwrap_or("-");
                println!("{s:<32} {:<36} x{} {first}", r.first().map(|c| c.fragment.as_str()).unwrap_or("-"), r.len());
            }
        }
        "speed" => {
            // Root speed of clips: horizontal displacement of bone 0 over the clip / duration.
            for p in &args[1..] {
                let a = ntw_formats::anim::Anim::read(&vfs.read(p).unwrap()).unwrap();
                let (s, e) = (a.frames[0][0].translation, a.frames[a.frames.len() - 1][0].translation);
                let d = ((e[0] - s[0]).powi(2) + (e[2] - s[2]).powi(2)).sqrt();
                println!("{p}: frames {} dur {:.3} start {:.3?} end {:.3?} speed {:.3}", a.frames.len(), a.duration, s, e, d / a.duration.max(1e-6));
            }
        }
        other => eprintln!("unknown command {other}"),
    }
}

fn parse(s: &str) -> usize {
    if let Some(h) = s.strip_prefix("0x") { usize::from_str_radix(h, 16).unwrap() } else { s.parse().unwrap() }
}
