//! Runs the ORIGINAL campaign HUD (`ui\campaign ui\layout` and its scripts) from the player's install
//! (read-only) against the eur_napoleon start position, with our `CampaignUI` functions. Skipped when
//! the install is not there. Log: `cargo test -p ntw_script --test campaign_ui -- --nocapture`.

use std::path::PathBuf;
use std::rc::Rc;

use ntw_formats::loc::Localisation;
use ntw_script::ui::{CampaignLink, CampaignRequest, CampaignSelection, FrontEndFacts, NodeId, PointerEvent, UiScriptHost};
use ntw_script::{ScriptHost, ScriptSource};
use ntw_sim::campaign::{CampaignCommand, CharacterKind};
use ntw_script::ui::test_support::no_errors;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

struct Setup {
    scripts: ScriptHost,
    host: UiScriptHost,
    root: NodeId,
}

fn setup() -> Option<Setup> {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return None;
    }
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let db = ntw_data::GameDatabase::from_vfs(&vfs).unwrap();
    let bytes = std::fs::read(dir.join("campaigns/eur_napoleon/startpos.esf")).unwrap();
    let mut loaded = ntw_campaign::read(&bytes, &db).unwrap();
    loaded.set_human("france");
    let scripts = ScriptHost::new(loaded.model, "france", ScriptSource::from_install(&dir).unwrap()).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let host = UiScriptHost::new(ScriptSource::from_install(&dir).unwrap(), loc, FrontEndFacts::default(), (1280.0, 960.0)).unwrap();
    host.install_campaign(CampaignLink { state: scripts.shared_state(), human: "france".into(), campaign: "eur_napoleon".into(), db: Rc::new(db) })
        .unwrap();
    let root = host.load_root_layout("data/ui/campaign ui/layout").unwrap();
    host.campaign_ready();
    host.campaign_update_funds("Early May");
    Some(Setup { scripts, host, root })
}

fn errors(host: &UiScriptHost) -> Vec<String> {
    let log = host.take_log();
    for l in &log {
        println!("  {l}");
    }
    log.into_iter().filter(|l| l.starts_with("ERROR")).collect()
}

fn find(host: &UiScriptHost, root: NodeId, id: &str) -> Option<NodeId> {
    let mut out = None;
    host.world().visit_visible(root, &mut |n, node| {
        if out.is_none() && node.data.id == id {
            out = Some(n);
        }
    });
    out
}

fn text(host: &UiScriptHost, n: NodeId) -> String {
    host.world().get(n).and_then(|x| x.current().map(|s| s.text.clone())).unwrap_or_default()
}

#[test]
fn start_up_has_no_script_errors_and_an_empty_selection_bar() {
    let Some(s) = setup() else { return };
    assert!(errors(&s.host).is_empty());
    let selector = find(&s.host, s.root, "selector").expect("selection bar");
    // The layout's sample text "ygT" is cleared by the engine's ClearHud.
    assert_eq!(text(&s.host, selector), "");
    let funds = find(&s.host, s.root, "funds").expect("funds");
    assert!(!text(&s.host, funds).is_empty());
}

#[test]
fn selecting_an_army_shows_its_unit_cards() {
    let Some(s) = setup() else { return };
    let (general, units) = {
        let st = s.scripts.state();
        let m = &st.model;
        let f = m.faction_by_key("france").unwrap().id;
        m.world
            .forces
            .values()
            .filter(|x| x.faction == f && !x.is_navy)
            .find_map(|x| {
                let c = x.commander?;
                (m.world.characters.get(&c)?.kind == CharacterKind::General).then_some((c, x.units.len()))
            })
            .unwrap()
    };
    s.host.campaign_select(CampaignSelection::Character(general));
    assert!(errors(&s.host).is_empty());
    let army_tab = find(&s.host, s.root, "army_tab").expect("army tab");
    // A general's army always has Army | Recruitment, in that order (the exe's `0x009855B0`, the
    // user's side-by-side check 2026-10-07; regression: Army only).
    let recruit_tab = find(&s.host, s.root, "recruitment_tab").expect("the army's recruitment tab");
    let x = |n: NodeId| s.host.world().get(n).unwrap().rect.x;
    assert!(x(army_tab) < x(recruit_tab), "Army left of Recruitment");
    let group = find(&s.host, s.root, "UnitCardGroup").unwrap();
    let cards = s.host.world().get(group).unwrap().children.len();
    assert_eq!(cards, units);
    let selector = find(&s.host, s.root, "selector").unwrap();
    assert!(text(&s.host, selector).contains(", "));

    // Clicking a card selects it and it stays "Selected" (campaign card selection, a regression
    // check for the event-before-transition order): the click event runs first and the card
    // manager's ManageSelection sets "Selected"; the click's transition then starts from
    // "Selected", the state the event left (`0x0102E340` calls `0x01035620` after the event, and
    // `0x01035620` reads the current state on entry, CONFIRMED). The shipped cards' "Selected"
    // state has no transition for a press, a release or the mouse leaving (CampaignUnitCard,
    // CampaignCharacterCard, BattleUnitCard: `ui_probe -- statekey Selected <key>`), so the
    // selection holds.
    if units >= 2 {
        let kids = s.host.world().get(group).unwrap().children.clone();
        let state = |n: NodeId| s.host.world().get(n).unwrap().state_name().to_owned();
        let click = |h: &UiScriptHost, n: NodeId| {
            for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
                h.pointer(n, e);
            }
        };
        click(&s.host, kids[0]);
        no_errors(&s.host);
        assert_eq!(state(kids[0]), "Selected", "the clicked card is selected");
        click(&s.host, kids[1]);
        no_errors(&s.host);
        assert_eq!(state(kids[1]), "Selected", "the second card is selected");
        assert_eq!(state(kids[0]), "Default", "and the first one no longer");
    }

    // The general's own card shows his portrait (the exe's army panel cards have DisplayAsUnit
    // false, `0x009FCFB0`; Portrait = "data/" + his card picture, `0x008C9EF0`); the others are
    // unit cards (user side by side 2026-10-07; regression: the bodyguard card for everyone).
    let env = s.host.script_env(s.root).unwrap();
    let (display_as_unit, portrait, second_portrait): (bool, String, Option<String>) = s
        .host
        .lua()
        .load("local u = CampaignUI.ReviewPanelInfo().units_info.Units return u[1].DisplayAsUnit, u[1].Portrait, u[2] and u[2].Portrait")
        .set_environment(env.clone())
        .eval()
        .unwrap();
    assert!(!display_as_unit);
    assert!(portrait.starts_with("data/ui/portraits/") && portrait.contains("/Cards/"), "{portrait}");
    assert!(second_portrait.is_none_or(|p| p.is_empty()), "only the commander's card has a portrait");

    // The Lists rows are the commanders' character details: a general is shown as himself (his
    // portrait card), a colonel by his unit's card (`0x009AD250`'s ShowAsCharacter / CommandedUnit).
    let rows: Vec<String> = s
        .host
        .lua()
        .load(
            "local out = {}\nfor _, r in ipairs(CampaignUI.RetrieveFactionMilitaryForceLists('france', true)) do\n\
             out[#out + 1] = r.AgentType .. ' ' .. tostring(r.ShowAsCharacter) .. ' ' .. tostring(r.CommandedUnit ~= nil and r.CommandedUnit.DisplayAsUnit == true)\n\
             end return out",
        )
        .set_environment(env)
        .eval()
        .unwrap();
    assert!(rows.iter().any(|r| r.starts_with("General ")), "{rows:?}");
    assert!(rows.iter().any(|r| r.starts_with("colonel ")), "{rows:?}");
    for r in &rows {
        if r.starts_with("General ") {
            assert_eq!(r, "General true false", "{rows:?}");
        } else if r.starts_with("colonel ") {
            assert_eq!(r, "colonel false true", "{rows:?}");
        }
    }

    // The army's recruitment tab opens its panel without a script error, wherever the army is.
    click(&s.host, recruit_tab);
    assert!(errors(&s.host).is_empty());
}

#[test]
fn settlement_recruitment_cards_queue_a_unit() {
    let Some(s) = setup() else { return };
    let paris = s.scripts.state().model.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    // Paris's first slot (sAdmin2) upgrades to sAdmin3, which needs `admin1_public_schooling`
    // (`building_level_required_technology_junctions`): give France that technology first.
    {
        let mut st = s.scripts.state_mut();
        let france = st.model.world.factions.values().find(|f| f.key == "france").unwrap().id;
        let d = st.model.world.faction_details.get_mut(&france).unwrap();
        for (k, state) in d.technologies.iter_mut() {
            if k == "admin1_public_schooling" {
                *state = 0;
            }
        }
    }
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    assert!(errors(&s.host).is_empty());
    let selector = find(&s.host, s.root, "selector").unwrap();
    assert_eq!(text(&s.host, selector), "Paris, France");
    // The construction tab comes first; building in the first slot queues a construction.
    let upgrade = find(&s.host, s.root, "Building1_Upgrade1").expect("upgrade option");
    click(&s.host, upgrade);
    assert!(errors(&s.host).is_empty());
    let reqs = s.host.take_campaign_requests();
    assert!(reqs.iter().any(|r| matches!(r, CampaignRequest::Command(CampaignCommand::ConstructBuilding { region, slot: ntw_sim::campaign::SlotRef::Slot(0), .. }) if *region == paris)), "requests: {reqs:?}");
    let tab = find(&s.host, s.root, "recruitment_tab").expect("recruitment tab");
    click(&s.host, tab);
    let card = find(&s.host, s.root, "Inf_Line_French_Fusiliers!recruitable!3").expect("fusiliers card");
    click(&s.host, card);
    let _ = errors(&s.host);
    let reqs = s.host.take_campaign_requests();
    assert!(
        reqs.iter().any(|r| matches!(r, CampaignRequest::Command(CampaignCommand::Recruit { region, unit_key }) if *region == paris && unit_key == "Inf_Line_French_Fusiliers")),
        "requests: {reqs:?}"
    );
}

fn click(host: &UiScriptHost, n: NodeId) {
    for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
        host.pointer(n, e);
    }
}

