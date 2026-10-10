use super::*;
use ntw_sim::campaign::treaties::DiplomaticAction as D;

const A: FactionId = FactionId(1);
const B: FactionId = FactionId(2);


/// The negotiation appliers (`0x00BB3810` / `0x00B1A790` for the money, the treaty appliers for
/// the stance rows), with the row names `BuildOfferAndDemandStrings` reports. Region rows
/// (`0x00B449F0`) and technology rows have no model command and are dropped.
#[test]
fn negotiation_rows_map_to_their_appliers() {
    // The region rows are DEFERRED: no command is sent for them.
    assert_eq!(deal_item_commands(&NegotiationItem::Regions(vec!["eur_france".into()]), A, B), Vec::new());
    // The lump sum (0x00BB3810(amount, 3)) is a state gift, the schedule a regular payment.
    assert_eq!(
        deal_item_commands(&NegotiationItem::Payment { amount: 500, turns: 0 }, A, B),
        vec![CampaignCommand::Diplomacy { a: A, b: B, action: D::StateGift(500) }]
    );
    assert_eq!(
        deal_item_commands(&NegotiationItem::Payment { amount: 100, turns: 3 }, A, B),
        vec![CampaignCommand::Diplomacy { a: A, b: B, action: D::RegularPayment(100, 3) }]
    );
    // The stance / access / gift rows pass straight through.
    for action in [D::Alliance, D::BreakTrade, D::CancelMilitaryAccess, D::BecomeProtectorate] {
        assert_eq!(deal_item_commands(&NegotiationItem::Action(action), A, B), vec![CampaignCommand::Diplomacy { a: A, b: B, action }]);
    }
    // Technologies are still dropped: no granting address is reachable (see `accept_deal`).
    assert_eq!(deal_item_commands(&NegotiationItem::Technologies(vec!["admin1_public_schooling".into()]), A, B), Vec::new());
    // Every row type has a name the panel's rows report.
    assert_eq!(negotiation_action_name(&NegotiationItem::Regions(vec![])), "transfer_region");
    assert_eq!(negotiation_action_name(&NegotiationItem::Technologies(vec![])), "transfer_technology");
    assert_eq!(negotiation_action_name(&NegotiationItem::Payment { amount: 1, turns: 0 }), "payments");
    assert_eq!(negotiation_action_name(&NegotiationItem::Payment { amount: 1, turns: 2 }), "payments");
}

// ----- A campaign HUD host without the install: a MADE-UP one-region world and a made-up root
// script that records what the engine calls on it (no game file is read).

const HUMAN: &str = "test_faction_a";
const REGION: RegionId = RegionId(10);
const FORT_LEVEL: &str = "test_fort_level";

/// The made-up root script: every engine call the selection makes is recorded in `calls`, the
/// panel infos in `fort_panel` / `selected_entity`.
const ROOT: &str = "calls = {}\n\
    function ClearHud() table.insert(calls, 'ClearHud') end\n\
    function ClearReviewPanel() end\n\
    function ClearReviewPanelTabs() end\n\
    function ReviewPanelTabInit() end\n\
    function CreateReviewPanelTabAtPosition(title, key, i, state) table.insert(calls, key) end\n\
    function GenerateFortConstructionPanel(info) fort_panel = info end\n\
    function GenerateConstructionPanel(info) end\n\
    function GenerateAgentsPanel(info) agents_panel = info end\n\
    function GenerateArmyPanel(info) army_panel = info end\n\
    function GenerateNavyPanel(info) army_panel = info end\n\
    function SetSelectedEntity(e, name) selected_entity = e end\n";

struct TestHud {
    _scripts: crate::ScriptHost,
    host: UiScriptHost,
    root: NodeId,
}

fn test_hud() -> TestHud {
    test_hud_with(GameDatabase::test_fixture())
}

/// [`test_hud`] over database `db` (the fixture with some made-up rows added).
fn test_hud_with(db: GameDatabase) -> TestHud {
    use ntw_sim::calendar::{Calendar, Date, HALF_EARLY};
    use ntw_sim::campaign::{BuildingRef, Faction, GovernmentType, Region, Settlement, World};
    use ntw_sim::fixed::Fixed20;
    let mut w = World::default();
    w.factions.insert(
        A,
        Faction {
            id: A,
            key: HUMAN.into(),
            treasury: 1000,
            government: GovernmentType::AbsoluteMonarchy,
            government_key: String::new(),
            tax_lower: "tax_normal".into(),
            tax_upper: "tax_normal".into(),
            diplomacy: Default::default(),
        },
    );
    w.turn_order = vec![A];
    w.regions.insert(
        REGION,
        Region {
            id: REGION,
            key: "test_region_10".into(),
            owner: A,
            settlement: Settlement { key: "settlement:test_region_10:town".into(), position: (Fixed20::from_int(10), Fixed20::from_int(0)) },
            slots: Vec::new(),
            road: None,
            fortification: Some(BuildingRef { level_key: FORT_LEVEL.into(), health: 100 }),
            population: 1000,
            base_gdp: 100,
            gdp: 100,
            wealth_growth_offset: 0,
            discontent_growth: 0,
            town_wealth: 0,
            town_wealth_growth: 0,
            tax_exempt: false,
            religions: Vec::new(),
            class_bases: Vec::new(),
            population_state: Default::default(),
            recruitment_queue: Vec::new(),
            construction: Vec::new(),
            garrison: None,
            fleet: None,
        },
    );
    let start = Date { year: 1805, season: 1, month: 0, half: HALF_EARLY };
    let mut model = CampaignModel::new(Calendar::new(start, 0), ntw_sim::rng::CaRng::new(1), w);
    model.rules = std::sync::Arc::new(ntw_sim::campaign::CampaignRules::test_rules());
    let scripts = crate::ScriptHost::new(model, HUMAN, crate::ScriptSource::empty()).unwrap();
    let source = crate::ScriptSource::empty().with_memory_file("ui/test/root", super::super::host::tests::layout_bytes_with_root(ROOT, ""));
    let host =
        UiScriptHost::new(source, ntw_formats::loc::Localisation::new(), super::super::host::tests::facts(), (100.0, 100.0)).unwrap();
    host.install_campaign(CampaignLink {
        state: scripts.shared_state(),
        human: HUMAN.into(),
        campaign: "test_campaign".into(),
        map: "test_map".into(),
        db: Rc::new(db),
    })
    .unwrap();
    let root = host.load_root_layout("ui/test/root").unwrap();
    host.campaign_ready();
    TestHud { _scripts: scripts, host, root }
}

impl TestHud {
    /// The interned address a script would see for `c`. The `*_value` helpers need the
    /// interning store (`UI_FIDELITY.md` 9.8), which lives on the campaign HUD.
    fn char_addr(&self, c: CharacterId) -> Value {
        character_value(&self.host.campaign_ui().unwrap(), c)
    }
    fn region_addr(&self, r: RegionId) -> Value {
        region_value(&self.host.campaign_ui().unwrap(), r)
    }
    fn unit_addr(&self, u: UnitId) -> Value {
        unit_value(&self.host.campaign_ui().unwrap(), u)
    }
    fn force_addr(&self, f: ForceId) -> Value {
        force_value(&self.host.campaign_ui().unwrap(), f)
    }
    fn fort_addr(&self, r: RegionId) -> Value {
        fort_value(&self.host.campaign_ui().unwrap(), r)
    }
    /// The root script's environment (its globals).
    fn root_env(&self) -> Table {
        self.host.script_env(self.root).expect("the root has scripts")
    }
    /// The tabs created since the last call, in order.
    fn tabs(&self) -> Vec<String> {
        let env = self.root_env();
        let calls: Table = env.get("calls").unwrap();
        let out = calls.sequence_values::<String>().map(Result::unwrap).filter(|c| c.ends_with("_tab")).collect();
        env.set("calls", self.host.lua().create_table().unwrap()).unwrap();
        out
    }
    fn errors(&self) -> Vec<String> {
        self.host.take_log().into_iter().filter(|l| l.starts_with("ERROR") || l.starts_with("UNKNOWN CampaignUI")).collect()
    }
    /// MADE-UP walls rules: the standing `FORT_LEVEL` is level 0 of `test_fort` (a
    /// `settlement_fortification` chain) and upgrades to two level 1s, `FORT_KEEP` (400) first,
    /// then the cheaper `FORT_BASTION` (50) -- a modded slot that offers several levels.
    fn with_fort_chain(&self) {
        use ntw_sim::campaign::BuildingRules;
        let mut st = self._scripts.state_mut();
        let rules = std::sync::Arc::make_mut(&mut st.model.rules);
        let level = |level: i32, cost: i32| BuildingRules { chain: "test_fort".into(), level, cost, turns: 2, ..Default::default() };
        rules.buildings.insert(FORT_LEVEL.into(), BuildingRules { upgrades_to: vec![FORT_KEEP.into(), FORT_BASTION.into()], ..level(0, 100) });
        rules.buildings.insert(FORT_KEEP.into(), level(1, 400));
        rules.buildings.insert(FORT_BASTION.into(), level(1, 50));
        rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
    }
    fn set_treasury(&self, gold: i32) {
        self._scripts.state_mut().model.world.factions.get_mut(&A).unwrap().treasury = gold;
    }
    /// The commands the calls in `script` request.
    fn requests_of(&self, script: &str) -> Vec<CampaignRequest> {
        self.host.take_campaign_requests();
        self.host.lua().load(script).exec().unwrap();
        self.host.take_campaign_requests()
    }
}

const FORT_KEEP: &str = "test_fort_keep";
const FORT_BASTION: &str = "test_fort_bastion";
const MAP_FORT0: &str = "test_map_fort_0";
const MAP_FORT1: &str = "test_map_fort_1";
const ROAD0: &str = "test_road_0";

impl TestHud {
    /// MADE-UP map fort rules: chain `test_map_fort` of slot type `fort` (as vanilla `fFort`),
    /// levels `MAP_FORT0` (0) and `MAP_FORT1` (1).
    fn with_map_fort_chain(&self) {
        use ntw_sim::campaign::BuildingRules;
        let mut st = self._scripts.state_mut();
        let rules = std::sync::Arc::make_mut(&mut st.model.rules);
        let level = |level: i32| BuildingRules { chain: "test_map_fort".into(), level, cost: 300, turns: 2, ..Default::default() };
        rules.buildings.insert(MAP_FORT0.into(), BuildingRules { upgrades_to: vec![MAP_FORT1.into()], ..level(0) });
        rules.buildings.insert(MAP_FORT1.into(), level(1));
        rules.chain_slots.insert("test_map_fort".into(), vec!["fort".into()]);
    }
    /// MADE-UP road rules: chain `test_road` of slot type `settlement_road`, level `ROAD0`.
    fn with_road_chain(&self) {
        use ntw_sim::campaign::BuildingRules;
        let mut st = self._scripts.state_mut();
        let rules = std::sync::Arc::make_mut(&mut st.model.rules);
        rules.buildings.insert(ROAD0.into(), BuildingRules { chain: "test_road".into(), level: 0, cost: 10, turns: 1, ..Default::default() });
        rules.chain_slots.insert("test_road".into(), vec!["settlement_road".into()]);
    }
    /// The info a settlement panel tab hands `GenerateConstructionPanel`.
    fn construction(&self, panel: ConstructionPanel) -> Table {
        let ui = self.host.campaign_ui().unwrap();
        match construction_info(self.host.lua(), self.host.inner(), &ui, REGION, panel).unwrap() {
            Value::Table(t) => t,
            v => panic!("no construction info: {v:?}"),
        }
    }
}

fn slots_of(info: &Table) -> Vec<Table> {
    info.get::<Table>("slots").unwrap().sequence_values().map(Result::unwrap).collect()
}
fn keys_of(list: &Table) -> Vec<(String, i32)> {
    list.sequence_values::<Table>().map(Result::unwrap).map(|e| (e.get("building_key").unwrap(), e.get("type").unwrap())).collect()
}

