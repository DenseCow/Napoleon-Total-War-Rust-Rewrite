//! Research helper for SpeedTree `.spt` files (read-only, through the Vfs).
//!   cargo run -p ntw_formats --example spt_probe -- all            parse every .spt, print failures + section stats
//!   cargo run -p ntw_formats --example spt_probe -- dump <substr>  print one file's parsed contents
//!   cargo run -p ntw_formats --example spt_probe -- tokens <substr> walk the raw top-level layout
use ntw_formats::pack::Vfs;
use ntw_formats::speedtree::SptFile;
use std::collections::BTreeMap;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn spt_paths(vfs: &Vfs) -> Vec<String> {
    let mut v: Vec<String> = vfs
        .packs()
        .iter()
        .flat_map(|p| p.entries().iter().map(|e| e.path.clone()))
        .filter(|p| p.to_ascii_lowercase().ends_with(".spt"))
        .collect();
    v.sort();
    v.dedup();
    v
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let paths = spt_paths(&vfs);
    match args.first().map(String::as_str) {
        Some("all") => {
            let mut sections: BTreeMap<u32, usize> = BTreeMap::new();
            let mut stats: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
            let mut bump = |k: &str, v: String| *stats.entry(k.into()).or_default().entry(v).or_default() += 1;
            let (mut ok, mut bad) = (0, 0);
            for p in &paths {
                let b = vfs.read(p).unwrap();
                match SptFile::read(&b) {
                    Ok(f) => {
                        ok += 1;
                        if f.trailing != 0 {
                            println!("TRAILING {} {p}", f.trailing);
                        }
                        for t in &f.extra.order {
                            *sections.entry(*t).or_default() += 1;
                        }
                        bump("levels", f.tree.branch_levels.len().to_string());
                        bump("leaf textures", f.leaves.textures.len().to_string());
                        for lt in &f.leaves.textures { bump("leaf origin 4004", format!("{:?}", lt.t4004)); bump("leaf size 4005", format!("{:?}", lt.t4005)); }
                        bump("wind", f.wind.is_some().to_string());
                        bump("leaf_lods", f.leaf_lods.is_some().to_string());
                        bump("seed", format!("{:?}", f.tree.seed.map(|s| if s == 0 || s == 1 { s } else { 2 })));
                        bump("t2002", format!("{:?}", f.tree.t2002));
                        bump("t2004", format!("{:?}", f.tree.t2004));
                        for (i, l) in f.tree.branch_levels.iter().enumerate() {
                            bump(&format!("level{i} cross/segs"), format!("{} {}", l.cross_sections, l.segments));
                        }
                        let x = &f.extra;
                        bump("new_wind", format!("{:?}", x.new_wind));
                        bump("22000", format!("{:?}", x.t22000));
                        bump("floor", format!("{:?}", x.floor));
                        bump("cluster", format!("{:?}", x.cluster));
                        bump("16013/16014", format!("{:?} {:?}", x.t16013, x.t16014));
                        bump("root", format!("{:?}", x.root.as_ref().map(|r| r.values.clone())));
                        bump("frond_suppl", format!("{:?}", x.frond_supplement));
                        bump("leaf_placement", format!("{:?}", x.leaf_placement));
                        bump("global_suppl", format!("{:?}", x.global_supplement));
                        for l in &f.tree.branch_levels { bump("flare count", format!("{:?}", l.supplemental.flare.as_ref().map(|f| f.t16003))); }
                        if let Some(fr) = &f.extra.fronds {
                            bump("frond values", format!("{:?}", fr.values.iter().filter(|(t, _)| *t != 13009 && *t < 13010).collect::<Vec<_>>()));
                            bump("frond textures", fr.textures.len().to_string());
                            bump("meshes", format!("{:?}", x.meshes.as_ref().map(|m| m.meshes.iter().map(|m| m.vertices.len()).collect::<Vec<_>>())));
                            bump("leaf mesh flags 72001", format!("{:?}", x.leaf_meshes.as_ref().map(|v| v.iter().map(|l| l.iter().find(|(t, _)| *t == 72001).map(|(_, v)| format!("{v:?}"))).collect::<Vec<_>>())));
                            let ty = fr.values.iter().find(|(t, _)| *t == 13003).map(|(_, v)| format!("{v:?}"));
                            bump("frond type / 13005 variance", format!("{ty:?} {:?}", fr.spline.as_ref().map(|s| s.variance)));
                        }
                    }
                    Err(e) => {
                        bad += 1;
                        println!("FAIL {p}: {e}");
                    }
                }
            }
            println!("{ok} ok, {bad} failed of {}", paths.len());
            println!("sections: {sections:?}");
            for (k, v) in stats {
                println!("{k}: {v:?}");
            }
        }
        Some("dump") => {
            let p = paths.iter().find(|p| p.to_ascii_lowercase().contains(&args[1].to_ascii_lowercase())).unwrap();
            println!("{p}");
            let f = SptFile::read(&vfs.read(p).unwrap()).unwrap();
            let mut s = format!("{f:#?}");
            // drop the 500-entry sample tables
            while let Some(i) = s.find("table: [") {
                let j = i + s[i..].find("],\n").unwrap_or(0);
                s.replace_range(i..j + 2, "table: <500>,");
            }
            println!("{s}");
        }
        Some("atlas") => {
            // atlas <climate folder> <composite stem> <spt substr> <out.png>: the composite diffuse
            // with that tree's leaf (red) and frond (blue) rectangles outlined
            let dir = format!(r"rigidmodels\vegetation\battle\{}", args[1]);
            let bytes = vfs.read(&format!(r"{dir}\textures\{}_diffuse.dds", args[2])).unwrap();
            let dds = ntw_formats::dds::Dds::parse(&bytes).unwrap();
            let (w, h) = (dds.width as usize, dds.height as usize);
            let rgba = dds.decode_rgba8(0);
            let mut rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| {
                let a = u16::from(p[3]);
                [((u16::from(p[0]) * a + 40 * (255 - a)) / 255) as u8, ((u16::from(p[1]) * a + 40 * (255 - a)) / 255) as u8, ((u16::from(p[2]) * a + 40 * (255 - a)) / 255) as u8]
            }).collect();
            let text = String::from_utf8_lossy(&vfs.read(&format!(r"{dir}\{}.txt", args[2])).unwrap()).into_owned();
            let maps = ntw_formats::vegetation::read_composite_map(&text);
            let e = maps.iter().find(|e| e.spt.to_ascii_lowercase().contains(&args[3].to_ascii_lowercase())).expect("no entry");
            println!("{} leaves {:?} fronds {:?}", e.spt, e.leaves, e.fronds);
            let mut outline = |r: &[f32; 4], c: [u8; 3]| {
                let (x0, y0, x1, y1) = ((r[0] * w as f32) as usize, (r[1] * h as f32) as usize, ((r[2] * w as f32) as usize).min(w - 1), ((r[3] * h as f32) as usize).min(h - 1));
                for x in x0..=x1 { for y in [y0, y1] { rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&c); } }
                for y in y0..=y1 { for x in [x0, x1] { rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&c); } }
            };
            for r in &e.leaves { outline(r, [255, 0, 0]); }
            for r in &e.fronds { outline(r, [0, 0, 255]); }
            write_png(&args[4], w as u32, h as u32, &rgb);
        }
        Some("forest") => {
            // the densest 50 m cells of a battle map's tree lists, with their species
            let map = ntw_formats::battle_terrain::BattleMap::load(&vfs, &args[1]).unwrap();
            let mut cells: BTreeMap<(i32, i32), (usize, BTreeMap<String, usize>)> = BTreeMap::new();
            let mut per_list = Vec::new();
            for list in &map.trees {
                let mut n = 0;
                for g in &list.groups {
                    for t in &g.instances {
                        n += 1;
                        let k = ((t.position.0 / 50.0).floor() as i32, (t.position.1 / 50.0).floor() as i32);
                        let e = cells.entry(k).or_default();
                        e.0 += 1;
                        *e.1.entry(g.species.clone()).or_default() += 1;
                    }
                }
                per_list.push(n);
            }
            println!("trees per list {per_list:?}");
            let mut v: Vec<_> = cells.into_iter().filter(|(k, _)| k.0.abs() < 20 && k.1.abs() < 20).collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.1.0));
            for (k, (n, sp)) in v.iter().take(8) {
                println!("cell x {} y {} (map metres {}, {}): {n} trees {sp:?}", k.0, k.1, k.0 * 50 + 25, k.1 * 50 + 25);
            }
        }
        Some("genall") => {
            use ntw_formats::speedtree::generate::compute_tree;
            use ntw_formats::speedtree::params::TreeParams;
            use ntw_formats::vegetation::TreeModelFile;
            let mut ratios = Vec::new();
            for p in &paths {
                let f = SptFile::read(&vfs.read(p).unwrap()).unwrap();
                let tp = TreeParams::from_spt(&f);
                let g = compute_tree(&tp);
                let mut lo = f32::MAX;
                let mut hi = f32::MIN;
                for v in g.mesh.positions.iter().chain(g.leaves.iter().map(|l| &l.pos)) { lo = lo.min(v[1]); hi = hi.max(v[1]); }
                for fr in &g.fronds { for n in &fr.nodes { lo = lo.min(n.pos[2]); hi = hi.max(n.pos[2]); } }
                let dir = p.rsplit_once(92 as char).unwrap().0;
                let tm = vfs.read(&format!("{dir}{}data.tree_model", 92 as char)).ok().and_then(|b| TreeModelFile::read(&b).ok());
                let model_h = tm.as_ref().and_then(|m| m.record(p)).map(|r| r.values[12] - r.values[9]).unwrap_or(0.0);
                let r = (hi - lo) / model_h;
                ratios.push(r);
                println!("{:6.3} gen {:7.2} model {:7.2}  v {:6} lv {:5} fr {:4} ft {} {}", r, hi - lo, model_h, g.mesh.positions.len(), g.leaves.len(), g.fronds.len(), tp.frond_type, p.rsplit(92 as char).next().unwrap());
            }
            ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!("ratio min {} median {} max {}", ratios[0], ratios[ratios.len() / 2], ratios[ratios.len() - 1]);
        }
        Some("gen") => generate_cmd(&vfs, &paths, &args[1], args.get(2).map(String::as_str)),
        _ => eprintln!("usage: spt_probe all | dump <substr> | gen <substr> [out.png]"),
    }
}

