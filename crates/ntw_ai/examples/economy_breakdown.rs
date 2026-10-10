//! Income and expenses of one faction by category, round by round (0-B research tool; reads the
//! install only).
//!
//! ```text
//! cargo run -p ntw_ai --release --example economy_breakdown -- <human> <faction> [rounds] [ai|noai]
//! ```
//! `human` is the human faction (`none` for an all-AI campaign); `ai` runs the campaign AI for the
//! other factions (as the game does), `noai` lets every faction stand still. Prints, before every
//! End Turn: treasury, taxes, trade, other income, land and naval upkeep (with effects and plain),
//! units, regions, and the effect sums that move these numbers.

use std::path::PathBuf;
use std::sync::Arc;

use ntw_ai::campaign::driver;
use ntw_ai::campaign::CampaignAiData;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::economy;
use ntw_sim::campaign::effects::Effects;
use ntw_sim::campaign::Terrain;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let human = args.first().cloned().unwrap_or_else(|| "france".into());
    let focus = args.get(1).cloned().unwrap_or_else(|| "britain".into());
    let rounds: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let ai = args.get(3).is_none_or(|s| s != "noai");
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    let sp = dir.join(r"campaigns\eur_napoleon\startpos.esf");
    let db = GameDatabase::from_install(&dir).expect("db");
    let data = Arc::new(CampaignAiData::from_install(&dir, &db).expect("ai data"));
    let mut loaded = ntw_campaign::read_file(&sp, &db).expect("startpos");
    if human != "none" {
        assert!(loaded.set_human(&human), "no faction {human}");
    }
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let files = GameFiles { vfs: &vfs };
    let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    loaded.model.terrain = Some(Terrain(Arc::new(ntw_campaign::pathing::build_grid(&map))));
    ntw_campaign::trade::attach_map(&mut loaded.model, &map.regions);
    let mut m = loaded.model;
    let ctx = driver::context_for(&m, "eur_napoleon");
    if !m.turn.started {
        m.start_campaign();
    }
    let f = m.faction_by_key(&focus).expect("focus faction").id;
    println!("human {human}, focus {focus}, AI {}", if ai { "on" } else { "off" });
    for round in 0..=rounds {
        let fx = Effects::compute(&m);
        let inc = economy::faction_income(&m, f);
        let (mut land, mut naval, mut land_plain, mut naval_plain, mut land_units, mut ships) = (0, 0, 0, 0, 0, 0);
        for force in m.world.forces.values().filter(|x| x.faction == f) {
            for u in &force.units {
                let Some(r) = m.rules.units.get(&u.unit_key) else { continue };
                let c = economy::unit_upkeep(&fx, f, r);
                if r.is_naval {
                    naval += c;
                    naval_plain += r.upkeep;
                    ships += 1;
                } else {
                    land += c;
                    land_plain += r.upkeep;
                    land_units += 1;
                }
            }
        }
        let regions = economy::regions_owned(&m, f);
        let fx_line: Vec<String> = ["upkeep_cost_mod_land_all", "upkeep_cost_mod_naval_all", "tax_bonus_minister", "tax_bonus_technology", "admin_cost_mod"]
            .iter()
            .map(|k| format!("{k}={}", fx.faction(f, k)))
            .collect();
        let treasury = m.world.factions[&f].treasury;
        println!(
            "round {round}: treasury {treasury} | taxes {} trade {} other {} | upkeep land {land} (plain {land_plain}, {land_units} units) naval {naval} (plain {naval_plain}, {ships} ships) | net {:+} | regions {regions} | {}",
            inc.taxes,
            inc.trade,
            inc.other,
            inc.net(),
            fx_line.join(" ")
        );
        if round < rounds {
            if ai {
                driver::end_turn(&mut m, &data, &ctx);
            } else {
                m.end_turn();
            }
        }
    }
}
