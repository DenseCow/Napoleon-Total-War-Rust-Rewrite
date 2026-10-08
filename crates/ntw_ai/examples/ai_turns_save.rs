//! Research helper for save compatibility (analysis/campaign/SAVE_COMPAT.md): plays End Turns of
//! the eur_napoleon startpos with the campaign AI (france human, no Lua scripts) and writes the
//! save to a file of ours (never the game's save folder), then prints `save_check`'s report.
//!
//! ```text
//! cargo run -p ntw_ai --example ai_turns_save -- <turns> <out.save>
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use ntw_ai::campaign::CampaignAiData;
use ntw_ai::campaign::driver;
use ntw_ai::campaign::keys::read_ai_keys;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::Terrain;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(turns), Some(out)) = (args.first().and_then(|s| s.parse::<u32>().ok()), args.get(1)) else {
        eprintln!("usage: ai_turns_save <turns> <out.save>");
        std::process::exit(2);
    };
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    let db = GameDatabase::from_install(&dir).expect("db");
    let data = Arc::new(CampaignAiData::from_install(&dir, &db).expect("AI tables"));
    let startpos = EsfFile::open(dir.join(r"campaigns\eur_napoleon\startpos.esf")).expect("startpos");
    let mut loaded = ntw_campaign::read_esf(&startpos, &db).expect("load");
    loaded.set_human("france");
    let vfs = Vfs::open_install(&dir).expect("packs");
    let map = CampaignMap::load(&GameFiles { vfs: &vfs, data_dir: Some(&dir) }, &loaded.info.map_key).expect("map");
    let mut m = loaded.model;
    m.terrain = Some(Terrain(Arc::new(ntw_campaign::pathing::build_grid(&map))));
    let mut ctx = driver::context_for(&m, "eur_napoleon");
    ctx.ai_keys = read_ai_keys(&startpos.root);
    for _ in 0..turns {
        driver::end_turn(&mut m, &data, &ctx);
    }
    let save = ntw_campaign::save::write_save(&startpos, &m, "france", 0).expect("write");
    std::fs::write(out, save.to_bytes().expect("bytes")).expect("write file");
    let r = ntw_campaign::save_check::check(&save);
    println!(
        "{out}: turn {}, {} forces, {} characters; {} violations, {} commanders without obstacle {:?}, AI block rebuilt {}",
        m.calendar.turn_number(),
        m.world.forces.len(),
        m.world.characters.len(),
        r.violations.len(),
        r.commanders_without_obstacle.len(),
        r.commanders_without_obstacle,
        r.ai_block_rebuilt
    );
    for v in r.violations.iter().take(20) {
        println!("  {v}");
    }
}