/// Minimal PNG writer (RGB8) for research previews.
pub fn write_png(path: &str, w: u32, h: u32, rgb: &[u8]) {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let mut raw = Vec::with_capacity((w * 3 + 1) as usize * h as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w as usize * 3..(y + 1) * w as usize * 3]);
    }
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
    let mut f = b"\x89PNG\r\n\x1a\n".to_vec();
    let chunk = |f: &mut Vec<u8>, ty: &[u8], data: &[u8]| {
        f.extend((data.len() as u32).to_be_bytes());
        let mut c = ty.to_vec();
        c.extend_from_slice(data);
        f.extend_from_slice(&c);
        f.extend(crc(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(&mut f, b"IHDR", &ihdr);
    chunk(&mut f, b"IDAT", &z);
    chunk(&mut f, b"IEND", &[]);
    std::fs::write(path, f).unwrap();
}

/// Generates one tree, prints statistics, compares to data.tree_model, and draws a side view.
fn generate_cmd(vfs: &Vfs, paths: &[String], needle: &str, png: Option<&str>) {
    use ntw_formats::speedtree::generate::compute_tree;
    use ntw_formats::speedtree::params::TreeParams;
    use ntw_formats::vegetation::TreeModelFile;
    let p = paths.iter().find(|p| p.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())).expect("no such .spt");
    let f = SptFile::read(&vfs.read(p).unwrap()).unwrap();
    let t = TreeParams::from_spt(&f);
    let t0 = std::time::Instant::now();
    let g = compute_tree(&t);
    let dt = t0.elapsed();
    let m = &g.mesh;
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in m.positions.iter().chain(g.leaves.iter().map(|l| &l.pos)) {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let tris: usize = m.lod_strips.first().map(|s| s.iter().map(|x| x.len().saturating_sub(2)).sum()).unwrap_or(0);
    println!("{p}\n size {:.3} (file {} ± {}), seed {}, {} levels", g.size, t.size, t.size_variance, t.seed, t.levels.len());
    println!(" {} branches, {} vertices, {} strip triangles, {} leaves, {} fronds, {:?}", g.branches.len(), m.positions.len(), tris, g.leaves.len(), g.fronds.len(), dt);
    println!(" bounds (Y-up) min {lo:?} max {hi:?}");
    let dir = p.rsplit_once('\\').unwrap().0;
    if let Ok(tm) = vfs.read(&format!("{dir}\\data.tree_model")).map(|b| TreeModelFile::read(&b).unwrap())
        && let Some(r) = tm.record(p)
    {
        println!(" data.tree_model: {:?}", r.values);
    }
    let Some(out) = png else { return };
    // side view: x right, y up; plus a top view on the right half
    let (w, h) = (1024usize, 768usize);
    let mut img = vec![30u8; w * h * 3];
    let mut depth = vec![f32::MIN; w * h];
    let span = (hi[1] - lo[1]).max(hi[0] - lo[0]).max(hi[2] - lo[2]).max(1e-3) * 1.05;
    let s = (h as f32 - 20.0) / span;
    let to_px = |v: [f32; 3], half: usize| -> (f32, f32, f32) {
        if half == 0 {
            (256.0 + (v[0] - (lo[0] + hi[0]) * 0.5) * s, h as f32 - 10.0 - (v[1] - lo[1]) * s, v[2])
        } else {
            (768.0 + (v[0] - (lo[0] + hi[0]) * 0.5) * s, h as f32 * 0.5 - (v[2] - (lo[2] + hi[2]) * 0.5) * s, v[1])
        }
    };
    let mut tri = |a: (f32, f32, f32), b: (f32, f32, f32), c: (f32, f32, f32), col: [u8; 3]| {
        let minx = a.0.min(b.0).min(c.0).floor().max(0.0) as usize;
        let maxx = (a.0.max(b.0).max(c.0).ceil() as usize).min(w - 1);
        let miny = a.1.min(b.1).min(c.1).floor().max(0.0) as usize;
        let maxy = (a.1.max(b.1).max(c.1).ceil() as usize).min(h - 1);
        let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if area.abs() < 1e-6 || minx > maxx || miny > maxy {
            return;
        }
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((b.0 - px) * (c.1 - py) - (b.1 - py) * (c.0 - px)) / area;
                let w1 = ((c.0 - px) * (a.1 - py) - (c.1 - py) * (a.0 - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a.2 + w1 * b.2 + w2 * c.2;
                let i = y * w + x;
                if z > depth[i] {
                    depth[i] = z;
                    img[i * 3..i * 3 + 3].copy_from_slice(&col);
                }
            }
        }
    };
    for half in 0..2 {
        for strip in m.lod_strips.first().into_iter().flatten() {
            for k in 2..strip.len() {
                let (i0, i1, i2) = (strip[k - 2] as usize, strip[k - 1] as usize, strip[k] as usize);
                if i0 == i1 || i1 == i2 || i0 == i2 || i2 >= m.positions.len() {
                    continue;
                }
                let n = m.normals[i0];
                let l = (n[0] * 0.5 + n[1] * 0.7 + n[2] * 0.5).abs().min(1.0) * 0.8 + 0.2;
                let col = [(150.0 * l) as u8, (110.0 * l) as u8, (80.0 * l) as u8];
                tri(to_px(m.positions[i0], half), to_px(m.positions[i1], half), to_px(m.positions[i2], half), col);
            }
        }
        for lf in &g.leaves {
            let (x, y, z) = to_px(lf.pos, half);
            let r = (t.leaf_textures[lf.texture].size[0] * s * 0.5).max(1.0);
            tri((x - r, y - r, z), (x + r, y - r, z), (x, y + r, z), [60, 140, 50]);
        }
        let fm = &g.frond_mesh;
        for strip in &fm.strips {
            for k in 2..strip.len() {
                let (i0, i1, i2) = (strip[k - 2] as usize, strip[k - 1] as usize, strip[k] as usize);
                let n = fm.normals[i0];
                let l = (n[0] * 0.5 + n[1] * 0.7 + n[2] * 0.5).abs().min(1.0) * 0.6 + 0.4;
                tri(to_px(fm.positions[i0], half), to_px(fm.positions[i1], half), to_px(fm.positions[i2], half), [(60.0 * l) as u8, (150.0 * l) as u8, (70.0 * l) as u8]);
            }
        }
    }
    write_png(out, w as u32, h as u32, &img);
    println!(" wrote {out}");
}