/// The settlement walls are the construction panel's **last** slot card, built like any other
/// slot (CONFIRMED: `0x00A01F50` appends the settlement's fortification slot after its slot
/// list; runtime breakpoint and the user's in-game check, 2026-10-07). The frame's ordinary
/// calls -- `BeginUpgrade` / `BeginConstruction`, `CancelConstruction`, `RepairBuilding`,
/// `DemolishBuilding` -- address `SlotRef::Walls` through the card's `slot_key`, and the
/// construction tab's info carries no `infrastructure` key (the exe pushes none).
#[test]
fn walls_are_the_construction_panels_last_slot_card() {
    let hud = test_hud();
    hud.with_fort_chain();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let info = hud.construction(ConstructionPanel::Settlement);
    assert!(info.get::<Value>("infrastructure").unwrap().is_nil(), "the construction tab is no infrastructure panel");
    let slots = slots_of(&info);
    let walls = slots.last().expect("the walls slot");
    let built = keys_of(&walls.get("buildings").unwrap());
    assert_eq!(built, vec![(FORT_LEVEL.to_owned(), 1)], "the standing walls, BUILT");
    let first: Table = walls.get::<Table>("buildings").unwrap().get(1).unwrap();
    let key: String = first.get("slot_key").unwrap();
    assert_eq!(key, fortification_slot_key("test_region_10"));
    assert_eq!(keys_of(&walls.get("upgrades").unwrap()), vec![(FORT_KEEP.to_owned(), 4), (FORT_BASTION.to_owned(), 4)]);
    // The frame's own calls build, cancel, repair and demolish the walls slot.
    let reqs = hud.requests_of(&format!(
        "CampaignUI.BeginUpgrade('{FORT_KEEP}', '{key}'); CampaignUI.CancelConstruction('{FORT_KEEP}', '{key}'); \
         CampaignUI.RepairBuilding('{FORT_LEVEL}', '{key}'); CampaignUI.DemolishBuilding('{FORT_LEVEL}', '{key}')"
    ));
    assert_eq!(
        reqs,
        vec![
            CampaignRequest::Command(CampaignCommand::ConstructBuilding { region: REGION, slot: SlotRef::Walls, level_key: FORT_KEEP.into() }),
            CampaignRequest::Command(CampaignCommand::CancelConstruction { region: REGION, slot: SlotRef::Walls }),
            CampaignRequest::Command(CampaignCommand::RepairBuilding { region: REGION, slot: SlotRef::Walls }),
            CampaignRequest::Command(CampaignCommand::DemolishBuilding { region: REGION, slot: SlotRef::Walls }),
        ]
    );
    let can: bool = hud.host.lua().load(format!("return CampaignUI.CanDemolishBuilding('{key}')")).eval().unwrap();
    assert!(can, "the standing walls can be demolished");
    // An empty walls slot offers the fortification chain's level 0 as a constructable card.
    hud._scripts.state_mut().model.world.regions.get_mut(&REGION).unwrap().fortification = None;
    let slots = slots_of(&hud.construction(ConstructionPanel::Settlement));
    assert_eq!(keys_of(&slots.last().unwrap().get("buildings").unwrap()), vec![(FORT_LEVEL.to_owned(), 3)]);
    // With that level restricted by the scripts there is nothing to show: no walls entry
    // (`0x00B7A0E0`).
    hud._scripts.state_mut().model.world.restricted_buildings.insert(FORT_LEVEL.into());
    assert!(slots_of(&hud.construction(ConstructionPanel::Settlement)).is_empty());
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The infrastructure tab is the settlement's **road** slot (CONFIRMED: `0x00A021B0` lists the
/// road slot alone and sets `infrastructure`), the tab added whenever the road slot exists
/// (`0x0099A200`); it is a construction panel (`GenerateConstructionPanel`), not the fort panel.
#[test]
fn the_infrastructure_tab_is_the_road_slot() {
    let hud = test_hud();
    hud.tabs();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    // No road standing, building or on offer: the tab stays (the exe tests the road slot's
    // presence), and its panel's entry is dropped (`0x00B7A0E0`).
    assert!(hud.tabs().contains(&"infrastructure_tab".to_owned()), "the road slot exists, so its tab does");
    assert!(slots_of(&hud.construction(ConstructionPanel::Infrastructure)).is_empty());
    hud.with_road_chain();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let tabs = hud.tabs();
    assert!(tabs[0] == "construction_tab" && tabs.contains(&"infrastructure_tab".to_owned()), "{tabs:?}");
    let info = hud.construction(ConstructionPanel::Infrastructure);
    assert!(info.get::<bool>("infrastructure").unwrap());
    let slots = slots_of(&info);
    assert_eq!(slots.len(), 1, "the road slot alone");
    assert_eq!(keys_of(&slots[0].get("buildings").unwrap()), vec![(ROAD0.to_owned(), 3)]);
    let reqs = hud.requests_of(&format!("CampaignUI.BeginConstruction('{ROAD0}', '{}')", road_slot_key("test_region_10")));
    assert_eq!(reqs, vec![CampaignRequest::Command(CampaignCommand::ConstructBuilding { region: REGION, slot: SlotRef::Road, level_key: ROAD0.into() })]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// A repair's card shows its progress over the repair's own length
/// (`CampaignModel::repair_turns`), not over the level's build time.
#[test]
fn a_repair_card_counts_the_repair_length() {
    let hud = test_hud();
    hud.with_fort_chain();
    {
        let mut st = hud._scripts.state_mut();
        std::sync::Arc::make_mut(&mut st.model.rules).buildings.get_mut(FORT_LEVEL).unwrap().turns = 10;
        let r = st.model.world.regions.get_mut(&REGION).unwrap();
        r.fortification.as_mut().unwrap().health = 50;
        // floor(0.5 × 10) = 5 turns of repair, 3 of them left.
        r.construction.push(ntw_sim::campaign::ConstructionItem { slot: SlotRef::Walls, level_key: FORT_LEVEL.into(), turns_remaining: 3, cost: 0 });
    }
    let slots = slots_of(&hud.construction(ConstructionPanel::Settlement));
    let card: Table = slots.last().unwrap().get::<Table>("buildings").unwrap().get(1).unwrap();
    assert!(card.get::<bool>("being_repaired").unwrap());
    assert_eq!(card.get::<u32>("turns_to_completion").unwrap(), 3);
    assert_eq!(card.get::<f32>("percent_complete").unwrap(), 40.0);
    // The repair cost is shown while the repair runs (`0x009C8190` writes `0x00B66410(slot)`
    // whatever the repair state; review: it showed 0), though no second repair can start.
    assert!(!card.get::<bool>("can_repair").unwrap());
    let cost = hud._scripts.state().model.repair_cost(REGION, SlotRef::Walls);
    assert!(cost > 0);
    assert_eq!(card.get::<i32>("repair_cost").unwrap(), cost);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// A slot address keeps naming its own slot over a reload (`campaign_ready`): ids are never
/// handed to another slot, so an address a script kept cannot come to name a different one.
#[test]
fn slot_addresses_survive_a_reload() {
    let hud = test_hud();
    let ui = hud.host.campaign_ui().unwrap();
    let walls = slot_value(&ui, REGION, SlotRef::Walls);
    hud.host.campaign_ready();
    let road = slot_value(&ui, REGION, SlotRef::Road);
    assert_eq!(slot_from_entity(&ui, &walls), Some((REGION, SlotRef::Walls)));
    assert_eq!(slot_from_entity(&ui, &road), Some((REGION, SlotRef::Road)));
    let again = slot_value(&ui, REGION, SlotRef::Walls);
    let (Value::Table(a), Value::Table(b)) = (&walls, &again) else { panic!("slot addresses are tables") };
    assert_eq!(a.to_pointer(), b.to_pointer(), "the same slot is the same address");
}

/// The map fort's panel (`GenerateFortConstructionPanel`) shows the fort chain (`fFort`, slot
/// type `fort`): the standing level and one upgrade card that is always greyed (`affordable` =
/// `0x0047BA10()`, constant false), and none of the fort's actions builds anything --
/// `UpgradeFort` and `BuildFort` are switched off in the exe (CONFIRMED by the bytes), the rest
/// are PLACEHOLDER no-ops (the model's map fort has no building state).
#[test]
fn the_map_fort_panel_never_builds() {
    let hud = test_hud();
    hud.with_map_fort_chain();
    hud.with_fort_chain();
    hud.set_treasury(1_000_000);
    hud.tabs();
    hud.host.campaign_select(CampaignSelection::Fort(REGION));
    assert_eq!(hud.tabs(), vec!["construction_tab".to_owned()], "a fort's panel is its construction tab (0x52)");
    let env = hud.root_env();
    let info: Table = env.get("fort_panel").expect("the fort panel was generated");
    assert!(info.get::<bool>("controlable").unwrap());
    let forts: Vec<Table> = info.get::<Table>("forts").unwrap().sequence_values().map(Result::unwrap).collect();
    let rows: Vec<(String, i32, bool)> =
        forts.iter().map(|r| (r.get("building_key").unwrap(), r.get("type").unwrap(), r.get("affordable").unwrap())).collect();
    assert_eq!(rows, vec![(MAP_FORT0.to_owned(), 1, true), (MAP_FORT1.to_owned(), 4, false)], "the walls are not on the fort panel");
    let entity: Value = env.get("selected_entity").unwrap();
    assert_eq!(entity_of(&entity, TAG_FORT), Some(REGION.0 as i32), "the scripts get a fort address");
    let fort = hud.fort_addr(REGION);
    let f = hud.host.lua().create_function(move |_, ()| Ok(fort.clone())).unwrap();
    hud.host.lua().globals().set("__test_fort", f).unwrap();
    let reqs = hud.requests_of(
        "local f = __test_fort(); CampaignUI.UpgradeFort(f); CampaignUI.UpgradeFort(1); CampaignUI.RepairFort(f); \
         CampaignUI.CancelUpgradeFort(f); CampaignUI.CancelFortRepair(f); CampaignUI.DemolishFort(f); CampaignUI.BuildFort(1)",
    );
    assert!(reqs.is_empty(), "{reqs:?}");
    let details: Table = hud.host.lua().load("return CampaignUI.FortDetails(__test_fort())").eval().unwrap();
    assert_eq!(details.get::<String>("Key").unwrap(), MAP_FORT0);
    // The settlement's own tabs: construction first, and no fort panel. With a settlement
    // selected, `FortDetails` addresses no map fort: the default details.
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let details: Table = hud.host.lua().load("return CampaignUI.FortDetails(1)").eval().unwrap();
    assert_eq!(details.get::<String>("Key").unwrap(), "", "a settlement has no map fort");
    let tabs = hud.tabs();
    assert_eq!(tabs[0], "construction_tab", "{tabs:?}");
    let entity: Value = hud.root_env().get("selected_entity").unwrap();
    assert_eq!(entity_of(&entity, TAG_REGION), Some(REGION.0 as i32));
    hud.host.campaign_select(CampaignSelection::None);
    assert!(hud.tabs().is_empty());
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// A loaded fort's own level is its standing level (`Fort::key`, INFERRED the record's level
/// key), and the upgrade card is the next level index after it.
#[test]
fn a_loaded_fort_shows_its_own_level() {
    let hud = test_hud();
    hud.with_map_fort_chain();
    {
        let mut st = hud._scripts.state_mut();
        let fort = ntw_sim::campaign::Fort { id: ntw_sim::campaign::FortId(1), region: REGION, position: None, key: MAP_FORT1.into() };
        st.model.world.forts.insert(fort.id, fort);
    }
    hud.host.campaign_select(CampaignSelection::Fort(REGION));
    let info: Table = hud.root_env().get("fort_panel").unwrap();
    let rows: Vec<(String, i32)> =
        info.get::<Table>("forts").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|r| (r.get("building_key").unwrap(), r.get("type").unwrap())).collect();
    assert_eq!(rows, vec![(MAP_FORT1.to_owned(), 1)], "standing at level 1, no level 2 to offer");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// `FindNextFortUpgradeLevel` (`0x00B430C0`, CONFIRMED) takes the first record of the next
/// level index and gives none when the scripts restricted it: a second fort chain's level of
/// the same index is not offered instead (review: the panel skipped to it).
#[test]
fn a_restricted_next_fort_level_gives_no_upgrade_card() {
    let hud = test_hud();
    hud.with_map_fort_chain();
    {
        use ntw_sim::campaign::BuildingRules;
        let mut st = hud._scripts.state_mut();
        let rules = std::sync::Arc::make_mut(&mut st.model.rules);
        // Sorts after `MAP_FORT1` among the level-1 records.
        rules.buildings.insert("test_more_fort_1".into(), BuildingRules { chain: "test_more_fort".into(), level: 1, cost: 300, turns: 2, ..Default::default() });
        rules.chain_slots.insert("test_more_fort".into(), vec!["fort".into()]);
        st.model.world.restricted_buildings.insert(MAP_FORT1.into());
    }
    hud.host.campaign_select(CampaignSelection::Fort(REGION));
    let info: Table = hud.root_env().get("fort_panel").unwrap();
    let rows: Vec<(String, i32)> =
        info.get::<Table>("forts").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|r| (r.get("building_key").unwrap(), r.get("type").unwrap())).collect();
    assert_eq!(rows, vec![(MAP_FORT0.to_owned(), 1)]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The building browser lists an empty slot only when the model's option rule, with the
/// scripts' restricted levels, leaves something to start there (review: a restricted walls
/// slot was listed as a construction site).
#[test]
fn the_building_browser_skips_a_fully_restricted_empty_slot() {
    let hud = test_hud();
    hud.with_fort_chain();
    hud._scripts.state_mut().model.world.regions.get_mut(&REGION).unwrap().fortification = None;
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let rows = || -> Vec<i32> {
        let t: Table = hud.host.lua().load("return CampaignUI.BuildingBrowserDetails()").eval().unwrap();
        t.get::<Table>("slots").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|e| e.get("type").unwrap()).collect()
    };
    assert_eq!(rows(), vec![7], "the empty walls slot can take the fortification's level 0");
    hud._scripts.state_mut().model.world.restricted_buildings.insert(FORT_LEVEL.into());
    assert!(rows().is_empty(), "nothing to start there once the scripts restrict it");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The building browser lists the walls slot between the region's slots and the road, as the
/// exe's `0x009B5AF0` does (type 7, fortification).
#[test]
fn the_building_browser_lists_the_walls() {
    let hud = test_hud();
    hud.with_fort_chain();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let t: Table = hud.host.lua().load("return CampaignUI.BuildingBrowserDetails()").eval().unwrap();
    let rows: Vec<(i32, String)> =
        t.get::<Table>("slots").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|e| (e.get("type").unwrap(), e.get("building_key").unwrap())).collect();
    assert_eq!(rows, vec![(7, FORT_LEVEL.to_owned())]);
    // "View tree" on the walls row (review crash: the walls' slot index was taken as a
    // `Region::slots` index): the tree of the walls slot, its upgrades available.
    let tree: Table = hud
        .host
        .lua()
        .load("local t = CampaignUI.BuildingBrowserDetails(); return CampaignUI.__BuildingTreeNodes(t.slots[1].slot)")
        .eval()
        .unwrap();
    assert_eq!(tree.get::<String>("slot_key").unwrap(), fortification_slot_key("test_region_10"));
    let nodes: Vec<(String, String)> =
        tree.get::<Table>("nodes").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|n| (n.get("key").unwrap(), n.get("state").unwrap())).collect();
    assert!(nodes.contains(&(FORT_LEVEL.to_owned(), "normal".to_owned())), "{nodes:?}");
    assert!(nodes.contains(&(FORT_KEEP.to_owned(), "available".to_owned())), "{nodes:?}");
    // An unresearched level is not available (the model's option rule checks technology).
    {
        let mut st = hud._scripts.state_mut();
        std::sync::Arc::make_mut(&mut st.model.rules).building_techs.insert(FORT_KEEP.into(), vec!["test_missing_tech".into()]);
    }
    let tree: Table = hud
        .host
        .lua()
        .load("local t = CampaignUI.BuildingBrowserDetails(); return CampaignUI.__BuildingTreeNodes(t.slots[1].slot)")
        .eval()
        .unwrap();
    let keep: Option<String> =
        tree.get::<Table>("nodes").unwrap().sequence_values::<Table>().map(Result::unwrap).find(|n| n.get::<String>("key").unwrap() == FORT_KEEP).map(|n| n.get("state").unwrap());
    assert_eq!(keep.as_deref(), Some("unavailable"));
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// `FortDetails` as the exe's `0x009E49D0` -> `0x009AA250`: one subject (the named level, an
/// empty key = the map fort's standing level), always the same eight keys, and default empty
/// strings / 0 when there is no level (no fort chain, an unknown key) -- never a nil the
/// tooltip trips on.
#[test]
fn fort_details_describe_one_subject() {
    const KEYS: [&str; 8] = ["Key", "Name", "ShortDescription", "LongDescription", "IconFilename", "InfoFilename", "Level", "MaxLevel"];
    let hud = test_hud();
    hud.host.campaign_select(CampaignSelection::Fort(REGION));
    let lua = hud.host.lua();
    let details = |script: &str| -> Table {
        let t: Table = lua.load(script).eval().unwrap();
        let keys: std::collections::BTreeSet<String> = t.pairs::<String, Value>().map(|p| p.unwrap().0).collect();
        assert_eq!(keys, KEYS.iter().map(|k| (*k).to_owned()).collect(), "{script}");
        t
    };
    let key_level = |t: &Table| -> (String, i32, i32) { (t.get("Key").unwrap(), t.get("Level").unwrap(), t.get("MaxLevel").unwrap()) };
    // No fort chain at all: the default details, strings empty and levels 0.
    let empty = details("return CampaignUI.FortDetails(1, '')");
    assert_eq!(key_level(&empty), (String::new(), 0, 0));
    assert_eq!(empty.get::<String>("Name").unwrap(), "");
    assert_eq!(empty.get::<String>("ShortDescription").unwrap(), "");
    hud.with_map_fort_chain();
    // The standing fort: no key, a nil key or an empty key (the frame's `building_key`).
    for script in ["return CampaignUI.FortDetails(1)", "return CampaignUI.FortDetails(1, nil)", "return CampaignUI.FortDetails(1, '')"] {
        assert_eq!(key_level(&details(script)), (MAP_FORT0.to_owned(), 0, 1), "{script}");
    }
    // The upgrade card's level, as the card's tooltip asks (`FortDetails(g_fort_ptr, key)`).
    assert_eq!(key_level(&details(&format!("return CampaignUI.FortDetails(1, '{MAP_FORT1}')"))), (MAP_FORT1.to_owned(), 1, 1));
    // Any building level is described as itself (the exe looks the key up in all of
    // `building_levels`); its own level never falls back to 0.
    let other = details("return CampaignUI.FortDetails(1, 'test_building_level_2')");
    assert_eq!(key_level(&other), ("test_building_level_2".to_owned(), 1, 1));
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    // An unknown key: the default details.
    let unknown = details("return CampaignUI.FortDetails(1, 'test_no_such_level')");
    assert_eq!(key_level(&unknown), (String::new(), 0, 0));
    // No fort addressed at all (nothing selected, no fort argument): the default table, not nil.
    hud.host.campaign_select(CampaignSelection::None);
    let none = details("return CampaignUI.FortDetails(1, '')");
    assert_eq!(key_level(&none), (String::new(), 0, 0));
    assert_eq!(key_level(&details(&format!("return CampaignUI.FortDetails(1, '{MAP_FORT1}')"))), (MAP_FORT1.to_owned(), 1, 1));
    // A region address names that region's map fort, whatever is selected -- and nothing when
    // no fort stands there (review: it described the level-0 fort).
    let region = region_value(&hud.host.campaign_ui().unwrap(), REGION);
    let f = lua.create_function(move |_, ()| Ok(region.clone())).unwrap();
    lua.globals().set("__test_region", f).unwrap();
    hud.host.campaign_select(CampaignSelection::Fort(REGION));
    assert_eq!(key_level(&details("return CampaignUI.FortDetails(__test_region())")), (String::new(), 0, 0));
    {
        let mut st = hud._scripts.state_mut();
        let fort = ntw_sim::campaign::Fort { id: ntw_sim::campaign::FortId(1), region: REGION, position: None, key: MAP_FORT1.into() };
        st.model.world.forts.insert(fort.id, fort);
    }
    hud.host.campaign_select(CampaignSelection::None);
    assert_eq!(key_level(&details("return CampaignUI.FortDetails(__test_region())")), (MAP_FORT1.to_owned(), 1, 1));
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    assert_eq!(key_level(&details("return CampaignUI.FortDetails(__test_region(), '')")), (MAP_FORT1.to_owned(), 1, 1));
}

/// A tab a state function asks for is generated on pointer events that fire no Lua event but
/// run state functions too: a release elsewhere, and a click on a disabled state (both still
/// transition, `0x0102E340`).
#[test]
fn pointer_events_without_a_lua_event_still_generate_a_requested_tab() {
    use super::super::world::PointerEvent;
    for (event, old_disabled) in [(PointerEvent::LeftUpElsewhere, false), (PointerEvent::LeftUp, true)] {
        let hud = test_hud();
        // A settlement: construction (current) and infrastructure (2), both generated by
        // GenerateConstructionPanel, which records `generated`; the state function asks for tab 2.
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        assert_eq!(hud.tabs()[1], "infrastructure_tab");
        let env = hud.root_env();
        hud.host.lua().load("function GenerateConstructionPanel(info) generated = true end").set_environment(env.clone()).exec().unwrap();
        let button = hud.host.world().find(hud.root, "button").unwrap();
        super::super::host::tests::add_state_transition(&hud.host, button, event, |s| {
            s[0].disabled = old_disabled;
            s[1].enter_function = "PickTab".into();
        });
        let benv = super::super::host::tests::component_env(&hud.host, button);
        hud.host.lua().load("function PickTab() CampaignUI.ReviewPanelTabSelectionSet_1_Indexed(2) end").set_environment(benv).exec().unwrap();
        hud.host.pointer(button, event);
        assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 2, "{event:?}");
        assert_eq!(env.get::<Option<bool>>("generated").unwrap(), Some(true), "{event:?}: the selected tab was generated");
        assert!(hud.errors().is_empty());
    }
}

/// The card template indexes `{"neutral", "ally", "foe"}` with the answer and drops the badge on 0.
#[test]
fn relationship_answers_index_the_card_badge_states() {
    use ntw_sim::campaign::Stance;
    assert_eq!(relationship_to_players_faction(true, Stance::War), 0);
    assert_eq!(relationship_to_players_faction(false, Stance::Neutral), 1);
    for s in [Stance::Allied, Stance::Protectorate, Stance::Patron] {
        assert_eq!(relationship_to_players_faction(false, s), 2);
    }
    assert_eq!(relationship_to_players_faction(false, Stance::War), 3);
    let hud = test_hud();
    let r: Value = hud.host.lua().load("return CampaignUI.CharactersRelationshipToPlayersFaction(1)").eval().unwrap();
    assert!(r.is_nil(), "a non-character address answers nil");
    assert!(hud.errors().is_empty(), "the call is bound, not an UNKNOWN stub");
}

/// The agent panel's `CampaignUI` calls are bound as PROVISIONAL no-ops: they exist (no UNKNOWN
/// stub), queue nothing, and `CanAgentEmbarkOrDisembark` answers false.
#[test]
fn agent_panel_calls_are_bound_as_no_ops() {
    let hud = test_hud();
    hud.host.take_log();
    let lua = hud.host.lua();
    for name in
        ["AgentCardSelectionChanged", "AgentEmbarkOrDisembark", "AgentGentlemanDuel", "AgentRakeAssassinate", "AgentRakeSubterfuge", "AgentRogueSabotageArmy"]
    {
        let r: Value = lua.load(format!("return CampaignUI.{name}(1, 'x')")).eval().unwrap();
        assert!(r.is_nil(), "{name}");
    }
    assert!(!lua.load("return CampaignUI.CanAgentEmbarkOrDisembark(1)").eval::<bool>().unwrap());
    assert!(hud.host.take_campaign_requests().is_empty());
    assert!(hud.errors().is_empty(), "no UNKNOWN stub was reached");
    // A settlement with no agent has no agents tab (INFERRED condition, see `tabs_for`).
    hud.tabs();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    assert!(!hud.tabs().contains(&"agents_tab".to_owned()));
}

const OTHER: &str = "test_faction_b";
const RAKE: CharacterId = CharacterId(100);
const FOREIGN_GENTLEMAN: CharacterId = CharacterId(101);
const GENERAL: CharacterId = CharacterId(102);
const OWN_GENTLEMAN: CharacterId = CharacterId(103);

impl TestHud {
    /// Puts a made-up character into the model; `abilities` become his saved `AgentAbilities`.
    fn add_character(&self, id: CharacterId, faction: FactionId, kind: CharacterKind, garrisoned_in: Option<RegionId>, abilities: &[(&str, i32)]) {
        use ntw_sim::fixed::Fixed20;
        let mut st = self._scripts.state_mut();
        let w = &mut st.model.world;
        if !w.factions.contains_key(&B) {
            let mut f = w.factions[&A].clone();
            f.id = B;
            f.key = OTHER.into();
            w.factions.insert(B, f);
        }
        let position = w.regions[&REGION].settlement.position;
        w.characters.insert(
            id,
            ntw_sim::campaign::Character {
                id,
                faction,
                kind,
                position: if garrisoned_in.is_some() { position } else { (Fixed20::from_int(500), Fixed20::from_int(500)) },
                movement_points: 10,
                max_movement_points: 10,
                base_movement_points: 10,
                garrisoned_in,
            },
        );
        if !abilities.is_empty() {
            let d = w.character_details.entry(id).or_default();
            d.abilities = abilities.iter().map(|(k, l)| ((*k).to_owned(), *l, String::new())).collect();
        }
    }

    /// Opens review-panel tab `key` of the current selection, as a click on it does.
    fn open_tab(&self, key: &str) {
        let ui = self.host.campaign_ui().unwrap();
        let i = ui.tabs.borrow().iter().position(|t| t.key() == key).expect("tab listed") + 1;
        self.host.lua().load(format!("CampaignUI.ReviewPanelTabSelectionSet_1_Indexed({i})")).exec().unwrap();
    }

    /// Evaluates `CampaignUI.<name>(<character address>)`.
    fn ask(&self, name: &str, c: CharacterId) -> Value {
        let lua = self.host.lua();
        let f: Function = lua.load(format!("return function(a) return CampaignUI.{name}(a) end")).eval().unwrap();
        f.call(self.char_addr(c)).unwrap()
    }
}

/// Makes the root record, in its `order` list, the calls a review-panel tab change makes
/// (`clear`, `init <index>=<state>`, `gen <tab>` .. `end <tab>` around tab 1's
/// (construction) and tab 2's (infrastructure) generator, both `GenerateConstructionPanel`, told apart by the
/// index just opened), `entity` for `SetSelectedEntity`).
/// The generators call the globals `on_gen1` / `on_gen2` when a test sets them.
fn record_tab_calls(hud: &TestHud) {
    hud.host
        .lua()
        .load(
            "order = {}\n\
             local function note(s) order[#order + 1] = s end\n\
             function ClearReviewPanel() note('clear') end\n\
             function ReviewPanelTabInit(title, i, state) note('init ' .. i .. '=' .. state) if state == 2 then opening = i end end\n\
             function GenerateConstructionPanel(info)\n\
                 local i = opening\n\
                 note('gen ' .. i)\n\
                 local hook = (i == 1 and on_gen1) or (i == 2 and on_gen2)\n\
                 if hook then hook() end\n\
                 note('end ' .. i)\n\
             end\n\
             function SetSelectedEntity(e, name) note('entity') end\n\
             function request(i) CampaignUI.ReviewPanelTabSelectionSet_1_Indexed(i) end",
        )
        .set_environment(hud.root_env())
        .exec()
        .unwrap();
}

/// The calls [`record_tab_calls`] recorded since the last call; drained.
fn take_tab_calls(hud: &TestHud) -> Vec<String> {
    let env = hud.root_env();
    let order: Vec<String> = env.get::<Table>("order").unwrap().sequence_values().map(Result::unwrap).collect();
    env.set("order", hud.host.lua().create_table().unwrap()).unwrap();
    order
}

/// A tab request takes effect inside its call, on the current selection's tabs (as the exe's
/// `0x00A20620` does): an index out of range or the tab already current is ignored; in range
/// the review panel is cleared, the old tab deselected (state 1), the new one selected
/// (state 2) and generated, before the call returns. Each request does that (there and back
/// again rebuilds both). The tab is kept by its identity when the same selection is refreshed;
/// a different selection opens its own first tab.
#[test]
fn a_tab_request_applies_to_the_selection_it_was_made_for() {
    let hud = test_hud();
    // A second settlement of the same faction, so that both selections have several tabs.
    const OTHER_REGION: RegionId = RegionId(12);
    {
        let mut st = hud._scripts.state_mut();
        let mut r = st.model.world.regions[&REGION].clone();
        r.id = OTHER_REGION;
        r.key = "test_region_12".into();
        r.settlement.key = "settlement:test_region_12:town".into();
        st.model.world.regions.insert(OTHER_REGION, r);
    }
    hud.host.campaign_select(CampaignSelection::Settlement(OTHER_REGION));
    assert!(hud.tabs().len() >= 2, "{:?}", hud.tabs());
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let tabs = hud.tabs();
    assert_eq!(tabs[..2], ["construction_tab", "infrastructure_tab"]);
    record_tab_calls(&hud);
    let ui = hud.host.campaign_ui().unwrap();
    let request = |i: usize| hud.host.lua().load(format!("CampaignUI.ReviewPanelTabSelectionSet_1_Indexed({i})")).exec().unwrap();
    request(tabs.len() + 1);
    assert_eq!((ui.current_tab.get(), take_tab_calls(&hud)), (1, vec![]), "out of range: ignored");
    request(2);
    assert_eq!(ui.current_tab.get(), 2);
    assert_eq!(take_tab_calls(&hud), ["clear", "init 1=1", "init 2=2", "gen 2", "end 2"], "inside the call");
    request(2);
    assert!(take_tab_calls(&hud).is_empty(), "the tab already current: no rebuild");
    request(1);
    request(2);
    assert_eq!(
        take_tab_calls(&hud),
        ["clear", "init 2=1", "init 1=2", "gen 1", "end 1", "clear", "init 1=1", "init 2=2", "gen 2", "end 2"],
        "there and back again: both rebuilt"
    );
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    assert_eq!(ui.tabs.borrow()[ui.current_tab.get() - 1].key(), tabs[1], "the same selection keeps that tab");
    hud.host.campaign_select(CampaignSelection::Settlement(OTHER_REGION));
    assert_eq!(ui.current_tab.get(), 1, "a different selection opens its first tab");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// [`record_tab_calls`], plus the tab-list calls of a selection change: `tabs clear`
/// (`ClearReviewPanelTabs`), `hud` (`ClearHud`) and `create <index>=<state>`
/// (`CreateReviewPanelTabAtPosition`).
fn record_selection_calls(hud: &TestHud) {
    record_tab_calls(hud);
    hud.host
        .lua()
        .load(
            "local function note(s) order[#order + 1] = s end\n\
             function ClearReviewPanelTabs() note('tabs clear') end\n\
             function ClearHud() note('hud') end\n\
             function CreateReviewPanelTabAtPosition(title, key, i, state) note('create ' .. i .. '=' .. state) end",
        )
        .set_environment(hud.root_env())
        .exec()
        .unwrap();
}

/// A selection change makes the exe's calls in the exe's order (round 15, CONFIRMED): no
/// `ClearReviewPanel`; `ClearReviewPanelTabs` and `ClearHud` every time (regression: ClearHud
/// only on a deselection); per tab its creation, then either its opening (the tab kept on a
/// refresh) or `ReviewPanelTabInit(.., 1)` (regression: one ReviewPanelTabInit for the current
/// tab after all the creations); the first tab opened after the list when none was kept;
/// `SetSelectedEntity` last.
#[test]
fn a_selection_change_builds_its_tabs_in_the_exes_order() {
    let hud = test_hud();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let n = hud.tabs().len();
    assert!(n >= 2);
    hud.host.lua().load("CampaignUI.ReviewPanelTabSelectionSet_1_Indexed(2)").set_environment(hud.root_env()).exec().unwrap();
    record_selection_calls(&hud);
    let rest = |from: usize| (from..=n).flat_map(|i| [format!("create {i}=1"), format!("init {i}=1")]).collect::<Vec<_>>();

    // A refresh keeps tab 2 (infrastructure): opened at its addition.
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let mut want: Vec<String> = ["tabs clear", "hud", "create 1=1", "init 1=1", "create 2=1", "init 2=2", "gen 2", "end 2"].map(String::from).to_vec();
    want.extend(rest(3));
    want.push("entity".into());
    assert_eq!(take_tab_calls(&hud), want);
    assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 2);

    // Another selection: the first tab opened after the whole list.
    hud.host.campaign_select(CampaignSelection::None);
    assert_eq!(take_tab_calls(&hud), ["tabs clear", "hud"], "a deselection");
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let mut want: Vec<String> = ["tabs clear", "hud"].map(String::from).to_vec();
    want.extend(rest(1));
    want.extend(["init 1=2", "gen 1", "end 1", "entity"].map(String::from));
    assert_eq!(take_tab_calls(&hud), want);
    assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 1);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// A tab request made while a selection change builds the tab list (from ClearHud, a tab's
/// creation, or the opened tab's own generator) is refused, as the exe has no tab set then
/// (its request would read a null pointer, round 15), and logged once per HUD (regression:
/// silently ignored, or acted on mid-build). A request after the change works.
#[test]
fn a_tab_request_while_the_tab_list_is_built_is_refused_and_logged_once() {
    let hud = test_hud();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    record_selection_calls(&hud);
    hud.host
        .lua()
        .load(
            "local hud_ = ClearHud\nfunction ClearHud() hud_() request(2) end\n\
             local create = CreateReviewPanelTabAtPosition\nfunction CreateReviewPanelTabAtPosition(...) create(...) request(2) end\n\
             function on_gen1() request(2) end",
        )
        .set_environment(hud.root_env())
        .exec()
        .unwrap();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let calls = take_tab_calls(&hud);
    assert!(!calls.iter().any(|c| c == "gen 2" || c == "clear"), "no request took effect: {calls:?}");
    assert_eq!(calls.iter().filter(|c| *c == "gen 1").count(), 1, "{calls:?}");
    assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 1);
    let errors = hud.errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("null tab set"), "{errors:?}");
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    assert!(hud.errors().is_empty(), "logged once");

    hud.host.lua().load("on_gen1 = nil request(2)").set_environment(hud.root_env()).exec().unwrap();
    assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 2, "after the change, a request works");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// A generator that asks for another tab during a tab change: that change nests inside the
/// first one (as in the exe), each tab generated once, nothing left pending.
#[test]
fn a_generator_asking_for_a_tab_during_a_tab_change_gets_it_inside_that_change() {
    let hud = test_hud();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    record_tab_calls(&hud);
    hud.host.lua().load("function on_gen2() on_gen2 = nil request(1) end").set_environment(hud.root_env()).exec().unwrap();
    hud.host.lua().load("request(2)").set_environment(hud.root_env()).exec().unwrap();
    assert_eq!(
        take_tab_calls(&hud),
        ["clear", "init 1=1", "init 2=2", "gen 2", "clear", "init 2=1", "init 1=2", "gen 1", "end 1", "end 2"]
    );
    assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 1);
    hud.host.hover(None);
    assert!(take_tab_calls(&hud).is_empty(), "nothing pending at the event's end");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// Generators that keep asking for each other's tab nest without a limit of ours (the exe has
/// none) until Lua's nested C-call limit stops them with its error, which is logged; the HUD
/// keeps working afterwards.
#[test]
fn an_endless_tab_ping_pong_ends_with_luas_error_not_a_crash() {
    let hud = test_hud();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    record_tab_calls(&hud);
    let env = hud.root_env();
    hud.host.lua().load("function on_gen1() request(2) end\nfunction on_gen2() request(1) end").set_environment(env.clone()).exec().unwrap();
    hud.host.lua().load("request(2)").set_environment(env.clone()).exec().unwrap();
    let errors = hud.errors();
    assert!(!errors.is_empty() && errors.iter().all(|e| e.contains("C stack overflow")), "{errors:?}");
    assert!(take_tab_calls(&hud).len() > 20, "it nested many times first");
    env.set("on_gen1", Value::Nil).unwrap();
    env.set("on_gen2", Value::Nil).unwrap();
    let next = 3 - hud.host.campaign_ui().unwrap().current_tab.get();
    hud.host.lua().load(format!("request({next})")).set_environment(env).exec().unwrap();
    assert_eq!(take_tab_calls(&hud).last().map(String::as_str), Some(if next == 1 { "end 1" } else { "end 2" }));
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The agents tab: listed last when the settlement has an agent, and its info is the shape
/// `ui/agents.luac` reads (CONFIRMED): parallel `agents` / `characters` lists, `card_id` a string,
/// `name`, the character details with `Abilities` and `IsGuerilla`, and a `controlable` table.
#[test]
fn agents_tab_lists_the_settlements_agents_with_the_info_the_panel_reads() {
    let hud = test_hud();
    // A general alone is no agent: no tab.
    hud.add_character(GENERAL, A, CharacterKind::General, Some(REGION), &[]);
    hud.tabs();
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    assert!(!hud.tabs().contains(&"agents_tab".to_owned()));

    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
    hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[]);
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    let tabs = hud.tabs();
    assert_eq!(tabs.last().map(String::as_str), Some("agents_tab"), "the agents tab comes last: {tabs:?}");
    hud.open_tab("agents_tab");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());

    let info: Table = hud.root_env().get("agents_panel").expect("GenerateAgentsPanel was called");
    let agents: Table = info.get("agents").unwrap();
    let characters: Table = info.get("characters").unwrap();
    assert!(info.get::<Table>("controlable").is_ok());
    assert_eq!((agents.raw_len(), characters.raw_len()), (2, 2), "the two agents, not the general");
    let card = |i: usize| -> (String, String, String, Table) {
        let a: Table = agents.get(i).unwrap();
        (a.get("card_id").unwrap(), a.get("name").unwrap(), a.get("agent_type_name").unwrap(), characters.get(i).unwrap())
    };
    let (id1, name1, type1, rake) = card(1);
    let (id2, _, _, gentleman) = card(2);
    assert_eq!((id1.as_str(), id2.as_str()), ("agent_100", "agent_101"), "unique string card ids");
    assert_eq!(name1, rake.get::<String>("Name").unwrap());
    // `agents[i].agent_type_name`, the second of the only two fields `InitialiseAgent` reads.
    // This fixture has no localisation and no cultures, so the fallback (the ESF type name)
    // shows; the loc key and the culture are the install test's business.
    assert_eq!(type1, CharacterKind::Rake.esf_name());
    // Parallel lists: characters[i] is agents[i]'s character.
    assert_eq!(entity_of(&rake.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(RAKE.0));
    assert_eq!(entity_of(&gentleman.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(FOREIGN_GENTLEMAN.0));
    // The details the card reads, and the buttons' fields.
    for key in ["Flag", "SmallFlag", "CommanderType", "Attributes"] {
        assert!(!rake.get::<Value>(key).unwrap().is_nil(), "{key}");
    }
    assert!(!rake.get::<bool>("IsGuerilla").unwrap());
    let abilities = |t: &Table| -> Vec<(String, bool)> {
        let a: Table = t.get("Abilities").unwrap();
        AGENT_BUTTON_ABILITIES.iter().map(|k| ((*k).to_owned(), a.get::<bool>(*k).unwrap())).collect()
    };
    // No saved abilities: from the type (INFERRED mapping).
    let on = |v: Vec<(String, bool)>| v.into_iter().filter(|x| x.1).map(|x| x.0).collect::<Vec<_>>();
    assert_eq!(on(abilities(&rake)), ["can_assassinate", "can_sabotage", "can_sabotage_army"]);
    assert_eq!(on(abilities(&gentleman)), ["can_research", "can_duel"]);

    // Saved abilities win: a rake whose save gives only `can_assassinate`.
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2), ("can_sabotage", -1)]);
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    hud.open_tab("agents_tab");
    let info: Table = hud.root_env().get("agents_panel").unwrap();
    let rake: Table = info.get::<Table>("characters").unwrap().get(1).unwrap();
    assert_eq!(on(abilities(&rake)), ["can_assassinate"]);
    assert!(hud.errors().is_empty());
}

