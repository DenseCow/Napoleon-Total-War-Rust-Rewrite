//! Start-position / save-game summariser. Field meanings are INFERRED from
//! inspection (see WORKER3_REPORT.md section 3); positions within each record are CONFIRMED.
use crate::esf::{Esf, Item, Rec, RecArr, Val};
use std::collections::BTreeMap;

pub fn rec<'a>(items: &'a [Item], e: &Esf, name: &str) -> Option<&'a Rec> {
    items.iter().find_map(|i| match i { Item::Rec(r) if e.name(r.name) == name => Some(r), _ => None })
}
pub fn recs<'a>(items: &'a [Item], e: &Esf, name: &str) -> Vec<&'a Rec> {
    items.iter().filter_map(|i| match i { Item::Rec(r) if e.name(r.name) == name => Some(r), _ => None }).collect()
}
pub fn arr<'a>(items: &'a [Item], e: &Esf, name: &str) -> Option<&'a RecArr> {
    items.iter().find_map(|i| match i { Item::RecArr(r) if e.name(r.name) == name => Some(r), _ => None })
}
pub fn vals(items: &[Item]) -> Vec<&Val> { items.iter().filter_map(|i| if let Item::V(v) = i { Some(v) } else { None }).collect() }
pub fn s(v: &Val) -> String { match v { Val::Utf16(x) | Val::Ascii(x) => x.clone(), o => crate::esf::fmt_val(o) } }
pub fn n(v: &Val) -> i64 {
    match v { Val::Bool(x) => *x as i64, Val::I8(x) => *x as i64, Val::I16(x) => *x as i64, Val::I32(x) => *x as i64, Val::I64(x) => *x,
        Val::U8(x) => *x as i64, Val::U16(x) => *x as i64, Val::U32(x) => *x as i64, Val::U64(x) => *x as i64, Val::F32(x) => *x as i64, _ => 0 }
}
fn date(e: &Esf, items: &[Item]) -> String {
    rec(items, e, "DATE").map(|d| { let v = vals(&d.children); format!("{}/{}/{}/{}", n(v[0]), n(v[1]), n(v[2]), n(v[3])) }).unwrap_or_default()
}
fn loc(e: &Esf, r: Option<&Rec>) -> String { let _ = e; r.map(|r| vals(&r.children).iter().map(|v| s(v)).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ")).unwrap_or_default() }
fn short_name(x: &str) -> String { x.rsplit("names_name_names_").next().unwrap_or(x).to_string() }
fn strs(v: &[&Val]) -> Vec<String> { v.iter().map(|x| s(x)).collect() }

pub fn run(e: &Esf, what: Option<&str>) {
    let root = &e.root.children;
    let what = what.unwrap_or("all");
    if let Some(b) = rec(root, e, "BUILD") { println!("BUILD: {:?}", strs(&vals(&b.children))); }
    if let Some(h) = rec(root, e, "SAVE_GAME_HEADER") {
        let v = vals(&h.children);
        println!("SAVE_GAME_HEADER v{}: faction={} portrait={} u32a={} year={} season={} flag={} DATE={}", h.ver, s(v[0]), s(v[1]), n(v[2]), n(v[3]), s(v[4]), s(v[5]), date(e, &h.children));
        if let Some(m) = arr(&h.children, e, "MAPS") { for it in &m.items { let v = vals(it); println!("  MAPS: name={} w={} h={} i32={} preview=u32[{}]", s(v[0]), n(v[1]), n(v[2]), n(v[3]), if let Val::Arr{start,end,..} = v[4] { (end-start)/4 } else {0}); } }
    }
    if let Some(p) = rec(root, e, "CAMPAIGN_PREOPEN_MAP_INFO") {
        let v = vals(&p.children);
        println!("PREOPEN v{}: campaign={} map={} date={}", p.ver, s(v[0]), s(v[1]), date(e, &p.children));
        if let Some(fi) = arr(&p.children, e, "FACTION_INFOS") {
            let flagged: Vec<String> = fi.items.iter().filter(|it| { let v = vals(it); n(v[2]) != 0 || n(v[3]) != 0 || n(v[4]) != 0 || n(v[7]) != 0 || !s(v[1]).is_empty() }).map(|it| { let v = vals(it); format!("{}({:?} b{}{} i{} i{})", s(v[0]), s(v[1]), n(v[2]), n(v[3]), n(v[4]), n(v[7])) }).collect();
            println!("  FACTION_INFOS {} entries; non-default: {}", fi.items.len(), flagged.join(", "));
        }
        if let Some(vo) = arr(&p.children, e, "VICTORY_CONDITION_OPTIONS") {
            for it in &vo.items { let v = vals(it);
                let blocks: Vec<String> = arr(it, e, "VICTORY_CONDITIONS_BLOCK").map(|b| b.items.iter().filter_map(|bi| rec(bi, e, "CAMPAIGN_VICTORY_CONDITIONS")).map(|c| vc(e, c)).collect()).unwrap_or_default();
                println!("  VICTORY {}: {}", s(v[0]), blocks.join(" | ")); }
        }
        if let Some(ro) = arr(&p.children, e, "REGION_OWNERSHIPS_BY_THEATRE") {
            for it in &ro.items { let v = vals(it); let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
                if let Some(r) = arr(it, e, "REGION_OWNERSHIPS") { for x in &r.items { let w = vals(x); by.entry(s(w[1])).or_default().push(s(w[0])); } }
                println!("  REGION_OWNERSHIPS theatre={} : {} owners", s(v[0]), by.len());
                if what == "all" || what == "regions" { for (f, rs) in &by { println!("    {} ({}): {}", f, rs.len(), rs.join(" ")); } } }
        }
    }
    let env = match rec(root, e, "CAMPAIGN_ENV") { Some(x) => x, None => return };
    println!("CAMPAIGN_ENV v{} values {:?}", env.ver, strs(&vals(&env.children)));
    if let Some(cs) = rec(&env.children, e, "CAMPAIGN_SETUP") {
        let v = vals(&cs.children);
        print!("CAMPAIGN_SETUP: key={} u32={} str={:?} bool={}", s(v[0]), n(v[1]), s(v[2]), n(v[3]));
        if let Some(o) = rec(&cs.children, e, "CAMPAIGN_SETUP_OPTIONS") { print!(" OPTIONS={:?}", strs(&vals(&o.children)));
            if let Some(m) = rec(&o.children, e, "CAMPAIGN_SETUP_INGAME_MODIFIABLES") { print!(" INGAME_MODIFIABLES={:?}", strs(&vals(&m.children))); } }
        println!();
        if let Some(ps) = rec(&cs.children, e, "CAMPAIGN_PLAYERS_SETUP").and_then(|p| arr(&p.children, e, "PLAYERS_ARRAY")) {
            let humans: Vec<String> = ps.items.iter().filter_map(|it| rec(it, e, "CAMPAIGN_PLAYER_SETUP")).filter(|p| { let v = vals(&p.children); n(v[1]) != 0 || n(v[2]) != 0 || n(v[3]) != 0 }).map(|p| { let v = vals(&p.children); format!("{}({}{}{})", s(v[0]), n(v[1]), n(v[2]), n(v[3])) }).collect();
            println!("  PLAYERS_ARRAY {} entries; any bool set: {}", ps.items.len(), humans.join(" "));
        }
    }
    if let Some(cm) = rec(&env.children, e, "CAMPAIGN_CAMERA_MANAGER") { if let Some(c) = rec(&cm.children, e, "CAMPAIGN_CAMERA") { println!("CAMERA: {:?}", strs(&vals(&c.children))); } }
    let m = match rec(&env.children, e, "CAMPAIGN_MODEL") { Some(x) => x, None => return };
    println!("CAMPAIGN_MODEL v{} direct values: {:?}", m.ver, strs(&vals(&m.children)));
    if let Some(md) = rec(&m.children, e, "CAMPAIGN_MAP_DATA") { println!("MAP_DATA: {:?}", strs(&vals(&md.children))); }
    if let Some(rs) = rec(&m.children, e, "RandSeed") { println!("RandSeed: {}", s(vals(&rs.children)[0])); }
    if let Some(c) = rec(&m.children, e, "CAMPAIGN_CALENDAR") { let v = vals(&c.children); println!("CALENDAR: turns_per_year?={} turn?={} date={} last_u32={}", n(v[0]), n(v[1]), date(e, &c.children), n(v[2])); }
    if let Some(t) = rec(&m.children, e, "TURN_TIMER") { println!("TURN_TIMER: {:?}", strs(&vals(&t.children))); }
    if let Some(h) = rec(&m.children, e, "HISTORICAL_CHARACTER_MANAGER").and_then(|h| arr(&h.children, e, "CREATED_CHARACTER_ARRAY")) { println!("HISTORICAL_CHARACTERS created ({}): {}", h.items.len(), h.items.iter().map(|i| s(vals(i)[0])).collect::<Vec<_>>().join(" ")); }
    if let Some(h) = rec(&m.children, e, "HISTORICAL_EVENT_MANAGER").and_then(|h| arr(&h.children, e, "TRIGGERED_DYNAMIC_EVENT_ARRAY")) { println!("TRIGGERED_DYNAMIC_EVENTS: {}", h.items.len()); }
    if let Some(pb) = rec(&m.children, e, "PENDING_BATTLE") { println!("PENDING_BATTLE v{} first values {:?}", pb.ver, strs(&vals(&pb.children)).iter().take(10).collect::<Vec<_>>()); }
    if let Some(er) = rec(&m.children, e, "EPISODIC_RESTRICTIONS") {
        let parts: Vec<String> = er.children.iter().filter_map(|i| if let Item::RecArr(a) = i { Some(format!("{}={}", e.name(a.name), a.items.len())) } else { None }).collect();
        println!("EPISODIC_RESTRICTIONS v{}: {} | values {:?}", er.ver, parts.join(" "), strs(&vals(&er.children)));
    }
    if let Some(tm) = rec(&m.children, e, "CAMPAIGN_TRADE_MANAGER") {
        let parts: Vec<String> = tm.children.iter().filter_map(|i| if let Item::RecArr(a) = i { Some(format!("{}={}", e.name(a.name), a.items.len())) } else { None }).collect();
        println!("TRADE_MANAGER: {}", parts.join(" "));
        if let Some(c) = arr(&tm.children, e, "COMMODITIES_ORDER") { println!("  COMMODITIES: {}", c.items.iter().map(|i| s(vals(i)[0])).collect::<Vec<_>>().join(" ")); }
        if let Some(c) = arr(&tm.children, e, "RESOURCES_ORDER") { println!("  RESOURCES: {}", c.items.iter().map(|i| s(vals(i)[0])).collect::<Vec<_>>().join(" ")); }
    }
    let w = match rec(&m.children, e, "WORLD") { Some(x) => x, None => return };
    let mut idkey: BTreeMap<i64, String> = BTreeMap::new();
    let fa = arr(&w.children, e, "FACTION_ARRAY").unwrap();
    let factions: Vec<&Rec> = fa.items.iter().filter_map(|it| rec(it, e, "FACTION")).collect();
    for f in &factions { let v = vals(&f.children); idkey.insert(n(v[0]), s(v[1])); }
    println!("FACTIONS: {}", factions.len());
    let (mut tot_chars, mut tot_armies, mut tot_navies, mut tot_units) = (0, 0, 0, 0);
    let mut stance_tot: BTreeMap<String, usize> = BTreeMap::new();
    for (fi, f) in factions.iter().enumerate() {
        let v = vals(&f.children);
        let id = n(v[0]); let key = s(v[1]); let disp = s(v[2]);
        let treasury = rec(&f.children, e, "FACTION_ECONOMICS").map(|x| n(vals(&x.children)[0])).unwrap_or(0);
        let rel = v.iter().map(|x| s(x)).find(|x| x.starts_with("rel_")).unwrap_or_default();
        let gov = rec(&f.children, e, "GOVERNMENT").and_then(|g| arr(&g.children, e, "GOV_IMP")).and_then(|a| a.items.first()).and_then(|i| i.iter().find_map(|x| if let Item::Rec(r) = x { Some(e.name(r.name).to_string()) } else { None })).unwrap_or_default();
        let chars: Vec<&Rec> = arr(&f.children, e, "CHARACTER_ARRAY").map(|a| a.items.iter().filter_map(|i| rec(i, e, "CHARACTER")).collect()).unwrap_or_default();
        let mut ctype: BTreeMap<String, usize> = BTreeMap::new();
        for c in &chars { *ctype.entry(s(vals(&c.children)[1])).or_default() += 1; }
        let (mut armies, mut navies, mut units) = (0, 0, 0);
        if let Some(aa) = arr(&f.children, e, "ARMY_ARRAY") { for it in &aa.items { for x in it { if let Item::Rec(r) = x { let nm = e.name(r.name); if nm == "ARMY" { armies += 1 } else if nm == "NAVY" { navies += 1 }
            units += arr(&r.children, e, "UNITS_ARRAY").map(|u| u.items.len()).unwrap_or(0); } } } }
        let mut stance: BTreeMap<String, Vec<String>> = BTreeMap::new();
        if let Some(dr) = rec(&f.children, e, "DIPLOMACY_MANAGER").and_then(|d| arr(&d.children, e, "DIPLOMACY_RELATIONSHIPS_ARRAY")) {
            for it in &dr.items { if let Some(r) = rec(it, e, "DIPLOMACY_RELATIONSHIP") { let dv = vals(&r.children); let other = idkey.get(&n(dv[0])).cloned().unwrap_or(format!("id{}", n(dv[0])));
                let st = dv.iter().find_map(|x| if let Val::Utf16(t) = x { Some(t.clone()) } else { None }).unwrap_or("?".into());
                *stance_tot.entry(st.clone()).or_default() += 1;
                stance.entry(st).or_default().push(other); } }
        }
        let techs = rec(&f.children, e, "FACTION_TECHNOLOGY_MANAGER").and_then(|t| arr(&t.children, e, "techs")).map(|t| {
            let mut st: BTreeMap<i64, usize> = BTreeMap::new(); for i in &t.items { *st.entry(n(vals(i)[1])).or_default() += 1; } format!("techs={} state_hist={:?}", t.items.len(), st) }).unwrap_or_default();
        tot_chars += chars.len(); tot_armies += armies; tot_navies += navies; tot_units += units;
        let non_neutral: Vec<String> = stance.iter().filter(|(k, _)| k.as_str() != "neutral").map(|(k, v)| format!("{}:[{}]", k, v.join(","))).collect();
        println!("  [{}] {} \"{}\" id={} treasury={} {} gov={} chars={} {:?} armies={} navies={} units={} {} | {} neutral; {}",
            fi, key, disp, id, treasury, rel, gov.trim_start_matches("GOVERNMENT::"), chars.len(), ctype, armies, navies, units, techs, stance.get("neutral").map(|x| x.len()).unwrap_or(0), non_neutral.join(" "));
        if what == "chars" || what == key {
            for c in &chars {
                let cv = vals(&c.children);
                let d = rec(&c.children, e, "CHARACTER_DETAILS").unwrap();
                let names: Vec<String> = recs(&d.children, e, "CAMPAIGN_LOCALISATION").iter().take(2).map(|r| short_name(&loc(e, Some(r)))).collect();
                let traits: Vec<String> = rec(&d.children, e, "TRAITS").and_then(|t| arr(&t.children, e, "TRAIT")).map(|t| t.items.iter().map(|i| { let v = vals(i); format!("{}={}", s(v[0]), n(v[1])) }).collect()).unwrap_or_default();
                let anc: Vec<String> = arr(&d.children, e, "AgentAncillaries").map(|t| t.items.iter().map(|i| s(vals(i)[0])).collect()).unwrap_or_default();
                let lv = rec(&c.children, e, "LOCOMOTABLE").map(|l| vals(&l.children).iter().take(10).map(|x| n(x)).collect::<Vec<_>>()).unwrap_or_default();
                println!("     char {} {:?} born={} pos_raw=({},{}) i32pair=({},{}) traits={:?} anc={:?} force_ref={}", s(cv[1]), names.join(" "), date(e, &d.children), lv[0], lv[1], lv[8], lv[9], traits, anc, n(cv[2]));
            }
            if let Some(aa) = arr(&f.children, e, "ARMY_ARRAY") { for it in &aa.items { for x in it { if let Item::Rec(r) = x {
                let mf = rec(&r.children, e, "MILITARY_FORCE").map(|m| vals(&m.children).iter().map(|v| n(v)).collect::<Vec<_>>()).unwrap_or_default();
                let us: Vec<String> = arr(&r.children, e, "UNITS_ARRAY").map(|u| u.items.iter().filter_map(|i| i.iter().find_map(|x| if let Item::Rec(r) = x { Some(r) } else { None })).map(|lu| {
                    let key = rec(&lu.children, e, "UNIT").and_then(|u| rec(&u.children, e, "UNIT_RECORD_KEY")).map(|k| s(vals(&k.children)[0])).unwrap_or_default();
                    let uv = rec(&lu.children, e, "UNIT").map(|u| vals(&u.children).iter().take(4).map(|v| n(v)).collect::<Vec<_>>()).unwrap_or_default();
                    format!("{}({}/{} x{})", key, uv.get(1).unwrap_or(&0), uv.get(2).unwrap_or(&0), uv.get(3).unwrap_or(&0)) }).collect()).unwrap_or_default();
                println!("     {} force(id,commander)={:?} units: {}", e.name(r.name), &mf[..2.min(mf.len())], us.join(", "));
            } } } }
        }
    }
    println!("TOTAL characters={} armies={} navies={} units={} diplomacy_stance_counts={:?}", tot_chars, tot_armies, tot_navies, tot_units, stance_tot);
    if let Some(rm) = rec(&w.children, e, "REGION_MANAGER").and_then(|r| arr(&r.children, e, "REGIONS_ARRAY")) {
        println!("REGIONS: {}", rm.items.len());
        if what == "all" || what == "regions" {
            for it in &rm.items { let r = match rec(it, e, "REGION") { Some(x) => x, None => continue }; let v = vals(&r.children);
                let st = rec(&r.children, e, "SETTLEMENT");
                let (sname, spos) = st.map(|st| { let sv = vals(&st.children); let sg = rec(&st.children, e, "SIEGEABLE_GARRISON_RESIDENCE").map(|g| { let gv = vals(&g.children); (n(gv[9]), n(gv[10])) }).unwrap_or((0, 0)); (s(sv[1]), sg) }).unwrap_or_default();
                let res: Vec<String> = arr(&r.children, e, "RESOURCES_ARRAY").map(|a| a.items.iter().map(|i| s(vals(i)[0])).collect()).unwrap_or_default();
                let mut blds = Vec::new();
                if let Some(sm) = rec(&r.children, e, "REGION_SLOT_MANAGER") { if let Some(sa) = arr(&sm.children, e, "REGION_SLOT_ARRAY") { for si in &sa.items { if let Some(sl) = rec(si, e, "REGION_SLOT") {
                    if let Some(b) = rec(&sl.children, e, "BUILDING_MANAGER").and_then(|bm| rec(&bm.children, e, "BUILDING")) { blds.push(s(vals(&b.children)[1])); } } } } }
                let pop = rec(&r.children, e, "POPULATION").map(|p| n(vals(&p.children)[0])).unwrap_or(0);
                let st2: Vec<String> = v.iter().filter_map(|x| if let Val::Utf16(t) = x { Some(t.clone()) } else { None }).collect();
                println!("  {} settlement={} pos_raw={:?} pop={} strs={:?} resources={:?} buildings={:?}", s(v[0]), sname, spos, pop, &st2[1..], res, blds);
            }
        }
    }
}

fn vc(e: &Esf, c: &Rec) -> String {
    let v = vals(&c.children);
    let regions: Vec<String> = arr(&c.children, e, "REGION_KEYS").map(|a| a.items.iter().map(|i| s(vals(i)[0])).collect()).unwrap_or_default();
    let dates: Vec<String> = recs(&c.children, e, "DATE").iter().map(|d| { let v = vals(&d.children); format!("{}/{}/{}/{}", n(v[0]), n(v[1]), n(v[2]), n(v[3])) }).collect();
    format!("regions{:?} vals={:?} dates={:?}", regions, v.iter().map(|x| n(x)).collect::<Vec<_>>(), dates)
}
