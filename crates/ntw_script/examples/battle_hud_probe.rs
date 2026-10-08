//! Research helper: runs the original battle HUD (`data/ui/battle ui/layout`) headless with a
//! made-up battle (3 units), then clicks components. Read-only. Usage:
//!   cargo run -p ntw_script --example battle_hud_probe -- [--tree] [--phase conflict|finished] [--call Name] id1,id2,...
//! Prints the scripts' log, the battle requests after each step, and with `--tree` the visible components.
use ntw_formats::loc::Localisation;
use ntw_script::ScriptSource;
use ntw_script::ui::battle::{self, BattleHudFacts, HudPhase, HudSideResult, HudUnit};
use ntw_script::ui::{FrontEndFacts, NodeId, PointerEvent, UiScriptHost, UiWorld};

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn print_tree(w: &UiWorld, id: NodeId, depth: usize) {
    let Some(n) = w.get(id) else { return };
    if !n.visible && std::env::var_os("UI_RUN_ALL").is_none() {
        return;
    }
    let text = n.current().map(|s| s.text.as_str()).unwrap_or("");
    println!(
        "{:indent$}{} #{} [{}] {:.0},{:.0} {:.0}x{:.0} {}",
        "",
        n.data.id,
        id,
        n.state_name(),
        n.rect.x,
        n.rect.y,
        n.rect.w,
        n.rect.h,
        if text.is_empty() { String::new() } else { format!("{text:?}") },
        indent = depth * 2
    );
    for &c in &n.children {
        print_tree(w, c, depth + 1);
    }
}

fn dump(host: &UiScriptHost) {
    let mut unknown = std::collections::BTreeMap::<String, usize>::new();
    for l in host.take_log() {
        if l.starts_with("UNKNOWN") {
            *unknown.entry(l).or_default() += 1;
        } else {
            println!("  {l}");
        }
    }
    for (l, n) in unknown {
        println!("  {l} x{n}");
    }
    for r in battle::take_requests(host).unwrap() {
        println!("  battle request {r:?}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let source = ScriptSource::from_install(&dir).unwrap();
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { game_version: "1.3.0".into(), ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0)).unwrap();
    battle::install(&host, ScriptSource::from_install(&dir).unwrap()).unwrap();
    let phase = match args.iter().position(|a| a == "--phase").and_then(|i| args.get(i + 1)).map(String::as_str) {
        Some("conflict") => HudPhase::Conflict,
        Some("finished") => HudPhase::Finished,
        _ => HudPhase::Deployment,
    };
    let unit = |id: u32, key: &str, cat: &str| HudUnit {
        id,
        key: key.into(),
        portrait: format!("data/ui/units/icons/{key}"),
        men: 100,
        max_men: 120,
        has_ammo: true,
        ammo_percent: 80.0,
        experience: 2,
        category: cat.into(),
        selected: id == 1,
        ..Default::default()
    };
    let facts = BattleHudFacts {
        phase,
        elapsed_s: 30.0,
        total_s: 2100.0,
        speed: 1.0,
        units: vec![unit(1, "Inf_Line_French_Fusiliers", "infantry"), unit(2, "Inf_Gren_French_Grenadiers", "infantry"), unit(3, "Cav_Heavy_French_Cuirassiers", "cavalry")],
        player_won: (phase == HudPhase::Finished).then_some(true),
        results: vec![
            HudSideResult { name: "France".into(), faction: "france".into(), men_start: 340, men_alive: 300, kills: 200, units_start: 3, units_left: 3, flag: String::new() },
            HudSideResult { name: "Austria".into(), faction: "austria".into(), men_start: 340, men_alive: 140, kills: 40, units_start: 3, units_left: 0, flag: String::new() },
        ],
        battle_name: "Arcole".into(),
        player_faction: "france".into(),
        ..Default::default()
    };
    battle::set_facts(&host, &facts).unwrap();
    let root = match battle::load_hud(&host) {
        Ok(r) => r,
        Err(e) => {
            println!("load_hud: {e}");
            host.root().unwrap()
        }
    };
    println!("== loaded");
    if let Some(code) = args.iter().position(|a| a == "--lua").and_then(|i| args.get(i + 1)) {
        if let Err(e) = host.lua().load(code.as_str()).exec() {
            println!("--lua: {e}");
        }
    }
    dump(&host);
    let calls: Vec<&String> = args.iter().enumerate().filter(|(i, _)| *i > 0 && args[i - 1] == "--call").map(|(_, a)| a).collect();
    for name in ["__start__"].into_iter().map(str::to_owned).chain(calls.iter().map(|s| s.to_string())) {
        if name == "__start__" {
            println!("== CreateCards");
            let r = battle::create_cards(&host).and_then(|()| battle::update_cards(&host)).and_then(|()| battle::update_orders(&host));
            println!("  -> {r:?}");
            dump(&host);
            if phase == HudPhase::Deployment {
                println!("== ShowDeploymentPopup");
                println!("  -> {:?}", battle::call_global(&host, "ShowDeploymentPopup", |_| Ok(mlua::MultiValue::new())));
                dump(&host);
            }
            continue;
        }
        println!("== call {name}");
        println!("  -> {:?}", battle::call_global(&host, &name, |_| Ok(mlua::MultiValue::new())));
        dump(&host);
    }
    if let Some(code) = args.iter().position(|a| a == "--lua-end").and_then(|i| args.get(i + 1)) {
        if let Err(e) = host.lua().load(code.as_str()).exec() {
            println!("--lua-end: {e}");
        }
        dump(&host);
    }
    for t in 0..3 {
        host.pulse(1000.0 * f64::from(t));
    }
    println!("== pulses");
    dump(&host);
    let tree = args.iter().any(|a| a == "--tree");
    let mut skip = false;
    for a in &args {
        if skip {
            skip = false;
            continue;
        }
        if a == "--phase" || a == "--call" || a == "--lua" || a == "--lua-end" || a == "--lua-final" {
            skip = true;
            continue;
        }
        if a.starts_with("--") {
            continue;
        }
        for id in a.split(',').filter(|s| !s.is_empty()) {
            let mut target = None;
            host.world().visit_visible(root, &mut |n, node| {
                if target.is_none() && node.data.id == id {
                    target = Some(n);
                }
            });
            println!("== click {id} ({target:?})");
            if let Some(t) = target {
                for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
                    host.pointer(t, e);
                }
                println!("  card click: {:?}", battle::click(&host, t, false));
            }
            dump(&host);
        }
    }
    for t in 3..8 {
        host.pulse(1000.0 * f64::from(t));
    }
    dump(&host);
    if let Some(code) = args.iter().position(|a| a == "--lua-final").and_then(|i| args.get(i + 1)) {
        if let Err(e) = host.lua().load(code.as_str()).exec() {
            println!("--lua-final: {e}");
        }
        dump(&host);
    }
    if tree {
        print_tree(&host.world(), root, 0);
    }
}