/// The questions `ShowAgentButtons` asks with the agent's Address, from the model.
#[test]
fn agent_button_questions_answer_from_the_model() {
    let hud = test_hud();
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
    hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[("can_duel", 1)]);
    hud.add_character(OWN_GENTLEMAN, A, CharacterKind::Gentleman, Some(REGION), &[("can_receive_duel", 1)]);
    hud.host.take_log();
    let truth = |name: &str, c: CharacterId| match hud.ask(name, c) {
        Value::Boolean(b) => b,
        v => panic!("{name} answered {v:?}"),
    };
    // The residence: the settlement the agent is garrisoned in (its region's address).
    assert_eq!(entity_of(&hud.ask("CharacterResidence", RAKE), TAG_REGION), Some(REGION.0 as i32));
    assert!(!truth("CharacterInEnemyResidence", RAKE), "his own settlement");
    assert!(truth("CharacterInEnemyResidence", FOREIGN_GENTLEMAN), "another faction's settlement");
    assert!(!truth("IsCharacterInPortResidence", RAKE));
    // Assassination: the rake has a known foreign target the model's gate accepts.
    assert!(truth("ValidAssassinationTargets", RAKE));
    assert!(!truth("ValidAssassinationTargets", FOREIGN_GENTLEMAN), "a gentleman is no spy");
    // A duel: the foreign gentleman meets one who may receive it in the same residence.
    assert!(truth("ValidDuelTargetsInResidence", FOREIGN_GENTLEMAN));
    assert!(!truth("ValidDuelTargetsInResidence", RAKE), "no can_duel");
    // Nothing to base a yes on: no forces, no buildings, no schools.
    assert!(!truth("ValidSabotageArmyTarget", RAKE));
    assert!(!truth("ValidSabotageTarget", RAKE));
    assert!(!truth("CharacterInValidEnemyUniversity", FOREIGN_GENTLEMAN));
    // An address that is not a character answers false / nil.
    let lua = hud.host.lua();
    assert!(lua.load("return CampaignUI.CharacterResidence(1)").eval::<Value>().unwrap().is_nil());
    assert!(!lua.load("return CampaignUI.ValidAssassinationTargets(1)").eval::<bool>().unwrap());

    // A port: an agent standing on a port slot's position is in that slot's residence.
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        let at = (ntw_sim::fixed::Fixed20::from_int(40), ntw_sim::fixed::Fixed20::from_int(0));
        w.regions.get_mut(&REGION).unwrap().slots.push(ntw_sim::campaign::RegionSlot {
            key: "port:test_region_10:harbour".into(),
            slot_type: "port".into(),
            building: None,
            position: Some(at),
            port: true,
            holder: None,
            id: 7,
        });
        let ch = w.characters.get_mut(&RAKE).unwrap();
        ch.garrisoned_in = None;
        ch.position = at;
    }
    assert!(truth("IsCharacterInPortResidence", RAKE));
    let ui = hud.host.campaign_ui().unwrap();
    assert_eq!(slot_from_entity(&ui, &hud.ask("CharacterResidence", RAKE)), Some((REGION, SlotRef::Slot(0))));
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    assert!(hud.host.take_campaign_requests().is_empty(), "questions queue nothing");
}

