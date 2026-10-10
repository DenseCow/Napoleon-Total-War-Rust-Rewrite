//! Campaign run for the characters slot (0-G, `analysis/fidelity/CHARACTERS_FIDELITY.md`): starts a
//! campaign as the game does (script host, campaign scripts, AI), plays N End Turns and prints what the
//! trait and ancillary triggers did (gains, refusals) and the counts per turn. Reads the install only.
//!
//! ```text
//! cargo run -p ntw_ai --release --example character_run -- [turns] [campaign] [faction]
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use ntw_ai::campaign::driver;
use ntw_ai::campaign::CampaignAiData;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::pack::Vfs;
use ntw_script::{ScriptContext, ScriptHost, ScriptSource};
use ntw_sim::campaign::{CampaignCommand, Terrain};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let turns: u32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(5);
    let campaign = args.get(1).cloned().unwrap_or_else(|| "eur_napoleon".into());
    let human = args.get(2).cloned().unwrap_or_else(|| "france".into());
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    let db = GameDatabase::from_install(&dir).expect("db");
    for w in &db.load_warnings {
        eprintln!("WARN game data: {w}");
    }
    let vfs = Vfs::open_install(&dir).expect("packs");
    let files = GameFiles { vfs: &vfs };
    let data = Arc::new(CampaignAiData::load(&vfs, &db).expect("AI tables"));
    let startpos = files.read(&format!("campaigns/{campaign}/startpos.esf")).expect("startpos");
    let mut loaded = ntw_campaign::read(&startpos, &db).expect("startpos");
    assert!(loaded.set_human(&human), "no faction {human}");
    let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    loaded.model.terrain = Some(Terrain(Arc::new(ntw_campaign::pathing::build_grid(&map))));
    let source = ScriptSource::from_install(&dir).expect("scripts");
    let mut host = ScriptHost::new(loaded.model, &human, source).expect("script host");
    if let Err(e) = host.load_campaign(&campaign) {
        eprintln!("warning: campaign scripts: {e}");
    }
    driver::install(&mut host, data);
    for name in ["NewSession", "NewCampaignStarted"] {
        let _ = host.fire(name, ScriptContext::for_faction(&human));
    }
    host.start_campaign();
    let mut seen = 0;
    let olds = |st: &ntw_script::ScriptState| -> std::collections::BTreeMap<i32, (String, u32)> {
        let y = st.model.calendar.date.year;
        st.model.world.character_details.iter().filter_map(|(id, d)| { let b = d.birth?; (y.saturating_sub(b.year) >= 51).then(|| (id.raw(), (d.surname.rsplit('_').next().unwrap_or("").to_string(), y - b.year))) }).collect()
    };
    let mut before = olds(&host.state());
    for t in 1..=turns {
        if host.model().pending_battle.is_some() {
            let _ = host.apply(CampaignCommand::Autoresolve);
        }
        host.apply(CampaignCommand::EndTurn).expect("end turn");
        let st = host.state();
        let new = &st.character_log[seen..];
        let gained = new.iter().filter(|l| l.starts_with("trait gained")).count();
        let raised = new.iter().filter(|l| l.starts_with("trait points")).count();
        let anc = new.iter().filter(|l| l.starts_with("ancillary gained")).count();
        let refused = new.iter().filter(|l| l.contains("refused")).count();
        let year = st.model.calendar.date.year;
        let old = st.model.world.character_details.values().filter(|d| d.birth.is_some_and(|b| year.saturating_sub(b.year) >= 51)).count();
        println!("turn {t} ({:?}): {} characters ({old} aged 51+); traits gained {gained}, trait points {raised}, ancillaries {anc}, refused {refused}", st.model.calendar.date, st.model.world.characters.len());
        if std::env::var_os("CHAR_LOG").is_some() {
            for l in new {
                println!("  {l}");
            }
        }
        let now = olds(&st);
        for (id, (name, age)) in &before {
            if !st.model.world.characters.contains_key(&ntw_sim::campaign::CharacterId(*id)) && !now.contains_key(id) {
                println!("  gone (aged 51+): {name} age {age}");
            }
        }
        before = now;
        seen = st.character_log.len();
    }
    let st = host.state();
    let w = &st.model.world;
    let traits: usize = w.character_details.values().map(|d| d.traits.len()).sum();
    let ancs: usize = w.character_details.values().map(|d| d.ancillaries.len()).sum();
    let unknown = st.log.iter().filter(|l| l.contains("UNKNOWN stub conditions")).count();
    println!("end: {} characters, {traits} traits, {ancs} ancillaries; {unknown} stub condition calls", w.characters.len());
    let mut stubs: std::collections::BTreeMap<String, usize> = Default::default();
    for l in st.log.iter().filter(|l| l.contains("UNKNOWN stub conditions.")) {
        let name = l.split("conditions.").nth(1).and_then(|x| x.split('(').next()).unwrap_or("?");
        *stubs.entry(name.to_string()).or_default() += 1;
    }
    let mut v: Vec<_> = stubs.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!("stub conditions: {:?}", &v[..v.len().min(25)]);
}