#[test]
fn hud_buttons_show_their_tooltips() {
    let Some(s) = setup() else { return };
    let end_turn = find(&s.host, s.root, "button_end_turn").unwrap();
    s.host.campaign_hover(Some(end_turn));
    assert!(errors(&s.host).is_empty());
    let tip = find(&s.host, s.root, "Tooltip").expect("tooltip shown");
    assert!(!text(&s.host, tip).is_empty());
    s.host.campaign_hover(None);
    assert!(find(&s.host, s.root, "Tooltip").is_none(), "tooltip hidden");
}

/// The radar (template.map_image.lua via theatre_map.lua) gets its theatre's pictures and one
/// entry per region whose PaletteEntry finds the region's colour in the lookup picture's palette;
/// the recoloured lookup picture ends up on a component.
#[test]
fn radar_map_gets_its_pictures_and_region_colours() {
    let Some(s) = setup() else { return };
    assert!(errors(&s.host).is_empty());
    let t: mlua::Table = s.host.lua().load("return CampaignUI.RegionsInTheatre('europe_main', 'france', nil, 4)").eval().unwrap();
    assert_eq!(t.get::<String>("Radar").unwrap(), "data/campaign_maps/nap_europe/stratradar_europe.tga");
    assert_eq!(t.get::<String>("Map").unwrap(), "data/campaign_maps/nap_europe/europe_map.tga");
    assert_eq!(t.get::<String>("Overlay").unwrap(), "data/campaign_maps/nap_europe/europe_lookup.tga");
    let mut entries = Vec::new();
    for e in t.sequence_values::<mlua::Table>() {
        let e = e.unwrap();
        entries.push((e.get::<String>("Key").unwrap(), e.get::<i32>("PaletteEntry").unwrap()));
        let rgb: mlua::Table = e.get("OwnerRGB").unwrap();
        assert!(rgb.get::<i32>("r").is_ok());
    }
    let found = entries.iter().filter(|(_, p)| *p >= 0).count();
    println!("{found} of {} regions found in the lookup palette", entries.len());
    assert!(entries.len() > 50 && found == entries.len(), "{:?}", entries.iter().filter(|(_, p)| *p < 0).collect::<Vec<_>>());
    let mut palette: Vec<i32> = entries.iter().map(|e| e.1).collect();
    palette.sort();
    palette.dedup();
    assert_eq!(palette.len(), entries.len(), "each region has its own palette entry");
    let w = s.host.world();
    assert!(!w.runtime_images.is_empty(), "the radar made its palette picture");
    let shown = w.ids().filter_map(|n| w.get(n)).any(|n| n.data.images.iter().any(|i| i.path.starts_with(ntw_script::ui::RUNTIME_IMAGE_PREFIX)));
    assert!(shown, "a component shows the palette picture");
}

/// TheatreList / HomeTheatre / TheatreMapDimensions name the theatre as the exe does: the
/// `campaign_map_playable_areas` key, its area as Id, its bounds from regions.esf.
#[test]
fn theatre_keys_and_bounds() {
    let Some(s) = setup() else { return };
    let lua = s.host.lua();
    let list: mlua::Table = lua.load("return CampaignUI.TheatreList(false)").eval().unwrap();
    let e: mlua::Table = list.get(1).unwrap();
    assert_eq!(e.get::<String>("Key").unwrap(), "1244818741");
    assert_eq!(e.get::<String>("Id").unwrap(), "europe_main");
    assert_eq!(e.get::<String>("Name").unwrap(), "Europe");
    assert_eq!(lua.load("return CampaignUI.HomeTheatre('france')").eval::<String>().unwrap(), "1244818741");
    let (x, y, w, h): (f32, f32, f32, f32) = lua.load("return CampaignUI.TheatreMapDimensions('1244818741')").eval().unwrap();
    assert_eq!((x, y, w, h), (-410.0, -190.0, 750.0, 385.0));
    assert!(errors(&s.host).is_empty());
}

/// The building browser (build_browser button) lists the player's capital's slots from
/// `BuildingBrowserDetails`: Paris's settlement buildings as "capital" entries, resource, town and
/// port slots, and the road.
#[test]
fn building_browser_lists_the_capital() {
    let Some(s) = setup() else { return };
    let t: mlua::Table = s.host.lua().load("return CampaignUI.BuildingBrowserDetails()").eval().unwrap();
    assert_eq!(t.get::<String>("region_name").unwrap(), "France");
    let slots: Vec<mlua::Table> = t.get::<mlua::Table>("slots").unwrap().sequence_values().map(Result::unwrap).collect();
    let row = |e: &mlua::Table| (e.get::<String>("building_key").unwrap(), e.get::<i32>("type").unwrap(), e.get::<String>("location").unwrap());
    let rows: Vec<_> = slots.iter().map(row).collect();
    assert!(rows.contains(&("sAdmin2_magistrate".into(), 1, "Paris".into())), "{rows:?}");
    assert!(rows.contains(&("rTimber1_timber_logging_camp".into(), 8, "Limoges Forests".into())), "{rows:?}");
    assert!(rows.contains(&("tEducation1_college".into(), 3, "Orléans".into())), "{rows:?}");
    assert!(rows.iter().any(|r| r.1 == 6), "the road: {rows:?}");
    let stables = slots.iter().find(|e| e.get::<String>("building_key").unwrap() == "rHorse1_stables").unwrap();
    assert_eq!(stables.get::<String>("image").unwrap(), "data/ui/buildings/icons/eu_rhorse1_stables.tga");
    let root = s.root;
    let browser = find(&s.host, root, "build_browser").expect("button");
    click(&s.host, browser);
    assert!(errors(&s.host).is_empty());
}

/// Applies the HUD's model commands as the game does, then re-sends the selection.
fn apply_requests(s: &mut Setup, sel: CampaignSelection) -> usize {
    let mut n = 0;
    for r in s.host.take_campaign_requests() {
        if let CampaignRequest::Command(cmd) = r {
            s.scripts.apply(cmd).expect("command applies");
            n += 1;
        }
    }
    s.host.campaign_select(sel);
    n
}

#[test]
fn settlement_panel_builds_cancels_and_recruits() {
    let Some(mut s) = setup() else { return };
    let (paris, treasury) = {
        let st = s.scripts.state();
        let m = &st.model;
        (m.world.regions.values().find(|r| r.key == "eur_france").unwrap().id, m.faction_by_key("france").unwrap().treasury)
    };
    let gold = |s: &Setup| s.scripts.state().model.faction_by_key("france").unwrap().treasury;
    let sel = CampaignSelection::Settlement(paris);
    s.host.campaign_select(sel);
    assert!(errors(&s.host).is_empty());
    // Resting the pointer on a slot shows its upgrades (template.BuildingFrame.lua's SelectPassive
    // reads the root's g_repair_construction_button through the Construction module).
    assert!(find(&s.host, s.root, "Building3_Upgrade1").is_none());
    let slot = find(&s.host, s.root, "Building3").expect("third slot");
    s.host.pointer(slot, PointerEvent::Enter);
    let upgrade = find(&s.host, s.root, "Building3_Upgrade1").expect("upgrade shown on hover");
    click(&s.host, upgrade);
    assert!(errors(&s.host).is_empty());
    assert_eq!(apply_requests(&mut s, sel), 1);
    assert!(gold(&s) < treasury);
    // The slot now shows its construction; clicking it cancels with a refund.
    let slot = find(&s.host, s.root, "Building3").expect("third slot");
    assert!(find(&s.host, slot, "constructing_animation").is_some());
    click(&s.host, slot);
    assert!(errors(&s.host).is_empty());
    assert_eq!(apply_requests(&mut s, sel), 1);
    assert_eq!(gold(&s), treasury);
    // The empty prestige slot offers only France's own prestige building, greyed (too dear).
    let empty = find(&s.host, s.root, "Slot5").expect("empty slot");
    s.host.pointer(empty, PointerEvent::Enter);
    assert!(find(&s.host, s.root, "Constructable1").is_some());
    assert!(find(&s.host, s.root, "Constructable2").is_none());
    let cost = find(&s.host, s.root, "Constructable1").and_then(|c| find(&s.host, c, "building_cost")).unwrap();
    // Full price: Paris's timber camp (`building_cost_mod_all` −10 %) maps to 19 chains through
    // `effect_bonus_value_building_chain_junctions`, and no prestige chain is one of them.
    assert_eq!(text(&s.host, cost), "15000");
    // Recruiting keeps the recruitment tab open, and the queued card cancels.
    let tab = find(&s.host, s.root, "recruitment_tab").unwrap();
    click(&s.host, tab);
    let card = find(&s.host, s.root, "Inf_Line_French_Fusiliers!recruitable!3").expect("fusiliers card");
    click(&s.host, card);
    assert!(errors(&s.host).is_empty());
    assert_eq!(apply_requests(&mut s, sel), 1);
    let queued = find(&s.host, s.root, "Inf_Line_French_Fusiliers!enqueued!0").expect("queued card on the same tab");
    click(&s.host, queued);
    assert_eq!(apply_requests(&mut s, sel), 1);
    assert_eq!(gold(&s), treasury);
    assert!(errors(&s.host).is_empty());
}

#[test]
fn building_browser_entry_shows_the_slot_tree() {
    let Some(s) = setup() else { return };
    let browser = find(&s.host, s.root, "build_browser").expect("button");
    click(&s.host, browser);
    let entry = find(&s.host, s.root, "building_browser_entry3").expect("third entry (Paris, Ordnance Factory)");
    click(&s.host, entry);
    assert!(errors(&s.host).is_empty());
    let state = |id: &str| find(&s.host, s.root, id).and_then(|n| s.host.world().get(n).map(|x| x.state_name().to_owned()));
    // The standing level and its ancestors are "normal"; its upgrade can be built now.
    assert_eq!(state("sCannon1_cannon_foundry").as_deref(), Some("normal"));
    assert_eq!(state("sCannon2_ordnance_factory").as_deref(), Some("normal"));
    assert_eq!(state("sCannon3_great_arsenal").as_deref(), Some("available"));
    // Other nations' prestige buildings never appear.
    assert!(find(&s.host, s.root, "sPrest_austria_heldenplatz").is_none());
    assert!(find(&s.host, s.root, "vertical node link").is_some());
}