/// The agents panel's hover tooltip reads `agent.name` and `agent.agent_type_name`
/// (CONFIRMED, `template.unitcard_tooltip.lua:115`), so the tab's info must carry both.
#[test]
fn the_agents_info_carries_the_hover_tooltip_fields() {
    let hud = test_hud();
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
    hud.host.campaign_select(CampaignSelection::Settlement(REGION));
    hud.open_tab("agents_tab");
    let info: Table = hud.root_env().get("agents_panel").expect("the panel ran");
    let a: Table = info.get::<Table>("agents").unwrap().get(1).unwrap();
    assert_eq!(a.get::<String>("name").unwrap(), "rake", "the agent type's on-screen name");
    assert_eq!(a.get::<String>("agent_type_name").unwrap(), "rake");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The agent action popups: `RequestDuelTargets` / `RequestAssassinationTargets` /
/// `RequestSabotageTargets` answer the model's own target lists (CONFIRMED arity 2 and the
/// fields of a row, `agent_options.lua:109/118/128` and `agent_action.lua:21`), and
/// `InstigateDuel` / `InstigateAssassination` / `InstigateSabotage` / `SabotageArmy` queue the
/// model's commands.
#[test]
fn the_agent_action_calls_answer_the_model_and_queue_its_commands() {
    let hud = test_hud();
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
    hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[("can_duel", 1)]);
    hud.add_character(OWN_GENTLEMAN, A, CharacterKind::Gentleman, Some(REGION), &[("can_receive_duel", 1)]);
    hud.host.take_log();
    let call2 = |name: &str, a: Value, b: Value| -> Value {
        let lua = hud.host.lua();
        let f: Function = lua.load(format!("return function(a, b) return CampaignUI.{name}(a, b) end")).eval().unwrap();
        f.call::<Value>((a, b)).unwrap()
    };
    // The assassination list: the foreign gentleman the model's gate accepts, as rows. The
    // address field is `Address`, CONFIRMED from both shipped row templates -- it is NOT `target`
    // (see [`TargetRow`]); `Name`, `Chance`, `Flag` and `Attributes` are read too, and
    // `Utilities.CreateCharacterCard` concatenates `Flag` unguarded.
    let list = call2("RequestAssassinationTargets", hud.char_addr(RAKE), hud.char_addr(FOREIGN_GENTLEMAN));
    let rows: Table = list.as_table().expect("a list of targets").clone();
    assert_eq!(rows.raw_len(), 1, "one known foreign target the model's gate accepts");
    let row: Table = rows.get(1).unwrap();
    assert_eq!(entity_of(&row.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(FOREIGN_GENTLEMAN.0));
    // `Faction.FlagPath` is the flag **folder** -- `agent_action.lua:21` pc 37-40 appends
    // "/small.tga" to it -- and `Faction.Key`/`Name` are what the duel pane's stance line reads.
    let fac: Table = row.get("Faction").unwrap();
    assert_eq!(fac.get::<String>("Key").unwrap(), OTHER);
    assert!(!fac.get::<String>("Name").unwrap().is_empty());
    assert!(fac.get::<String>("FlagPath").is_ok());
    assert!(row.get::<String>("Flag").unwrap().ends_with("/small.tga"), "the character card's flag");
    assert!(!row.get::<String>("Name").unwrap().is_empty(), "the target's name");
    assert!(row.get::<i64>("Chance").unwrap() > 0, "the model's own percentage");
    assert!(row.get::<Table>("Attributes").is_ok(), "the card indexes Attributes unguarded");
    // A gentleman may not assassinate: an empty list, and the popup is then not opened.
    assert_eq!(call2("RequestAssassinationTargets", hud.char_addr(FOREIGN_GENTLEMAN), hud.char_addr(RAKE)).as_table().unwrap().raw_len(), 0);
    // Duel targets: the foreign gentleman (who may duel) meets the A-side gentleman in the same
    // residence, who may receive it.
    let duels = call2("RequestDuelTargets", hud.char_addr(FOREIGN_GENTLEMAN), hud.char_addr(OWN_GENTLEMAN));
    assert_eq!(duels.as_table().unwrap().raw_len(), 1);
    let row: Table = duels.as_table().unwrap().get(1).unwrap();
    assert_eq!(entity_of(&row.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(OWN_GENTLEMAN.0));
    // The duel row's `Chance` is the number the model's duel is rolled at: the target picks the
    // weapon worse for the challenger (`duel_weapon`), so the smaller of the two chances. Give
    // the challenger pistols skill only: pistols would read 95, swords reads 50, the row 50.
    hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().attributes =
        vec![("duelling_pistols".into(), 6)];
    let row: Table = call2("RequestDuelTargets", hud.char_addr(FOREIGN_GENTLEMAN), hud.char_addr(OWN_GENTLEMAN)).as_table().unwrap().get(1).unwrap();
    {
        use ntw_sim::campaign::agents::{Weapon, duel_chance};
        let m = &hud._scripts.state().model;
        let pistols = duel_chance(m, FOREIGN_GENTLEMAN, OWN_GENTLEMAN, Weapon::Pistols).unwrap();
        let swords = duel_chance(m, FOREIGN_GENTLEMAN, OWN_GENTLEMAN, Weapon::Swords).unwrap();
        assert!(pistols > swords, "the fixture must make the weapons differ: {pistols} vs {swords}");
        assert_eq!(row.get::<i64>("Chance").unwrap(), i64::from(swords), "the weapon the target would pick");
    }
    // No building to sabotage in this settlement: an empty list.
    let sab = call2("RequestSabotageTargets", hud.char_addr(RAKE), hud.char_addr(FOREIGN_GENTLEMAN));
    assert_eq!(sab.as_table().unwrap().raw_len(), 0);
    // An address that is not a character answers an empty list.
    assert_eq!(call2("RequestDuelTargets", Value::Integer(1), Value::Integer(2)).as_table().unwrap().raw_len(), 0);

    // The actions themselves queue the model's commands.
    let queued = hud.host.take_campaign_requests();
    assert!(queued.is_empty(), "the target lists queue nothing: {queued:?}");
    call2("InstigateDuel", hud.char_addr(OWN_GENTLEMAN), hud.char_addr(FOREIGN_GENTLEMAN));
    call2("InstigateAssassination", hud.char_addr(RAKE), hud.char_addr(FOREIGN_GENTLEMAN));
    call2("SabotageArmy", hud.char_addr(RAKE), hud.force_addr(ntw_sim::campaign::ForceId(4242)));
    let queued = hud.host.take_campaign_requests();
    let cmds: Vec<CampaignCommand> = queued.into_iter().filter_map(|r| match r {
        CampaignRequest::Command(c) => Some(c),
        _ => None,
    }).collect();
    assert_eq!(
        cmds,
        vec![
            CampaignCommand::Duel { challenger: OWN_GENTLEMAN, target: FOREIGN_GENTLEMAN },
            CampaignCommand::Assassinate { agent: RAKE, target: FOREIGN_GENTLEMAN },
            // A force that does not exist: the command is queued anyway and the model refuses it.
            CampaignCommand::SabotageArmy { agent: RAKE, force: ntw_sim::campaign::ForceId(4242) },
        ]
    );
    // A target of the wrong kind queues nothing.
    call2("InstigateDuel", hud.char_addr(OWN_GENTLEMAN), hud.region_addr(REGION));
    assert!(hud.host.take_campaign_requests().is_empty());
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The agent action mask. `agent_options.Initialise` shows a button per bit and **divorces** the
/// ones whose bit is clear (`agent_options.lua:0` pc 45-56), so a bit that is not set is a
/// button that never appears -- which is why the five `MoveIntoTarget` actions are left out
/// rather than faked (see [`agent_options_mask`]).
#[test]
fn the_agent_options_mask_is_the_models_own_gates() {
    let hud = test_hud();
    // A rake who may assassinate, a foreign gentleman in the same residence who may duel, and a
    // French gentleman who may receive one -- the fixture of the call test above.
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
    hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[("can_duel", 1)]);
    hud.add_character(OWN_GENTLEMAN, A, CharacterKind::Gentleman, Some(REGION), &[("can_receive_duel", 1)]);
    let mask = |c: CharacterId| {
        let st = hud._scripts.state();
        let m = &st.model;
        agent_options_mask(m, c, m.world.characters[&c].faction)
    };
    use action_bit::*;
    // The rake has exactly one assassination candidate and no building to sabotage.
    assert_eq!(mask(RAKE), ASSASSINATE, "one known foreign target the model's gate accepts");
    // The foreign gentleman has one duel partner in his residence and nothing else.
    assert_eq!(mask(FOREIGN_GENTLEMAN), DUEL);
    // Our own gentleman is nobody's target: no bits at all, so the popup shows no button.
    assert_eq!(mask(OWN_GENTLEMAN), 0);
    // The five `MoveIntoTarget` bits and the dead `counterspy` bit are never set, whatever the
    // model says -- `MoveIntoTarget` is a logging stub, so a button for one of them would be a
    // button that does nothing.
    let never = VISIT | EMBED | RESEARCH | STEAL_RESEARCH | COUNTERSPY;
    for c in [RAKE, FOREIGN_GENTLEMAN, OWN_GENTLEMAN] {
        assert_eq!(mask(c) & never, 0, "the MoveIntoTarget actions stay out of the mask");
    }
}

/// The agents panel's three action buttons open the target picker, and an action with no valid
/// target says so in the log instead of doing nothing silently (CONFIRMED arity 1 for each call,
/// `ui/agents.luac`; the three lines it stands for, `agent_options.lua:109/118/128`).
///
/// The fixture root script does not define `OpenAgentActionPopup` (the real
/// `layout.root.luac` does, `layout.root.lua:1187`, and the install test
/// `an_agent_action_button_opens_the_target_picker` drives that), so here only the gate is
/// checked: with a valid target the call reaches the root, without one it logs.
#[test]
fn an_agent_action_button_reports_itself_when_there_is_no_target() {
    let hud = test_hud();
    // A rake alone in his own residence: no foreign character, so no assassination candidate.
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
    hud.host.take_log();
    let lua = hud.host.lua();
    for name in ["AgentRakeAssassinate", "AgentRakeSubterfuge", "AgentGentlemanDuel"] {
        let f: Function = lua
            .load(format!("return function(a) return CampaignUI.{name}(a) end"))
            .eval()
            .unwrap();
        f.call::<mlua::MultiValue>(hud.char_addr(RAKE)).unwrap();
    }
    let log = hud.host.take_log();
    for name in ["assassinate", "sabotage", "duel"] {
        assert!(log.iter().any(|l| l == &format!("agent action {name}: no valid target")), "{log:?}");
    }
    // The two remaining gaps say which they are, so a click is never a silent no-op.
    for name in ["AgentRogueSabotageArmy", "MoveIntoTarget"] {
        let f: Function = lua
            .load(format!("return function(...) return CampaignUI.{name}(...) end"))
            .eval()
            .unwrap();
        f.call::<mlua::MultiValue>(mlua::MultiValue::new()).unwrap();
    }
    let log = hud.host.take_log();
    assert!(log.iter().any(|l| l.contains("no army target list")), "{log:?}");
    assert!(log.iter().any(|l| l.contains("MoveIntoTarget is not implemented")), "{log:?}");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The address representation, which is the thing `agent_options.lua:34` reads
/// (`string.find(tostring(target), "CHARACTER")`, pc 2-9). CONFIRMED against the exe, so these
/// are the original's own rules rather than ours (`UI_FIDELITY.md` 9):
///
/// - an address is a **userdata with a metatable**, not light userdata, because it has to be able
///   to answer `__tostring`;
/// - `__tostring` is `sprintf("%s (0x0%x)", metatable.type, pointer)` (`0x01058F60`), and
///   `metatable.type` is the C++ signature string the exe's registration interns for its
///   `Lua::Pointer<T>` binding -- which for a character contains `CHARACTER`;
/// - `__eq` compares the wrapped pointers (`0x01058F20`), so `==` is **identity**. Lua tables
///   compare by reference, so the interning per `(tag, id)` is what makes `==` come out right:
///   the same entity must be the *same* table, and two different entities must not be.
#[test]
fn an_address_stringifies_with_the_originals_type_name_and_compares_by_identity() {
    let hud = test_hud();
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
    let lua = hud.host.lua();
    let fn_of = |src: &str| lua.load(src).eval::<Function>().unwrap();

    // `tostring` reaches the `__tostring` metamethod and prints the original's format, with the
    // exe's own type string in place 1 and `0x0` + the pointer in place 2.
    let printed: String = fn_of("return function(a) return tostring(a) end").call(hud.char_addr(RAKE)).unwrap();
    assert_eq!(
        printed,
        format!("{} (0x0{:x})", address_type_name(TAG_CHARACTER), TAG_CHARACTER | RAKE.0 as u32 as usize),
        "the original's __tostring format, verbatim"
    );
    assert!(printed.contains("CHARACTER"), "and that is what agent_options.lua:34 tests for: {printed}");
    assert!(printed.starts_with("class UTILITYDLL::LUA::State &__thiscall"), "the type name is the exe's own signature string: {printed}");

    // `==` is identity, in both directions: the same entity is the same table even across two
    // separate calls, and two entities are never the same.
    let same: bool = fn_of("return function(a) local b = a return a == b end").call::<bool>(hud.char_addr(RAKE)).unwrap();
    assert!(same, "one address compared with itself");
    let two_calls: bool =
        fn_of("return function(a, b) return a == b end").call::<bool>((hud.char_addr(RAKE), hud.char_addr(RAKE))).unwrap();
    assert!(two_calls, "two separate builds of the same entity's address are interned to one table");
    let different: bool =
        fn_of("return function(a, b) return a == b end").call::<bool>((hud.char_addr(RAKE), hud.char_addr(OWN_GENTLEMAN))).unwrap();
    assert!(!different, "two different characters are never the same address");
    let cross_kind: bool =
        fn_of("return function(a, b) return a == b end").call::<bool>((hud.char_addr(RAKE), hud.region_addr(REGION))).unwrap();
    assert!(!cross_kind, "a character address is not a region address");

    // And the round trip that all of the above exists for: the shipped type test now matches, so
    // `agent_options.Initialise` builds its `CampaignCharacter` handle. CONFIRMED the branch is
    // `string.find(tostring(target), "CHARACTER") and CampaignCharacter(target) or nil`
    // (pc 2-21), and the only reader of the handle is the popup's teardown, which calls
    // `Release()` (the proto at line 89).
    let handle_is_nil: bool =
        fn_of(r#"return function(a) return string.find(tostring(a), "CHARACTER") == nil end"#).call::<bool>(hud.char_addr(RAKE)).unwrap();
    assert!(!handle_is_nil, "agent_options.lua:34's type test now takes the character branch");
    let handle_is_nil: bool =
        fn_of(r#"return function(a) return string.find(tostring(a), "CHARACTER") == nil end"#).call::<bool>(hud.region_addr(REGION)).unwrap();
    assert!(handle_is_nil, "and a region address still takes the other branch, as in the original");
}

/// The interning itself, and that the payload every `CampaignUI.*` binding recovers the id from
/// still round trips -- including from a row's `Address` field, which is how the shipped row
/// templates hand an address back (`character_duel_info_pane.lua:52/57`).
#[test]
fn one_entity_keeps_the_same_address_object_and_its_payload_round_trips() {
    let hud = test_hud();
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
    let first = hud.char_addr(RAKE);
    let second = hud.char_addr(RAKE);
    assert!(
        matches!((&first, &second), (Value::Table(a), Value::Table(b)) if a == b),
        "interned: one table per (tag, id)"
    );
    assert_eq!(entity_of(&first, TAG_CHARACTER), Some(RAKE.0));
    let row: Table = hud.host.lua().create_table().unwrap();
    row.set("Address", first.clone()).unwrap();
    assert_eq!(entity_of(&row.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(RAKE.0), "the Address field round trips");
    assert_eq!(entity_of(&first, TAG_REGION), None, "and the tag still decides which kind it is");
}

/// Round 5's item 1, as a test: the agent action menu's target is a **character or a
/// settlement** and nothing else. The reading is CONFIRMED from the exe's own registration
/// document for `CampaignUI.MoveIntoTarget` (see [`AgentMenuTarget`]), so this pins the two
/// kinds and the refusal of everything else rather than a guess.
#[test]
fn the_agent_menu_target_is_a_character_or_a_settlement() {
    let hud = test_hud();
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
    let m = &hud._scripts.state().model;
    assert_eq!(menu_target_of(m, &hud.char_addr(RAKE)), Some(AgentMenuTarget::Character(RAKE)));
    assert_eq!(menu_target_of(m, &hud.region_addr(REGION)), Some(AgentMenuTarget::Settlement(REGION)));
    // Neither an unknown character nor a non-address is a target: the original's menu is opened
    // for a thing that is there, and each of the two engine call sites tests its target first.
    let ghost = entity_payload(TAG_CHARACTER, 4242);
    assert_eq!(menu_target_of(m, &ghost), None, "a character id that is not in the world");
    assert_eq!(menu_target_of(m, &Value::Table(hud.host.lua().create_table().unwrap())), None);
    assert_eq!(menu_target_of(m, &Value::Integer(1)), None);
    assert_eq!(menu_target_of(m, &Value::Nil), None);
    assert_eq!(menu_target_of(m, &hud.fort_addr(REGION)), None, "a fort is not one of the two documented kinds");
}

/// Round 5's item 2, as a test: the two percentages `agent_options.Initialise` puts on the
/// Infiltrate and Sabotage Army buttons are the model's own success chances -- `spy_chance` on the
/// target settlement and `army_sabotage_chance` on the target's force -- and not the two zeros the
/// engine call used to be handed. See [`agent_options_percentages`] for what is CONFIRMED here and
/// what is INFERRED (which force a target names).
#[test]
fn the_two_agent_menu_percentages_are_the_models_own_success_chances() {
    use ntw_sim::campaign::ForceId;
    use ntw_sim::campaign::agents::{SpyTarget, army_sabotage_chance, spy_chance};
    let hud = test_hud();
    // The rake is A's, in A's own settlement; a second settlement belongs to B, which is the only
    // kind of target either chance accepts (`spy_chance` refuses the agent's own faction).
    const FOREIGN_REGION: RegionId = RegionId(11);
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_sabotage_army", 1), ("can_spy", 1)]);
    {
        let mut st = hud._scripts.state_mut();
        let mut r = st.model.world.regions[&REGION].clone();
        r.id = FOREIGN_REGION;
        r.key = "test_region_11".into();
        r.settlement.key = "settlement:test_region_11:town".into();
        r.owner = B;
        r.settlement.position = (ntw_sim::fixed::Fixed20::from_int(140), ntw_sim::fixed::Fixed20::from_int(40));
        r.garrison = None;
        r.fleet = None;
        r.fortification = None;
        st.model.world.regions.insert(FOREIGN_REGION, r);
    }
    // A foreign army standing in that settlement, under a known colonel: the target a Sabotage
    // Army percentage is read off.
    let force = ForceId(5150);
    let colonel = CharacterId(140);
    hud.add_character(colonel, B, CharacterKind::Colonel, Some(FOREIGN_REGION), &[]);
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        let u = ntw_sim::campaign::CampaignUnit {
            id: ntw_sim::campaign::UnitId(80),
            unit_key: "test_unit".into(),
            men: 100,
            max_men: 100,
            character: Some(colonel),
            officer_name: Default::default(),
        };
        w.forces.insert(force, ntw_sim::campaign::MilitaryForce {
            id: force,
            faction: B,
            commander: Some(colonel),
            units: vec![u],
            is_navy: false,
        });
    }
    let m = &hud._scripts.state().model;
    let pcts = |t| agent_options_percentages(m, RAKE, A, t);

    // A settlement target: the Infiltrate percentage is the model's spying chance on it, and the
    // Sabotage Army percentage is that army's.
    let (infiltrate, sabotage_army) = pcts(Some(AgentMenuTarget::Settlement(FOREIGN_REGION)));
    assert_eq!(infiltrate, spy_chance(m, RAKE, SpyTarget::Settlement(FOREIGN_REGION)).expect("the rake may spy on it"));
    assert_eq!(sabotage_army, army_sabotage_chance(m, RAKE, force).expect("the rake may sabotage that army"));

    // A character target names no settlement, so there is no Infiltrate percentage to show -- and
    // naming the army's own commander does find the force.
    let (infiltrate_by_character, sabotage_by_commander) = pcts(Some(AgentMenuTarget::Character(colonel)));
    assert_eq!(infiltrate_by_character, 0);
    assert_eq!(sabotage_by_commander, sabotage_army);

    // No target: nothing to compute from, and nothing invented.
    assert_eq!(pcts(None), (0, 0));
}

/// Round 5's audit of the address representation: what the metatable does **not** carry, and the
/// one kind that was missing a name.
///
/// The original's address metatable is exactly `type` + `__tostring` + `__eq`, with **no
/// `__index`** (`0x0105AE10`'s raw listing, `UI_FIDELITY.md` 9.2). So an address is not a table of
/// methods: reading a field off one gives nil, and there is no metamethod to catch a script that
/// indexes one. Ours answers nil for the same reads, so every operation a shipped script performs
/// on an address gives the same answer -- the scripts only stringify one (`agent_options.lua:34`,
/// and `string.find` over that), and never index or iterate it. **PROVISIONAL difference:** a
/// script that indexed an address would *raise* in the original and read nil here; a script that
/// `pairs`'d one would raise there and walk the single payload field here. Neither happens in any
/// shipped `.luac`, and this records it rather than pretending it away.
///
/// The second half is the audit's one real find: `FactionDetails`' `Address` had no signature of
/// its own and stringified as the `void` stand-in. It now prints the exe's own
/// `EMPIRECAMPAIGN::FACTION` signature (`0x01371770`).
#[test]
fn an_address_has_no_index_and_a_faction_address_names_its_own_kind() {
    let hud = test_hud();
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
    let lua = hud.host.lua();
    // The shipped way to reach an address's kind: `Utilities.CreateCharacterCard` and friends do
    // `tostring(address)`; nothing reads a field off one, because there is no `__index` to read.
    let printed: String = lua
        .load("return function(a) return tostring(a) end")
        .eval::<Function>()
        .unwrap()
        .call(hud.char_addr(RAKE))
        .unwrap();
    assert!(printed.contains("EMPIRECAMPAIGN::CHARACTER"), "{printed}");
    assert!(
        !printed.contains("operator <<<void>"),
        "a kind we have a signature for never falls back to the void stand-in: {printed}"
    );
    // Reading an unknown field off an address is nil, not a method call: there is no `__index`.
    let fields_are_nil: bool = lua
        .load("return function(a) return a.Release == nil and a[1] == nil and a.Address ~= nil end")
        .eval::<Function>()
        .unwrap()
        .call(hud.char_addr(RAKE))
        .unwrap();
    assert!(fields_are_nil, "no __index on an address; only the payload field is readable");

    // `FactionDetails`' `Address`, read through the same `__tostring` the diplomacy panel would.
    let details: Table = lua.load("return CampaignUI.FactionDetails('test_faction_a')").eval().unwrap();
    let addr: Value = details.get("Address").expect("Address is a CONFIRMED field of FactionDetails");
    let printed: String =
        lua.load("return function(a) return tostring(a) end").eval::<Function>().unwrap().call(addr.clone()).unwrap();
    assert!(printed.contains("EMPIRECAMPAIGN::FACTION"), "0-E round 5: {printed}");
    assert_eq!(entity_of(&addr, TAG_FACTION), Some(A.0));
    assert_eq!(entity_of(&addr, TAG_FORCE), None, "and it is not a force address any more");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The commander pool panel: `CanRecruitCommander(force, is_navy)` and
/// `AvailableCommandersForRecruitment(force, is_navy)` (CONFIRMED arity 2,
/// `enlist_commander.lua:38` / `army.lua:708`), and `PromoteUnits(force, candidate)` queues the
/// hire (`enlist_commander.lua:106`).
#[test]
fn the_commander_pool_answers_from_the_model_and_hiring_queues_the_command() {
    use ntw_sim::campaign::ForceId;
    let hud = test_hud();
    // An army of the human faction under a colonel, and a General candidate in his pool.
    let force = ForceId(4242);
    {
        let mut st = hud._scripts.state_mut();
        let elapsed = st.model.calendar.turns_elapsed;
        let w = &mut st.model.world;
        if !w.factions.contains_key(&B) {
            let mut f = w.factions[&A].clone();
            f.id = B;
            f.key = OTHER.into();
            w.factions.insert(B, f);
        }
        w.faction_details.entry(A).or_default().capital = Some(REGION);
        w.characters.insert(CharacterId(120), ntw_sim::campaign::Character {
            id: CharacterId(120),
            faction: A,
            kind: CharacterKind::Colonel,
            position: w.regions[&REGION].settlement.position,
            movement_points: 10,
            max_movement_points: 10,
            base_movement_points: 10,
            garrisoned_in: Some(REGION),
        });
        let mut u = ntw_sim::campaign::CampaignUnit { id: UnitId(70), unit_key: "test_unit".into(), men: 100, max_men: 100, character: Some(CharacterId(120)), officer_name: Default::default() };
        u.men = 100;
        w.forces.insert(force, ntw_sim::campaign::MilitaryForce { id: force, faction: A, commander: Some(CharacterId(120)), units: vec![u], is_navy: false });
        let c = CharacterId(121);
        w.characters.insert(c, ntw_sim::campaign::Character {
            id: c,
            faction: A,
            kind: CharacterKind::General,
            position: w.regions[&REGION].settlement.position,
            movement_points: 10,
            max_movement_points: 10,
            base_movement_points: 10,
            garrisoned_in: None,
        });
        w.faction_details.get_mut(&A).unwrap().general_pool = (vec![c], elapsed + 2);
    }
    hud.host.take_log();
    let call2 = |name: &str, a: Value, b: Value| -> Value {
        let lua = hud.host.lua();
        let f: Function = lua.load(format!("return function(a, b) return CampaignUI.{name}(a, b) end")).eval().unwrap();
        f.call::<Value>((a, b)).unwrap()
    };
    let ask_gate = |f: ForceId, navy: bool| -> bool {
        let lua = hud.host.lua();
        let fun: Function = lua.load("return function(f, n) return CampaignUI.CanRecruitCommander(f, n) end").eval().unwrap();
        fun.call::<bool>((hud.force_addr(f), navy)).unwrap()
    };
    // The model's gate: the treasury is 1000 and the candidate is affordable. A campaign that
    // has not started yet lets the faction act (`may_act`), so it answers true.
    assert!(ask_gate(force, false));
    assert!(!ask_gate(ForceId(999), false), "no such force");
    {
        let mut st = hud._scripts.state_mut();
        st.model.turn.humans = vec![A];
        st.model.start_campaign();
    }
    assert!(ask_gate(force, false));
    // The second argument is redundant for us: the pool follows the force's own kind (an army
    // takes generals), so asking with `true` gives the same answer as with `false`.
    assert!(ask_gate(force, true));
    // Another faction's turn, and a purse too small for the candidate (the state borrow is
    // released before each question).
    {
        let mut st = hud._scripts.state_mut();
        st.model.turn.current = Some(B);
    }
    assert!(!ask_gate(force, false), "not A's turn");
    {
        let mut st = hud._scripts.state_mut();
        st.model.turn.current = Some(A);
        st.model.world.factions.get_mut(&A).unwrap().treasury = 1;
    }
    assert!(!ask_gate(force, false), "the treasury cannot pay");
    {
        let mut st = hud._scripts.state_mut();
        st.model.world.factions.get_mut(&A).unwrap().treasury = 1000;
    }

    let list = call2("AvailableCommandersForRecruitment", hud.force_addr(force), Value::Boolean(false));
    let t = list.as_table().expect("the pool table").clone();
    assert_eq!(t.raw_len(), 1, "one candidate");
    let row: Table = t.get(1).unwrap();
    assert_eq!(entity_of(&row.get::<Value>("commander_pointer").unwrap(), TAG_CHARACTER), Some(121));
    assert_eq!(row.get::<String>("Name").unwrap(), "General", "the agent type's on-screen name");
    assert!(row.get::<String>("RecruitmentCost").unwrap().parse::<i32>().unwrap() > 0);
    assert!(row.get::<bool>("IsRecruitable").unwrap(), "the treasury can pay");
    assert_eq!(t.get::<i32>("TurnsToNextPoolFill").unwrap(), 2);
    assert!(t.get::<f32>("MaxDistanceToTrack").unwrap() > 0.0);
    assert!(t.get::<i32>("MaxGeneralsAllowed").unwrap() > t.get::<i32>("CurrentNumGenerals").unwrap());
    // No force, no table.
    assert!(call2("AvailableCommandersForRecruitment", hud.force_addr(ForceId(999)), Value::Boolean(false)).is_nil());

    // The player confirms the candidate: the hire is queued.
    call2("PromoteUnits", hud.force_addr(force), hud.char_addr(CharacterId(121)));
    let cmds: Vec<CampaignCommand> = hud
        .host
        .take_campaign_requests()
        .into_iter()
        .filter_map(|r| match r {
            CampaignRequest::Command(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(cmds, vec![CampaignCommand::HireGeneral { character: CharacterId(121), into: Some(force) }]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// `CanPromoteUnit(card address)` (CONFIRMED arity 1, `army.lua:692`) and the unit row's
/// `PromotionCost` (CONFIRMED field, `army.lua:825`).
#[test]
fn the_promote_gate_and_price_come_from_the_model() {
    use ntw_sim::campaign::{ForceId, MilitaryForce};
    let hud = test_hud();
    let force = ForceId(4242);
    let unit = UnitId(70);
    {
        use ntw_sim::campaign::effects::SavedBonus;
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        w.faction_details.entry(A).or_default().capital = Some(REGION);
        w.characters.insert(CharacterId(120), ntw_sim::campaign::Character {
            id: CharacterId(120),
            faction: A,
            kind: CharacterKind::Colonel,
            position: w.regions[&REGION].settlement.position,
            movement_points: 10,
            max_movement_points: 10,
            base_movement_points: 10,
            garrisoned_in: Some(REGION),
        });
        w.forces.insert(force, MilitaryForce {
            id: force,
            faction: A,
            commander: Some(CharacterId(120)),
            units: vec![ntw_sim::campaign::CampaignUnit { id: unit, unit_key: "test_unit".into(), men: 100, max_men: 100, character: Some(CharacterId(120)), officer_name: Default::default() }],
            is_navy: false,
        });
        w.faction_details.get_mut(&A).unwrap().bonus_base = vec![SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new() }];
        st.model.turn.humans = vec![A];
        st.model.start_campaign();
    }
    hud.host.take_log();
    hud.host.campaign_select(CampaignSelection::Character(CharacterId(120)));
    let lua = hud.host.lua();
    let gate = |u: UnitId| -> bool {
        let f: Function = lua.load("return function(u) return CampaignUI.CanPromoteUnit(u) end").eval().unwrap();
        f.call::<bool>(hud.unit_addr(u)).unwrap()
    };
    assert!(gate(unit), "the faction may promote in the field and has no General yet");
    assert!(!gate(UnitId(999)), "no such unit");
    assert!(!lua.load("return CampaignUI.CanPromoteUnit(1)").eval::<bool>().unwrap(), "not a unit address");
    // The army panel's unit row carries the promotion price and what the player may know of it.
    hud.tabs();
    hud.host.campaign_select(CampaignSelection::Character(CharacterId(120)));
    hud.open_tab("army_tab");
    let info: Table = hud.root_env().get("army_panel").expect("GenerateArmyPanel ran");
    let units: Table = info.get::<Table>("units_info").unwrap().get("Units").unwrap();
    let row: Table = units.get(1).unwrap();
    assert!(row.get::<i32>("PromotionCost").unwrap() > 0, "the field promotion has a price");
    assert_eq!(row.get::<i32>("spying_data_level").unwrap(), LEVEL_OWNED);
    assert_eq!(row.get::<i32>("knowledge_mask").unwrap(), KNOWLEDGE_OWNED);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    // A **naval** promotion is free (CONFIRMED: the naval unit class's slot +0x44 is a return-0
    // stub, so `0x008E2260` pays `0x00BAF500(0, 2)`), and the row must not show a price for it:
    // `army.lua:825`'s `SelectedUnitsPromotionCost` sums the rows, so a non-zero value here
    // would put a price on the button for a promotion the model charges nothing for.
    let ship = UnitId(71);
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        w.characters.insert(
            CharacterId(121),
            ntw_sim::campaign::Character {
                id: CharacterId(121),
                faction: A,
                kind: CharacterKind::Captain,
                position: w.regions[&REGION].settlement.position,
                movement_points: 10,
                max_movement_points: 10,
                base_movement_points: 10,
                garrisoned_in: Some(REGION),
            },
        );
        w.forces.insert(
            ForceId(4243),
            ntw_sim::campaign::MilitaryForce {
                id: ForceId(4243),
                faction: A,
                commander: Some(CharacterId(121)),
                units: vec![ntw_sim::campaign::CampaignUnit { id: ship, unit_key: "test_ship".into(), men: 10, max_men: 10, character: Some(CharacterId(121)), officer_name: Default::default() }],
                is_navy: true,
            },
        );
        // The admiral-at-sea effect the naval gate needs (`promote_admiral_at_sea`, bonus 65); the
        // difficulty handicap (faction +0x8D4) would otherwise win over the base list.
        let d = w.faction_details.get_mut(&A).unwrap();
        d.bonus_with_difficulty = Vec::new();
        d.bonus_base.push(ntw_sim::campaign::effects::SavedBonus { kind: 1, bonus: 65, value: 1.0, qualifier: String::new() });
    }
    assert!(hud.host.lua().load("return function(u) return CampaignUI.CanPromoteUnit(u) end").eval::<Function>().unwrap().call::<bool>(hud.unit_addr(ship)).unwrap());
    hud.host.campaign_select(CampaignSelection::Character(CharacterId(121)));
    hud.open_tab("navy_tab");
    let info: Table = hud.root_env().get("army_panel").expect("GenerateNavyPanel ran");
    // A navy's rows are under `Ships` (CONFIRMED: `force_info` fills `Units` or `Ships`).
    let ships: Table = info.get::<Table>("units_info").unwrap().get("Ships").unwrap();
    let row: Table = ships.get(1).unwrap();
    assert_eq!(row.get::<i32>("PromotionCost").unwrap(), 0, "a naval promotion is free: no price on the row");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The fog of war as the interface sees it: `SpyingDataLevelCharacter` /
/// `SpyingDataLevelUnit` (CONFIRMED arity 1 and the `>= ADVANCED` gate the root's double click
/// uses, `layout.root.lua:1093`), the levels `utilities.lua` defines (CONFIRMED values) and the
/// model's own knowledge.
#[test]
fn a_unit_of_the_players_own_force_with_no_commander_is_owned() {
    let hud = test_hud();
    let unit = UnitId(81);
    {
        let mut st = hud._scripts.state_mut();
        let force = ntw_sim::campaign::ForceId(5151);
        st.model.world.forces.insert(force, ntw_sim::campaign::MilitaryForce {
            id: force,
            faction: A,
            commander: None,
            units: vec![ntw_sim::campaign::CampaignUnit { id: unit, unit_key: "test_unit".into(), men: 10, max_men: 10, character: None, officer_name: Default::default() }],
            is_navy: false,
        });
    }
    let m = &hud._scripts.state().model;
    assert_eq!(spying_level_unit(m, A, unit), LEVEL_OWNED, "own force, no commander");
    assert_eq!(spying_level_unit(m, B, unit), LEVEL_PASSIVE, "a foreign commanderless force is unchanged");
}

#[test]
fn the_spying_data_levels_follow_the_models_sight() {
    let hud = test_hud();
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
    hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[]);
    hud.host.take_log();
    let level = |name: &str, a: Value| -> i32 {
        let lua = hud.host.lua();
        let f: Function = lua.load(format!("return function(a) return CampaignUI.{name}(a) end")).eval().unwrap();
        f.call::<i32>(a).unwrap()
    };
    // One's own character is OWNED; the foreign one stands in the settlement the player sees, so
    // it is at least ADVANCED. No shroud in the test model: `sees` is true everywhere.
    assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(RAKE)), LEVEL_OWNED);
    assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(FOREIGN_GENTLEMAN)), LEVEL_ADVANCED);
    // A character the faction does not know about (a hidden spy) is only PASSIVE.
    {
        use ntw_sim::campaign::visibility::{CellSet, Shroud, SightGrid};
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        let ch = w.characters.get_mut(&FOREIGN_GENTLEMAN).unwrap();
        ch.position = (ntw_sim::fixed::Fixed20::from_int(900), ntw_sim::fixed::Fixed20::from_int(900));
        // A shroud that has never seen anything: the foreign character is unknown.
        let grid = SightGrid::centred(8, 8, 8);
        w.sight_grid = Some(grid);
        let visible = {
            let mut s = CellSet::new(8, 8);
            let cells: Vec<(u32, u32)> = w
                .characters
                .values()
                .filter(|c| c.faction == A)
                .flat_map(|c| grid.disc((c.position.0.to_f32(), c.position.1.to_f32()), 1.0))
                .collect();
            for (x, z) in cells {
                s.set(x, z);
            }
            s
        };
        w.shrouds.insert(A, Shroud { explored: visible.clone(), visible, hidden: CellSet::new(8, 8), active: true });
    }
    assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(RAKE)), LEVEL_OWNED);
    // Known, but under the shroud: only the basic level (its card may not be opened).
    assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(FOREIGN_GENTLEMAN)), LEVEL_BASIC);
    // Hidden: the faction does not know him at all, so even the list leaves him out.
    hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().hidden = true;
    assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(FOREIGN_GENTLEMAN)), LEVEL_PASSIVE);
    hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().hidden = false;
    // The unit of a force the player has never seen is unknown as well.
    {
        use ntw_sim::campaign::{ForceId, MilitaryForce};
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        w.forces.insert(ForceId(77), MilitaryForce {
            id: ForceId(77),
            faction: B,
            commander: Some(FOREIGN_GENTLEMAN),
            units: vec![ntw_sim::campaign::CampaignUnit { id: UnitId(78), unit_key: "test_unit".into(), men: 10, max_men: 10, character: None, officer_name: Default::default() }],
            is_navy: false,
        });
    }
    assert_eq!(level("SpyingDataLevelUnit", hud.unit_addr(UnitId(78))), LEVEL_BASIC);
    hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().hidden = true;
    assert_eq!(level("SpyingDataLevelUnit", hud.unit_addr(UnitId(78))), LEVEL_PASSIVE);
    // Not a character / unit address at all.
    assert_eq!(level("SpyingDataLevelCharacter", hud.region_addr(REGION)), LEVEL_INVALID);
    assert_eq!(level("SpyingDataLevelUnit", Value::Integer(1)), LEVEL_INVALID);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}
