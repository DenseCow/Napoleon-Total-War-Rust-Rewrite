//! Research helper: dump `groupformations.bin` from the install. Read-only. Usage:
//!   cargo run -p ntw_formats --example gf_probe -- [name filter]
use ntw_formats::group_formation::{self, ElementBody};
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default().to_ascii_lowercase();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let t = group_formation::read(&vfs.read("groupformations.bin").unwrap()).unwrap();
    for g in t.iter().filter(|g| g.name.to_ascii_lowercase().contains(&filter)) {
        let h: Vec<String> = g.header.iter().map(|w| format!("{w:#x}/{}", f32::from_bits(*w))).collect();
        println!("== {} [{}] factions {:?}", g.name, h.join(" "), g.factions);
        for e in &g.elements {
            match &e.body {
                ElementBody::Block { kind, words, classes } => {
                    let w: Vec<String> = words.iter().map(|w| if *w > 0x1000_0000 && *w != u32::MAX { format!("{}f", f32::from_bits(*w)) } else { format!("{}", *w as i32) }).collect();
                    println!("  #{} kind {kind} [{}] classes {:?}", e.id, w.join(" "), classes);
                }
                b => println!("  #{} {:?}", e.id, b),
            }
        }
    }
}
