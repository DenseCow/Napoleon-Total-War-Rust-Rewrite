//! Survey of the pathfinder's run-time obstacles in a save (`analysis/campaign/PATHFINDING.md` §8).
//! Reads the file only.
//!
//! ```text
//! cargo run -p ntw_campaign --example obstacle_probe -- <file.save> [obstacle index]
//! ```

use std::collections::HashMap;

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

/// A character's `LOCOMOTABLE` #17, #18 arrays and its first 17 values as text.
type Loco = (Vec<u32>, Vec<u32>, String);

thread_local! {
    static LOCO: std::cell::RefCell<HashMap<u32, Loco>> = std::cell::RefCell::new(HashMap::new());
}

const FIX: f64 = 1.0 / (1u64 << 20) as f64;

fn u32s(r: &[EsfNode], i: usize) -> Vec<u32> {
    r.get(i).and_then(|n| n.as_u32_array()).map(<[u32]>::to_vec).unwrap_or_default()
}

fn int(r: &[EsfNode], i: usize) -> i64 {
    r.get(i).and_then(EsfNode::as_int).unwrap_or(-1)
}

fn item_nodes(r: &[EsfNode], i: usize) -> Vec<&Vec<EsfNode>> {
    match r.get(i) {
        Some(EsfNode::RecordArray(a)) => a.items.iter().collect(),
        _ => Vec::new(),
    }
}