/// The `Attributes` table of a character card ([`attributes_table`]): the primary attribute is
/// the character type's main attribute (`agents::main_attribute`, the exe's `0x00A198C0`), not
/// the highest one; `PrimaryLevel` is his rank + 1, at most 9 (`0x009AE759..0x009AE768`); a
/// `PLACEHOLDER` picture row gives the empty path; the pips keep their own pictures. The
/// `agent_attributes` rows are MADE UP (the fixture has none).
#[test]
fn character_cards_show_the_main_attribute_and_its_rank() {
    use ntw_data::characters::AgentAttributeRecord;
    let mut db = GameDatabase::test_fixture();
    db.campaign.characters.attributes = ntw_data::Table::from_rows(
        0,
        vec![
            AgentAttributeRecord { key: "research".into(), icon: "PLACEHOLDER".into() },
            AgentAttributeRecord { key: "duelling_pistols".into(), icon: "made/up/pistols.tga".into() },
            AgentAttributeRecord { key: "subterfuge".into(), icon: "made/up/spying.tga".into() },
        ],
    );
    let hud = test_hud_with(db);
    hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
    hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[("can_duel", 1)]);
    let attributes = |target: CharacterId| -> Table {
        let lua = hud.host.lua();
        let f: Function = lua.load("return function(a, b) return CampaignUI.RequestAssassinationTargets(a, b) end").eval().unwrap();
        let rows: Table = f.call((hud.char_addr(RAKE), hud.char_addr(target))).unwrap();
        let row: Table = rows.get(1).expect("the gentleman is a target");
        row.get("Attributes").unwrap()
    };
    let set_attributes = |list: Vec<(String, i32)>| {
        hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().attributes = list;
    };

    // Pistols is his highest attribute, but a gentleman's main attribute is research: the
    // primary is research (rank 3 -> level 4), whose row is `PLACEHOLDER` -> the empty path.
    set_attributes(vec![("duelling_pistols".into(), 6), ("research".into(), 3)]);
    let a = attributes(FOREIGN_GENTLEMAN);
    assert_eq!(a.get::<String>("PrimaryAttributeName").unwrap(), "research");
    assert_eq!(a.get::<i64>("PrimaryLevel").unwrap(), 4);
    assert_eq!(a.get::<String>("PrimaryAttributePath").unwrap(), "", "a PLACEHOLDER row names no file");
    let pip: Table = a.get(1).unwrap();
    assert_eq!(pip.get::<String>("PipPath").unwrap(), "made/up/pistols.tga");
    assert_eq!(pip.get::<i64>("Value").unwrap(), 6);

    // A tie (and the old fallback) cannot pick another attribute: still research.
    set_attributes(vec![("duelling_pistols".into(), 3), ("research".into(), 3)]);
    assert_eq!(attributes(FOREIGN_GENTLEMAN).get::<String>("PrimaryAttributeName").unwrap(), "research");
    // Rank 9 -> level 10, clamped to 9.
    set_attributes(vec![("research".into(), 9)]);
    assert_eq!(attributes(FOREIGN_GENTLEMAN).get::<i64>("PrimaryLevel").unwrap(), 9);
    // No attribute list at all: still research (not `command_land`), rank -1 -> level 0.
    set_attributes(Vec::new());
    let a = attributes(FOREIGN_GENTLEMAN);
    assert_eq!(a.get::<String>("PrimaryAttributeName").unwrap(), "research");
    assert_eq!(a.get::<i64>("PrimaryLevel").unwrap(), 0);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// Polish: `CampaignUi::addresses` kept the address of every queue item ever shown. A finished
