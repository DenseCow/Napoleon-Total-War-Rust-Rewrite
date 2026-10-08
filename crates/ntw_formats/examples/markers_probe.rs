//! Research probe for battle-map `.markers` files (read-only): splits each file into its marker
//! groups by the 16-byte group keys and prints, per group class, the item count and bytes.
//! `cargo run -p ntw_formats --example markers_probe [-- hex <class>]`
use ntw_formats::pack::Vfs;
use std::collections::BTreeMap;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn u16_at(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o], b[o + 1]]) }
fn u32_at(b: &[u8], o: usize) -> u32 { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let paths: Vec<String> = vfs.list("battleterrain").into_iter().filter(|p| p.ends_with(".markers")).map(str::to_owned).collect();
    let mut stats: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
    for p in &paths {
        let b = vfs.read(p).unwrap();
        // root: u16 len, name, key16, u32 1, u16 count
        let n = u16_at(&b, 0) as usize;
        let mut o = 2 + 2 * n + 16 + 4;
        let count = u16_at(&b, o) as usize;
        o += 2;
        // children: key16, u16 len, name, key16, u32 1, u32 items, payload
        let mut starts = Vec::new();
        for _ in 0..count {
            // find the next key: 12 zero bytes + u32 id + u16 len (11..15) + 'X\0'
            let mut q = o;
            while q + 22 <= b.len() {
                if b[q..q + 12].iter().all(|&x| x == 0) && (0x9d900..0x9da00).contains(&u32_at(&b, q + 12)) && (5..40).contains(&u16_at(&b, q + 16)) && b[q + 19] == 0 && b[q + 18].is_ascii_uppercase() {
                    break;
                }
                q += 1;
            }
            starts.push(q);
            let nl = u16_at(&b, q + 16) as usize;
            o = q + 18 + 2 * nl + 16 + 4 + 4;
        }
        for (k, &s) in starts.iter().enumerate() {
            let id = u32_at(&b, s + 12);
            let nl = u16_at(&b, s + 16) as usize;
            let body = s + 18 + 2 * nl + 16 + 4;
            let items = u32_at(&b, body) as usize;
            let end = starts.get(k + 1).copied().unwrap_or(b.len());
            stats.entry(id).or_default().push((items, end - body - 4));
            if args.first().map(String::as_str) == Some("hex") && args.get(1).and_then(|s| u32::from_str_radix(s, 16).ok()) == Some(id) && items > 0 {
                println!("{p} class {id:x} items {items} bytes {}", end - body - 4);
                for row in b[body + 4..end.min(body + 4 + 1200)].chunks(16) {
                    println!("   {}", row.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" "));
                }
            }
        }
    }
    for (id, v) in &stats {
        let nonempty: Vec<_> = v.iter().filter(|x| x.0 > 0).take(8).collect();
        println!("class {id:x}: {} groups, non-empty (items, bytes) {nonempty:?}", v.len());
    }
}