/// The settlement panel's demolish button (0-E round N+2): `CanDemolishBuilding` (`0x009E0930` ->
/// `0x009B7920`) answers from the model's `can_demolish` (ported by 0-G) and `DemolishBuilding`
/// (`0x009E2380` -> queue id `0x84`) queues the command; applying it removes the building.
#[test]
fn demolish_button_removes_a_standing_building() {
    let Some(mut s) = setup() else { return };
    let (paris, region, slot_key) = {
        let st = s.scripts.state();
        let m = &st.model;
        let r = m.world.regions.values().find(|r| r.key == "eur_france").unwrap();
        let slot = r.slots.iter().find(|x| x.building.is_some()).expect("a built slot in Paris");
        (r.id, r.id, slot.key.clone())
    };
    let sel = CampaignSelection::Settlement(paris);
    s.host.campaign_select(sel);
    assert!(errors(&s.host).is_empty());

    let lua = s.host.lua();
    assert!(
        lua.load(format!("return CampaignUI.CanDemolishBuilding('{slot_key}')")).eval::<bool>().unwrap(),
        "the model says the slot can be demolished"
    );
    // An empty slot cannot: the panel's guard is the model's can_demolish, not just "is selected".
    let empty = {
        let st = s.scripts.state();
        let r = &st.model.world.regions[&region];
        r.slots.iter().find(|x| x.building.is_none()).map(|x| x.key.clone()).expect("an empty slot")
    };
    assert!(!lua.load(format!("return CampaignUI.CanDemolishBuilding('{empty}')")).eval::<bool>().unwrap());

    lua.load(format!("CampaignUI.DemolishBuilding('whatever_level', '{slot_key}')")).exec().unwrap();
    assert!(errors(&s.host).is_empty());
    let reqs = s.host.take_campaign_requests();
    assert!(
        reqs.iter().any(|r| matches!(r, CampaignRequest::Command(CampaignCommand::DemolishBuilding { region: rr, .. }) if *rr == region)),
        "requests: {reqs:?}"
    );
    assert_eq!(apply_all(&mut s, reqs), 1, "the command applies");
    s.host.campaign_select(sel);
    assert!(s.scripts.state().model.world.regions[&region].slots.iter().all(|x| x.key != slot_key || x.building.is_none()));
    assert!(errors(&s.host).is_empty());
}

/// Applies the requests the HUD queued, as the game does, and re-sends the selection.
fn apply_all(s: &mut Setup, reqs: Vec<CampaignRequest>) -> usize {
    let mut n = 0;
    for r in reqs {
        if let CampaignRequest::Command(cmd) = r {
            s.scripts.apply(cmd).expect("command applies");
            n += 1;
        }
    }
    n
}

/// Campaign bug 2 ("walls can't be built"), matched to the original: the settlement walls are the
/// **last card of the settlement's construction panel**, built from that card like any slot
/// building (CONFIRMED, 2026-10-07: `0x00A01F50` appends the settlement's fortification slot after
/// its slot list; a breakpoint on it hit when London was selected in the original, and the user saw
/// "Small Star Fort" as the last card of London's construction panel). Shipped layout and scripts:
/// Paris' panel has one card per `settlement:` slot and then the walls; hovering the walls card shows
/// `sFortifications1_settlement_fortifications` as its one constructable, and clicking it queues the
/// walls in `FORTIFICATION_SLOT` and pays for them. The infrastructure tab is the road slot's
/// panel (`0x00A021B0`) and opens without script errors.
/// Run on the install: `cargo test -p ntw_script --test campaign_ui walls -- --nocapture`.
#[test]
fn walls_are_the_last_construction_card_and_build_from_it() {
    let Some(mut s) = setup() else { return };
    let (paris, france, n) = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let france = m.faction_by_key("france").unwrap().id;
        let paris = m.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
        // Money and every technology, so the first wall level can be started.
        m.world.factions.get_mut(&france).unwrap().treasury = 200_000;
        for (_, state) in m.world.faction_details.get_mut(&france).unwrap().technologies.iter_mut() {
            *state = ntw_sim::campaign::effects::TECH_RESEARCHED;
        }
        let r = &m.world.regions[&paris];
        assert!(r.fortification.is_none(), "no walls at the start (no BUILDING under any FORTIFICATION_SLOT)");
        let opts = m.construction_options(paris, ntw_sim::campaign::SlotRef::Walls);
        assert_eq!(opts.iter().map(|o| o.level_key.as_str()).collect::<Vec<_>>(), vec!["sFortifications1_settlement_fortifications"]);
        assert!(opts[0].affordable && opts[0].tech, "{opts:?}");
        (paris, france, r.slots.iter().filter(|x| x.key.starts_with("settlement:")).count())
    };
    let sel = CampaignSelection::Settlement(paris);
    s.host.campaign_select(sel);
    assert!(errors(&s.host).is_empty());
    // The walls are the card after the settlement's own slots, and nothing comes after them.
    let walls = n + 1;
    let card = find(&s.host, s.root, &format!("Slot{walls}")).or_else(|| find(&s.host, s.root, &format!("Building{walls}")));
    let card = card.unwrap_or_else(|| panic!("the walls card, number {walls} after {n} settlement slots"));
    for id in [format!("Slot{}", walls + 1), format!("Building{}", walls + 1)] {
        assert!(find(&s.host, s.root, &id).is_none(), "{id}: nothing after the walls");
    }
    s.host.pointer(card, PointerEvent::Enter);
    // The panel numbers its constructable cards across all slots (`g_constructables_id`), so the
    // walls' card is the one constructable shown while the walls slot is hovered.
    let mut shown = Vec::new();
    s.host.world().visit_visible(s.root, &mut |n, node| {
        if node.data.id.starts_with("Constructable") {
            shown.push(n);
        }
    });
    assert_eq!(shown.len(), 1, "one wall level on offer");
    let constructable = shown[0];
    let gold = s.scripts.state().model.faction_by_key("france").unwrap().treasury;
    click(&s.host, constructable);
    assert!(errors(&s.host).is_empty());
    let reqs = s.host.take_campaign_requests();
    assert!(
        reqs.iter().any(|r| matches!(r, CampaignRequest::Command(CampaignCommand::ConstructBuilding { region, slot: ntw_sim::campaign::SlotRef::Walls, level_key })
            if *region == paris && level_key == "sFortifications1_settlement_fortifications")),
        "requests: {reqs:?}"
    );
    assert_eq!(apply_all(&mut s, reqs), 1, "the walls are queued");
    s.host.campaign_select(sel);
    let cost = s.scripts.state().model.construction_options(paris, ntw_sim::campaign::SlotRef::Walls).first().map(|o| o.cost);
    assert!(cost.is_none(), "no second wall level while building");
    assert!(s.scripts.state().model.faction_by_key("france").unwrap().treasury < gold, "the walls are paid for");
    assert!(s.scripts.state().model.world.regions[&paris].construction.iter().any(|c| c.slot == ntw_sim::campaign::SlotRef::Walls));
    // The walls card now shows its construction, still the last card.
    let card = find(&s.host, s.root, &format!("Building{walls}")).or_else(|| find(&s.host, s.root, &format!("Slot{walls}"))).expect("the walls card");
    assert!(find(&s.host, card, "constructing_animation").is_some(), "the queued walls are shown as under construction");
    assert!(errors(&s.host).is_empty());
    // The infrastructure tab is the road's construction panel: the road slot alone, no walls.
    let tab = find(&s.host, s.root, "infrastructure_tab").expect("every settlement has its road tab");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty(), "the road panel opens");
    assert!(find(&s.host, s.root, "Slot2").is_none() && find(&s.host, s.root, "Building2").is_none(), "the road slot alone");
    // A walls slot with nothing to show (no walls, nothing building, the one level restricted by
    // the scripts; another French region) is dropped, as the exe's `0x00B7A0E0` does.
    let other = {
        let st = s.scripts.state();
        let m = &st.model;
        let r = m.world.regions.values().find(|r| r.owner == france && r.id != paris && r.fortification.is_none()).expect("another French region");
        (r.id, r.slots.iter().filter(|x| x.key.starts_with("settlement:")).count())
    };
    s.scripts.state_mut().model.world.restricted_buildings.insert("sFortifications1_settlement_fortifications".into());
    s.host.campaign_select(CampaignSelection::Settlement(other.0));
    assert!(errors(&s.host).is_empty(), "the panel opens without script errors");
    let walls = other.1 + 1;
    assert!(find(&s.host, s.root, &format!("Slot{walls}")).is_none(), "no walls card with nothing to show");
    // A road slot with nothing to show: the tab stays (the exe tests the slot's presence), and its
    // panel opens on an empty `slots` table without script errors.
    {
        let mut st = s.scripts.state_mut();
        let roads: Vec<String> = st
            .model
            .rules
            .buildings
            .iter()
            .filter(|(_, b)| st.model.rules.chain_slots.get(&b.chain).is_some_and(|t| t.iter().any(|t| t == "settlement_road")))
            .map(|(k, _)| k.clone())
            .collect();
        st.model.world.regions.get_mut(&other.0).unwrap().road = None;
        st.model.world.restricted_buildings.extend(roads);
    }
    s.host.campaign_select(CampaignSelection::Settlement(other.0));
    let tab = find(&s.host, s.root, "infrastructure_tab").expect("the road tab with nothing to show");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty(), "the empty road panel opens without script errors");
}