/// Character id -> (x, z) from every `CHARACTER` record ({LOCOMOTABLE{x, z, ..}, DETAILS, id, ..}).
fn characters(r: &EsfRecord, out: &mut HashMap<u32, (f64, f64)>) {
    if r.name == "CHARACTER"
        && let (Some(EsfNode::Record(loc)), Some(id)) = (r.children.first(), r.children.get(2).and_then(EsfNode::as_int))
    {
        LOCO.with(|m| m.borrow_mut().insert(id as u32, (u32s(&loc.children, 17), u32s(&loc.children, 18), loc.children.iter().take(17).map(|n| format!("{n:?}")).collect::<Vec<_>>().join(" "))));
        out.insert(id as u32, (int(&loc.children, 0) as f64 * FIX, int(&loc.children, 1) as f64 * FIX));
    }
    for n in &r.children {
        match n {
            EsfNode::Record(x) => characters(x, out),
            EsfNode::RecordArray(a) => {
                for it in &a.items {
                    for m in it {
                        if let EsfNode::Record(x) = m {
                            characters(x, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(file) = args.first() else {
        eprintln!("usage: obstacle_probe FILE [obstacle index]");
        std::process::exit(2);
    };
    let pick: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let esf = EsfFile::open(file).expect("reads");
    let pfr = esf.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").expect("pathfinder");
    let grid: &[EsfNode] = item_nodes(&pfr.children, 0)[0];
    let mut chars = HashMap::new();
    characters(&esf.root, &mut chars);
    // #1: count-prefixed point lists (Fixed20 x, z pairs)?
    let pool = u32s(grid, 1);
    let mut lists: Vec<(usize, Vec<(f64, f64)>)> = Vec::new();
    let mut i = 0;
    while i < pool.len() {
        let n = pool[i] as usize;
        let n = n * 2;
        if n == 0 || i + 1 + n > pool.len() {
            println!("#1 does not split at {i} (n {n})");
            break;
        }
        let pts = pool[i + 1..i + 1 + n].chunks(2).map(|c| (c[0] as i32 as f64 * FIX, c.get(1).map_or(0.0, |&z| z as i32 as f64 * FIX))).collect();
        lists.push((i, pts));
        i += 1 + n;
    }
    println!("#0 = {}; #1: {} u32 in {} lists", int(grid, 0), pool.len(), lists.len());
    let ob = match grid.get(3) {
        Some(EsfNode::Record(x)) => u32s(&x.children, 0),
        _ => Vec::new(),
    };
    println!("OBSTACLE_BOUNDARIES: {} u32; first 80: {:x?}", ob.len(), &ob[..ob.len().min(80)]);
    // OBSTACLE_BOUNDARIES as records {a, n, n x {flags, link}, packed cell}: the kinds of the
    // run-time boundaries (link bit 21) by pool index.
    let mut rt_kind: HashMap<u32, (u32, u32)> = HashMap::new();
    let mut j = 0;
    let mut recs = 0;
    let mut a_vals: HashMap<u32, u32> = HashMap::new();
    while j + 1 < ob.len() {
        let (a, n) = (ob[j], ob[j + 1] as usize);
        if j + 2 + 2 * n + 1 > ob.len() || n > 64 {
            println!("OBSTACLE_BOUNDARIES does not split at {j} (a {a}, n {n})");
            break;
        }
        *a_vals.entry(a).or_default() += 1;
        for k in 0..n {
            let (fl, link) = (ob[j + 2 + 2 * k], ob[j + 3 + 2 * k]);
            if link & 0x20_0000 != 0 {
                rt_kind.insert(link & 0x1F_FFFF, (fl, ob[j + 2 + 2 * n]));
            }
        }
        recs += 1;
        j += 3 + 2 * n;
    }
    println!("OBSTACLE_BOUNDARIES: {recs} records, a values {a_vals:?}, {} run-time boundaries", rt_kind.len());
    let lists_rec = match grid.get(6) {
        Some(EsfNode::Record(x)) => x,
        _ => return,
    };
    let chobs = item_nodes(&lists_rec.children, 3);
    println!("{} character obstacles", chobs.len());
    let Some(item) = chobs.get(pick) else { return };
    let (Some(EsfNode::Record(orec)), Some(id)) = (item.first(), item.get(1).and_then(EsfNode::as_int)) else { return };
    let o: &[EsfNode] = &orec.children;
    let pos = chars.get(&(id as u32)).copied();
    println!("obstacle {pick}: character {id} at {pos:?}");
    LOCO.with(|m| {
        if let Some((a, b, s)) = m.borrow().get(&(id as u32)) {
            println!("  LOCOMOTABLE: {s}");
            println!("  LOCOMOTABLE #17 ({}): {:?}", a.len(), a.iter().take(40).map(|&v| v as i32 as f64 * FIX).collect::<Vec<_>>());
            println!("  LOCOMOTABLE #18 ({}): {:?}", b.len(), b.iter().take(40).map(|&v| v as i32 as f64 * FIX).collect::<Vec<_>>());
        }
    });
    println!(
        "  #1..#4 {} {} {} {}; #6 kind {}; bbox ({:.3}, {:.3})..({:.3}, {:.3}); u16 {:?}",
        int(o, 1), int(o, 2), int(o, 3), int(o, 4), int(o, 6),
        int(o, 7) as f64 * FIX, int(o, 8) as f64 * FIX, int(o, 9) as f64 * FIX, int(o, 10) as f64 * FIX,
        (11..19).map(|k| int(o, k)).collect::<Vec<_>>()
    );
    for (k, b) in item_nodes(o, 0).iter().enumerate() {
        let v = b.first().and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
        if v.is_empty() {
            continue;
        }
        let hi: Vec<u32> = v.iter().map(|x| x >> 31).collect();
        let lo: Vec<u32> = v.iter().map(|x| x & 0x7FFF_FFFF).collect();
        println!("  BOUNDARIES[{k}]: {} entries, high bits {:?}.., low {:?} .. {:?}", v.len(), &hi[..hi.len().min(4)], &lo[..lo.len().min(12)], &lo[lo.len().saturating_sub(4)..]);
        let mut kinds: HashMap<u32, u32> = HashMap::new();
        for x in &lo {
            if let Some(&(fl, _)) = rt_kind.get(x) {
                *kinds.entry(fl & 0xF).or_default() += 1;
            }
        }
        println!("    kinds of these run-time polygons: {kinds:?}");
    }
    for (k, b) in item_nodes(o, 5).iter().enumerate() {
        println!("  MANAGED[{k}]: {:?} {:?}", b.first().and_then(EsfNode::as_int), b.get(1).and_then(EsfNode::as_u32_array));
    }
    // Lists whose points fall inside the bbox.
    let (x0, z0, x1, z1) = (int(o, 7) as f64 * FIX, int(o, 8) as f64 * FIX, int(o, 9) as f64 * FIX, int(o, 10) as f64 * FIX);
    let inside: Vec<&(usize, Vec<(f64, f64)>)> = lists.iter().filter(|(_, p)| p.iter().all(|&(x, z)| x >= x0 - 0.01 && x <= x1 + 0.01 && z >= z0 - 0.01 && z <= z1 + 0.01)).collect();
    println!("  {} #1 lists inside the bbox; first few:", inside.len());
    for (off, p) in inside.iter().take(6) {
        println!("    @{off}: {:?}", p.iter().map(|&(x, z)| ((x * 1000.0).round() / 1000.0, (z * 1000.0).round() / 1000.0)).collect::<Vec<_>>());
    }
}