/// or cancelled item's address is dropped at the next panel build; a queued item keeps its own
/// table (identity).
#[test]
fn finished_queue_items_leave_the_address_store() {
    let hud = test_hud();
    let item = |id: i32| ntw_sim::campaign::RecruitmentItem { id: RecruitmentItemId(id), unit_key: "x".into(), turns_remaining: 1, cost: 0 };
    hud._scripts.state_mut().model.world.regions.get_mut(&REGION).unwrap().recruitment_queue = vec![item(500), item(501)];
    let ui = hud.host.campaign_ui().unwrap();
    let build = || recruitment_info(hud.host.lua(), hud.host.inner(), &ui, REGION, false).unwrap();
    let held = |id: i32| ui.addresses.borrow().get(&(TAG_QUEUE_ITEM, id)).cloned();
    build();
    let kept = held(501).expect("shown, so interned");
    assert!(held(500).is_some());
    hud._scripts.state_mut().model.world.regions.get_mut(&REGION).unwrap().recruitment_queue.remove(0);
    build();
    assert!(held(500).is_none(), "the finished item's address is gone");
    assert_eq!(held(501), Some(kept), "the queued item keeps its own table");
}

/// The human (A) in a negotiation with a MADE-UP second faction B (treasury 300) that A
/// proposed, the counterpart set as the "started" event leaves it, and a 500 lump sum offered.
fn negotiation_with_offer(hud: &TestHud) {
    use ntw_sim::campaign::{Faction, GovernmentType};
    hud._scripts.state_mut().model.world.factions.insert(
        B,
        Faction {
            id: B,
            key: "test_faction_b".into(),
            treasury: 300,
            government: GovernmentType::AbsoluteMonarchy,
            government_key: String::new(),
            tax_lower: "tax_normal".into(),
            tax_upper: "tax_normal".into(),
            diplomacy: Default::default(),
        },
    );
    // The negotiation begun by its command, and the "started" event delivered (the counterpart set,
    // the counts seen).
    hud._scripts.state_mut().model.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    let n = hud._scripts.state().model.negotiations.clone();
    let ui = hud.host.campaign_ui().unwrap();
    let mut state = ui.negotiation.borrow_mut();
    (state.seen_begun, state.seen_ended) = (n.begun, n.ended);
    state.target = n.current.map(|c| c.serial);
    state.offers.push(DealRow { item: NegotiationItem::Payment { amount: 500, turns: 0 }, applied: false });
}

