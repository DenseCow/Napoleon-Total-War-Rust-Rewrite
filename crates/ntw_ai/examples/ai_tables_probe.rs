//! Dumps the AI-related DB tables of the install (read-only), for research.
//! `cargo run -p ntw_ai --example ai_tables_probe -- [table] [max_rows]`
use ntw_ai::tables::{self, layouts};
use ntw_formats::pack::Vfs;

fn main() {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| {
        r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data".to_string()
    });
    let vfs = Vfs::open_install(&dir).expect("open install");
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--grep") {
        grep(&vfs, &args[2], args.get(3).map_or("", String::as_str));
        return;
    }
    if args.get(1).map(String::as_str) == Some("--ls") {
        // `--ls <prefix>`: lists VFS files under a prefix with their sizes.
        for p in vfs.list(&args[2]) {
            println!("{p} {}", vfs.read(p).map_or(0, |b| b.len()));
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--extract") {
        // `--extract <path> <out_file>`: copies one VFS file out (for research tools; keep the
        // copy out of git).
        std::fs::write(&args[3], vfs.read(&args[2]).expect("read")).expect("write");
        return;
    }
    if args.get(1).map(String::as_str) == Some("--cat") {
        // `--cat <path> [max_bytes]`: prints a file as text (lossy UTF-8).
        let max: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4000);
        let b = vfs.read(&args[2]).expect("read");
        println!("{}", String::from_utf8_lossy(&b[..b.len().min(max)]));
        return;
    }
    let only = args.get(1).cloned();
    let max: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(40);
    let all = [
        ("campaign_ai_personalities", layouts::CAMPAIGN_AI_PERSONALITIES),
        ("campaign_ai_personality_junctions", layouts::CAMPAIGN_AI_PERSONALITY_JUNCTIONS),
        ("campaign_ai_managers", layouts::CAMPAIGN_AI_MANAGERS),
        ("campaign_ai_manager_behaviour_junctions", layouts::CAMPAIGN_AI_MANAGER_BEHAVIOUR_JUNCTIONS),
        ("cdir_configs", layouts::CDIR_CONFIGS),
        ("cdir_desire_priorities", layouts::CDIR_DESIRE_PRIORITIES),
        ("cdir_unit_qualities", layouts::CDIR_UNIT_QUALITIES),
        ("cdir_unit_balance_groups", layouts::CDIR_UNIT_BALANCE_GROUPS),
        ("cdir_unit_balance_group_qualities", layouts::CDIR_UNIT_BALANCE_GROUP_QUALITIES),
        ("cdir_unit_balances", "siisffi"),
        ("cdir_faction_junctions", layouts::CDIR_JUNCTIONS),
        ("cdir_campaign_junctions", layouts::CDIR_JUNCTIONS),
        ("campaign_difficulty_handicap_effects", layouts::CAMPAIGN_DIFFICULTY_HANDICAP_EFFECTS),
        ("building_units_allowed", layouts::BUILDING_UNITS_ALLOWED),
        ("building_chains", layouts::BUILDING_CHAINS),
        ("diplomatic_relations_attitudes", layouts::DIPLOMATIC_RELATIONS_ATTITUDES),
        ("units_to_exclusive_faction_permissions", "ssb"),
        ("units_to_groupings_military_permissions", "ss"),
        ("campaign_variables", "sf"),
    ];
    for (name, codes) in all {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        println!("== {name} files={:?}", vfs.list(&format!("db/{name}_tables/")));
        // Each file as it is (a research dump; the game merges them by key: tables::load_raw).
        for path in vfs.list(&format!("db/{name}_tables/")) {
            let table = vfs
                .read(path)
                .map_err(|e| e.to_string())
                .and_then(|b| ntw_formats::db::DbTable::read(&b, &tables::schema(codes)).map_err(|e| format!("{e:?}")));
            match table {
                Ok(t) => {
                    println!("   {path}: {} rows", t.rows.len());
                    for r in t.rows.iter().take(max) {
                        println!("   {r:?}");
                    }
                }
                Err(e) => println!("   {path}: ERROR {e}"),
            }
        }
    }
}

/// `--grep <text> [prefix]`: lists VFS files containing `text` (ASCII or UTF-16LE).
#[allow(dead_code)]
fn grep(vfs: &Vfs, text: &str, prefix: &str) {
    let ascii = text.as_bytes().to_vec();
    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    for path in vfs.list(prefix) {
        let Ok(bytes) = vfs.read(path) else { continue };
        let has = |n: &[u8]| bytes.windows(n.len()).any(|w| w == n);
        if has(&ascii) || has(&utf16) {
            println!("{path}");
        }
    }
}
