//! Runs the ORIGINAL campaign HUD (`ui\campaign ui\layout` and its scripts) from the player's install
//! (read-only) against the eur_napoleon start position, with our `CampaignUI` functions. Skipped when
//! the install is not there. Log: `cargo test -p ntw_script --test campaign_ui -- --nocapture`.

#[path = "../../ntw_campaign/tests/common/mod.rs"]
mod common;

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
    /// The player's faction key.
    human: String,
}

fn setup() -> Option<Setup> {
    setup_in("eur_napoleon", "france")
}

/// [`setup`] in `campaign` with `human` as the player's faction. Britain plays the Coalition
/// campaign, `mp_eur_napoleon` (the front end starts it; `coalition_campaign_start_requests_the_chosen_faction`).
fn setup_in(campaign: &str, human: &str) -> Option<Setup> {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return None;
    }
    let bytes = std::fs::read(dir.join(format!("campaigns/{campaign}/startpos.esf"))).unwrap();
    Some(setup_loaded(&dir, &open_install(&dir), |db| ntw_campaign::read(&bytes, db).unwrap(), Some((campaign, human))))
}

/// The install's packs and game database, parsed once for [`setup_loaded`] to reuse.
fn open_install(dir: &std::path::Path) -> (ntw_formats::pack::Vfs, Rc<ntw_data::GameDatabase>) {
    let vfs = ntw_formats::pack::Vfs::open_install(dir).unwrap();
    let db = Rc::new(ntw_data::GameDatabase::from_vfs(&vfs).unwrap());
    (vfs, db)
}