/// A deal applies at most once (the appliers skip a row marked applied, +0x1E8): proposing
/// twice, or proposing and then accepting, sends the deal's commands once; each leaves the
/// result "accepted" (+0x28 = 1, `Finished()`).
#[test]
fn a_deal_applies_at_most_once() {
    let gift = vec![CampaignRequest::Command(CampaignCommand::Diplomacy { a: A, b: B, action: D::StateGift(500) })];
    for script in ["CampaignUI.Propose() CampaignUI.Propose()", "CampaignUI.ProposeDeal() CampaignUI.AcceptOffer()"] {
        let hud = test_hud();
        negotiation_with_offer(&hud);
        assert_eq!(hud.requests_of(script), gift, "{script}");
        let finished: Option<bool> = hud.host.lua().load("return CampaignUI.Finished()").eval().unwrap();
        assert_eq!(finished, Some(true), "{script}");
        assert!(hud.errors().is_empty(), "{script}");
    }
    // Cancel empties the deal (CLEAR): a new deal applies again.
    let hud = test_hud();
    negotiation_with_offer(&hud);
    assert_eq!(hud.requests_of("CampaignUI.AcceptOffer()"), gift);
    assert_eq!(hud.requests_of("CampaignUI.Cancel() CampaignUI.AcceptOffer()"), Vec::new(), "the cancelled deal is empty");
    hud.host.campaign_ui().unwrap().negotiation.borrow_mut().offers.push(DealRow { item: NegotiationItem::Payment { amount: 500, turns: 0 }, applied: false });
    assert_eq!(hud.requests_of("CampaignUI.AcceptOffer()"), gift, "a new deal after Cancel");
    // Without a counterpart nothing is applied.
    hud.host.campaign_ui().unwrap().negotiation.borrow_mut().release();
    assert_eq!(hud.requests_of("CampaignUI.AcceptOffer() CampaignUI.Propose()"), Vec::new());
}

