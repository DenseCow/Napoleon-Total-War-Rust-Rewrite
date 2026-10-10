//! Research helper: runs the original campaign HUD (`ui\campaign ui\layout`) headless against a
//! real start position, with our `CampaignUI` functions, and drives it. Read-only.
//!   cargo run -p ntw_script --release --example campaign_hud_probe -- [--tree] [--all] [--campaign eur_napoleon]
//!       [--faction france] [char:<n>] [region:<key>] [none] [id1,id2,...]
//! `char:<n>` selects the human faction's n-th army commander, `region:<key>` a settlement,
//! `none` clears the selection; other words click components by id. The scripts' log is printed
//! after each step and, with `--tree`, the visible components at the end (`--all`: hidden too).
//! `--apply` applies the model commands the HUD asks for (build, recruit, cancel, ...) and
//! re-sends the selection afterwards, as the game does; `treasury` prints the human's treasury and
//! the selected region's queues; `endturn` ends the turn.
use std::rc::Rc;

use ntw_formats::loc::Localisation;
use ntw_script::ui::{CampaignLink, CampaignRequest, CampaignSelection, FrontEndFacts, NodeId, PointerEvent, UiScriptHost, UiWorld};
use ntw_script::{ScriptHost, ScriptSource};
use ntw_sim::campaign::CharacterKind;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn print_tree(w: &UiWorld, id: NodeId, depth: usize, all: bool) {
    let Some(n) = w.get(id) else { return };
    if !n.visible && !all {
        return;
    }
    let text = n.current().map(|s| s.text.as_str()).unwrap_or("");
    println!(
        "{:indent$}{} #{} [{}]{} {:.0},{:.0} {:.0}x{:.0} {}",
        "",
        n.data.id,
        id,
        n.state_name(),
        if n.visible { "" } else { " (hidden)" },
        n.rect.x,
        n.rect.y,
        n.rect.w,
        n.rect.h,
        if text.is_empty() { String::new() } else { format!("{text:?}") },
        indent = depth * 2
    );
    for &c in &n.children {
        print_tree(w, c, depth + 1, all);
    }
}

fn drain(host: &UiScriptHost) {
    drain_apply(host, None, &mut CampaignSelection::None);
}