/// A map fort as a selection of its own (`CampaignSelection::Fort`): the original scripts get a fort
/// address in `SetSelectedEntity`, the review panel shows the fort's own panel (its
/// `construction_tab`, CONFIRMED key id 0x52), the original fort panel (`GenerateFortConstructionPanel`) opens without script
/// errors on the `fFort` chain, its actions build nothing (`UpgradeFort` / `BuildFort` are switched
/// off in the exe, the rest are PLACEHOLDER no-ops), and the fort tooltip runs on every
/// `FortDetails` table without the nil-description error (`TechTreeItem_Tooltip.lua:75`).
#[test]
fn fort_selection_opens_the_map_fort_panel() {
    let Some(s) = setup() else { return };
    let paris = s.scripts.state().model.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    s.host.campaign_select(CampaignSelection::Fort(paris));
    assert!(errors(&s.host).is_empty(), "the fort selection is shown");
    assert!(find(&s.host, s.root, "infrastructure_tab").is_none(), "a fort has no road tab");
    // The fort's panel is its construction tab (key id 0x52, `0x00989BB0`), whose generator is the fort one.
    let tab = find(&s.host, s.root, "construction_tab").expect("the fort's own panel tab");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty(), "the fort panel opens");
    let details: mlua::Table = s.host.lua().load("return CampaignUI.FortDetails(1)").eval().unwrap();
    assert_eq!(details.get::<String>("Key").unwrap(), "fFort1_wooden_artillery_fort", "the map fort's chain, not the walls");
    s.host.lua().load("CampaignUI.UpgradeFort(1); CampaignUI.DemolishFort(1); CampaignUI.RepairFort(1); CampaignUI.BuildFort(1)").exec().unwrap();
    assert!(errors(&s.host).is_empty());
    assert!(s.host.take_campaign_requests().is_empty(), "no fort action builds anything");
    for key in ["", "fFort2_western_artillery_fort", "sFortifications1_settlement_fortifications", "test_no_such_level"] {
        s.host
            .lua()
            .load(format!(
                "local env = __ntw_root_env\n\
                 local a = env.Component.CreateComponentFromTemplate('TechTreeItem_Tooltip', 'test_fort_tip', env.Address, 0, 0)\n\
                 assert(a, 'the tooltip template was created')\n\
                 UIComponent(a):LuaCall('InitialiseBuilding', CampaignUI.FortDetails(1, '{key}'), CampaignUI.FortEffects(1, '{key}'), false, false, nil, true)\n\
                 UIComponent(a):Destroy()"
            ))
            .exec()
            .unwrap_or_else(|e| panic!("tooltip on FortDetails(1, '{key}'): {e}"));
        let errs: Vec<String> = errors(&s.host).into_iter().filter(|e| !e.contains("WARN")).collect();
        assert!(errs.is_empty(), "tooltip on FortDetails(1, '{key}'): {errs:?}");
    }
}

/// The agents tab (`ui/agents.luac`'s `GenerateAgentsPanel(info)`, reached through the root's
/// forwarder) opening on the original scripts with zero script errors. The info's shape and the
/// panel's engine calls are CONFIRMED from the panel's bytecode (CHARACTER_UI_HOOKS.md "Agents
/// panel"). The fixture puts a French agent in Paris (the agent kept, a new rake when France has
/// none), so `agents_tab` is listed; opening it creates the cards and selects the player's first,
/// which asks the agent button questions; clicking the card selects it again.
/// Run on the install: `cargo test -p ntw_script --test campaign_ui agents_panel -- --nocapture`.
#[test]
fn agents_panel_opens_without_script_errors() {
    let Some(s) = setup() else { return };
    let (paris, agent) = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let france = m.faction_by_key("france").unwrap().id;
        let paris = m.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
        let at = m.world.regions[&paris].settlement.position;
        let is_agent = |k: CharacterKind| matches!(k, CharacterKind::Rake | CharacterKind::Gentleman);
        let agent = match m.world.characters.values().find(|c| c.faction == france && is_agent(c.kind)).map(|c| c.id) {
            Some(a) => a,
            None => {
                let mut c = m.world.characters.values().find(|c| c.faction == france).cloned().expect("a French character");
                c.id = ntw_sim::campaign::CharacterId(m.world.characters.keys().map(|k| k.0).max().unwrap_or(0) + 1);
                c.kind = CharacterKind::Rake;
                m.world.characters.insert(c.id, c.clone());
                c.id
            }
        };
        let c = m.world.characters.get_mut(&agent).unwrap();
        c.garrisoned_in = Some(paris);
        c.position = at;
        (paris, agent)
    };
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    assert!(errors(&s.host).is_empty(), "the settlement is shown");
    let tab = find(&s.host, s.root, "agents_tab").expect("the agents tab is listed");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty(), "the agents panel opens");
    let card = find(&s.host, s.root, &format!("agent_{}", agent.0)).expect("the agent's card");
    click(&s.host, card);
    assert!(errors(&s.host).is_empty(), "the agent's card selects");
    assert!(s.host.take_campaign_requests().iter().all(|r| !matches!(r, CampaignRequest::Command(_))), "no model command");
}

/// A general with no portrait in the model (a recruitment-pool hire: the pool gives him empty
/// details) keeps his unit card in the army bar and in the Lists (PLACEHOLDER until generated
/// characters get portraits): no portrait-less character card, no script error. Review 2026-10-07.
#[test]
fn a_general_without_a_portrait_keeps_his_unit_card() {
    let Some(s) = setup() else { return };
    let general = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let f = m.faction_by_key("france").unwrap().id;
        let g = m
            .world
            .forces
            .values()
            .filter(|x| x.faction == f && !x.is_navy && !x.units.is_empty())
            .find_map(|x| x.commander.filter(|c| m.world.characters.get(c).is_some_and(|ch| ch.kind == CharacterKind::General)))
            .unwrap();
        m.world.character_details.entry(g).or_default().portrait = Default::default();
        g
    };
    s.host.campaign_select(CampaignSelection::Character(general));
    assert!(errors(&s.host).is_empty());
    let env = s.host.script_env(s.root).unwrap();
    let (display_as_unit, portrait): (bool, String) = s
        .host
        .lua()
        .load("local u = CampaignUI.ReviewPanelInfo().units_info.Units return u[1].DisplayAsUnit, u[1].Portrait")
        .set_environment(env.clone())
        .eval()
        .unwrap();
    assert!(display_as_unit && portrait.is_empty());
    let row: Option<String> = s
        .host
        .lua()
        .load(
            "local me = CampaignUI.ReviewPanelInfo().commander\n\
             for _, r in ipairs(CampaignUI.RetrieveFactionMilitaryForceLists('france', true)) do\n\
             if r.Address == me then\n\
             return tostring(r.ShowAsCharacter) .. ' ' .. tostring(r.CommandedUnit ~= nil) end end",
        )
        .set_environment(env)
        .eval()
        .unwrap();
    let row = row.expect("the general has a row in the Lists");
    assert_eq!(row, "false true", "the Lists row shows his unit card");
    click(&s.host, find(&s.host, s.root, "button_lists").unwrap());
    assert!(errors(&s.host).is_empty(), "the Lists open without a script error");
}

/// The naval recruitment tab (0-E round N+2): no naval generator exists (CONFIRMED), so
/// `naval_recruitment_tab` reuses `GenerateRecruitmentPanel` with the naval selector, and the panel
/// lists the region's ships and its naval capacity only. The tab is a character panel's
/// (0-G's trace of `FUN_0099A200`).
#[test]
fn naval_recruitment_tab_lists_only_ships() {
    let Some(s) = setup() else { return };
    let (admiral, port) = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let france = m.faction_by_key("france").unwrap().id;
        let admiral = m
            .world
            .forces
            .values()
            .filter(|f| f.is_navy && f.faction == france)
            .filter_map(|f| f.commander)
            .find(|c| m.world.characters.get(c).is_some_and(|x| x.kind == CharacterKind::Admiral))
            .expect("a French admiral with a navy");
        // The fixture the panel needs: a port with a building, whose `naval_recruitment_points`
        // (0x00B61EE0, CONFIRMED) give the navy its capacity.
        let port = m
            .world
            .regions
            .values()
            .find(|r| r.owner == france && r.slots.iter().any(|s| s.port && s.building.is_some()))
            .expect("a French port with a building")
            .id;
        m.world.characters.get_mut(&admiral).unwrap().garrisoned_in = Some(port);
        (admiral, port)
    };
    let capacity = s.scripts.state().model.recruitment_points(port, true);
    assert!(capacity > 0, "the port has naval recruitment points");

    s.host.campaign_select(CampaignSelection::Character(admiral));
    assert!(errors(&s.host).is_empty());
    let tab = find(&s.host, s.root, "naval_recruitment_tab").expect("naval recruitment tab");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty());

    let mut cards = Vec::new();
    s.host.world().visit_visible(s.root, &mut |_, node| {
        if node.data.id.split('!').nth(1) == Some("recruitable") {
            cards.push(node.data.id.clone());
        }
    });
    assert!(!cards.is_empty(), "the naval tab lists the port's ships");
    let st = s.scripts.state();
    // A ship has no `unit_stats_land` row (its stats are in `unit_stats_naval`), which is the model
    // rules' own test (`UnitRules::is_naval` / `autoresolve`).
    for id in &cards {
        let u = &st.model.rules.units[id.split('!').next().unwrap()];
        assert!(u.is_naval || u.autoresolve.is_none(), "{id} is not a ship ({})", u.category);
    }
    drop(st);

    // The selector really splits: the same region's land recruitment tab has land units only.
    s.host.campaign_select(CampaignSelection::Settlement(port));
    assert!(errors(&s.host).is_empty());
    let tab = find(&s.host, s.root, "recruitment_tab").expect("recruitment tab");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty());
    let mut land = Vec::new();
    s.host.world().visit_visible(s.root, &mut |_, node| {
        if node.data.id.split('!').nth(1) == Some("recruitable") {
            land.push(node.data.id.clone());
        }
    });
    let st = s.scripts.state();
    assert!(!land.is_empty(), "the land tab lists the port's units");
    // The land tab is not just the naval one: it also offers the region's land units and the trade
    // vessels (a `units` row with no `unit_stats_land` row is a ship, but the exe lists the trade
    // ones on both tabs -- CONFIRMED by the fact that the selector is the manager's `+0xA4` bool,
    // INFERRED: what it filters).
    let naval_keys: Vec<String> = cards.iter().map(|id| id.split('!').next().unwrap().to_owned()).collect();
    assert!(!land.is_empty());
    for id in &land {
        let key = id.split('!').next().unwrap();
        let u = &st.model.rules.units[key];
        assert!(!naval_keys.contains(&key.to_owned()) || u.is_naval || u.autoresolve.is_none(), "{id} is duplicated on both tabs");
    }
    assert!(land.iter().any(|id| !naval_keys.contains(&id.split('!').next().unwrap().to_owned())), "the land tab has units the naval tab does not");
}