/// The payment caps push the treasury (0x00BCAFE0, the HUD's "funds"): MaxPlayer the local
/// player's, with no guard (also after `End()`); MaxOpposition the side that is not the local
/// player's, 0 and one log line without a counterpart (the exe would read a null pointer).
#[test]
fn payment_caps_are_the_treasuries() {
    let hud = test_hud();
    negotiation_with_offer(&hud);
    let caps = |hud: &TestHud| -> (f64, f64) {
        hud.host.lua().load("return CampaignUI.MaxPlayerPaymentAllowed(), CampaignUI.MaxOppositionPaymentAllowed()").eval().unwrap()
    };
    assert_eq!(caps(&hud), (1000.0, 300.0));
    assert!(hud.errors().is_empty());
    hud.host.campaign_ui().unwrap().negotiation.borrow_mut().release();
    assert_eq!(caps(&hud), (1000.0, 0.0));
    assert_eq!(caps(&hud), (1000.0, 0.0));
    let errors = hud.errors();
    assert_eq!(errors.len(), 1, "logged once: {errors:?}");
    assert!(errors[0].contains("MaxOppositionPaymentAllowed"), "{errors:?}");
}

/// The per-frame negotiation sync: nothing is due while the model's counts stay as seen; counts
/// below the seen ones (a different model) post nothing and are logged once.
#[test]
fn negotiation_sync_posts_only_on_a_count_change() {
    let hud = test_hud();
    negotiation_with_offer(&hud);
    hud.host.take_log();
    for t in [0.0, 8.0] {
        hud.host.pulse(t);
    }
    let ui = hud.host.campaign_ui().unwrap();
    assert_eq!(ui.negotiation.borrow().offers.len(), 1, "no change: the deal is kept");
    assert!(hud.host.take_log().is_empty());
    ui.negotiation.borrow_mut().seen_begun = 99;
    for t in [16.0, 24.0] {
        hud.host.pulse(t);
    }
    let log = hud.host.take_log();
    assert_eq!(log.iter().filter(|l| l.contains("negotiation counts went back")).count(), 1, "{log:?}");
    assert_eq!(ui.negotiation.borrow().target, None);
}

/// `InitialiseRegionInfoDetails(region)` answers the region info table (`0x009E69B0` →
/// `0x009AF570`), whose `Name` region_details.lua puts in the panel's title. Bug: the call was an
/// UNKNOWN stub, so the title kept the layout's " XXX Details".
#[test]
fn region_info_details_carry_the_region_name_for_the_title() {
    let hud = test_hud();
    let lua = hud.host.lua();
    lua.globals().set("r", hud.region_addr(REGION)).unwrap();
    let t: Table = lua.load("return CampaignUI.InitialiseRegionInfoDetails(r)").eval().unwrap();
    // The fixture has no loc table: the name falls back to the key, as every region table's.
    assert_eq!(t.get::<String>("Name").unwrap(), "test_region_10");
    assert!(lua.load("return CampaignUI.InitialiseRegionInfoDetails(r).Address == r").eval::<bool>().unwrap());
    assert_eq!(t.get::<String>("OwningFactionKey").unwrap(), HUMAN);
    assert_eq!(t.get::<i64>("PopulationNumber").unwrap(), 1000);
    assert_eq!(t.get::<String>("Population").unwrap(), "1000", "\"%d\" of the population (0x009FB8F0)");
    assert_eq!(t.get::<String>("Settlement").unwrap(), "test_region_10");
    assert!(hud.errors().is_empty(), "the call is bound, not an UNKNOWN stub");
    let none: Value = lua.load("return CampaignUI.InitialiseRegionInfoDetails()").eval().unwrap();
    assert!(none.is_nil());
}
