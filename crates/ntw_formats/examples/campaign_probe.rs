//! Research helper for the campaign map (read-only, through the Vfs).
//!   cargo run -p ntw_formats --example campaign_probe -- tree <vfs path> [max_depth] [max_items]
//!   cargo run -p ntw_formats --example campaign_probe -- hex <vfs path> [len] [offset]
use ntw_formats::campaign_map::{GameFiles, Heightmap, RegionMap, SplineFile, SuperTexture};
use ntw_formats::dds::{DdsFormat, decode_blocks_rgba8};
use ntw_formats::esf::{EsfFile, EsfNode};
use std::collections::BTreeMap;
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn short(n: &EsfNode) -> String {
    let s = format!("{n:?}");
    if s.len() > 110 { format!("{}...", &s[..s.char_indices().nth(110).map_or(s.len(), |c| c.0)]) } else { s }
}

fn dump(nodes: &[EsfNode], depth: usize, max_depth: usize, max_items: usize) {
    let pad = "  ".repeat(depth);
    for (i, n) in nodes.iter().enumerate() {
        if i >= 40 {
            println!("{pad}... {} more children", nodes.len() - i);
            break;
        }
        match n {
            EsfNode::Record(r) => {
                println!("{pad}#{i} REC {} v{} ({} children)", r.name, r.version, r.children.len());
                if depth < max_depth {
                    dump(&r.children, depth + 1, max_depth, max_items);
                }
            }
            EsfNode::RecordArray(a) => {
                println!("{pad}#{i} ARR {} v{} ({} items)", a.name, a.version, a.items.len());
                if depth < max_depth {
                    for (j, item) in a.items.iter().take(max_items).enumerate() {
                        println!("{pad}  [item {j}]");
                        dump(item, depth + 2, max_depth, max_items);
                    }
                }
            }
            other => println!("{pad}#{i} {}", short(other)),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let files = GameFiles { vfs: &vfs };
    match args[0].as_str() {
        "tree" => {
            // A path on disk (e.g. a save) is read directly; anything else goes through the game files.
            let bytes = if std::path::Path::new(&args[1]).is_file() { std::fs::read(&args[1]).unwrap() } else { files.read(&args[1]).unwrap() };
            let esf = EsfFile::from_bytes(&bytes).unwrap();
            let d = args.get(2).map_or(3, |s| s.parse().unwrap());
            let m = args.get(3).map_or(1, |s| s.parse().unwrap());
            println!("REC {} v{}", esf.root.name, esf.root.version);
            dump(&esf.root.children, 1, d, m);
        }
        "hex" => {
            let b = files.read(&args[1]).unwrap();
            let len: usize = args.get(2).map_or(256, |s| s.parse().unwrap());
            let off: usize = args.get(3).map_or(0, |s| s.parse().unwrap());
            println!("len {}", b.len());
            for (i, c) in b[off..b.len().min(off + len)].chunks(16).enumerate() {
                let hex: Vec<String> = c.iter().map(|x| format!("{x:02x}")).collect();
                let asc: String = c.iter().map(|&x| if (32..127).contains(&x) { x as char } else { '.' }).collect();
                println!("{:08x}  {:<48} {}", off + i * 16, hex.join(" "), asc);
            }
        }
        "regions" => {
            let rm = RegionMap::read(&files.read(&format!("campaign_maps/{}/regions.esf", args[1])).unwrap()).unwrap();
            println!("bounds {:?}..{:?} theatre {:?} verts {} regions {} labels {}", rm.bounds_min, rm.bounds_max, rm.theatre, rm.vertices.len(), rm.regions.len(), rm.labels.len());
            for r in rm.regions.iter().take(args.get(2).map_or(5, |s| s.parse().unwrap())) {
                println!("  {} sea={} {:?}..{:?} areas {} faces {} settlement {:?}", r.key, r.is_sea, r.min, r.max, r.areas.len(), r.areas.iter().map(|a| a.faces.len() / 3).sum::<usize>(), r.settlement);
            }
            println!("with settlement: {}", rm.regions.iter().filter(|r| r.settlement.is_some()).count());
            println!("paris region: {:?}", rm.region_at(-212.2, 2.3).map(|r| &r.key));
        }
        "fit" => {
            // Border spline display bboxes vs region logic bboxes: least squares logic = a*disp + b per axis.
            let map = &args[1];
            let rm = RegionMap::read(&files.read(&format!("campaign_maps/{map}/regions.esf")).unwrap()).unwrap();
            let mut bb: BTreeMap<String, [f32; 4]> = BTreeMap::new();
            for p in files.list(&format!("campaign_maps/{map}/display/borders/")) {
                let f = SplineFile::read(&files.read(&p).unwrap()).unwrap();
                for s in &f.splines {
                    let key = s.name.split(':').nth(1).unwrap_or("").to_owned();
                    let e = bb.entry(key).or_insert([f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
                    for q in &s.points {
                        e[0] = e[0].min(q[0]);
                        e[1] = e[1].min(q[2]);
                        e[2] = e[2].max(q[0]);
                        e[3] = e[3].max(q[2]);
                    }
                }
            }
            let (mut xs, mut zs) = (Vec::new(), Vec::new());
            for (k, d) in &bb {
                let Some(r) = rm.regions.iter().find(|r| &r.key == k) else { continue };
                // Region bboxes use outline vertices; use them directly.
                xs.push((d[0], r.min.0));
                xs.push((d[2], r.max.0));
                zs.push((d[1], r.min.1));
                zs.push((d[3], r.max.1));
                zs.push((d[1], -r.max.1));
            }
            let fit = |v: &[(f32, f32)]| {
                let n = v.len() as f64;
                let (sx, sy) = v.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 as f64, a.1 + p.1 as f64));
                let (mx, my) = (sx / n, sy / n);
                let (mut sxy, mut sxx) = (0.0, 0.0);
                for p in v {
                    sxy += (p.0 as f64 - mx) * (p.1 as f64 - my);
                    sxx += (p.0 as f64 - mx).powi(2);
                }
                let a = sxy / sxx;
                let b = my - a * mx;
                let rms = (v.iter().map(|p| (a * p.0 as f64 + b - p.1 as f64).powi(2)).sum::<f64>() / n).sqrt();
                (a, b, rms)
            };
            let zs_pos: Vec<_> = zs.iter().copied().enumerate().filter(|(i, _)| i % 3 != 2).map(|x| x.1).collect();
            let zs_neg: Vec<_> = zs.chunks(3).flat_map(|c| [(c[0].0, c[2].1)]).collect();
            println!("regions matched {}", xs.len() / 2);
            println!("x: {:?}", fit(&xs));
            println!("z (same sign): {:?}", fit(&zs_pos));
            println!("z (flipped, min->-max): {:?}", fit(&zs_neg));
        }
        "fit2" => {
            // Mean distance from scaled border points to the nearest regions.esf vertex, for a range of scales/offsets.
            let map = &args[1];
            let rm = RegionMap::read(&files.read(&format!("campaign_maps/{map}/regions.esf")).unwrap()).unwrap();
            let mut grid: std::collections::HashMap<(i32, i32), Vec<(f32, f32)>> = Default::default();
            for &v in &rm.vertices {
                grid.entry(((v.0 / 4.0).floor() as i32, (v.1 / 4.0).floor() as i32)).or_default().push(v);
            }
            let near = |x: f32, z: f32| {
                let (cx, cz) = ((x / 4.0).floor() as i32, (z / 4.0).floor() as i32);
                let mut best = f32::MAX;
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        for v in grid.get(&(cx + dx, cz + dz)).into_iter().flatten() {
                            best = best.min(((v.0 - x).powi(2) + (v.1 - z).powi(2)).sqrt());
                        }
                    }
                }
                best.min(6.0)
            };
            let mut pts = Vec::new();
            for p in files.list(&format!("campaign_maps/{map}/display/{}/", args.get(2).map_or("borders", |s| s.as_str()))).into_iter().filter(|p| p.ends_with(".rigid_spline")) {
                for s in SplineFile::read(&files.read(&p).unwrap()).unwrap().splines {
                    pts.extend(s.points.iter().step_by(3).map(|q| (q[0], q[2])));
                }
            }
            let score = |s: f32, ox: f32, oz: f32, flip: f32| pts.iter().map(|p| near(p.0 * s + ox, flip * p.1 * s + oz)).sum::<f32>() / pts.len() as f32;
            let mut best = (f32::MAX, 0.0, 0.0, 0.0, 0.0);
            for flip in [1.0f32, -1.0] {
                for si in 0..=40 {
                    let s = 36.0 + si as f32 * 0.25;
                    let v = score(s, 0.0, 0.0, flip);
                    if v < best.0 { best = (v, s, 0.0, 0.0, flip); }
                }
            }
            println!("coarse best {best:?} ({} points)", pts.len());
            let (_, s0, _, _, flip) = best;
            for si in -20..=20 {
                for ox in -8..=8 {
                    for oz in -8..=8 {
                        let s = s0 + si as f32 * 0.02;
                        let v = score(s, ox as f32 * 0.5, oz as f32 * 0.5, flip);
                        if v < best.0 { best = (v, s, ox as f32 * 0.5, oz as f32 * 0.5, flip); }
                    }
                }
            }
            println!("fine best (mean dist, scale, off_x, off_z, z_sign) {best:?}");
        }
        "hfit" => {
            // Heightmap value vs spline height for road/river points, both row orientations.
            let map = &args[1];
            let rm = RegionMap::read(&files.read(&format!("campaign_maps/{map}/regions.esf")).unwrap()).unwrap();
            let hm = Heightmap::read(&files.read(&format!("campaign_maps/{map}/display/heightmap/heightmap.tga")).unwrap()).unwrap();
            let (mn, mx) = (rm.bounds_min, rm.bounds_max);
            println!("heightmap {}x{} bounds {mn:?}..{mx:?}", hm.width, hm.height);
            let mut pts = Vec::new();
            for kind in ["roads", "rivers", "borders"] {
                for p in files.list(&format!("campaign_maps/{map}/display/{kind}/")).into_iter().filter(|p| p.ends_with(".rigid_spline")) {
                    for s in SplineFile::read(&files.read(&p).unwrap()).unwrap().splines {
                        pts.extend(s.points.iter().filter(|q| q[1] > 0.0).map(|q| (q[0] / 0.0254, q[2] / 0.0254, q[1])));
                    }
                }
            }
            // Land vs sea: mean heightmap value at triangle centroids of land and sea regions.
            for north_up in [true, false] {
                let (mut land, mut sea) = ((0.0f64, 0usize), (0.0f64, 0usize));
                let mut hist_sea = [0usize; 8];
                for r in &rm.regions {
                    for a in &r.areas {
                        for t in a.faces.as_chunks::<3>().0.iter().step_by(5) {
                            let v = |i: u32| rm.vertices[i as usize];
                            let (x, z) = ((v(t[0]).0 + v(t[1]).0 + v(t[2]).0) / 3.0, (v(t[0]).1 + v(t[1]).1 + v(t[2]).1) / 3.0);
                            let u = (x - mn.0) / (mx.0 - mn.0) * hm.width as f32;
                            let tt = if north_up { (mx.1 - z) / (mx.1 - mn.1) } else { (z - mn.1) / (mx.1 - mn.1) };
                            let h = hm.sample(u, tt * hm.height as f32) as f64;
                            if r.is_sea { sea.0 += h; sea.1 += 1; hist_sea[(h as usize) / 32] += 1; } else { land.0 += h; land.1 += 1; }
                        }
                    }
                }
                println!("north_up={north_up}: land mean {:.1} ({}), sea mean {:.1} ({}) sea hist {hist_sea:?}", land.0 / land.1 as f64, land.1, sea.0 / sea.1 as f64, sea.1);
            }
            // Slot positions carry logic heights: fit y = k * h + c.
            let mut v = Vec::new();
            let mut types: BTreeMap<String, usize> = BTreeMap::new();
            for r in &rm.regions {
                for s in r.settlement.iter().flat_map(|s| &s.slots) {
                    *types.entry(s.slot_type.clone()).or_default() += 1;
                    let (x, y, z) = s.position;
                    let u = (x - mn.0) / (mx.0 - mn.0) * hm.width as f32;
                    let t = (mx.1 - z) / (mx.1 - mn.1) * hm.height as f32;
                    v.push((hm.sample(u, t) as f64, y as f64));
                }
            }
            println!("slot types {types:?}");
            let n = v.len() as f64;
            let (ma, mb) = (v.iter().map(|p| p.0).sum::<f64>() / n, v.iter().map(|p| p.1).sum::<f64>() / n);
            let sxy: f64 = v.iter().map(|p| (p.0 - ma) * (p.1 - mb)).sum();
            let sxx: f64 = v.iter().map(|p| (p.0 - ma).powi(2)).sum();
            let syy: f64 = v.iter().map(|p| (p.1 - mb).powi(2)).sum();
            let k = sxy / sxx;
            let rms = (v.iter().map(|p| (k * p.0 + mb - k * ma - p.1).powi(2)).sum::<f64>() / n).sqrt();
            println!("slots {}: corr {:.4} y = {k:.6} * h + {:.6} rms {rms:.4}; ratio y/h median {:.6}", v.len(), sxy / (sxx * syy).sqrt(), mb - k * ma, {
                let mut r: Vec<f64> = v.iter().filter(|p| p.0 > 2.0).map(|p| p.1 / p.0).collect();
                r.sort_by(f64::total_cmp);
                r.get(r.len() / 2).copied().unwrap_or(0.0)
            });
            let mut hist = [0usize; 16];
            hm.values.iter().for_each(|&v| hist[v as usize / 16] += 1);
            println!("heightmap histogram /16: {hist:?}");
            for north_up in [true, false] {
                let v: Vec<(f64, f64)> = pts
                    .iter()
                    .map(|&(x, z, y)| {
                        let u = (x - mn.0) / (mx.0 - mn.0) * hm.width as f32;
                        let t = if north_up { (mx.1 - z) / (mx.1 - mn.1) } else { (z - mn.1) / (mx.1 - mn.1) };
                        (hm.sample(u, t * hm.height as f32) as f64, y as f64)
                    })
                    .collect();
                let n = v.len() as f64;
                let (mx_, my) = (v.iter().map(|p| p.0).sum::<f64>() / n, v.iter().map(|p| p.1).sum::<f64>() / n);
                let sxy: f64 = v.iter().map(|p| (p.0 - mx_) * (p.1 - my)).sum();
                let sxx: f64 = v.iter().map(|p| (p.0 - mx_).powi(2)).sum();
                let syy: f64 = v.iter().map(|p| (p.1 - my).powi(2)).sum();
                let a = sxy / sxx;
                println!("north_up={north_up}: n {} corr {:.4} y = {:.6e} * h + {:.6e}", v.len(), sxy / (sxx * syy).sqrt(), a, my - a * mx_);
            }
        }
        "ddspng" => {
            // ddspng <vfs path> <out.png>: mip 0 of any DDS, decoded to RGBA, for looking at atlases.
            let b = vfs.read(&args[1]).expect("read");
            let d = ntw_formats::dds::Dds::parse(&b).expect("dds");
            write_png(&args[2], d.width, d.height, &d.decode_rgba8(0));
            println!("{}x{} {:?}", d.width, d.height, d.format);
        }
        "hpng" => {
            // heightmap downsampled 4x, contrast-stretched, as PNG.
            let hm = Heightmap::read(&files.read(&format!("campaign_maps/{}/display/heightmap/heightmap.tga", args[1])).unwrap()).unwrap();
            let (w, h) = (hm.width / 4, hm.height / 4);
            let mut px = Vec::new();
            for r in 0..h {
                for c in 0..w {
                    let v = hm.at(c as i64 * 4, r as i64 * 4);
                    let g = (u32::from(v) * 4).min(255) as u8;
                    px.extend_from_slice(&[g, g, g, 255]);
                }
            }
            write_png(&args[2], w, h, &px);
        }
        "tile" => {
            // tile <map> <level> <index> <out.png> [fmt]
            let map = &args[1];
            let base = format!("campaign_maps/{map}/display/supertexture/supertexture");
            let st = SuperTexture::read_index(&files.read(&format!("{base}.stpi")).unwrap()).unwrap();
            let stpd = files.read(&format!("{base}.stpd")).unwrap();
            println!("{}x{} tile {} levels {:?} unk {}", st.width, st.height, st.tile_size, st.levels.iter().map(|l| (l.tiles_x, l.tiles_y)).collect::<Vec<_>>(), st.unknown_6);
            let level: usize = args[2].parse().unwrap();
            let idx: usize = args[3].parse().unwrap();
            let t = st.levels[level].tiles[idx];
            println!("tile {t:?}");
            let raw = SuperTexture::inflate_tile(&stpd, &t, idx).unwrap();
            println!("first 32 bytes {:02x?}", &raw[..32]);
            let fmt = match args.get(5).map(String::as_str) { Some("dxt1") => DdsFormat::Dxt1, Some("dxt3") => DdsFormat::Dxt3, _ => DdsFormat::Dxt5 };
            let (w, h) = if fmt == DdsFormat::Dxt1 { (512, 1024) } else { (512, 512) };
            let rgba = decode_blocks_rgba8(fmt, w, h, &raw);
            write_png(&args[4], w, h, &rgba);
            let opaque: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
            write_png(&format!("{}_rgb.png", args[4]), w, h, &opaque);
            let alpha: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[3], p[3], p[3], 255]).collect();
            write_png(&format!("{}_a.png", args[4]), w, h, &alpha);
        }
        _ => eprintln!("unknown command"),
    }
}

/// Minimal PNG writer (RGBA8) for looking at decoded data. Research output only.
pub fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let mut raw = Vec::with_capacity((w * h * 4 + h) as usize);
    for row in rgba.chunks_exact(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |ty: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = ty.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc(&c).to_be_bytes());
    };
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, out).unwrap();
}
