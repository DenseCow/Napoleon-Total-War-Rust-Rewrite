//! Research helper (read-only): durations of the clips an animation table gives for some slots,
//! e.g. the melee and firing slots of musket infantry.
//!   cargo run -p ntw_formats --example clip_durations -- <table> <slot prefix> [<slot prefix> ...]
use ntw_formats::anim::Anim;
use ntw_formats::battle_animation::AnimationTables;
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let tables = AnimationTables::from_vfs(&vfs).unwrap();
    if args.is_empty() {
        for t in tables.table_names() {
            println!("{t}");
        }
        return;
    }
    let table = &args[0];
    let Some(t) = tables.table(table) else {
        println!("no table {table}");
        return;
    };
    let _ = t;
    for prefix in &args[1..] {
        for i in 0..=20 {
            let slot = if i == 0 { prefix.clone() } else { format!("{prefix}_{i}") };
            let clips = tables.resolve(table, &slot);
            for c in clips {
                let d = vfs.read(&c.clip.filename).ok().and_then(|b| Anim::read(&b).ok()).map(|a| a.duration);
                println!("{slot:30} {:60} {:?}", c.clip.filename, d);
            }
        }
    }
}