/// Prints the log and the HUD's requests; with a script host, applies the model commands (and
/// selections) and re-sends the selection once anything changed (the game does this through its
/// change counter).
fn drain_apply(host: &UiScriptHost, mut scripts: Option<&mut ScriptHost>, sel: &mut CampaignSelection) {
    for l in host.take_log() {
        if !l.starts_with("out.") || std::env::var_os("PROBE_OUT").is_some() {
            println!("  {l}");
        }
    }
    let mut changed = false;
    for r in host.take_campaign_requests() {
        println!("  request {r:?}");
        let Some(s) = scripts.as_deref_mut() else { continue };
        match r {
            CampaignRequest::Command(cmd) => {
                println!("    -> {:?}", s.apply(cmd).map(|e| e.len()));
                changed = true;
            }
            CampaignRequest::EndTurn => {
                println!("    -> {:?}", s.apply(ntw_sim::campaign::CampaignCommand::EndTurn).map(|e| e.len()));
                changed = true;
            }
            CampaignRequest::Select(x) => {
                *sel = x;
                changed = true;
            }
            CampaignRequest::CameraTo(..) => {}
        }
    }
    if changed {
        host.campaign_select(*sel);
        for l in host.take_log() {
            if l.starts_with("ERROR") {
                println!("  {l}");
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let campaign = opt("--campaign").unwrap_or_else(|| "eur_napoleon".into());
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let db = ntw_data::GameDatabase::from_vfs(&vfs).unwrap();
    if let Some(sub) = opt("--variants") {
        use ntw_formats::db::{DbTable, Schema};
        for (path, codes) in [
            ("db/building_faction_variants_tables/building_faction_variants", "s,s,o,o,o"),
            ("db/building_culture_variants_tables/building_culture_variants", "s,s,o,o,o,o,o"),
        ] {
            let bytes = vfs.read(path).unwrap();
            let t = DbTable::read(&bytes, &Schema::from_codes(codes).unwrap()).unwrap();
            println!("== {path}: {} rows", t.rows.len());
            for r in t.rows.iter().filter(|r| r.first().and_then(|v| v.as_str()).is_some_and(|k| k.contains(sub.as_str()))) {
                println!("  {r:?}");
            }
        }
        return;
    }
    if let Some(k) = opt("--faction-info") {
        println!("{:?}", db.faction(&k));
        return;
    }
    if let Some(sub) = opt("--levels") {
        for b in db.building_levels.iter().filter(|b| b.key.contains(sub.as_str())) {
            println!("{b:?}");
        }
        return;
    }
    let rel = format!("campaigns/{campaign}/startpos.esf");
    let bytes = std::fs::read(std::path::Path::new(&dir).join(&rel)).or_else(|_| vfs.read(&rel)).unwrap();
    let mut loaded = ntw_campaign::read(&bytes, &db).unwrap();
    let human = opt("--faction").unwrap_or_else(|| loaded.info.header.faction_key.clone());
    loaded.set_human(&human);
    let mut scripts = ScriptHost::new(loaded.model, &human, ScriptSource::from_install(&dir).unwrap()).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { game_version: "1.3.0".into(), ..Default::default() };
    let host = UiScriptHost::new(ScriptSource::from_install(&dir).unwrap(), loc, facts, (1280.0, 960.0), db.limits.clone()).unwrap();
    host.install_campaign(CampaignLink { state: scripts.shared_state(), human: human.clone(), campaign: campaign.clone(), map: loaded.info.map_key.clone(), theatres: loaded.info.theatres().map(str::to_owned).collect(), db: Rc::new(db) })
        .unwrap();
    let root = host.load_root_layout("data/ui/campaign ui/layout").unwrap();
    println!("== loaded");
    drain(&host);
    host.campaign_ready();
    host.campaign_update_funds("Early May");

    println!("== start");
    drain(&host);
    let mut skip = false;
    let apply = args.iter().any(|a| a == "--apply");
    let mut sel = CampaignSelection::None;
    // The UI clock (`OnUpdatePulse` and `CampaignUI.Time`): each step is a second of UI time, pulsed
    // at 100 ms, so scripted transitions (drop-downs, slides) run to their end between steps.
    let mut clock_ms = 0.0;
    for a in &args {
        if skip {
            skip = false;
            continue;
        }
        if a.starts_with("--") {
            skip = matches!(a.as_str(), "--campaign" | "--faction");
            continue;
        }
        if let Some(n) = a.strip_prefix("char:") {
            let n: usize = n.parse().unwrap_or(0);
            let c = {
                let st = scripts.state();
                let m = &st.model;
                let f = m.faction_by_key(&human).map(|f| f.id);
                m.world
                    .forces
                    .values()
                    .filter(|x| Some(x.faction) == f && x.commander.is_some())
                    .filter_map(|x| x.commander)
                    .filter(|c| m.world.characters.get(c).is_some_and(|ch| ch.kind == CharacterKind::General))
                    .nth(n)
            };
            println!("== select character {c:?}");
            if let Some(c) = c {
                sel = CampaignSelection::Character(c);
                host.campaign_select(sel);
            }
        } else if let Some(k) = a.strip_prefix("region:") {
            let r = scripts.state().model.world.regions.values().find(|r| r.key == k).map(|r| r.id);
            println!("== select region {r:?}");
            if let Some(r) = r {
                sel = CampaignSelection::Settlement(r);
                host.campaign_select(sel);
            }
        } else if let Some(id) = a.strip_prefix("hover:") {
            let mut target = None;
            host.world().visit_visible(root, &mut |n, node| {
                if target.is_none() && node.data.id == id {
                    target = Some(n);
                }
            });
            println!("== hover {id} ({target:?})");
            host.campaign_hover(target);
        } else if a == "labels" {
            let regs: Vec<_> = scripts.state().model.world.regions.values().take(3).map(|r| r.id).collect();
            let list = regs.iter().enumerate().map(|(i, r)| (*r, 300.0 + 200.0 * i as f32, 300.0)).collect();
            host.campaign_set_view((1.0, 2.0, 3.0), list, None);
            println!("== labels");
        } else if a == "treasury" {
            let st = scripts.state();
            let m = &st.model;
            let t = m.faction_by_key(&human).map_or(0, |f| f.treasury);
            println!("== treasury {t}");
            println!("   restricted buildings {:?}", st.model.world.restricted_buildings);
            if let CampaignSelection::Settlement(r) = sel
                && let Some(r) = m.world.regions.get(&r)
            {
                for c in &r.construction {
                    println!("   construction slot {:?} {} turns {} cost {}", c.slot, c.level_key, c.turns_remaining, c.cost);
                }
                for q in &r.recruitment_queue {
                    println!("   recruitment {q:?}");
                }
                for (i, s) in r.slots.iter().enumerate() {
                    let standing = s.building.as_ref().map(|b| b.level_key.as_str()).unwrap_or("-");
                    let options: Vec<String> = m
                        .rules
                        .buildings
                        .iter()
                        .filter_map(|(k, b)| m.can_build(r.id, ntw_sim::campaign::SlotRef::Slot(i), k).ok().map(|c| format!("{k}(l{} {c} {}t)", b.level, b.turns)))
                        .collect();
                    println!("   slot {i} {} [{}] {standing}: {}", s.key, s.slot_type, options.join(", "));
                }
            }
        } else if a == "agents" {
            let st = scripts.state();
            let m = &st.model;
            let f = m.faction_by_key(&human).map(|f| f.id);
            for (c, ch) in m.world.characters.iter() {
                if Some(ch.faction) != f {
                    continue;
                }
                let key = ch.garrisoned_in.and_then(|r| m.world.regions.get(&r)).map(|r| r.key.clone());
                let details = m.world.character_details.get(c);
                println!("  {c:?} {:?} {:?} garrison={:?} ({key:?}) abilities={:?}", ch.kind, ch.position, ch.garrisoned_in, details.map(|d| d.abilities.clone()));
            }
        } else if let Some(k) = a.strip_prefix("stagecapture:") {
            // Stages a capture of region <k> by the human (the model's preview, as after a
            // battle won at that settlement) and lets the HUD open its capture screen.
            let mut st = scripts.state_mut();
            let m = &mut st.model;
            let f = m.faction_by_key(&human).map(|f| f.id);
            let r = m.world.regions.values().find(|r| r.key == k).map(|r| r.id);
            if let (Some(f), Some(r)) = (f, r) {
                let p = m.capture_preview(r, f, None, false);
                m.pending_capture = Some(p);
            }
            drop(st);
            println!("== stage capture {k} -> screen opened {}", host.campaign_capture_screen());
        } else if a == "endturn" {
            println!("== end turn -> {:?}", scripts.apply(ntw_sim::campaign::CampaignCommand::EndTurn).map(|e| e.len()));
            host.campaign_select(sel);
        } else if a == "none" {
            println!("== select none");
            sel = CampaignSelection::None;
            host.campaign_select(sel);
        } else {
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
                }
            }
        }
        for _ in 0..10 {
            clock_ms += 100.0;
            host.pulse(clock_ms);
        }
        drain_apply(&host, if apply { Some(&mut scripts) } else { None }, &mut sel);
    }
    if args.iter().any(|a| a == "--tree") {
        print_tree(&host.world(), root, 0, args.iter().any(|a| a == "--all"));
    }
}