/// The agent-type name the agents tab's card tooltip shows
/// (`agents[i].agent_type_name`, CONFIRMED: `ui\templates\template.unitcard_tooltip.luac`, the
/// `InitialiseAgent` proto at line 115, reads `name` and `agent_type_name` and nothing else).
///
/// The loc key is `agent_culture_details_onscreen_name_<agent><culture>`, the agent type's `agents`
/// key with the culture's key straight after it. **CONFIRMED against the shipped data:** the
/// install has exactly 53 of these keys and `agent_culture_details` exactly 53 rows, and every
/// row's `<agent><culture>` pair is a key. The culture is a character's (his faction's, as
/// The **hire** (`enlist_commander`), the panel the original opens through `panel_manager`.
///
/// `panel_manager` is not an engine object we have to write: `ui\panelmanager.luac` is a shipped
/// Lua module that `ui\campaign ui\layout.root.luac` itself requires (`Utilities.Require("PanelManager")`
/// at pc 29-31, then `SetRootAndEnvironment(Address, CampaignUI)` at pc 65-68), so it runs in our
/// host like any other module -- CONFIRMED by the click reaching `PanelManager.lua:312` in
/// `panelmanager.OpenPanel` and the panel appearing under the root.
///
/// What the module could not finish was our own row data: `enlist_commander_entry.lua:54` hands
/// each trait to `ui\templates\character_trait_entry.luac`'s `SetTraitTooltip`, which calls
/// `Utilities.GetEffectList(row.Effects, row.AttributeEffects)`, and that takes `#attribute_effects`
/// first (`utilities.lua:182`, pc 2). With no `AttributeEffects` the whole `InitEnlistCommander`
/// aborted inside the first trait's tooltip, so the rows were built but the script never returned.
/// Fix: the commander rows carry both lists.
///
/// Run on the install: `cargo test -p ntw_script --test campaign_ui enlist_commander -- --nocapture`.
#[test]
fn enlist_commander_panel_opens_and_lists_a_candidate() {
    let Some(s) = setup() else { return };
    let general = {
        let st = s.scripts.state();
        let m = &st.model;
        let f = m.faction_by_key("france").unwrap().id;
        m.world
            .forces
            .values()
            .filter(|x| x.faction == f && !x.is_navy)
            .find_map(|x| x.commander)
            .unwrap()
    };
    s.host.campaign_select(CampaignSelection::Character(general));
    assert!(errors(&s.host).is_empty());
    let promote = find(&s.host, s.root, "army_promote").expect("the army panel's promote button");
    click(&s.host, promote);
    assert!(errors(&s.host).is_empty(), "the enlist-commander panel opens without a script error");
    assert!(find(&s.host, s.root, "enlist_commander").is_some(), "the panel is in the tree");
    // The pool list got its rows: `enlist_commander.lua:78` (FillAvailableCommanders) makes one
    // `commander_<Name>` component per candidate, so at least one must exist and be visible.
    let mut rows = Vec::new();
    s.host.world().visit_visible(s.root, &mut |_n, node| {
        if node.data.id.starts_with("commander_") {
            rows.push(node.data.id.clone());
        }
    });
    assert!(!rows.is_empty(), "at least one commander row: {rows:?}");
}

/// A card's own left click reaches its card group (`ui\templates\cards.luac`'s `OnLeftClickUp`,
/// the proto at line 26). That function does nothing unless `Component.Call("IsDragged")` answers
/// exactly `false`; the binding was missing, `false == nil` is false, and **every unit and agent
/// card in the campaign HUD (and in the battle HUD) silently ignored its click**. The proof is the
/// agents panel: `SelectAgentCard` -- reached only through `cards.lua:26` -- is what calls
/// `ShowAgentButtons`, and that is what makes the rake's action buttons appear.
///
/// Run on the install: `cargo test -p ntw_script --test campaign_ui a_card_left_click -- --nocapture`.
#[test]
fn a_card_left_click_shows_the_agents_action_buttons() {
    let Some(s) = setup() else { return };
    let (paris, rake) = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let france = m.faction_by_key("france").unwrap().id;
        let paris = m.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
        let at = m.world.regions[&paris].settlement.position;
        let rake = match m.world.characters.values().find(|c| c.faction == france && c.kind == CharacterKind::Rake).map(|c| c.id) {
            Some(a) => a,
            None => {
                let mut c = m.world.characters.values().find(|c| c.faction == france).cloned().expect("a French character");
                c.id = ntw_sim::campaign::CharacterId(m.world.characters.keys().map(|k| k.0).max().unwrap_or(0) + 1);
                c.kind = CharacterKind::Rake;
                m.world.characters.insert(c.id, c.clone());
                c.id
            }
        };
        let c = m.world.characters.get_mut(&rake).unwrap();
        c.garrisoned_in = Some(paris);
        c.position = at;
        (paris, rake)
    };
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    assert!(errors(&s.host).is_empty());
    click(&s.host, find(&s.host, s.root, "agents_tab").expect("the agents tab"));
    assert!(errors(&s.host).is_empty());
    let card = find(&s.host, s.root, &format!("agent_{}", rake.0)).expect("the rake's card");
    // Before the click the action buttons are hidden: `ShowButton` (agents.lua:40) is what makes
    // one visible, and only `SelectAgentCard` calls it.
    assert!(
        find(&s.host, s.root, "rogue_assassinate").is_none(),
        "the assassinate button is hidden until the card is selected"
    );
    click(&s.host, card);
    assert!(errors(&s.host).is_empty(), "the card's click reaches the manager");
    assert!(
        find(&s.host, s.root, "rogue_assassinate").is_some(),
        "ShowAgentButtons made the rake's assassinate button visible"
    );
}

/// Where the `enlist_commander` panel lands, and **why**: it is not placed by our host at all.
///
/// CONFIRMED chain, all read off the install's own bytecode:
/// - `layout.root.lua:790` pc 181-185 is `huds.RegisterHud(g_hud, true)` -- **two** arguments, the
///   `g_hud` component and the boolean `true`, not a width and a height.
/// - `huds` is the shipped Lua module `ui\huds.luac` (`Utilities.Require("Huds")`, `layout.root.lua:0`
///   K[7]), whose `RegisterHud` (proto at line 15) does `h_height, h_width = UIComponent(hud):Bounds()`
///   when that flag is `true` and `Dimensions()` when it is not, clamps `h_width` to 1280, and then
///   `s_height, s_width = UIComponent(hud:Parent("root")):Dimensions()`. `UIComponent:Bounds` is the
///   box around the component and its direct children (`0x010133C0`, CONFIRMED), 1280 x 241 here.
/// - `panelmanager.lua` pc 185-196 places every panel with `huds.MoveRelativeToHUD(panel,
///   horizontal, vertical, offset)`, and `enlist_commander`'s `Side` is `huds.g_centre`
///   (`panelmanager.lua` pc 188-190). The centre branch of `Huds.MoveRelativeToHUD` (proto at line 47)
///   is `x = TruncToInt((s_width - w)/2)` and `y = TruncToInt((s_height - h_height - h)/2)`.
/// - `CoreUtils.TruncToInt(v)` is `v - (v % 1)` (`coreutils.lua:9`), which floors a negative half.
///
/// So the panel's position is the original's own arithmetic on numbers our host supplies, and this
/// test pins every one of them. The scripts work in the HUD layout's 1280 x 960 frame whatever the
/// screen (debugger sitting in the original, 2026-10-07), where `veneer_DY` -- the HUD band
/// `g_hud = this:Find("veneer_DY")` -- is 1280 x 241, so the panel (624 x 720) lands at
/// `(1280-624)/2 = 328` and `TruncToInt((960-241-720)/2) = TruncToInt(-0.5) = -1`: **-1, not a
/// "near the top of the screen"**, and the one-pixel overhang is the original's, not ours. On a
/// bigger screen the panel (docked centre) is then moved by half the growth: the panel is opened on
/// a 1920 x 1080 screen, so the test cannot pass by the screen matching the layout, and checked
/// again after the screen changes to 1280 x 960 (one install load for both).
///
/// Run on the install: `cargo test -p ntw_script --test campaign_ui the_panel_lands_where_the_huds_formula_puts_it -- --nocapture`.
#[test]
fn the_panel_lands_where_the_huds_formula_puts_it() {
    let Some(s) = setup() else { return };
    s.host.set_screen(1920.0, 1080.0);
    let general = {
        let st = s.scripts.state();
        let m = &st.model;
        let f = m.faction_by_key("france").unwrap().id;
        m.world
            .forces
            .values()
            .filter(|x| x.faction == f && !x.is_navy)
            .find_map(|x| x.commander)
            .unwrap()
    };
    s.host.campaign_select(CampaignSelection::Character(general));
    assert!(errors(&s.host).is_empty());
    // `g_hud`: the HUD band `layout.root.lua:790` hands to `RegisterHud`. Its size is what becomes
    // `h_width` / `h_height`, so it is the first half of the formula.
    let hud = find(&s.host, s.root, "veneer_DY").expect("the HUD band");
    let hr = s.host.world().get(hud).unwrap().rect;
    // `s_width` / `s_height` come from `UIComponent(hud:Parent("root")):Dimensions()`: the root's
    // layout size in the scripts' frame (1280 x 960), whatever the screen.
    let (s_width, s_height) = (1280.0f32, 960.0f32);
    assert_eq!((hr.w, hr.h), (1280.0, 241.0), "the HUD band's own layout size");

    click(&s.host, find(&s.host, s.root, "army_promote").expect("the army panel's promote button"));
    assert!(errors(&s.host).is_empty(), "the enlist-commander panel opens without a script error");
    let panel = find(&s.host, s.root, "enlist_commander").expect("the panel is in the tree");
    for (sw, sh) in [(1920.0f32, 1080.0f32), (1280.0, 960.0)] {
        s.host.set_screen(sw, sh);
        let r = s.host.world().get(panel).unwrap().rect;
        // `CoreUtils.TruncToInt(v)` is `v - (v % 1)`, and Lua 5.1's `%` is `a - floor(a/b)*b`, so
        // with b = 1 that collapses to `floor(v)` -- which is why a negative half lands on -1, not 0.
        assert_eq!(r.w, 624.0, "the panel's layout width");
        // In the scripts' frame: centred on s_width, centred in the space above the HUD band.
        let (x, y) = (((s_width - r.w) / 2.0).floor(), ((s_height - hr.h - r.h) / 2.0).floor());
        assert_eq!((x, y), (328.0, -1.0));
        // On screen: moved by the centre dock's share of the growth.
        assert_eq!((r.x, r.y), (x + (sw - 1280.0) / 2.0, y + (sh - 960.0) / 2.0), "on a {sw} x {sh} screen");
    }
}