/// The HUD over the campaign `load` reads (a start position or a save), as `(campaign, human)`, or
/// for a save without it, as its own campaign and human player.
fn setup_loaded(dir: &std::path::Path, (vfs, db): &(ntw_formats::pack::Vfs, Rc<ntw_data::GameDatabase>), load: impl FnOnce(&ntw_data::GameDatabase) -> ntw_campaign::LoadedCampaign, played: Option<(&str, &str)>) -> Setup {
    let mut loaded = load(db);
    let (campaign, human) = match played {
        Some((c, h)) => (c.to_owned(), h.to_owned()),
        None => (loaded.info.campaign_key.clone(), loaded.info.players.iter().find(|p| p.is_human).expect("a human player").faction_key.clone()),
    };
    loaded.set_human(&human);
    if played.is_some() {
        // A start position gets the handicaps a new campaign adds (`0x008DD090`), as the game's turn-1 start does; a save keeps its own.
        ntw_sim::campaign::effects::apply_start_handicaps(&mut loaded.model);
    }
    let scripts = ScriptHost::new(loaded.model, &human, ScriptSource::from_install(dir).unwrap()).unwrap();
    let loc = Localisation::from_vfs(vfs).unwrap();
    let host = UiScriptHost::new(ScriptSource::from_install(dir).unwrap(), loc, FrontEndFacts::default(), (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    host.install_campaign(CampaignLink { state: scripts.shared_state(), human: human.clone(), campaign, map: loaded.info.map_key.clone(), theatres: loaded.info.theatres().map(str::to_owned).collect(), db: Rc::clone(db) })
        .unwrap();
    let root = host.load_root_layout("data/ui/campaign ui/layout").unwrap();
    host.campaign_ready();
    host.campaign_update_funds("Early May");
    Setup { scripts, host, root, human }
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

/// The first visible node whose id starts with `prefix` (a card id's index suffix depends on the list order).
fn find_prefix(host: &UiScriptHost, root: NodeId, prefix: &str) -> Option<NodeId> {
    let mut out = None;
    host.world().visit_visible(root, &mut |n, node| {
        if out.is_none() && node.data.id.starts_with(prefix) {
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
    // The selection bar names the general alone (`0x009C2AF0`), not "<name>, <faction>".
    let selector = find(&s.host, s.root, "selector").unwrap();
    assert!(!text(&s.host, selector).contains(", "));

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
    let card = find_prefix(&s.host, s.root, "Inf_Line_French_Fusiliers!recruitable!").expect("fusiliers card");
    click(&s.host, card);
    let _ = errors(&s.host);
    let reqs = s.host.take_campaign_requests();
    assert!(
        reqs.iter().any(|r| matches!(r, CampaignRequest::Command(CampaignCommand::Recruit { region, unit_key, target: None }) if *region == paris && unit_key == "Inf_Line_French_Fusiliers")),
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
    s.host.pointer(end_turn, PointerEvent::Enter);
    assert!(errors(&s.host).is_empty());
    let tip = find(&s.host, s.root, "Tooltip").expect("tooltip shown");
    assert!(!text(&s.host, tip).is_empty());
    s.host.pointer(end_turn, PointerEvent::Leave);
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
    let card = find_prefix(&s.host, s.root, "Inf_Line_French_Fusiliers!recruitable!").expect("fusiliers card");
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
/// (`0x009E2380` -> queue id `0x84`) queues the command; applying it queues the slot's demolition (`0x00B286C0`), which the round end carries out.
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
    {
        let st = s.scripts.state();
        let r = &st.model.world.regions[&region];
        let index = r.slots.iter().position(|x| x.key == slot_key).unwrap();
        assert!(r.slots[index].building.is_some(), "queued, not done");
        assert!(r.construction.iter().any(|c| c.slot == ntw_sim::campaign::SlotRef::Slot(index) && c.kind == ntw_sim::campaign::ConstructionKind::Demolish));
    }
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

/// An agents-tab card shows the agent type's main-attribute picture: `GenerateAgentsPanel` calls
/// `CreateCharacterCard(..., ShowAttributes = true)` (`Agents.lua:234`), whose card adds the image
/// `{CharacterAttribute:1}` + `Attributes.PrimaryAttributePath` (`Utilities.lua:122-123`), CONFIRMED in
/// ui/agents.luac and ui/utilities.luac. A rake's main attribute is `subterfuge` (`agents` #8), whose
/// `agent_attributes` picture is the loose file `pips/skill_spying.tga` (in-game check (d), 2026-10-10).
#[test]
fn agents_tab_cards_show_the_main_attribute_picture() {
    let Some(s) = setup() else { return };
    let (paris, rake) = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let france = m.faction_by_key("france").unwrap().id;
        let paris = m.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
        // A copy of a French rake, stationed in Paris.
        let mut c = m.world.characters.values().find(|c| c.faction == france && c.kind == CharacterKind::Rake).cloned().expect("a French rake");
        let details = m.world.character_details.get(&c.id).cloned().unwrap_or_default();
        c.id = ntw_sim::campaign::CharacterId(m.world.characters.keys().map(|k| k.0).max().unwrap_or(0) + 1);
        c.garrisoned_in = Some(paris);
        c.position = m.world.regions[&paris].settlement.position;
        m.world.character_details.insert(c.id, details);
        let id = c.id;
        m.world.characters.insert(id, c);
        (paris, id)
    };
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    let tab = find(&s.host, s.root, "agents_tab").expect("the agents tab is listed");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty(), "{:?}", errors(&s.host));
    let card = find(&s.host, s.root, &format!("agent_{}", rake.0)).expect("the rake's card");
    let mut pictures = Vec::new();
    s.host.world().visit_visible(card, &mut |_, node| pictures.extend(node.data.images.iter().map(|i| i.path.clone())));
    assert!(pictures.iter().any(|p| p.ends_with("pips/skill_spying.tga")), "the card's pictures: {pictures:?}");
}


/// A general whose portrait the model drew (as for a recruitment-pool hire or a field promotion,
/// `ntw_sim::campaign::portraits`) shows it on his army card and his Lists row: no unit-card
/// stand-in, no script error. Review 2026-10-07 (then a PLACEHOLDER fallback), BACKLOG §0
/// "portraits of generated characters".
#[test]
fn a_generated_general_shows_his_drawn_portrait() {
    let Some(s) = setup() else { return };
    let (general, card) = {
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
        // As the pool makes him: no portrait yet, then one drawn from the european General decks.
        m.world.character_details.entry(g).or_default().portrait = Default::default();
        m.assign_portrait(g, "General");
        let card = m.world.character_details[&g].portrait.card.clone();
        assert!(card.starts_with("ui/portraits/european/Cards/general/"), "{card}");
        (g, card)
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
    assert!(!display_as_unit);
    assert_eq!(portrait, format!("data/{card}"));
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
    assert_eq!(row, "true false", "the Lists row shows him as himself");
    click(&s.host, find(&s.host, s.root, "button_lists").unwrap());
    assert!(errors(&s.host).is_empty(), "the Lists open without a script error");
}

/// The army card and the Lists row of a General `g`: his card's `Portrait` and whether the Lists
/// show him as himself.
fn general_card_and_row(s: &Setup, g: ntw_sim::campaign::CharacterId) -> (String, bool) {
    s.host.campaign_select(CampaignSelection::Character(g));
    assert!(errors(&s.host).is_empty());
    let env = s.host.script_env(s.root).unwrap();
    let portrait: String = s
        .host
        .lua()
        .load(
            "for _, u in ipairs(CampaignUI.ReviewPanelInfo().units_info.Units) do\n\
             if not u.DisplayAsUnit then return u.Portrait end end\n\
             return ''",
        )
        .set_environment(env.clone())
        .eval()
        .unwrap();
    let as_himself: bool = s
        .host
        .lua()
        .load(
            "local me = CampaignUI.ReviewPanelInfo().commander\n\
             for _, r in ipairs(CampaignUI.RetrieveFactionMilitaryForceLists('france', true)) do\n\
             if r.Address == me then return r.ShowAsCharacter == true and r.CommandedUnit == nil end end\n\
             return false",
        )
        .set_environment(env)
        .eval()
        .unwrap();
    (portrait, as_himself)
}

/// The real commands, not a simulated draw: a General hired from France's pool into an army
/// (`HireGeneral`, `0x00A1B8F0`) and a colonel promoted in the field (`PromoteUnit`, `0x008E1C20`)
/// each show a european General portrait on their army card and are listed as themselves.
/// Settles in-game check (a) of 2026-10-10 (analysis/fidelity/IN_GAME_CHECKS_2026-10-10.md).
#[test]
fn a_hired_and_a_field_promoted_general_show_european_general_portraits() {
    use ntw_sim::campaign::effects::SavedBonus;
    use ntw_sim::campaign::pool::PoolKind;
    let Some(s) = setup() else { return };
    let (hired, promoted) = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let f = m.faction_by_key("france").unwrap().id;
        let mut armies: Vec<_> = m.world.forces.values().filter(|x| x.faction == f && !x.is_navy && !x.units.is_empty() && x.commander.is_some()).map(|x| x.id).collect();
        armies.sort();
        // Hire: a fresh candidate into the first French army.
        let c = m.create_candidate(f, PoolKind::General).expect("a candidate");
        m.apply(CampaignCommand::HireGeneral { character: c, into: Some(armies[0]) }).expect("the hire applies");
        // Promotion: a second army made colonel-led (no General, no portrait yet), with the
        // faction's `promote_general_in_field` bonus (id 64) set.
        let army = armies[1];
        let officer = m.world.forces[&army].commander.unwrap();
        let unit = m.world.forces[&army].units.iter().position(|u| u.character == Some(officer)).expect("the commander's unit");
        m.world.characters.get_mut(&officer).unwrap().kind = CharacterKind::Colonel;
        m.world.character_details.entry(officer).or_default().portrait = Default::default();
        let d = m.world.faction_details.entry(f).or_default();
        d.bonus_base.push(SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new(), ..Default::default() });
        d.bonus_with_difficulty.clear();
        m.apply(CampaignCommand::PromoteUnit { force: army, unit }).expect("the promotion applies");
        assert_eq!(m.world.characters[&officer].kind, CharacterKind::General);
        (c, officer)
    };
    for g in [hired, promoted] {
        let card = s.scripts.state_mut().model.world.character_details[&g].portrait.card.clone();
        assert!(card.starts_with("ui/portraits/european/Cards/general/"), "{card}");
        let (portrait, as_himself) = general_card_and_row(&s, g);
        assert_eq!(portrait, format!("data/{card}"), "his army card shows his drawn portrait");
        assert!(as_himself, "the Lists row shows him as himself");
    }
    click(&s.host, find(&s.host, s.root, "button_lists").unwrap());
    assert!(errors(&s.host).is_empty(), "the Lists open without a script error");
}

/// A General the model could not give a portrait (here: no portrait allocator, as in an older own
/// save) keeps an empty `Portrait`, so his card falls back to its unit picture instead of loading
/// "data/", and the cause is logged once.
#[test]
fn a_general_without_a_portrait_keeps_his_unit_picture() {
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
        m.world.portraits.clear();
        m.world.character_details.entry(g).or_default().portrait = Default::default();
        m.assign_portrait(g, "General");
        assert!(m.world.character_details[&g].portrait.card.is_empty());
        g
    };
    let _ = s.host.take_log();
    s.host.campaign_select(CampaignSelection::Character(general));
    let env = s.host.script_env(s.root).unwrap();
    let read = "local u = CampaignUI.ReviewPanelInfo().units_info.Units return u[1].Portrait";
    for _ in 0..2 {
        let portrait: String = s.host.lua().load(read).set_environment(env.clone()).eval().unwrap();
        assert_eq!(portrait, "");
    }
    let log = s.host.take_log();
    let warned = log.iter().filter(|l| l.contains(&format!("WARN character {} (General): the campaign has no portrait allocator", general.0))).count();
    assert_eq!(warned, 1, "logged once: {log:?}");
    assert!(!log.iter().any(|l| l.starts_with("ERROR")), "{log:?}");
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

/// The enlist panel's traits show their on-screen names, not their keys (the 2026-10-10 in-game check
/// failed on raw keys too wide for `tx_enables`, so the columns overlapped), and each tooltip carries
/// the level's effects as the exe builds them (0x009AFD60; `traits_table` in `ui/campaign/characters.rs`).
/// `C_General_Brave` at 16 points is level 5 (`character_trait_levels`): "Popular Hero";
/// `trait_level_effects` gives `morale_bonus` 4 (priority 1, shown) and `prestige_military_generic` 1
/// (priority 0, left out); `trait_attribute_effects` gives `command_land` -1. `C_General_Attacker_Bad`
/// at 0 points is below its first level (4), so it is not shown.
///
/// Run on the install: `cargo test -p ntw_script --test campaign_ui enlist_traits -- --nocapture`.
#[test]
fn enlist_traits_show_their_names_and_effects() {
    use ntw_sim::campaign::details::CharacterTrait;
    let Some(s) = setup() else { return };
    let general = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let f = m.faction_by_key("france").unwrap().id;
        let pool = m.world.faction_details[&f].general_pool.0.clone();
        for c in pool {
            m.world.character_details.entry(c).or_default().traits = vec![
                CharacterTrait { key: "C_General_Attacker_Bad".into(), points: 0 },
                CharacterTrait { key: "C_General_Brave".into(), points: 16 },
            ];
        }
        m.world.forces.values().filter(|x| x.faction == f && !x.is_navy).find_map(|x| x.commander).unwrap()
    };
    s.host.campaign_select(CampaignSelection::Character(general));
    assert!(errors(&s.host).is_empty());
    click(&s.host, find(&s.host, s.root, "army_promote").expect("the promote button"));
    assert!(errors(&s.host).is_empty(), "the enlist-commander panel opens without a script error");
    let mut names = Vec::new();
    let mut tooltips = Vec::new();
    s.host.world().visit_visible(s.root, &mut |n, node| {
        if node.data.id == "tx_enables" {
            names.push(n);
        }
    });
    let shown: Vec<String> = names.iter().map(|&n| text(&s.host, n)).collect();
    println!("trait names: {shown:?}");
    assert!(!shown.is_empty(), "the candidates show their traits");
    assert!(shown.iter().all(|t| t == "Popular Hero"), "only the Brave level 5 name, once per candidate: {shown:?}");
    s.host.world().visit_visible(s.root, &mut |n, _| tooltips.push(n));
    let tips: Vec<String> = tooltips.into_iter().filter_map(|n| s.host.tooltip_text(n)).filter(|t| t.contains("morale")).collect();
    println!("trait tooltips: {tips:?}");
    let tip = tips.first().expect("a trait tooltip with the level's effects");
    assert!(tip.contains("-1 to Command in land battles."), "{tip}");
    assert!(tip.contains("+4 to morale in battles"), "{tip}");
    assert!(!tip.contains("Prestige"), "a priority-0 effect is left out: {tip}");
}

/// Review fix: the trait tooltip shows the level's effects as the model applies them
/// (`EffectRules::trait_level_rows`, summed once at load, 0x00F84220), not a second sum of the raw
/// table: when a mod's rules change a level's effects, the panel and the applied effects agree.
///
/// Run on the install: `cargo test -p ntw_script --test campaign_ui trait_tooltip_follows -- --nocapture`.
#[test]
fn trait_tooltip_follows_the_models_level_effects() {
    use ntw_sim::campaign::details::CharacterTrait;
    let Some(s) = setup() else { return };
    let (general, level_key) = {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let level_key = m.rules.effects.trait_level_row("C_General_Brave", 16).expect("Brave level 5").2.clone();
        // A rule change: morale_bonus twice (summed to 7), in place of the level's table rows.
        std::sync::Arc::make_mut(&mut m.rules).effects.insert_trait_level(level_key.clone(), [("morale_bonus", 2.0), ("morale_bonus", 5.0)]);
        let f = m.faction_by_key("france").unwrap().id;
        let pool = m.world.faction_details[&f].general_pool.0.clone();
        for c in pool {
            m.world.character_details.entry(c).or_default().traits = vec![CharacterTrait { key: "C_General_Brave".into(), points: 16 }];
        }
        let general = m.world.forces.values().filter(|x| x.faction == f && !x.is_navy).find_map(|x| x.commander).unwrap();
        (general, level_key)
    };
    {
        let st = s.scripts.state_mut();
        let rules = &st.model.rules.effects;
        assert_eq!(rules.trait_level_rows(&level_key), [("morale_bonus".to_string(), 7.0)], "the model sums the rows once");
        assert!(rules.trait_level_set(&level_key).is_some(), "and applies that set");
    }
    s.host.campaign_select(CampaignSelection::Character(general));
    click(&s.host, find(&s.host, s.root, "army_promote").expect("the promote button"));
    assert!(errors(&s.host).is_empty(), "the enlist-commander panel opens without a script error");
    let mut nodes = Vec::new();
    s.host.world().visit_visible(s.root, &mut |n, _| nodes.push(n));
    let tips: Vec<String> = nodes.into_iter().filter_map(|n| s.host.tooltip_text(n)).filter(|t| t.contains("morale")).collect();
    let tip = tips.first().expect("a trait tooltip with the level's effects");
    assert!(tip.contains("+7 to morale in battles"), "the panel shows the model's summed value: {tip}");
    assert!(!tip.contains("+4 to morale"), "not the raw table's row: {tip}");
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

/// One game frame for the negotiation tests: the HUD's queued model commands applied, as the game
/// applies them after a UI frame, then the next UI frame (which turns the campaign's negotiation
/// events into the panel's calls).
fn frame(s: &Setup, t: f64) {
    for r in s.host.take_campaign_requests() {
        if let CampaignRequest::Command(cmd) = r {
            s.scripts.state_mut().model.apply(cmd).expect("command applies");
        }
    }
    s.host.pulse(t);
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
    frame(&s, 0.0);
    no_errors(&s.host);
    assert_eq!(calls(&s.host, panel, "InitialiseNegotiation"), 1, "the negotiation-started event reaches the panel once");
    let finished = |s: &Setup| s.host.lua().load("return CampaignUI.Finished()").eval::<Option<bool>>().unwrap();
    assert_eq!(finished(&s), Some(false), "no result yet");
    s.host.lua().load(act).exec().unwrap();
    frame(&s, 8.0);
    no_errors(&s.host);
    assert!(find(&s.host, s.root, "diplomacy_panel").is_some(), "{act}: the panel stays open");
    close(&s, panel);
    no_errors(&s.host);
    assert!(find(&s.host, s.root, "diplomacy_panel").is_none(), "{act}: the negotiation screen closes");
    frame(&s, 16.0);
    no_errors(&s.host);
    assert_eq!(calls(&s.host, s.root, "EnableDiplomacy"), 1, "{act}: ending the negotiation re-enables diplomacy");
    frame(&s, 24.0);
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
    negotiation_ends_once("CampaignUI.ProposeDeal()", click_x);
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
    frame(&s, 0.0);
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
    frame(&s, 8.0);
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
    frame(&s, 0.0);
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
    frame(&s, 8.0);
    no_errors(&s.host);
    assert_eq!(calls(&s.host, panel, "InitialiseNegotiation"), 1, "the running panel's context gets the event");
    // No component script running: logged once, no event.
    // The begins run with the next frame; their "started" events find no context: logged once.
    for _ in 0..2 {
        s.host.lua().load("UIDiplomacyNegotiation('france', 'britain')").exec().unwrap();
        frame(&s, 16.0);
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

/// The treaty lines are read from the opposing faction's record about the player (0x009F2B80 →
/// 0x00B750D0) and joined with a blank line (0x00B0B120, "\n\n"): Britain granting Austria access
/// for 3 turns while Austria grants none reads "Has military access to your lands (turns
/// remaining: 3)", not "Grants military access"; Austria's embargo is "Their trade embargo".
#[test]
fn treaty_lines_read_the_opposing_factions_record() {
    let Some(s) = setup_in("mp_eur_napoleon", "britain") else { return };
    {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let (gb, at) = (m.faction_by_key("britain").unwrap().id, m.faction_by_key("austria").unwrap().id);
        let r = &mut m.world.relationships;
        r.get_mut(&(gb, at)).unwrap().military_access_turns = 3;
        r.get_mut(&(at, gb)).unwrap().military_access_turns = 0;
        r.get_mut(&(at, gb)).unwrap().trade_embargo_turns = 4;
    }
    let got: String = s.host.lua().load("return CampaignUI.RetrieveExistingTreaties('britain', 'austria')").eval().unwrap();
    let line = |k: &str, n: &str| loc_text(&format!("diplomacy_strings_string_current_treaty_{k}")).replace("%d", n);
    let want = [line("alliance", ""), line("trade_agreement", ""), line("has_military_access_turns", "3"), line("trade_embargoed", "4")];
    assert_eq!(got, want.join("\n\n"));
}

/// The diplomacy screen against the three original screens (Britain, Early January 1805 →
/// France / Ottoman Empire / Austria → Open Negotiations; UI_FIDELITY.md §4.7): the ranking words
/// (`GetFactionRankingStrings` 0x008C7170, strings put straight into `power_dy` / `wealth_dy`, bug
/// 2026-10-07 "Power: 0"), the option buttons (0x009B5220 over the records of 0x00BF5A60), the
/// greeting by attitude (`receive_<attitude>`, 0x00C55CC0) and its diplomat card
/// (`MinisterPortraitPath` 0x009ED8A0), and the leader's regnal numeral (0x00A0FE80).
/// One test per difference below; they share this setup.
fn britain_negotiates(with: &str) -> Option<(Setup, NodeId)> {
    let s = setup_in("mp_eur_napoleon", "britain")?;
    no_errors(&s.host);
    let panel = open_negotiations(&s, with);
    frame(&s, 0.0);
    no_errors(&s.host);
    Some((s, panel))
}

fn loc_text(key: &str) -> String {
    let dir = data_dir();
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    Localisation::from_vfs(&vfs).unwrap().get(key).unwrap_or_else(|| panic!("no loc {key}")).to_owned()
}

/// (1) Power / Wealth words: the original shows Britain and France "Terrifying" / "Spectacular",
/// the Ottomans "Mighty" / "Rich", Austria "Terrifying" / "Rich" (the screenshots).
#[test]
fn faction_rankings_are_the_originals_words() {
    let Some((s, panel)) = britain_negotiates("france") else { return };
    let rank = |f: &str| -> (String, String) {
        s.host.lua().load(format!("local d = CampaignUI.FactionDetails('{f}') return d.PowerRanking, d.WealthRanking")).eval().unwrap()
    };
    let words = |p: u8, w: u8| {
        (
            loc_text(&format!("random_localisation_strings_string_power_category_{p}")),
            loc_text(&format!("random_localisation_strings_string_wealth_category_{w}")),
        )
    };
    assert_eq!(rank("britain"), words(1, 1), "Britain: Terrifying / Spectacular");
    assert_eq!(rank("france"), words(1, 1), "France: Terrifying / Spectacular");
    assert_eq!(rank("ottomans"), words(2, 2), "Ottomans: Mighty / Rich");
    // Austria's power 6650 is 3rd at the Coalition start (France 9850, Britain 8040; the values
    // themselves: `faction_power_is_the_originals_at_the_coalition_start`).
    assert_eq!(rank("austria"), words(1, 2), "Austria: Terrifying / Rich");
    for id in ["power_dy", "wealth_dy"] {
        assert!(!text(&s.host, find(&s.host, panel, id).unwrap()).is_empty(), "{id} shows a word");
    }
}

/// Public opinion (`SetPublicOpinionStrings`, `RetrieveDiplomaticOpinions` 0x009F2830): the
/// player's column ("towards them") shows Britain's attitude towards the other side, the other
/// column ("towards us") the other side's towards Britain. The original's Ottoman negotiation
/// (2026-10-08 shot, IN_GAME_CHECKS_2026-10-10.md (f)): Britain's column Indifferent, the
/// Ottomans' Friendly. Bug 2026-10-12: the two were swapped.
#[test]
fn public_opinion_shows_each_sides_attitude_in_its_own_column() {
    let Some((s, panel)) = britain_negotiates("ottomans") else { return };
    let opinion = |column: &str| {
        let c = find(&s.host, panel, column).unwrap_or_else(|| panic!("no {column}"));
        text(&s.host, find(&s.host, c, "public_opinion_dy").unwrap_or_else(|| panic!("no opinion in {column}")))
    };
    assert_eq!(opinion("faction_left"), loc_text("diplomacy_strings_string_relationship_neutral"), "Britain towards the Ottomans");
    assert_eq!(opinion("faction_right"), loc_text("diplomacy_strings_string_relationship_friendly"), "the Ottomans towards Britain");
}

/// The relations panel's map, "Other nation's attitudes towards selected nation" (mode
/// `diplomacy_attitude_faction`, reading `FactionAttitudeTowardsOwner` of `RegionsInTheatre`,
/// set at 0x009F0C3D), colours each region by its owner's attitude towards the selected nation; the
/// other mode (`OwnerAttitude`) by the selected nation's towards the owner (0x009F0B2A..0x009F0C44).
/// Bug 2026-10-12 (IN_GAME_CHECKS_2026-10-10.md (f)): the two attitudes were swapped.
#[test]
fn relations_map_shows_the_owners_attitude_towards_the_selected_nation() {
    let Some(s) = setup_in("mp_eur_napoleon", "britain") else { return };
    let rows: Vec<(String, Option<i32>, Option<i32>)> = s
        .host
        .lua()
        .load(
            "local out = {} for _, r in ipairs(CampaignUI.RegionsInTheatre(nil, 'france', nil, 1)) do \
             out[#out + 1] = {r.OwnerKey, r.FactionAttitudeTowardsOwner, r.OwnerAttitude} end return out",
        )
        .eval::<Vec<mlua::Table>>()
        .unwrap()
        .into_iter()
        .map(|t| (t.get(1).unwrap(), t.get(2).unwrap(), t.get(3).unwrap()))
        .collect();
    let st = s.scripts.state();
    let m = &st.model;
    let france = m.faction_by_key("france").unwrap().id;
    let total = |from, to| m.world.relationships.get(&(from, to)).map(|r| r.attitude_total());
    for (owner, towards_france, france_towards) in &rows {
        if owner == "france" {
            continue;
        }
        let o = m.faction_by_key(owner).unwrap().id;
        assert_eq!(*towards_france, total(o, france), "{owner}'s attitude towards France");
        assert_eq!(*france_towards, total(france, o), "France's attitude towards {owner}");
    }
    // The original's map: Prussia deep red, the Ottomans' lands a lighter red.
    let shown = |owner: &str| rows.iter().find(|r| r.0 == owner).and_then(|r| r.1).unwrap_or(0);
    assert!(shown("prussia") < shown("ottomans") && shown("ottomans") < 0, "{rows:?}");
}

/// `RetrieveRemainingMilitaryAccessTurns(a, b)` (0x009F4380) gives the access a gives b first (a's
/// relationship record about b, `+0x78C`), then b's to a: the panel's "offer_access" buttons get
/// the first. Bug 2026-10-12: the two were swapped.
#[test]
fn military_access_turns_come_giver_first() {
    let Some((s, _panel)) = britain_negotiates("austria") else { return };
    {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let (b, a) = (m.faction_by_key("britain").unwrap().id, m.faction_by_key("austria").unwrap().id);
        m.world.relationships.entry((b, a)).or_default().military_access_turns = 7;
        m.world.relationships.entry((a, b)).or_default().military_access_turns = 3;
    }
    let turns: (i32, i32) = s.host.lua().load("return CampaignUI.RetrieveRemainingMilitaryAccessTurns('britain', 'austria')").eval().unwrap();
    assert_eq!(turns, (7, 3), "Britain gives Austria 7 turns, Austria gives Britain 3");
}

/// The panel's two lists as (State, Active) pairs.
type Buttons = Vec<(String, bool)>;

fn listed(s: &Setup) -> (Buttons, Buttons) {
    let t: mlua::Table = s.host.lua().load("return CampaignUI.BuildPossibleActions()").eval().unwrap();
    let list = |k: &str| -> Buttons {
        let l: mlua::Table = t.get(k).unwrap();
        l.sequence_values::<mlua::Table>().map(|e| e.unwrap()).map(|e| (e.get("State").unwrap(), e.get("Active").unwrap())).collect()
    };
    (list("Unilaterals"), list("OffersAndDemands"))
}

fn on(names: &[&str]) -> Buttons {
    names.iter().map(|n| (n.trim_start_matches('~').to_owned(), !n.starts_with('~'))).collect()
}

/// (2) Option buttons, at war (France): Present State Gift; Regions, Technology, Payments,
/// Request peace.
#[test]
fn negotiation_buttons_at_war() {
    let Some((s, _)) = britain_negotiates("france") else { return };
    assert_eq!(listed(&s), (on(&["state_gift"]), on(&["regions", "technology", "payments", "peace"])));
}

/// (2) At peace with a trade agreement (Ottomans): the held treaty's red cancel, the greyed
/// Joining Wars (not allied).
#[test]
fn negotiation_buttons_at_peace_with_trade() {
    let Some((s, _)) = britain_negotiates("ottomans") else { return };
    let (u, o) = listed(&s);
    assert_eq!(u, on(&["trade_cancel", "alliance", "state_gift", "war", "~request_join_war", "break_trade", "break_alliance"]));
    assert_eq!(o, on(&["access", "regions", "technology", "payments"]));
    let tip: String = s
        .host
        .lua()
        .load("for _, e in ipairs(CampaignUI.BuildPossibleActions().Unilaterals) do if e.State == 'request_join_war' then return e.Tooltip end end")
        .eval()
        .unwrap();
    assert_eq!(tip, loc_text("random_localisation_strings_string_join_war_tooltip_not_allied"));
}

/// (2) Allied with trade and access both ways (Austria): three cancels, no alliance / access
/// requests, Joining Wars greyed (no joinable war).
#[test]
fn negotiation_buttons_with_an_ally() {
    let Some((s, _)) = britain_negotiates("austria") else { return };
    let (u, o) = listed(&s);
    assert_eq!(
        u,
        on(&["trade_cancel", "access_cancel", "alliance_cancel", "state_gift", "war", "~request_join_war", "break_trade", "break_alliance"])
    );
    assert_eq!(o, on(&["regions", "technology", "payments"]));
}

/// (2) The panel builds one `diplomacy_button_<State>` per listed option (`CreateButtons`,
/// diplomacy_panel.luac:313). Bug 2026-10-08: none were built, because our `SetState` returned
/// nothing and CreateButtons destroys a button whose icon child reports no such state (the
/// original's SetState returns whether the state exists, `0x01013550` / `0x01035B30`).
#[test]
fn negotiation_buttons_are_built_on_the_panel() {
    let Some((s, panel)) = britain_negotiates("austria") else { return };
    let (u, o) = listed(&s);
    for (name, active) in u.iter().chain(&o) {
        let button = find(&s.host, panel, &format!("diplomacy_button_{name}")).unwrap_or_else(|| panic!("{name} button"));
        let label = find(&s.host, button, "button_tx").expect("the button's text");
        let w = s.host.world();
        assert_eq!(w.get(button).unwrap().state_name(), if *active { "normal" } else { "inactive" }, "{name}");
        // The text colour is the label state's own (the template's `button_tx` has one state per
        // option): the three cancels red, as in the original's Austria screen; the rest dark.
        let st = w.get(label).unwrap().current().unwrap();
        assert_eq!(st.name, *name);
        let [b, g, r, _] = st.font_colour.to_le_bytes();
        let red = r > 0x80 && g < 0x40 && b < 0x40;
        assert_eq!(red, name.ends_with("_cancel"), "{name}: colour {:08x}", st.font_colour);
    }
}

/// The Regions option opens the regions subpopup with one row per tradeable region on each side
/// (Britain offers, Austria is asked). Bugs 2026-10-10: the option click failed (the button's
/// NotifySelected passes `Component.FindChildAddress("button_tx")`, `0x01018020`, which we lacked),
/// then the lists were empty (`TradeableRegions` gave one table, not the two `0x009C5770` returns,
/// and its rows lacked the region info table's `Theatre`, which InitRegionList compares with
/// `HomeTheatre`). Ticking a row's checkbox selects the region for the deal.
#[test]
fn negotiation_regions_list_the_tradeable_regions() {
    let Some((s, panel)) = britain_negotiates("austria") else { return };
    click(&s.host, find(&s.host, panel, "diplomacy_button_regions").expect("the Regions option"));
    no_errors(&s.host);
    let popup = find(&s.host, panel, "regions").expect("the regions subpopup is shown");
    let names = |list: &str, faction: &str| -> (Vec<String>, Vec<String>) {
        let box_ = find(&s.host, find(&s.host, popup, list).unwrap(), "list_box").unwrap();
        let shown = s.host.world().get(box_).unwrap().children.iter().map(|&c| text(&s.host, c)).collect();
        let st = s.scripts.state();
        let m = &st.model;
        let f = m.faction_by_key(faction).unwrap().id;
        let want = m.tradeable_regions(f).map(|r| loc_text(&format!("regions_onscreen_{}", m.world.regions[&r].key))).collect();
        (shown, want)
    };
    let (shown, want) = names("list_offers", "britain");
    assert_eq!(shown, want);
    assert!(!shown.is_empty() && !shown.contains(&loc_text("regions_onscreen_eur_england")), "London is the capital: {shown:?}");
    let (shown, want) = names("list_demands", "austria");
    assert_eq!(shown, want);
    assert!(!shown.is_empty());
    // Tick the first offered region: the panel marks its entry Selected (RegionSelectionChange).
    click(&s.host, find(&s.host, find(&s.host, popup, "list_offers").unwrap(), "checkbox").unwrap());
    no_errors(&s.host);
    let selected: Option<bool> = s.host.script_env(panel).unwrap().raw_get::<mlua::Table>("offerable_regions").unwrap().get::<mlua::Table>(1).unwrap().get("Selected").unwrap();
    assert_eq!(selected, Some(true));
    // OK proposes the selection (`Propose(offers, demands, action)`, 0x009BF3C0): the regions
    // command puts the region in the model's regions record as an offer (UI_FIDELITY.md §4.9), and
    // the next TradeableRegions marks it CurrentlyOffered. Nothing is accepted yet.
    let first = {
        let st = s.scripts.state();
        let m = &st.model;
        m.tradeable_regions(m.faction_by_key("britain").unwrap().id).next().unwrap()
    };
    click(&s.host, find(&s.host, popup, "ok_regions").expect("the regions OK"));
    frame(&s, 0.1);
    no_errors(&s.host);
    {
        let st = s.scripts.state();
        let n = st.model.negotiations.current.as_ref().expect("the negotiation is open");
        assert_eq!((n.regions.offered.clone(), n.regions.demanded.clone()), (vec![first], vec![]));
        assert_eq!(st.model.world.regions[&first].owner, st.model.faction_by_key("britain").unwrap().id, "proposing gives nothing away");
    }
    let (offers, _): (mlua::Table, mlua::Table) = s.host.lua().load("return CampaignUI.TradeableRegions()").eval().unwrap();
    let row: mlua::Table = offers.get(1).unwrap();
    assert!(row.get::<bool>("CurrentlyOffered").unwrap());
    // Proposing the gift (the AI's evaluation accepts it: all gain) hands it over.
    s.host.lua().load("CampaignUI.ProposeDeal()").exec().unwrap();
    frame(&s, 0.2);
    no_errors(&s.host);
    {
        let st = s.scripts.state();
        assert_eq!(st.model.world.regions[&first].owner, st.model.faction_by_key("austria").unwrap().id);
    }
    // Removing the row's action (the item's `DiplomacyItem`, our action name) posts the regions
    // command with the clear flag (0x00C5CF20): the record empties.
    s.host.lua().load("CampaignUI:RemoveAction('regions')").exec().unwrap();
    frame(&s, 0.3);
    no_errors(&s.host);
    assert!(s.scripts.state().model.negotiations.current.as_ref().unwrap().regions.is_empty());
    // Demanding one of Austria's regions for nothing: the AI's evaluation refuses (all cost, no
    // gain): the result is "declined" and nothing changes hands.
    let asked = {
        let st = s.scripts.state();
        let m = &st.model;
        m.tradeable_regions(m.faction_by_key("austria").unwrap().id).next().unwrap()
    };
    s.host
        .lua()
        .load("local _, demands = CampaignUI.TradeableRegions() CampaignUI:Propose({}, { demands[1].Address }, 'regions')")
        .exec()
        .unwrap();
    frame(&s, 0.4);
    s.host.lua().load("CampaignUI.ProposeDeal()").exec().unwrap();
    frame(&s, 0.5);
    let finished: Option<bool> = s.host.lua().load("return CampaignUI.Finished()").eval().unwrap();
    assert_eq!(finished, Some(true), "declined is a result");
    no_errors(&s.host);
    let st = s.scripts.state();
    assert_eq!(st.model.world.regions[&asked].owner, st.model.faction_by_key("austria").unwrap().id);
}

/// `TradeableTechnologies` gives two lists, as `0x009C5AA0` (the panel reads two results), each
/// the model's `tradeable_technologies` for that side with the side's faction key; the Technology
/// option opens its popup without a script error. Bug 2026-10-10: one `{Proposer, Recipient}`
/// table of every technology.
#[test]
fn negotiation_technologies_are_two_lists() {
    let Some((s, panel)) = britain_negotiates("austria") else { return };
    let (offers, demands): (mlua::Table, mlua::Table) = s.host.lua().load("return CampaignUI.TradeableTechnologies()").eval().unwrap();
    let rows = |t: &mlua::Table| -> Vec<(String, String, i64)> {
        t.sequence_values::<mlua::Table>().map(|r| r.unwrap()).map(|r| (r.get("Key").unwrap(), r.get("FactionKey").unwrap(), r.get("tech_status").unwrap())).collect()
    };
    let want = |side: &str, other: &str| -> Vec<(String, String, i64)> {
        let st = s.scripts.state();
        let m = &st.model;
        let (a, b) = (m.faction_by_key(side).unwrap().id, m.faction_by_key(other).unwrap().id);
        m.tradeable_technologies(a, b).into_iter().map(|k| (k, side.to_owned(), 0)).collect()
    };
    assert_eq!(rows(&offers), want("britain", "austria"));
    assert_eq!(rows(&demands), want("austria", "britain"));
    assert!(offers.raw_len() > 0 && demands.raw_len() > 0, "Britain and Austria each have technologies to trade in 1805");
    click(&s.host, find(&s.host, panel, "diplomacy_button_technology").expect("the Technology option"));
    no_errors(&s.host);
}

/// (3) The greeting follows the recipient's attitude (hostile France, friendly Ottomans, very
/// friendly Austria: the three screenshot texts), with the culture's minister card 001.
#[test]
fn negotiation_greeting_by_attitude() {
    for (with, key, folder) in [
        ("france", "european_monarchy_receive_hostile", "european"),
        ("ottomans", "middle_east_monarchy_receive_friendly", "middle_east"),
        ("austria", "european_monarchy_receive_very_friendly", "european"),
    ] {
        let Some((s, panel)) = britain_negotiates(with) else { return };
        let speech = find(&s.host, panel, "speechbox").expect("the diplomat's speech bubble is shown");
        assert_eq!(text(&s.host, speech), loc_text(&format!("diplomacy_strings_string_{key}")), "{with}");
        let path: String = s.host.lua().load(format!("return CampaignUI.MinisterPortraitPath('{with}')")).eval().unwrap();
        assert_eq!(path, format!("ui/portraits/{folder}/Cards/minister/young/001.tga"));
    }
}

/// (3) The greeting is the model's pick, made once when the queued begin command runs
/// (0x00BF5A60): with a faction override row the pick draws the campaign RNG once; recreating the
/// negotiation object, rebuilding the buttons and reading the details draw nothing more.
#[test]
fn the_greeting_draws_the_rng_once_per_negotiation() {
    let Some(s) = setup_in("mp_eur_napoleon", "britain") else { return };
    {
        let mut st = s.scripts.state_mut();
        let m = &mut st.model;
        let gov = m.faction_by_key("austria").unwrap().government_key.clone();
        let key = ("receive_very_friendly".to_owned(), "european".to_owned(), gov, "austria".to_owned());
        std::sync::Arc::make_mut(&mut m.rules).negotiation_overrides.insert(key, "european_monarchy_receive_very_friendly".into());
    }
    let rng = |s: &Setup| s.scripts.state().model.rng;
    let before = rng(&s);
    let _panel = open_negotiations(&s, "austria");
    // The constructor only queues CCQ_DIPLOMACY_BEGIN_NEGOTIATION: nothing begins and nothing is
    // drawn until the game applies the command (commands are ordered for multiplayer and replays).
    assert_eq!((rng(&s), s.scripts.state().model.negotiations.begun), (before, 0), "nothing before the command runs");
    frame(&s, 0.0);
    no_errors(&s.host);
    let mut once = before;
    once.unit_float();
    assert_eq!(rng(&s), once, "one draw when the negotiation began");
    let greeting = s.scripts.state().model.negotiations.current.as_ref().and_then(|n| n.greeting.clone());
    assert!(greeting.is_some(), "the model holds the pick");
    s.host.lua().load("UIDiplomacyNegotiation() CampaignUI.BuildPossibleActions() CampaignUI.FactionDetails('austria')").exec().unwrap();
    frame(&s, 0.0);
    assert_eq!(rng(&s), once, "no further draw");
    assert_eq!(s.scripts.state().model.negotiations.current.as_ref().and_then(|n| n.greeting.clone()), greeting);
}

/// (4) The leader's name carries the regnal numeral (CHARACTER_DETAILS #4): "George III",
/// "Selim I", "Franz I".
#[test]
fn leader_names_carry_the_regnal_numeral() {
    let Some(s) = setup_in("mp_eur_napoleon", "britain") else { return };
    for (f, name) in [("britain", "George III"), ("ottomans", "Selim I"), ("austria", "Franz I")] {
        let got: String = s.host.lua().load(format!("return CampaignUI.FactionDetails('{f}').Leader.Name")).eval().unwrap();
        assert_eq!(got, name);
    }
}



/// The settlement labels draw under the HUD's panels: Labels.lua creates each `city_info_bar`
/// (priority -1) under the root, the root hears `OnAdoptChild` and sorts its children by
/// `Priority()` (`layout.root.lua:563`), so the labels come before the Diplomatic Relations panel
/// (47) and the HUD (0) in the draw order, even when they are made after the panel opened. Bug
/// 2026-10-09: "Wales, Wales" and "London, England" drew over the panel's bottom edge.
#[test]
fn settlement_labels_draw_under_the_diplomatic_relations_panel() {
    let Some(s) = setup() else { return };
    no_errors(&s.host);
    click(&s.host, find(&s.host, s.root, "button_diplomacy").unwrap());
    no_errors(&s.host);
    let panel = find(&s.host, s.root, "diplomatic_relations").expect("the panel opens");
    let regions: Vec<ntw_sim::campaign::RegionId> = {
        let st = s.scripts.state();
        let france = st.model.faction_by_key("france").unwrap().id;
        st.model.world.regions.values().filter(|r| r.owner == france).map(|r| r.id).take(3).collect()
    };
    let on_screen = regions.iter().enumerate().map(|(i, r)| (*r, 100.0 + 60.0 * i as f32, 700.0)).collect();
    s.host.campaign_set_view((1.0, 2.0, 3.0), on_screen, None);
    s.host.pulse(16.0);
    no_errors(&s.host);
    let w = s.host.world();
    let kids = w.get(s.root).unwrap().children.clone();
    let mut panel_top = panel;
    while let Some(p) = w.get(panel_top).and_then(|n| n.parent).filter(|&p| p != s.root) {
        panel_top = p;
    }
    let at = |n: NodeId| kids.iter().position(|&k| k == n).unwrap();
    let labels: Vec<usize> = kids.iter().enumerate().filter(|(_, k)| w.get(**k).unwrap().data.id.starts_with("label")).map(|(i, _)| i).collect();
    assert_eq!(labels.len(), regions.len(), "one label per settlement on screen");
    assert!(labels.iter().all(|&i| i < at(panel_top)), "labels {labels:?} before the panel at {}", at(panel_top));
    assert_eq!(labels, (0..labels.len()).collect::<Vec<_>>(), "the labels are the first children: under the whole HUD");
}

/// The region details panel shows the region: the root's ShowRegionInfo opens it with
/// `CampaignUI.InitialiseRegionInfoDetails(region)` and region_details.lua fills it to its end: the
/// title is the region's name, the tax income is Wealth × (UpperTax + LowerTax) truncated, the rate
/// their sum in percent, and the public order groups get one pip column per factor (each factor
/// with its sign's pip picture and text). Bugs 2026-10-09: the title kept the layout's
/// " XXX Details", then the script stopped at line 124 (`UpperTax`) with the rest of the panel at
/// the layout's defaults.
#[test]
fn region_details_panel_is_filled_from_the_campaign() {
    let Some(s) = setup() else { return };
    no_errors(&s.host);
    let lua = s.host.lua();
    let row: mlua::Table = lua.load("return CampaignUI.RetrieveFactionRegionList('france')[1]").eval().unwrap();
    let name: String = row.get("Name").unwrap();
    let address: mlua::Value = row.get("Address").unwrap();
    let show: mlua::Function = s.host.script_env(s.root).unwrap().raw_get("ShowRegionInfo").unwrap();
    show.call::<()>(address.clone()).expect("the panel's script runs to its end");
    no_errors(&s.host);
    let title = find(&s.host, s.root, "region_name").expect("the region details panel is open");
    assert_eq!(text(&s.host, title).trim(), name);
    assert!(!name.is_empty() && !name.contains("XXX"));

    let get = lua.create_function(|lua, a: mlua::Value| lua.load("return CampaignUI.InitialiseRegionInfoDetails(...)").call::<mlua::Table>(a)).unwrap();
    let d: mlua::Table = get.call(address).unwrap();
    let (wealth, upper, lower): (f64, f64, f64) = (d.get("Wealth").unwrap(), d.get("UpperTax").unwrap(), d.get("LowerTax").unwrap());
    assert!(wealth > 0.0 && upper > 0.0 && lower > 0.0, "France pays taxes: {wealth} {upper} {lower}");
    let treasury = find(&s.host, s.root, "treasury_text").unwrap();
    assert_eq!(text(&s.host, treasury).trim().parse::<f64>().unwrap(), (wealth * (upper + lower)).trunc());
    let rate = find(&s.host, s.root, "dy_tax_percent").unwrap();
    assert_eq!(text(&s.host, rate).trim(), format!("{:.1}%", (upper + lower) * 100.0));

    // The public order tables: the class's total and its factors, each with a pip picture and text.
    let (po_upper, _): (f64, f64) = lua.load("return CampaignUI.RegionsPublicOrders(...)").call(d.get::<mlua::Value>("Address").unwrap()).unwrap();
    let order: mlua::Table = d.get("UpperOrder").unwrap();
    assert_eq!(order.get::<f64>("Total").unwrap(), po_upper);
    assert!(order.raw_len() > 0, "France's upper class has public order factors");
    for f in order.sequence_values::<mlua::Table>() {
        let f = f.unwrap();
        let pip: String = f.get("Pip").unwrap();
        assert!(pip.starts_with("data/ui/campaign ui/pips/") && pip.ends_with(".tga"), "{pip}");
        assert!(!f.get::<String>("Tooltip").unwrap().is_empty());
        // Predicted is the prediction's magnitude minus the value's (0x009E7064).
        let (total, change) = (f.get::<f64>("Total").unwrap(), f.get::<f64>("Predicted").unwrap());
        assert!(total > 0.0 || total + change > 0.0);
    }
    let group = find(&s.host, s.root, "upper_public_order").expect("the upper class's pip group");
    let mut pips = 0;
    s.host.world().visit_visible(group, &mut |_, node| pips += usize::from(node.data.id.starts_with("pip")));
    assert!(pips > 0, "the upper class's pips are drawn");

    // Population: the projected trend and the growth factors in percent (the vanilla base growth 0.3%).
    assert!((1..=3).contains(&d.get::<i64>("PopulationChange").unwrap()));
    let growth: mlua::Table = d.get("PopulationGrowth").unwrap();
    assert_eq!((growth.get::<f64>("PipValue").unwrap() as f32, growth.get::<bool>("IsPercentage").unwrap()), (0.01, true));
    let factors: Vec<mlua::Table> = growth.sequence_values().map(Result::unwrap).collect();
    let base = factors.first().expect("the base growth factor");
    assert_eq!(base.get::<f64>("Total").unwrap() as f32, 0.29999998, "30 hundredths, as 0x00A88EF0 stores them");
    assert!(base.get::<String>("Tooltip").unwrap().contains("0.3%"), "{:?}", base.get::<String>("Tooltip"));
    let sum: f64 = factors.iter().map(|f| if f.get::<bool>("Positive").unwrap() { 1.0 } else { -1.0 } * f.get::<f64>("Total").unwrap()).sum();
    assert!((sum - growth.get::<f64>("Total").unwrap()).abs() < 1e-4, "the factors sum to the growth: {sum}");
    // Town wealth: the trend of the predicted growth and the growth's factors with the table's pictures.
    assert!((0..=4).contains(&d.get::<i64>("WealthChange").unwrap()));
    let town: mlua::Table = d.get("TownWealth").unwrap();
    assert_eq!(town.get::<i64>("PipValue").unwrap(), 50);
    for f in town.sequence_values::<mlua::Table>() {
        let pip: String = f.unwrap().get("Pip").unwrap();
        assert!(pip.starts_with("data/ui/campaign ui/pips/"), "{pip}");
    }
}

/// The region details panel against what the original stored in its own vanilla saves (read-only
/// evidence copies, `NTW_EVIDENCE_DIR`; IN_GAME_CHECKS_2026-10-10.md (g)): for every owned region,
/// the population, the wealth (GDP + town wealth), the town wealth growth, the population growth
/// with its factors (`REGION_FACTORS`, regions without an army in them: the map's area test is not
/// loaded here) and the religious breakdown are the saved values. Public order is checked against
/// the saved class factors by `economy_check` (CAMPAIGN_FIDELITY.md round 6).
#[test]
fn region_details_panel_shows_the_vanilla_saves_values() {
    use ntw_sim::campaign::population;
    let dir = data_dir();
    let ev = common::evidence_dir();
    let wanted = ["orig_fr_t1", "orig_fr_may1811", "auto_orig_spa_0245", "auto_nr4_t4", "auto_after_c8", "orig_over_nr4_0252"];
    let (saves, missing): (Vec<(&str, PathBuf)>, Vec<_>) = wanted.into_iter().map(|name| (name, ev.join(format!("{name}.save")))).partition(|(_, p)| p.is_file());
    if !missing.is_empty() {
        eprintln!("skipped evidence saves (not in {}): {:?}", ev.display(), missing.iter().map(|m| m.0).collect::<Vec<_>>());
    }
    if !dir.join("data.pack").is_file() || saves.is_empty() {
        eprintln!("skipped: no install at {} or no evidence saves in {}", dir.display(), ev.display());
        return;
    }
    let install = open_install(&dir);
    for (name, path) in &saves {
        let (mut regions, mut growths) = (0, 0);
        let s = setup_loaded(&dir, &install, |db| ntw_campaign::read_file(path, db).expect("save loads"), None);
        no_errors(&s.host);
        let lua = s.host.lua();
        let rows: Vec<mlua::Table> =
            lua.load(format!("return CampaignUI.RegionsInTheatre(nil, '{}', nil, 1)", s.human)).eval::<Vec<mlua::Table>>().unwrap();
        let details = lua.create_function(|lua, a: mlua::Value| lua.load("return CampaignUI.InitialiseRegionInfoDetails(...)").call::<mlua::Table>(a)).unwrap();
        for row in rows {
            let key: String = row.get("Key").unwrap();
            let d: mlua::Table = details.call(row.get::<mlua::Value>("Address").unwrap()).unwrap();
            let st = s.scripts.state();
            let m = &st.model;
            let r = m.world.regions.values().find(|r| r.key == key).unwrap();
            if m.world.factions.get(&r.owner).is_none_or(|f| f.key.is_empty()) {
                continue;
            }
            assert_eq!(d.get::<u32>("PopulationNumber").unwrap(), r.population, "{name} {key}: population");
            assert_eq!(d.get::<i64>("Wealth").unwrap(), i64::from(r.gdp) + i64::from(r.town_wealth), "{name} {key}: wealth");
            let town: mlua::Table = d.get("TownWealth").unwrap();
            assert_eq!(town.get::<i32>("Total").unwrap(), r.town_wealth_growth, "{name} {key}: town wealth growth");
            // The breakdown lists each religion with a share, in the region's order, as percentages.
            let sum: f32 = r.religions.iter().map(|x| x.1).sum();
            let saved: Vec<(&str, f32)> = r.religions.iter().filter(|x| x.1 > 0.0).map(|x| (x.0.as_str(), x.1 / sum * 100.0)).collect();
            let shown: Vec<(String, f32)> = d
                .get::<mlua::Table>("ReligiousBreakdown")
                .unwrap()
                .sequence_values::<mlua::Table>()
                .map(|e| e.unwrap())
                .map(|e| (e.get("Key").unwrap(), e.get("Percentage").unwrap()))
                .collect();
            assert_eq!(shown.len(), saved.len(), "{name} {key}: religions {shown:?} {saved:?}");
            for ((k, p), (sk, sp)) in shown.iter().zip(&saved) {
                assert!(k == sk && (p - sp).abs() < 1e-3, "{name} {key}: religion {k} {p} vs saved {sk} {sp}");
            }
            regions += 1;
            let p = &r.population_state;
            if p.factors[3] != 0.0 || population::hostile_units(m, r) != 0 {
                continue;
            }
            let growth: mlua::Table = d.get("PopulationGrowth").unwrap();
            assert_eq!(growth.get::<f32>("Total").unwrap().to_bits(), p.growth.to_bits(), "{name} {key}: population growth");
            // Each shown factor is the saved one with the same hundredths, in order (the panel omits zero factors; negative ones are signed by `Positive`).
            let shown: Vec<i32> = growth
                .sequence_values::<mlua::Table>()
                .map(|e| e.unwrap())
                .map(|e| if e.get::<bool>("Positive").unwrap() { 1 } else { -1 } * population::as_hundredths(e.get::<f32>("Total").unwrap()))
                .collect();
            let saved: Vec<i32> = p.factors.iter().map(|f| population::as_hundredths(*f)).filter(|h| *h != 0).collect();
            assert_eq!(shown, saved, "{name} {key}: the growth factors");
            growths += 1;
        }
        no_errors(&s.host);
        eprintln!("region details {name}: {regions} regions, {growths} with the growth factors compared");
        assert!(regions > 0 && growths > 0, "{name}: {regions} regions, {growths} growths");
    }
}

/// `RetrieveVisibleEnitityDetails` lists nothing while the labels preference is off
/// (`ui_show_campaign_labels`, index 0x44, read first by `0x009F4520`), and `ToggleLabels`
/// (`0x009F9CA0`) flips it. Bug: the list ignored the preference and ToggleLabels was missing.
#[test]
fn toggle_labels_empties_the_visible_settlements() {
    let Some(s) = setup() else { return };
    let regions: Vec<ntw_sim::campaign::RegionId> = {
        let st = s.scripts.state();
        let france = st.model.faction_by_key("france").unwrap().id;
        st.model.world.regions.values().filter(|r| r.owner == france).map(|r| r.id).take(3).collect()
    };
    let on_screen = regions.iter().enumerate().map(|(i, r)| (*r, 100.0 + 60.0 * i as f32, 700.0)).collect();
    s.host.campaign_set_view((1.0, 2.0, 3.0), on_screen, None);
    let lua = s.host.lua();
    let count = || -> usize { lua.load("return #CampaignUI.RetrieveVisibleEnitityDetails().Settlements").eval().unwrap() };
    let before = count();
    lua.load("CampaignUI.ToggleLabels()").exec().unwrap();
    let toggled = count();
    lua.load("CampaignUI.ToggleLabels()").exec().unwrap();
    assert_eq!(count(), before);
    let mut both = [before, toggled];
    both.sort();
    assert_eq!(both, [0, regions.len()], "one state lists the settlements, the other nothing");
    no_errors(&s.host);
}

/// The labels' fog test is the shroud's hide set alone (`0x009BAA90` → `0x00B11E70`): a settlement
/// France has never seen keeps its label, as its own do; putting its cell in the hide set drops it.
/// The INFERRED rule this replaces ("seen once") dropped the never-seen ones.
#[test]
fn a_never_seen_settlement_keeps_its_label_and_a_hidden_cell_loses_it() {
    use ntw_sim::campaign::visibility::FogState;
    let Some(s) = setup() else { return };
    let (own, unseen, cell) = {
        let st = s.scripts.state();
        let m = &st.model;
        let france = m.faction_by_key("france").unwrap().id;
        let at = |r: &ntw_sim::campaign::world::Region| (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
        let mut regions: Vec<_> = m.world.regions.values().collect();
        regions.sort_by_key(|r| r.id);
        let own = regions.iter().find(|r| r.owner == france).unwrap();
        let unseen = regions.iter().find(|r| m.fog_state(france, at(r)) == FogState::NeverSeen).expect("a settlement France has never seen");
        let grid = m.world.sight_grid.unwrap();
        let cell = grid.cell_of(at(unseen)).unwrap();
        assert_ne!(grid.cell_of(at(own)).unwrap(), cell, "hiding the unseen cell must leave the own settlement visible");
        (own.id, unseen.id, cell)
    };
    s.host.campaign_set_view((1.0, 2.0, 3.0), vec![(own, 100.0, 700.0), (unseen, 300.0, 700.0)], None);
    let lua = s.host.lua();
    let count = || -> usize { lua.load("return #CampaignUI.RetrieveVisibleEnitityDetails().Settlements").eval().unwrap() };
    assert_eq!(count(), 2, "the never-seen settlement is labelled too");
    {
        let mut st = s.scripts.state_mut();
        let france = st.model.faction_by_key("france").unwrap().id;
        st.model.world.shrouds.get_mut(&france).unwrap().hidden.set(cell.0, cell.1);
    }
    assert_eq!(count(), 1, "its cell in the hide set: no label");
    no_errors(&s.host);
}

/// A general's recruitment tab is his commander panel (the generator's character path, `0x009FE7B0`;
/// UI_FIDELITY.md §4.10): the faction's units from all its regions, each card's turns reading
/// `"<training>/<march>"` (the original's Wellesley shows "2/1"), and a card click recruits the unit
/// in its source region with the general as the item's target, which then is his queue.
/// Run on the install: `cargo test -p ntw_script --test campaign_ui generals_recruitment -- --nocapture`.
#[test]
fn a_generals_recruitment_tab_lists_the_factions_units_and_recruits_for_him() {
    let Some(mut s) = setup() else { return };
    let (france, general) = {
        let st = s.scripts.state();
        let m = &st.model;
        let france = m.faction_by_key("france").unwrap().id;
        let general = m
            .world
            .forces
            .values()
            .filter(|f| !f.is_navy && f.faction == france)
            .filter_map(|f| f.commander)
            .find(|c| m.world.characters.get(c).is_some_and(|x| x.kind == CharacterKind::General))
            .expect("a French general with an army");
        (france, general)
    };
    s.host.campaign_select(CampaignSelection::Character(general));
    assert!(errors(&s.host).is_empty());
    // The selection bar names the general alone (`0x009C2AF0`), not "<name>, <faction>".
    let selector = find(&s.host, s.root, "selector").unwrap();
    let title = text(&s.host, selector);
    assert!(!title.is_empty() && !title.contains(", "), "{title:?}");
    let tab = find(&s.host, s.root, "recruitment_tab").expect("recruitment tab");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty());

    let rec = s.scripts.state().model.commander_recruitment(france, general);
    assert!(rec.options.iter().any(|o| o.available()), "France can recruit through its general: {rec:?}");
    // An army is offered land units only (`0x00B64E00`), though French ports build ships.
    assert!(rec.options.iter().all(|o| !s.scripts.state().model.rules.is_naval_unit(&o.unit_key)), "{rec:?}");
    let mut cards = Vec::new();
    s.host.world().visit_visible(s.root, &mut |n, node| {
        if node.data.id.split('!').nth(1) == Some("recruitable") {
            cards.push((n, node.data.id.clone()));
        }
    });
    assert_eq!(cards.len(), rec.options.len(), "one card per option: {cards:?}");
    // The options come from more than one region (the faction's whole map).
    let sources: std::collections::BTreeSet<_> = rec.options.iter().map(|o| o.region).collect();
    assert!(sources.len() > 1, "{sources:?}");
    // The first available card shows its training and march turns.
    let (i, o) = rec.options.iter().enumerate().find(|(_, o)| o.available() && o.training_turns > 0).expect("an available option");
    let badge = format!("{}/{}", o.training_turns, o.travel_turns_rounded());
    let id = format!("{}!recruitable!{i}", o.unit_key);
    let (card, _) = *cards.iter().find(|(_, c)| *c == id).expect("the option's card");
    let mut texts = Vec::new();
    s.host.world().visit_visible(card, &mut |n, _| texts.push(text(&s.host, n)));
    assert!(texts.contains(&badge), "card {id} shows {badge}: {texts:?}");

    click(&s.host, card);
    let _ = errors(&s.host);
    let reqs = s.host.take_campaign_requests();
    let wanted = CampaignCommand::Recruit { region: o.region, unit_key: o.unit_key.clone(), target: Some(general) };
    assert!(reqs.iter().any(|r| matches!(r, CampaignRequest::Command(c) if *c == wanted)), "requests: {reqs:?}");
    let n = apply_all(&mut s, reqs);
    assert!(n >= 1);
    s.host.campaign_select(CampaignSelection::Character(general));
    let tab = find(&s.host, s.root, "recruitment_tab").expect("recruitment tab");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty());
    assert!(find_prefix(&s.host, s.root, &format!("{}!enqueued!", o.unit_key)).is_some(), "the item is his queue");
}

/// In-game check 2026-10-10: Britain's generals at the 1805 start are offered the four land units the
/// original shows (its faction unit set, 0x00F74CB0: the military grouping's units plus its permission
/// rows; no ships, no unit without a row such as the Brunswick Hussars).
#[test]
fn britains_generals_are_offered_the_four_land_units_of_the_original() {
    let Some(s) = setup_in("eur_napoleon", "britain") else { return };
    let st = s.scripts.state();
    let m = &st.model;
    let britain = m.faction_by_key("britain").unwrap().id;
    let want = ["Art_Foot_British_6_lber", "Cav_Light_British_Light_Dragoons", "Inf_Line_British_Foot", "Inf_Militia_British_Fencibles"];
    let mut generals = 0;
    for f in m.world.forces.values().filter(|f| !f.is_navy && f.faction == britain) {
        let Some(c) = f.commander else { continue };
        let rec = m.commander_recruitment(britain, c);
        let mut got: Vec<&str> = rec.options.iter().map(|o| o.unit_key.as_str()).collect();
        got.sort_unstable();
        assert_eq!(got, want, "general {c:?}");
        generals += 1;
    }
    assert!(generals > 0);
}

/// Regression (in-game check 2026-10-10): hovering a construction slot left an empty dark
/// "Tooltip" frame on screen after the pointer left. The slot's mouse off switches the root back
/// to the "standard" tooltip method, which creates the visible, empty "Tooltip" template; the
/// engine's mouse off then hands the root's SetTooltip handler an empty text (`0x0102EB40` →
/// `0x01035D20`), whose SetText hides it. Mouse on shows the building tooltip.
#[test]
fn a_building_slots_tooltip_goes_when_the_pointer_leaves() {
    let Some(s) = setup() else { return };
    let paris = s.scripts.state().model.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    let slot = find(&s.host, s.root, "Building1").expect("slot");
    s.host.pointer(slot, PointerEvent::Enter);
    assert!(errors(&s.host).is_empty(), "{:?}", errors(&s.host));
    assert!(find(&s.host, s.root, "TechTreeItem_Tooltip").is_some(), "building tooltip shown");
    s.host.pointer(slot, PointerEvent::Leave);
    assert!(errors(&s.host).is_empty(), "{:?}", errors(&s.host));
    for tip in ["TechTreeItem_Tooltip", "Tooltip"] {
        assert!(find(&s.host, s.root, tip).is_none(), "{tip} still shown");
    }
    // A whole click leaves nothing behind either.
    click(&s.host, slot);
    for tip in ["TechTreeItem_Tooltip", "Tooltip"] {
        assert!(find(&s.host, s.root, tip).is_none(), "{tip} still shown after a click");
    }
    // The slot asks for the details as (building, faction, region, slot)
    // (template.BuildingFrame.lua:392); regression: the region and faction were swapped.
    let (region, slot_key): (String, String) = s
        .host
        .lua()
        .load("local d = CampaignUI.BuildingDetails('gov_magistrate', 'france', 'eur_france', 'slot_x') return d.region_key, d.slot_key")
        .eval()
        .unwrap();
    assert_eq!((region.as_str(), slot_key.as_str()), ("eur_france", "slot_x"));
}

/// A component under the pointer that is destroyed takes its tooltip down (`0x0102F3F0` sends it
/// its mouse off): the selection change rebuilds the HUD buttons under the pointer.
#[test]
fn a_destroyed_component_under_the_pointer_takes_its_tooltip_down() {
    let Some(s) = setup() else { return };
    let paris = s.scripts.state().model.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    let slot = find(&s.host, s.root, "Building1").expect("slot");
    s.host.pointer(slot, PointerEvent::Enter);
    assert!(find(&s.host, s.root, "TechTreeItem_Tooltip").is_some(), "building tooltip shown");
    s.host.campaign_select(CampaignSelection::None);
    s.host.pulse(0.0);
    assert!(s.host.world().get(slot).is_none(), "the slot was destroyed");
    for tip in ["TechTreeItem_Tooltip", "Tooltip"] {
        assert!(find(&s.host, s.root, tip).is_none(), "{tip} still shown");
    }
    // The app's late mouse off for the destroyed slot does nothing.
    s.host.pointer(slot, PointerEvent::Leave);
    assert!(errors(&s.host).is_empty(), "{:?}", errors(&s.host));
}

/// Settlement recruitment cards fan out exactly as the original's `Recruitment.lua`
/// `RecrutmentCardPositions_OSX` places them (CONFIRMED, ui/recruitment.luac; the only placement
/// function its `SelectRecruitmentCard` calls): the selected card is `trunc(76 * 0.9)` = 68 px wide,
/// the others `trunc(68 * 0.9)` = 61 px at a pitch of `trunc(61 * 0.75)` = 45 px (narrowed by
/// `ceil((total - area) / count)` when the row is wider than the area), and the row is centred in the
/// area right of the queue's left edge. So the cards overlap by design (in-game check (e), 2026-10-10).
#[test]
fn settlement_recruitment_cards_fan_like_the_original() {
    let Some(s) = setup() else { return };
    let paris = s.scripts.state().model.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    let tab = find(&s.host, s.root, "recruitment_tab").expect("recruitment tab");
    click(&s.host, tab);
    // Cards slide into place through their target states.
    for i in 0..40 {
        s.host.pulse(f64::from(i) * 50.0);
    }
    assert!(errors(&s.host).is_empty(), "{:?}", errors(&s.host));
    let rect = |id: &str| s.host.world().get(find(&s.host, s.root, id).expect(id)).unwrap().rect;
    let group = rect("RecruitmentCardGroup");
    let queue0 = rect("queue_0");
    let mut cards = Vec::new();
    s.host.world().visit_visible(s.root, &mut |_, node| {
        if node.data.id.contains("!recruitable!") {
            cards.push(node.rect);
        }
    });
    assert!(cards.len() >= 3, "Paris recruits several units: {cards:?}");
    cards.sort_by(|a, b| a.x.total_cmp(&b.x));
    let selected: Vec<_> = cards.iter().filter(|c| c.w == 68.0).collect();
    assert_eq!(selected.len(), 1, "one selected card 68 px wide: {cards:?}");
    assert!(cards.iter().filter(|c| c.w != 68.0).all(|c| c.w == 61.0), "side cards 61 px wide: {cards:?}");
    let n = cards.len() as f32;
    let offset = queue0.x - group.x;
    let area = group.w - offset;
    let mut pitch = 45.0_f32;
    let mut total = 68.0 + (n - 1.0) * pitch;
    if total > area {
        pitch -= ((total - area) / n).ceil();
        total = 68.0 + (n - 1.0) * pitch;
    }
    let start = group.x + (offset + (area - total) * 0.5).trunc();
    assert_eq!(cards[0].x, start, "row centred in the area: {cards:?}");
    // Each slot advances one pitch from the row start.
    for (i, c) in cards.iter().enumerate() {
        let want = start + i as f32 * pitch;
        assert_eq!(c.x, want, "card {i} at slot x: {cards:?}");
    }
}

/// The settlement recruitment card's price is the region's recruitable entry cost (in-game check
/// (e), 2026-10-10), checked against the vanilla data: Paris's first card, the 6-lber foot battery,
/// costs `units` #7 = 610 scaled by Paris's `recruitment_mod_cost_land_all` of -16 (the iron mine's
/// local -3, the war minister's ministerial effectiveness at level 6: -8, and the campaign start's
/// normal-difficulty human handicap row: -5), `fistp(84 * 610 / 100)` = 512 (`0x00B31020` ->
/// `0x00B0D220`, CONFIRMED), the price the harness shot shows on turn 1. The card shows the model's
/// value, never its own.
#[test]
fn settlement_recruitment_card_price_is_the_regions_entry_cost() {
    let Some(s) = setup() else { return };
    let paris = s.scripts.state().model.world.regions.values().find(|r| r.key == "eur_france").unwrap().id;
    let (base, all, cost) = {
        let st = s.scripts.state();
        let m = &st.model;
        let r = m.world.regions.get(&paris).unwrap();
        let set = ntw_sim::campaign::economy::region_effect_set(m, r);
        let u = m.rules.units.get("Art_Foot_French_6_lber").expect("6-lber in vanilla");
        (u.campaign_cost, set.get_int("recruitment_mod_cost_land_all"), ntw_sim::campaign::economy::recruitment_cost_in(&m.rules, &set, "Art_Foot_French_6_lber", u))
    };
    assert_eq!((base, all, cost), (610, -16, 512));
    s.host.campaign_select(CampaignSelection::Settlement(paris));
    let tab = find(&s.host, s.root, "recruitment_tab").expect("recruitment tab");
    click(&s.host, tab);
    assert!(errors(&s.host).is_empty(), "{:?}", errors(&s.host));
    let first = find_prefix(&s.host, s.root, "Art_Foot_French_6_lber!recruitable!").expect("6-lber card");
    let price = find(&s.host, first, "Cost").expect("selected card's Cost text");
    assert_eq!(text(&s.host, price), "512");
}
