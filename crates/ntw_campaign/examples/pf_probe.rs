//! Research probe for `pathfinding.esf` (read-only). Prints statistics used to work out the layout.
//! `cargo run -p ntw_campaign --example pf_probe -- <map dir> <cmd> [args]`

use std::collections::BTreeMap;

use ntw_formats::campaign_map::RegionMap;
use ntw_formats::campaign_pathfinding::PathfindingFile;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::path::PathBuf::from(&args[0]);
    let pf = PathfindingFile::read(&std::fs::read(dir.join("pathfinding.esf")).unwrap()).unwrap();
    let regions = RegionMap::read(&std::fs::read(dir.join("regions.esf")).unwrap()).unwrap();
    let a = &pf.areas[0];
    let g = &a.grid;
    let lists = a.outline_lists().unwrap();
    let by_off: BTreeMap<usize, &[u32]> = lists.iter().map(|(o, l)| (*o, *l)).collect();
    let cells = g.expand();
    let cmd = args.get(1).map(String::as_str).unwrap_or("fields");
    match cmd {
        "fields" => {
            let sets = g.region_sets().unwrap();
            let mut by: BTreeMap<(&str, u8), usize> = BTreeMap::new();
            for c in &cells { for b in &c.boundaries { let r = b.region() as usize; let k = if r == 1023 { "none" } else if sets[r].is_empty() { "sea" } else if sets[r].len() == 1 { "land1" } else { "landN" }; *by.entry((k, (b.flags & 0xff) as u8)).or_default() += 1; } }
            println!("byte0 by kind {by:?}");
            // region field vs list length, flags bytes histogram
            let mut f_hist: BTreeMap<u32, usize> = BTreeMap::new();
            let mut reg: BTreeMap<u16, usize> = BTreeMap::new();
            for c in &cells {
                for b in &c.boundaries {
                    *f_hist.entry(b.flags).or_default() += 1;
                    *reg.entry(b.region()).or_default() += 1;
                }
            }
            println!("distinct flags {}: first {:?}", f_hist.len(), f_hist.iter().take(30).map(|(k, v)| format!("{k:08x}:{v}")).collect::<Vec<_>>());
            println!("regions {:?}", reg.iter().take(80).collect::<Vec<_>>());
            println!("region_map {:?}", g.region_map);
            println!("region_table {:?}", &g.region_table[..g.region_table.len().min(120)]);
            for i in 0..g.region_map.len().min(5) {
                println!("pf region {i} -> {}", regions.regions[g.region_map[i] as usize].key);
            }
        }
        "cell" => {
            let col: usize = args[2].parse().unwrap();
            let row: usize = args[3].parse().unwrap();
            let i = row * g.cols as usize + col;
            let c = &cells[i];
            println!("cell {i} header {:?}", c.header);
            for b in &c.boundaries {
                let l = by_off.get(&b.list_offset()).copied().unwrap_or(&[]);
                let pts: Vec<String> = l.iter().map(|&v| match a.vertex_units(v as usize) { Some((x, z)) => format!("{v}({x:.2},{z:.2})"), None => format!("S{v}") }).collect();
                println!("  flags {:08x} region {} off {} -> {}", b.flags, b.region(), b.list_offset(), pts.join(" "));
            }
        }
        "headers" => {
            // The 8 header bytes: per-position histograms, and how each byte relates to the
            // cell's own content and to its 4 neighbours (research for the UNKNOWN header).
            let (cols, rows) = (g.cols as usize, g.rows as usize);
            let class = |c: &ntw_formats::campaign_pathfinding::GridCell| -> u8 {
                // 0 all land single region, 1 all sea, 2 all off-map, 3 mixed/split
                if c.boundaries.len() == 1 {
                    let r = c.boundaries[0].region();
                    let k = c.boundaries[0].flags & 7;
                    if r == 1023 { 2 } else if k == 1 { 1 } else if k == 0 { 0 } else { 3 }
                } else {
                    3
                }
            };
            for p in 0..8 {
                let mut h: BTreeMap<u8, usize> = BTreeMap::new();
                for c in &cells {
                    *h.entry(c.header[p]).or_default() += 1;
                }
                let top: Vec<String> = h.iter().map(|(k, v)| format!("{k}:{v}")).take(40).collect();
                println!("byte {p}: {} values {}", h.len(), top.join(" "));
            }
            // Mean of each byte by the cell's own class.
            let mut by: BTreeMap<u8, ([u64; 8], u64)> = BTreeMap::new();
            for c in &cells {
                let e = by.entry(class(c)).or_insert(([0; 8], 0));
                for p in 0..8 {
                    e.0[p] += u64::from(c.header[p]);
                }
                e.1 += 1;
            }
            for (k, (s, n)) in &by {
                let m: Vec<String> = s.iter().map(|v| format!("{:.1}", *v as f64 / *n as f64)).collect();
                println!("class {k} ({n} cells): mean bytes {}", m.join(" "));
            }
            // Byte value vs distance (in cells, Chebyshev, up to 40) to the nearest cell of another class.
            let classes: Vec<u8> = cells.iter().map(class).collect();
            let mut agree = [[0usize; 2]; 8];
            for row in 0..rows {
                for col in 0..cols {
                    let i = row * cols + col;
                    let mut d = 255u8;
                    'r: for r in 1..=40usize {
                        for dr in -(r as i64)..=(r as i64) {
                            for dc in -(r as i64)..=(r as i64) {
                                if dr.unsigned_abs() as usize != r && dc.unsigned_abs() as usize != r {
                                    continue;
                                }
                                let (rr, cc) = (row as i64 + dr, col as i64 + dc);
                                if rr < 0 || cc < 0 || rr >= rows as i64 || cc >= cols as i64 {
                                    continue;
                                }
                                if classes[rr as usize * cols + cc as usize] != classes[i] {
                                    d = r as u8;
                                    break 'r;
                                }
                            }
                        }
                    }
                    for p in 0..8 {
                        agree[p][usize::from(cells[i].header[p] == d)] += 1;
                    }
                }
            }
            println!("byte == distance to another class (cells): {:?}", agree.map(|a| a[1]));
            // Is byte p == 255 exactly when the neighbour in direction d is off-map? (8-neighbourhood,
            // row-major from the south-west: SW S SE W E NW N NE)
            let dirs: [(i64, i64); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
            for p in 0..8 {
                let mut line = format!("byte {p} 255<->neighbour off-map:");
                for (dc, dr) in dirs {
                    let (mut same, mut n) = (0usize, 0usize);
                    for row in 1..rows - 1 {
                        for col in 1..cols - 1 {
                            let i = row * cols + col;
                            if classes[i] == 2 {
                                continue;
                            }
                            let j = ((row as i64 + dr) as usize) * cols + (col as i64 + dc) as usize;
                            same += usize::from((cells[i].header[p] == 255) == (classes[j] == 2));
                            n += 1;
                        }
                    }
                    line += &format!(" {:.3}", same as f64 / n.max(1) as f64);
                }
                println!("{line}");
            }
            // Shared values: byte p of a cell == byte q of its neighbour (dc, dr), over cells where
            // the value is not the common 31/32 (so equality is informative).
            for (dc, dr) in dirs {
                for p in 0..8 {
                    for q in 0..8 {
                        let (mut eq, mut n) = (0usize, 0usize);
                        for row in 1..rows - 1 {
                            for col in 1..cols - 1 {
                                let v = cells[row * cols + col].header[p];
                                if (30..=33).contains(&v) || v == 255 {
                                    continue;
                                }
                                let j = ((row as i64 + dr) as usize) * cols + (col as i64 + dc) as usize;
                                eq += usize::from(cells[j].header[q] == v);
                                n += 1;
                            }
                        }
                        if n > 100 && eq * 10 > n * 8 {
                            println!("byte {p} == neighbour ({dc},{dr}) byte {q}: {eq}/{n}");
                        }
                    }
                }
            }
            // Correlation of each byte (255 excluded) with the heightmap at 9 points of the cell.
            if let Ok(b) = std::fs::read(dir.join("display/heightmap/heightmap.tga")) {
                let hm = ntw_formats::campaign_map::Heightmap::read(&b).unwrap();
                let (mn, mx) = (regions.bounds_min, regions.bounds_max);
                let h = |x: f32, z: f32| {
                    let u = (x - mn.0) / (mx.0 - mn.0) * hm.width as f32;
                    let v = (mx.1 - z) / (mx.1 - mn.1) * hm.height as f32;
                    hm.sample(u, v)
                };
                let (x0, z0) = g.origin_units();
                let cs = g.cell_units();
                if let (Some(r), Some(c0)) = (args.get(2).and_then(|s| s.parse::<usize>().ok()), args.get(3).and_then(|s| s.parse::<usize>().ok())) {
                    for col in c0..(c0 + 24).min(cols) {
                        let (x, z) = (x0 + (col as f32 + 0.5) * cs, z0 + (r as f32 + 0.5) * cs);
                        let u = (x - mn.0) / (mx.0 - mn.0) * hm.width as f32;
                        let v = (mx.1 - z) / (mx.1 - mn.1) * hm.height as f32;
                        println!("({col},{r}) height {:.1} flipped {:.1} header {:?}", hm.sample(u, v), hm.sample(u, hm.height as f32 - v), cells[r * cols + col].header);
                    }
                }
                let pts = [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0), (0.0, 0.5), (0.5, 0.5), (1.0, 0.5), (0.0, 1.0), (0.5, 1.0), (1.0, 1.0)];
                for p in 0..8 {
                    let mut line = format!("byte {p} corr:");
                    for (fx, fz) in pts {
                        let (mut sx, mut sy, mut sxx, mut syy, mut sxy, mut n) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
                        for row in 0..rows {
                            for col in 0..cols {
                                let v = cells[row * cols + col].header[p];
                                if v == 255 {
                                    continue;
                                }
                                let x = f64::from(v);
                                let y = f64::from(h(x0 + (col as f32 + fx) * cs, z0 + (row as f32 + fz) * cs));
                                sx += x;
                                sy += y;
                                sxx += x * x;
                                syy += y * y;
                                sxy += x * y;
                                n += 1.0;
                            }
                        }
                        let c = (sxy / n - sx / n * sy / n) / ((sxx / n - (sx / n).powi(2)).sqrt() * (syy / n - (sy / n).powi(2)).sqrt());
                        line += &format!(" {c:.2}");
                    }
                    println!("{line}");
                }
            }
            // A run of cells along one row with their class, to see how the bytes change.
            if let (Some(r), Some(c0)) = (args.get(2).and_then(|s| s.parse::<usize>().ok()), args.get(3).and_then(|s| s.parse::<usize>().ok())) {
                for col in c0..(c0 + 24).min(cols) {
                    let c = &cells[r * cols + col];
                    println!("({col},{r}) class {} nb {} header {:?}", classes[r * cols + col], c.boundaries.len(), c.header);
                }
            }
            for (col, row) in [(100usize, 100usize), (101, 100), (102, 100), (200, 50), (50, 150), (300, 120)] {
                if row < rows && col < cols {
                    let c = &cells[row * cols + col];
                    println!("cell ({col},{row}) class {} header {:?}", classes[row * cols + col], c.header);
                }
            }
        }
        "rowscan" => {
            // which cells are compact (single boundary with offset 0), by row, as a text picture
            let step: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
            for row in (0..g.rows as usize).rev().step_by(step) {
                let mut line = String::new();
                for col in (0..g.cols as usize).step_by(step) {
                    let c = &cells[row * g.cols as usize + col];
                    let ch = if c.boundaries.len() == 1 && c.boundaries[0].list_offset() == 0 {
                        let r = c.boundaries[0].region();
                        if r == 1023 { '~' } else { (b'a' + (r % 26) as u8) as char }
                    } else {
                        '#'
                    };
                    line.push(ch);
                }
                println!("{line}");
            }
        }
        "orient" => {
            let (x0, z0) = g.origin_units();
            let cs = g.cell_units();
            let (w, h) = (g.cols as usize, g.rows as usize);
            let sets = g.region_sets().unwrap();
            println!("{} region sets ({} singles)", sets.len(), g.region_count);
            for (name, fr, fc) in [("identity", false, false), ("flip rows", true, false), ("flip cols", false, true), ("both", true, true)] {
                let (mut ok, mut n) = (0, 0);
                let mut none_kinds: BTreeMap<&str, usize> = BTreeMap::new();
                let mut empty_kinds: BTreeMap<&str, usize> = BTreeMap::new();
                for (i, c) in cells.iter().enumerate() {
                    let (mut col, mut row) = (i % w, i / w);
                    if fr { row = h - 1 - row; }
                    if fc { col = w - 1 - col; }
                    let (x, z) = (x0 + (col as f32 + 0.5) * cs, z0 + (row as f32 + 0.5) * cs);
                    let at = regions.region_at(x, z);
                    let kind = match at { None => "outside", Some(a) if a.is_sea => "sea", Some(_) => "land" };
                    for b in &c.boundaries {
                        let r = b.region() as usize;
                        if r == 1023 { *none_kinds.entry(kind).or_default() += 1; continue; }
                        let Some(set) = sets.get(r) else { continue };
                        if set.is_empty() { *empty_kinds.entry(kind).or_default() += 1; continue; }
                        if c.boundaries.len() == 1 {
                            n += 1;
                            if at.is_some_and(|a| set.iter().any(|&s| regions.regions[s as usize].key == a.key)) { ok += 1; }
                        }
                    }
                }
                println!("{name}: single-boundary cells in their region set {ok}/{n}; id 1023 at {none_kinds:?}; empty set at {empty_kinds:?}");
            }
        }
        "area" => {
            let (x0, z0) = g.origin_units();
            let cs = g.cell_units();
            let w = g.cols as usize;
            for perm in [[0usize, 1, 2, 3], [0, 2, 1, 3]] {
                // corner k -> (dx, dz) with 0 = SW, 3 = NE, perm decides 1 and 2.
                let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
                let (mut good, mut bad) = (0, 0);
                let mut signs = (0, 0);
                for (i, c) in cells.iter().enumerate() {
                    if c.boundaries.len() < 2 { continue; }
                    let (cx, cz) = (x0 + (i % w) as f32 * cs, z0 + (i / w) as f32 * cs);
                    let mut sum = 0.0f64;
                    for b in &c.boundaries {
                        let l = by_off[&b.list_offset()];
                        let p: Vec<(f64, f64)> = l.iter().map(|&v| {
                            if v < 4 { let k = perm[v as usize]; ((cx + corners[k].0 * cs) as f64, (cz + corners[k].1 * cs) as f64) }
                            else { let (x, z) = a.vertex_units(v as usize).unwrap(); (x as f64, z as f64) }
                        }).collect();
                        let mut s = 0.0;
                        for k in 0..p.len() { let (a1, b1) = (p[k], p[(k + 1) % p.len()]); s += a1.0 * b1.1 - b1.0 * a1.1; }
                        s *= 0.5;
                        if s > 0.0 { signs.0 += 1 } else { signs.1 += 1 }
                        sum += s.abs();
                    }
                    if (sum - (cs * cs) as f64).abs() < 0.01 { good += 1 } else { bad += 1; if bad < 4 { println!("  cell {i}: area {sum}"); } }
                }
                println!("corner perm {perm:?}: area ok {good}, not {bad}, signs (ccw, cw) {signs:?}");
            }
        }
        "png" => {
            // png <out> <px per unit> [x0 z0 x1 z1] [colour: region|flags0|flags1|flags2|flags3]
            let out = &args[2];
            let ppu: f32 = args[3].parse().unwrap();
            let (gx0, gz0) = g.origin_units();
            let cs = g.cell_units();
            let (mut wx0, mut wz0, mut wx1, mut wz1) = (gx0, gz0, gx0 + g.cols as f32 * cs, gz0 + g.rows as f32 * cs);
            if args.len() >= 8 {
                wx0 = args[4].parse().unwrap(); wz0 = args[5].parse().unwrap(); wx1 = args[6].parse().unwrap(); wz1 = args[7].parse().unwrap();
            }
            let mode = args.get(8).map(String::as_str).unwrap_or("region");
            let sets = g.region_sets().unwrap();
            let (pw, ph) = (((wx1 - wx0) * ppu) as u32, ((wz1 - wz0) * ppu) as u32);
            let mut rgb = vec![0u8; (pw * ph * 3) as usize];
            let w = g.cols as usize;
            for py in 0..ph {
                for px in 0..pw {
                    let x = wx0 + (px as f32 + 0.5) / ppu;
                    let z = wz1 - (py as f32 + 0.5) / ppu;
                    let (c, r) = (((x - gx0) / cs).floor() as i64, ((z - gz0) / cs).floor() as i64);
                    if c < 0 || r < 0 || c >= g.cols as i64 || r >= g.rows as i64 { continue; }
                    let i = r as usize * w + c as usize;
                    let (cx, cz) = (gx0 + c as f32 * cs, gz0 + r as f32 * cs);
                    let cell = &cells[i];
                    let mut hit = None;
                    for b in &cell.boundaries {
                        if cell.boundaries.len() == 1 { hit = Some(*b); break; }
                        let l = by_off[&b.list_offset()];
                        let corner = [(0.0, 0.0), (0.0, cs), (cs, 0.0), (cs, cs)];
                        let p: Vec<(f32, f32)> = l.iter().map(|&v| if v < 4 { (cx + corner[v as usize].0, cz + corner[v as usize].1) } else { a.vertex_units(v as usize).unwrap() }).collect();
                        let mut inside = false;
                        let mut j = p.len() - 1;
                        for k in 0..p.len() {
                            let (pi, pj) = (p[k], p[j]);
                            if (pi.1 > z) != (pj.1 > z) && x < (pj.0 - pi.0) * (z - pi.1) / (pj.1 - pi.1) + pi.0 { inside = !inside; }
                            j = k;
                        }
                        if inside { hit = Some(*b); break; }
                    }
                    let col = match hit {
                        None => [255, 0, 255],
                        Some(b) => match mode {
                            "region" => {
                                let r = b.region() as usize;
                                if r == 1023 { [0, 0, 0] }
                                else if sets.get(r).is_some_and(|s| s.is_empty()) { [40, 80, 200] }
                                else { let h = (r as u32).wrapping_mul(2654435761); [(h >> 24) as u8 | 64, (h >> 16) as u8 | 64, (h >> 8) as u8 & 0x7f] }
                            }
                            "lo" => { let v = b.flags as u8; if v & 2 != 0 { [0,0,0] } else if v & 1 != 0 { [20,30,120] } else { match v & 0xf0 { 0 => [255,255,255], 16 => [140,140,140], 32 => [230,40,40], 64 => [40,200,40], 128 => [60,120,255], _ => [255,200,0] } } }
                            m => {
                                let byte: u32 = m[5..].parse().unwrap();
                                let v = (b.flags >> (8 * byte)) as u8;
                                let h = (v as u32).wrapping_mul(2654435761);
                                if v == 0 { [0, 0, 0] } else { [(h >> 24) as u8 | 32, (h >> 16) as u8 | 32, (h >> 8) as u8 | 32] }
                            }
                        },
                    };
                    let o = ((py * pw + px) * 3) as usize;
                    rgb[o..o + 3].copy_from_slice(&col);
                }
            }
            write_png(out, pw, ph, &rgb);
            println!("wrote {out} {pw}x{ph}");
        }
        "grid" => {
            let data = dir.parent().unwrap().parent().unwrap().to_path_buf();
            let vfs = ntw_formats::pack::Vfs::open_install(&data).unwrap();
            let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(&data) };
            let map = ntw_formats::campaign_map::CampaignMap::load(&files, dir.file_name().unwrap().to_str().unwrap()).unwrap();
            let t = std::time::Instant::now();
            let pg = ntw_campaign::pathing::build_grid(&map);
            println!("grid {}x{} in {:?}", pg.width, pg.height, t.elapsed());
            let mut rgb = vec![0u8; (pg.width * pg.height * 3) as usize];
            for i in 0..pg.kind.len() {
                let (c, r) = (i % pg.width as usize, i / pg.width as usize);
                let o = (((pg.height as usize - 1 - r) * pg.width as usize + c) * 3) as usize;
                let col = if pg.road[i] { [255, 255, 255] } else { match pg.kind[i] { 1 => { let h = (pg.region[i] as u32).wrapping_mul(2654435761); [(h >> 24) as u8 / 2 + 60, (h >> 16) as u8 / 2 + 60, 40] } 2 => [30, 60, 160], _ => [0, 0, 0] } };
                rgb[o..o + 3].copy_from_slice(&col);
            }
            write_png(&args[2], pg.width, pg.height, &rgb);
        }
        _ => {}
    }
}

/// Minimal PNG writer (RGB, stored deflate blocks).
pub fn write_png(path: &str, w: u32, h: u32, rgb: &[u8]) {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let mut raw = Vec::new();
    for y in 0..h as usize {
        raw.push(0u8);
        raw.extend_from_slice(&rgb[y * w as usize * 3..(y + 1) * w as usize * 3]);
    }
    let mut z = vec![0x78, 0x01];
    for (i, chunk) in raw.chunks(65535).enumerate() {
        let last = (i + 1) * 65535 >= raw.len();
        z.push(last as u8);
        z.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |t: &[u8], d: &[u8]| {
        out.extend_from_slice(&(d.len() as u32).to_be_bytes());
        let mut td = t.to_vec();
        td.extend_from_slice(d);
        out.extend_from_slice(&td);
        out.extend_from_slice(&crc(&td).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, out).unwrap();
}