/// On a screen bigger than the HUD's 1280x960 layout, the scripts still work in that frame and
/// docking maps it to the screen (the original read under the debugger, 2026-10-07, at
/// 1920x1080: the root stays 1280x960 for the scripts, the Lists panel, docked right-centre, is
/// at (648, -1) for them and on screen at (1288, 59); o2 shows x 1290-1910). Regression: the
/// Lists sat at x = 648 on a wide screen, i.e. centre-right instead of at the right edge.
#[test]
fn the_lists_panel_docks_to_the_right_edge_of_a_wide_screen() {
    let Some(s) = setup() else { return };
    s.host.set_screen(1920.0, 1080.0);
    click(&s.host, find(&s.host, s.root, "button_lists").unwrap());
    no_errors(&s.host);
    let panel = find(&s.host, s.root, "entity_lists").expect("the Lists panel");
    let r = s.host.world().get(panel).unwrap().rect;
    assert_eq!((r.x, r.y, r.w), (1288.0, 59.0, 624.0), "on screen");
    // The HUD band stays centred at the bottom.
    let hud = s.host.world().get(find(&s.host, s.root, "veneer_DY").unwrap()).unwrap().rect;
    assert_eq!((hud.x, hud.y + hud.h), (319.0, 1080.0 - 1.0), "{hud:?}");
}

/// An agents-panel action button now reaches its target picker.
///
/// The chain is all shipped Lua and all CONFIRMED: `ui\agents.luac` calls
/// `CampaignUI.AgentRakeAssassinate(agent)` (arity 1) on the button; the original's exe then did
/// what `agent_options.Assassinate` does (`agent_options.lua:118`) -- ask
/// `CampaignUI.RequestAssassinationTargets(src, target)`, and if the list is not empty hand it to the
/// HUD root's `OpenAgentActionPopup(action, src, rows)` (`layout.root.lua:1187`, arity 3), which opens
/// the `agent_action` panel through `panel_manager`. Before this round the call was a no-op, so the
/// button was visible and did nothing.
///
/// Which agent has a valid target is the model's business, so the test looks for one rather than
/// assuming: it asks the three `Request*Targets` lists for every listed French agent and drives the
/// first non-empty pair.
///
/// Run on the install: `cargo test -p ntw_script --test campaign_ui an_agent_action_button -- --nocapture`.
#[test]
fn an_agent_action_button_opens_the_target_picker() {
    let Some(s) = setup() else { return };
    // (action, the panel button that asks for it)
    const ACTIONS: [(&str, &str); 3] =
        [("assassinate", "rogue_assassinate"), ("sabotage", "rogue_sabotage"), ("duel", "gentleman_duel")];
    // A fresh start position has no assassination target anywhere, so the test stages one the way
    // the card test above stages its rake: a French Rake in Paris and a foreign Gentleman in the
    // same settlement, the target given a rank so the model's `assassination_chance` gate is Some
    // (it returns None when the skill and the target's rank both floor to 0).
    let _ = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let france = m.faction_by_key("france").unwrap().id;
        let paris = m.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
        let at = m.world.regions[&paris].settlement.position;
        let next = m.world.characters.keys().map(|k| k.0).max().unwrap_or(0) + 1;
        let mut rake = match m.world.characters.values().find(|c| c.faction == france && c.kind == CharacterKind::Rake).cloned() {
            Some(c) => c,
            None => {
                let mut c = m.world.characters.values().find(|c| c.faction == france).cloned().expect("a French character");
                c.id = ntw_sim::campaign::CharacterId(next);
                c.kind = CharacterKind::Rake;
                c
            }
        };
        rake.garrisoned_in = Some(paris);
        rake.position = at;
        m.world.characters.insert(rake.id, rake.clone());
        let mut victim = m.world.characters.values().find(|c| c.faction != france).cloned().expect("a foreign character");
        victim.id = ntw_sim::campaign::CharacterId(next + 1);
        victim.kind = CharacterKind::Gentleman;
        victim.garrisoned_in = Some(paris);
        victim.position = at;
        m.world.characters.insert(victim.id, victim.clone());
        let d = m.world.character_details.entry(victim.id).or_default();
        d.attributes.push(("gentleman".into(), 5));
        d.abilities.push(("can_duel".into(), 1, String::new()));
        (rake.id, paris)
    };
    // Which agent has a valid target is the model's business, so ask the model -- the same gates our
    // `Request*Targets` bindings use -- rather than assuming a fixture.
    let found = {
        use ntw_sim::campaign::agents::{assassination_chance, building_sabotage_chance, duel_chance, knows_character, Weapon};
        let st = s.scripts.state();
        let m = &st.model;
        let f = m.faction_by_key("france").unwrap().id;
        let listed = |k: CharacterKind| {
            !matches!(
                k,
                CharacterKind::General | CharacterKind::Colonel | CharacterKind::Admiral | CharacterKind::Captain | CharacterKind::Minister
            )
        };
        let has = |a: ntw_sim::campaign::CharacterId, action: &str| -> bool {
            match action {
                "assassinate" => m
                    .world
                    .characters
                    .values()
                    .any(|t| t.faction != f && knows_character(m, f, t.id) && assassination_chance(m, a, t.id).is_some()),
                "duel" => m
                    .world
                    .characters
                    .values()
                    .any(|t| t.faction != f && t.garrisoned_in.is_some() && knows_character(m, f, t.id) && duel_chance(m, a, t.id, Weapon::Pistols).is_some()),
                _ => {
                    let Some(region) = m.world.characters.get(&a).and_then(|c| c.garrisoned_in) else { return false };
                    let Some(r) = m.world.regions.get(&region) else { return false };
                    r.slots.iter().enumerate().any(|(i, _)| building_sabotage_chance(m, a, region, i).is_some())
                }
            }
        };
        m.world
            .characters
            .values()
            .filter(|c| c.faction == f && c.garrisoned_in.is_some() && listed(c.kind))
            .find_map(|c| ACTIONS.iter().find(|a| has(c.id, a.0)).map(|a| (c.id, c.garrisoned_in.unwrap(), *a)))
    };
    let Some((agent, region, (action, button))) = found else {
        eprintln!("skipped: no French agent in this start position has a valid target");
        return;
    };
    s.host.campaign_select(CampaignSelection::Settlement(region));
    assert!(errors(&s.host).is_empty());
    click(&s.host, find(&s.host, s.root, "agents_tab").expect("the agents tab"));
    assert!(errors(&s.host).is_empty());
    click(&s.host, find(&s.host, s.root, &format!("agent_{}", agent.0)).expect("the agent's card"));
    assert!(errors(&s.host).is_empty());
    // `ShowAgentButtons` only shows the button whose gate the model accepts, which is the same gate
    // that put this action in the list above -- so it must be there now.
    let b = find(&s.host, s.root, button).unwrap_or_else(|| panic!("the {action} button is visible"));
    click(&s.host, b);
    assert!(errors(&s.host).is_empty(), "the {action} button opens its picker without a script error");
    // `layout.root.lua:1187` opened the `agent_action` panel, and `agent_action.Initialise(action,
    // src, rows)` filled its list box with one row per target.
    assert!(
        find(&s.host, s.root, "agent_action").is_some(),
        "the {action} target picker opened"
    );
    let mut rows = 0;
    s.host.world().visit_visible(s.root, &mut |_n, node| {
        // `agent_action.lua:21` names each row's component `"target" .. <index>`.
        if node.data.id.starts_with("target") {
            rows += 1;
        }
    });
    assert!(rows > 0, "the picker lists at least one target row");
    // Its X closes it (template.button_close.lua's `OnLeaveDepress`, see `panels_close_from_their_close_buttons`).
    let picker = find(&s.host, s.root, "agent_action").unwrap();
    click(&s.host, find(&s.host, picker, "button_close").expect("the picker's X"));
    no_errors(&s.host); // the picker closes without a script error
    assert!(find(&s.host, s.root, "agent_action").is_none(), "the X closed the {action} picker");
}

/// The technology tree's links only join technologies: every `vertical_link` / `horizontal_link`
/// `template.tech_entry.lua` draws lies inside the tree pane (`pane_tabs`). Bug 2026-10-07 (user
/// side by side): ParentX/Yoffset came from two table columns, and a 640 px link crossed the
/// panel's title. They are the exe's `0x00F21A30` offsets now (`technology_parent_offsets`).
#[test]
fn technology_links_stay_inside_the_tree() {
    let Some(s) = setup() else { return };
    click(&s.host, find(&s.host, s.root, "button_tech").unwrap());
    no_errors(&s.host);
    let pane = find(&s.host, s.root, "pane_tabs").expect("the tree pane");
    let p = s.host.world().get(pane).unwrap().rect;
    let mut links = Vec::new();
    s.host.world().visit_visible(pane, &mut |_n, node| {
        if node.data.id.ends_with("_link") {
            links.push((node.data.id.clone(), node.rect));
        }
    });
    assert!(links.iter().any(|(id, r)| id == "vertical_link" && r.h > 0.0), "the tree draws some links: {links:?}");
    for (id, r) in &links {
        assert!(r.x >= p.x && r.y >= p.y && r.x + r.w <= p.x + p.w && r.y + r.h <= p.y + p.h, "{id} {r:?} outside the pane {p:?}");
    }
}

/// `CharacterCultureType` 0x0089C240 reads), so the same agent type is named per culture -- a rake
/// is a "Spy" in `european` and a "Scout" in `tribal`.
///
/// Run on the install: `cargo test -p ntw_script --test campaign_ui agent_culture -- --nocapture`.
#[test]
fn every_agent_culture_row_has_its_onscreen_name_key() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let db = ntw_data::GameDatabase::from_vfs(&vfs).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let rows = db.campaign.agent_cultures.iter().collect::<Vec<_>>();
    assert_eq!(rows.len(), 53, "agent_culture_details rows on the install");
    let mut missing = Vec::new();
    for r in &rows {
        let key = format!("agent_culture_details_onscreen_name_{}{}", r.agent, r.culture);
        if loc.get(&key).is_none() {
            missing.push(key);
        }
    }
    assert!(missing.is_empty(), "{} of 53 rows have no loc name: {missing:?}", missing.len());
    // The keys are the rows' pairs and nothing else: one key per row, no spare.
    let keys = loc
        .iter()
        .filter(|(k, _)| k.starts_with("agent_culture_details_onscreen_name_"))
        .count();
    assert_eq!(keys, rows.len(), "one name key per agent-culture pair, no more");
    // The names really do differ per culture, so picking the right one matters.
    assert_eq!(loc.get("agent_culture_details_onscreen_name_rakeeuropean"), Some("Spy"));
    assert_eq!(loc.get("agent_culture_details_onscreen_name_raketribal"), Some("Scout"));
    assert_eq!(loc.get("agent_culture_details_onscreen_name_assassinmiddle_east"), Some("Hashishin"));
    assert_eq!(loc.get("agent_culture_details_onscreen_name_assassinindian"), Some("Thugee"));
}

/// Wraps the global `func` of component `id`'s scripts so its calls are counted (see `calls`).
/// Returns false if the component's scripts do not define it.
fn count_calls(host: &UiScriptHost, id: NodeId, func: &str) -> bool {
    let env = host.script_env(id).expect("the component has scripts");
    let wrap = "local env, name = ...\n\
        local f = rawget(env, name)\n\
        if type(f) ~= 'function' then return false end\n\
        rawset(env, '__calls_' .. name, 0)\n\
        rawset(env, name, function(...) rawset(env, '__calls_' .. name, rawget(env, '__calls_' .. name) + 1) return f(...) end)\n\
        return true";
    host.lua().load(wrap).call::<bool>((env, func)).unwrap()
}

/// How often the global `func` wrapped by [`count_calls`] ran.
fn calls(host: &UiScriptHost, id: NodeId, func: &str) -> i64 {
    let env = host.script_env(id).expect("the component has scripts");
    env.raw_get::<Option<i64>>(format!("__calls_{func}")).unwrap().unwrap_or(0)
}

/// Every HUD panel closes from its own close button and opens again (campaign bug 1, 2026-10-06).
/// Most panels' `button_close` is a `template.button_close.lua` instance that binds no event: the
/// layout gives its "depress" state the exit function `OnLeaveDepress`, which the engine runs when
/// the release moves the button out of that state (`0x01035620`), and that calls root's
/// `ClosePopup` with the `ParentPopup` property the panel set. Diplomacy binds the same function to
/// `OnMouseLClickUp` instead and its "depress" state has no exit function (no layout anywhere binds
/// an event to a function that is also one of its states' enter / exit functions: `ui_probe --
/// statefns`), so each close runs `OnLeaveDepress` and `ClosePopup` exactly once; government has
/// its own script.
/// Run on the install: `cargo test -p ntw_script --test campaign_ui panels_close -- --nocapture`.
#[test]
fn panels_close_from_their_close_buttons() {
    let Some(s) = setup() else { return };
    no_errors(&s.host);
    assert!(count_calls(&s.host, s.root, "ClosePopup"), "the root script defines ClosePopup");
    for (button, panel, close) in [
        ("button_tech", "technology", "button_close"),
        ("build_browser", "building_browser", "button_close"),
        ("button_lists", "entity_lists", "button_close"),
        ("button_missions", "missions", "button_close"),
        ("button_diplomacy", "diplomatic_relations", "button_close"),
        ("button_government", "government_screens", "government_screen_button_close"),
        ("button_tech", "technology", "button_close"),
    ] {
        click(&s.host, find(&s.host, s.root, button).unwrap());
        no_errors(&s.host); // the panel opens
        let p = find(&s.host, s.root, panel).unwrap_or_else(|| panic!("{panel} is shown"));
        let x = find(&s.host, p, close).unwrap_or_else(|| panic!("{panel}'s {close}"));
        let leave = count_calls(&s.host, x, "OnLeaveDepress");
        // InitState on the close button counts the state functions that run after the close.
        let env = s.host.script_env(x).unwrap();
        if env.raw_get::<Option<mlua::Function>>("InitState").unwrap().is_none() {
            env.raw_set("InitState", s.host.lua().create_function(|_, _: mlua::Value| Ok(())).unwrap()).unwrap();
        }
        assert!(count_calls(&s.host, x, "InitState"));
        let before = calls(&s.host, s.root, "ClosePopup");
        s.host.pointer(x, PointerEvent::Enter);
        s.host.pointer(x, PointerEvent::LeftDown);
        let inits = calls(&s.host, x, "InitState");
        s.host.pointer(x, PointerEvent::LeftUp);
        if leave {
            // Destroy only queues the panel (`0x01037230`) and its scripts stay, so the close
            // button still gets the new state's InitState after its exit function closed the
            // panel, as in the original (`0x01035620` checks the script environment).
            assert!(find(&s.host, s.root, panel).is_none(), "{panel} is detached at once");
            assert_eq!(calls(&s.host, x, "InitState") - inits, 1, "{panel}: the close button's new state is initialised");
        }
        s.host.pointer(x, PointerEvent::Leave);
        if leave {
            assert_eq!(calls(&s.host, x, "OnLeaveDepress"), 1, "{panel}: OnLeaveDepress runs once per click");
        }
        let closes = calls(&s.host, s.root, "ClosePopup") - before;
        // One UI frame: the destroy PanelManager asked for happens at its end.
        s.host.pulse(0.0);
        no_errors(&s.host); // the panel closes without a script error
        assert!(find(&s.host, s.root, panel).is_none(), "{close} closed {panel}");
        if panel == "government_screens" {
            assert!(closes <= 1, "{panel}: ClosePopup ran {closes} times");
        } else {
            assert!(leave, "{panel}'s {close} is a template.button_close.lua instance");
            assert_eq!(closes, 1, "{panel}: ClosePopup runs once per click");
        }
    }
}

/// Hovering a building slot drops its upgrade card down on the original's schedule, timed by the
/// UI clock alone: template.BuildingFrame.lua's SetUpgradeTargetState takes its start time from
/// `CampaignUI.Time()` and its OnUpdate measures `OnUpdatePulse`'s argument (ms) against it. The
/// leading card slides down from under the slot over `g_time_down` = 0.15 s (the others appear at
/// 0.8 x 0.15 = 0.12 s and slide sideways over `g_time_out` = 0.25 s). Bug 2026-10-07: `Time()`
/// read a wall clock started with the HUD, ahead of the pulse clock by the load time, so the card
/// stayed under the slot for seconds. All times here are UI-clock ms from the pulses sent. Also
/// bounds the hover's own cost in counts that do not depend on the machine (Lua instructions,
/// components created; the wall-clock bound it had was dropped as machine-dependent).
#[test]
fn slot_upgrade_drops_down_on_the_ui_clock() {
    // The slot hover's cost bounds: about twice what it measured on 2026-10-08 (9k Lua
    // instructions, 36 components), so a hover that rebuilds the panel or loops fails.
    const HOVER_KILO_OPS: u32 = 20;
    const HOVER_COMPONENTS: usize = 72;
    let Some(s) = setup() else { return };
    let paris = s.scripts.state().model.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    // The UI clock starts at 0 (the HUD's first frame); the wall clock has run through the setup.
    s.host.pulse(0.0);
    let slot = find(&s.host, s.root, "Building3").expect("third slot");
    // The hover's own cost, counted where the count is the same on every machine (a wall-clock
    // bound is not): the Lua instructions it runs, in thousands, and the components it creates.
    let kilo_ops = Rc::new(std::cell::Cell::new(0u32));
    let counter = kilo_ops.clone();
    s.host
        .lua()
        .set_hook(mlua::HookTriggers::new().every_nth_instruction(1000), move |_, _| {
            counter.set(counter.get() + 1);
            Ok(mlua::VmState::Continue)
        })
        .unwrap();
    let nodes = |s: &Setup| s.host.world().ids().count();
    let before = nodes(&s);
    s.host.pointer(slot, PointerEvent::Enter);
    s.host.lua().remove_hook();
    let created = nodes(&s).saturating_sub(before);
    println!("hover: {}k Lua instructions, {created} components created", kilo_ops.get());
    assert!(kilo_ops.get() <= HOVER_KILO_OPS && created <= HOVER_COMPONENTS, "the slot hover costs more than it did: {}k instructions, {created} components", kilo_ops.get());
    assert!(errors(&s.host).is_empty());
    let card = find(&s.host, s.root, "Building3_Upgrade1").expect("the leading card shows at once");
    let y = |s: &Setup| s.host.world().get(card).unwrap().offset.1;
    let (start, slot_y) = (y(&s), s.host.world().get(slot).unwrap().offset.1);
    assert_eq!(start, slot_y, "at 0 ms it is under the slot");
    s.host.pulse(75.0);
    let half = y(&s);
    s.host.pulse(149.0);
    let almost = y(&s);
    s.host.pulse(151.0);
    let end = y(&s);
    assert!(start < half && half < almost && almost < end, "slides over 0.15 s: {start} {half} {almost} {end}");
    assert!((half - (start + end) / 2.0).abs() < 1.0, "linearly: half way at 75 ms");
    s.host.pulse(500.0);
    assert_eq!(y(&s), end, "and stays there");
    assert!(errors(&s.host).is_empty());
}

/// Opens the negotiation screen the way the diplomatic relations screen's "Open Negotiations"
/// button does: its `OpenNegotiations` (scroll.lua:230) calls the root's
/// `ToggleDiplomacyPopup(<faction>)`, which opens `diplomacy_panel` through the PanelManager and
/// runs the panel's `Initialise(<faction>)`.
fn open_negotiations(s: &Setup, faction: &str) -> NodeId {
    let toggle: mlua::Function = s.host.script_env(s.root).unwrap().raw_get("ToggleDiplomacyPopup").unwrap();
    toggle.call::<()>(faction).unwrap();
    find(&s.host, s.root, "diplomacy_panel").expect("the negotiation screen opens")
}

/// Opens a negotiation with Britain, lets the campaign's "negotiation started" event arrive, runs
/// `act` (Lua) on the open negotiation, closes the panel with `close`, and checks the negotiation
/// ended the one way the original ends it: the root's `EnableDiplomacy` runs exactly once (hub
/// +0x378 is posted once by `CCQ_DIPLOMACY_END_NEGOTIATION`, however many times `End()` is called:
/// the X calls it, and the PanelManager's `ExitFunc` "OnExit" calls it again), and the screen
/// opens again.
fn negotiation_ends_once(act: &str, close: impl Fn(&Setup, NodeId)) {
    let Some(s) = setup() else { return };
    no_errors(&s.host);
    assert!(count_calls(&s.host, s.root, "EnableDiplomacy"), "the root script defines EnableDiplomacy");
    let panel = open_negotiations(&s, "britain");
    no_errors(&s.host);
    assert!(count_calls(&s.host, panel, "InitialiseNegotiation"));
    // Before the campaign's event the X is hidden (`SetCloseable(false)` hides `gilt_corner_TR`),
    // as in the original; the next UI frame delivers the event.
    assert!(find(&s.host, panel, "button_close").is_none(), "no X before the negotiation started");
    s.host.pulse(0.0);
    no_errors(&s.host);
    assert_eq!(calls(&s.host, panel, "InitialiseNegotiation"), 1, "the negotiation-started event reaches the panel once");
    let finished = |s: &Setup| s.host.lua().load("return CampaignUI.Finished()").eval::<Option<bool>>().unwrap();
    assert_eq!(finished(&s), Some(false), "no result yet");
    s.host.lua().load(act).exec().unwrap();
    s.host.pulse(8.0);
    no_errors(&s.host);
    assert!(find(&s.host, s.root, "diplomacy_panel").is_some(), "{act}: the panel stays open");
    close(&s, panel);
    no_errors(&s.host);
    assert!(find(&s.host, s.root, "diplomacy_panel").is_none(), "{act}: the negotiation screen closes");
    s.host.pulse(16.0);
    no_errors(&s.host);
    assert_eq!(calls(&s.host, s.root, "EnableDiplomacy"), 1, "{act}: ending the negotiation re-enables diplomacy");
    s.host.pulse(24.0);
    assert_eq!(calls(&s.host, s.root, "EnableDiplomacy"), 1, "{act}: once");
    assert_eq!(finished(&s), None, "{act}: the ended object answers nothing");
    open_negotiations(&s, "austria");
    no_errors(&s.host);
}

fn click_x(s: &Setup, panel: NodeId) {
    click(&s.host, find(&s.host, panel, "button_close").expect("the X is shown"));
}

/// The negotiation screen's X closes it, and the screen opens again afterwards (bug 2026-10-07:
/// the X did nothing). The X runs `ClosedByCloseButton` on the panel (the address the panel put in
/// its `ParentPopup` property), which does nothing until the panel is closeable: `Initialise`
/// ends with `SetCloseable(false)` (which also hides the X's frame), and only the campaign's
/// "negotiation started" event makes it closeable, through
/// `InitialiseNegotiation(greeting, true, false)` (0x00A147A0). The event follows the panel's own
/// constructor call `UIDiplomacyNegotiation(player, opposing)`, which used to fail (the host's
/// class was a table), so the event never came. Closing ends the negotiation
/// (`negotiation:End()`), and the campaign's "negotiation ended" event calls the root's
/// `EnableDiplomacy`, without which `ToggleDiplomacyPopup` refuses to open the panel again.
/// Run on the install: `cargo test -p ntw_script --test campaign_ui negotiation -- --nocapture`.
#[test]
fn negotiation_screen_closes_from_its_close_button_and_opens_again() {
    negotiation_ends_once("", click_x);
}

/// A proposal does not end the negotiation (review round 1: applying the deal used to, so the X's
/// `End()` posted nothing and diplomacy stayed locked).
#[test]
fn the_x_after_a_proposal_ends_the_negotiation_once() {
    negotiation_ends_once("CampaignUI.Propose()", click_x);
}

/// A declined offer gives the negotiation a result (`Finished()`, +0x28 = 2, 0x00C1F210) but does
/// not end it; the X then ends it (ClosedByCloseButton: finished, so `End()`).
#[test]
fn the_x_after_a_decline_ends_the_negotiation_once() {
    negotiation_ends_once("CampaignUI.DeclineOffer() assert(CampaignUI.Finished() == true, 'declined')", click_x);
}

/// "Cancel" with nothing to propose (CancelOffer, diplomacy_panel.lua:638: `End()` then
/// `ClosePopup()`) ends the negotiation once too, though `OnExit` calls `End()` again.
#[test]
fn cancel_offer_ends_the_negotiation_once() {
    negotiation_ends_once("", |s, panel| {
        let cancel: mlua::Function = s.host.script_env(panel).unwrap().raw_get("CancelOffer").unwrap();
        cancel.call::<()>(()).unwrap();
    });
}

/// The one-argument constructor (`RequestDiplomacy`'s pending move) starts a new object: nothing
/// of the previous negotiation is kept (+0xAC unset until an event sets it, so every accessor
/// answers nothing) and, the model having no pending moves (PROVISIONAL), no event follows; that
/// gap is logged once.
#[test]
fn the_pending_move_constructor_keeps_nothing_of_the_last_negotiation() {
    let Some(s) = setup() else { return };
    let panel = open_negotiations(&s, "britain");
    s.host.pulse(0.0);
    no_errors(&s.host);
    let proposer: Option<String> = s.host.lua().load("return CampaignUI.ProposerId()").eval().unwrap();
    assert_eq!(proposer.as_deref(), Some("france"));
    let env = s.host.script_env(panel).unwrap();
    let probe = "local n = UIDiplomacyNegotiation({}) \
        return n:ProposerId(), n:Finished(), n:BuildPossibleActions(), select('#', n:BuildOfferAndDemandStrings())";
    let (proposer, finished, actions, strings): (Option<String>, Option<bool>, Option<mlua::Table>, i64) =
        s.host.lua().load(probe).set_environment(env.clone()).eval().unwrap();
    assert_eq!((proposer, finished, actions.is_none(), strings), (None, None, true, 0));
    let log = s.host.take_log();
    assert_eq!(log.iter().filter(|l| l.contains("pending move")).count(), 1, "the gap is logged: {log:?}");
    s.host.lua().load("UIDiplomacyNegotiation({})").set_environment(env).exec().unwrap();
    assert!(s.host.take_log().iter().all(|l| !l.contains("pending move")), "logged once");
    assert!(count_calls(&s.host, panel, "InitialiseNegotiation"));
    s.host.pulse(8.0);
    no_errors(&s.host);
    assert_eq!(calls(&s.host, panel, "InitialiseNegotiation"), 0, "no event for the pending move");
}

/// The negotiation's events go to the script context the engine is running when the constructor
/// is called (0x01058750 reads the running thread's context), not to the component whose script
/// defined the function that calls it (review round 2: a stack walk picked the defining one). Here
/// a helper defined in the root's script runs inside the panel's named call, so the panel gets
/// "started". With no component script running at all, the gap is logged once.
#[test]
fn a_negotiation_belongs_to_the_running_script_context() {
    let Some(s) = setup() else { return };
    let panel = open_negotiations(&s, "britain");
    s.host.pulse(0.0);
    no_errors(&s.host);
    s.host.lua().load("CampaignUI.End()").exec().unwrap();
    let root_env = s.host.script_env(s.root).unwrap();
    s.host
        .lua()
        .load("function NtwTestMakeNegotiation() return UIDiplomacyNegotiation('france', 'britain') end")
        .set_environment(root_env.clone())
        .exec()
        .unwrap();
    let panel_env = s.host.script_env(panel).unwrap();
    let helper: mlua::Function = root_env.raw_get("NtwTestMakeNegotiation").unwrap();
    panel_env.raw_set("NtwTestCall", helper).unwrap();
    assert!(count_calls(&s.host, panel, "InitialiseNegotiation"));
    let call: mlua::Function = s.host.lua().globals().get("__ntw_call_if_defined").unwrap();
    let address: mlua::Value = panel_env.raw_get("Address").unwrap();
    assert!(call.call::<bool>((address, "NtwTestCall")).unwrap());
    s.host.pulse(8.0);
    no_errors(&s.host);
    assert_eq!(calls(&s.host, panel, "InitialiseNegotiation"), 1, "the running panel's context gets the event");
    // No component script running: logged once, no event.
    for _ in 0..2 {
        s.host.lua().load("UIDiplomacyNegotiation('france', 'britain')").exec().unwrap();
    }
    let log = s.host.take_log();
    assert_eq!(log.iter().filter(|l| l.contains("no component script running")).count(), 1, "{log:?}");
}

/// The negotiation screen lists the current treaties (bug 2026-10-07: it showed "Test"). The panel
/// creates one `string` template under `treaties` with `CreateComponentFromTemplate("string",
/// "treaty_string", treaties, 0, 0, {}, {treaties_text})`: a number key in the texts table is the
/// created component itself (0x010171A0 → 0x01032010), so its state gets the text in place of the
/// template's own "Test".
#[test]
fn negotiation_screen_lists_the_current_treaties() {
    let Some(s) = setup() else { return };
    let expected: String = s.host.lua().load("return CampaignUI.RetrieveExistingTreaties('france', 'britain')").eval().unwrap();
    assert!(!expected.is_empty(), "France and Britain are at war in 1805");
    let panel = open_negotiations(&s, "britain");
    no_errors(&s.host);
    let treaties = find(&s.host, panel, "treaties").unwrap();
    let line = find(&s.host, treaties, "treaty_string").expect("the treaty text component");
    assert_eq!(text(&s.host, line), expected);
}

/// FactionDetails' rankings are strings, as the exe pushes them (`GetFactionRankingStrings`
/// 0x008C7170 -> random_localisation_strings `power_category_<n>` ...), never numbers: the panel
/// puts them straight into its `power_dy` / `wealth_dy` texts (bug 2026-10-07: "Power: 0"). The
/// host's value is PROVISIONAL: the empty string 0x008C7170 starts from, until the ranking values
/// exist in the model. The test pins that exact value, so filling the rankings in updates it.
#[test]
fn faction_rankings_are_strings() {
    let Some(s) = setup() else { return };
    let rankings: (String, String, String) = s
        .host
        .lua()
        .load("local d = CampaignUI.FactionDetails('britain') return d.PowerRanking, d.WealthRanking, d.PrestigeRanking")
        .eval()
        .unwrap();
    assert_eq!(rankings, (String::new(), String::new(), String::new()), "PROVISIONAL: empty rankings");
    let panel = open_negotiations(&s, "britain");
    for id in ["power_dy", "wealth_dy"] {
        let shown = text(&s.host, find(&s.host, panel, id).unwrap());
        assert_eq!(shown, "", "{id} shows the PROVISIONAL empty ranking");
    }
}
