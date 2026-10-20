//! Tests of the diplomacy screen's protectorate stances ([`super::diplomacy`]): the exe's stance
//! table `0x0145D378` names 3 "patron" (the patron's record about its protectorate) and 4
//! "protectorate" (the protectorate's record about its patron).

use ntw_formats::loc::{LocEntry, LocFile, Localisation};
use ntw_sim::campaign::Stance;
use ntw_sim::campaign::negotiation::{TRADE_TIP_ROUTE_COUNT, TRADE_TIP_ROUTE_SHIFTED, TRADE_TIP_SEA_ROUTES, TradeAgreementTooltip};

use super::tests::{A, HudSetup, TestHud, test_hud_setup};
use super::*;

const B: FactionId = FactionId(2);
const C: FactionId = FactionId(3);

/// A HUD with A (the player), B and C in the game, C displayed as "Cland", and the treaty strings.
fn hud() -> TestHud {
    let mut loc = Localisation::new();
    let entries = [
        ("protectorate_of_player", "Your protectorate"),
        ("protectorate_of_non_player", "Protectorate of"),
        ("alliance", "Alliance"),
        ("trade_agreement", "Trade agreement"),
    ];
    loc.add(&LocFile {
        version: 1,
        entries: entries
            .iter()
            .map(|(k, t)| LocEntry { key: format!("diplomacy_strings_string_current_treaty_{k}"), text: (*t).into(), flag: false })
            .collect(),
    });
    let hud = test_hud_setup(GameDatabase::test_fixture(), HudSetup { loc, ..HudSetup::default() });
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        for (id, key) in [(B, "test_faction_b"), (C, "test_faction_c")] {
            let mut f = w.factions[&A].clone();
            f.id = id;
            f.key = key.into();
            w.factions.insert(id, f);
            w.faction_details.entry(id).or_default().out_of_game = Some(false);
        }
        w.faction_details.entry(C).or_default().display_name = "Cland".into();
    }
    hud
}

fn eval<T: mlua::FromLuaMulti>(hud: &TestHud, code: &str) -> T {
    hud.host.lua().load(code).eval().unwrap()
}

/// `RetrieveDiplomacyDetails` (0x009B2690): a patron (its record says 3, `IsStancePatron`) lists its
/// protectorate under "Protectorates", the protectorate (4, `IsStanceProtectorate`) its patron under
/// "ProtectorOf". Bug: the two lists were swapped. Factions out of the game (+0x824) are left out.
#[test]
fn diplomacy_details_list_protectorates_under_their_patron() {
    let hud = hud();
    hud._scripts.state_mut().model.world.set_stance(A, B, Stance::Patron).unwrap();
    let has = |faction: &str, list: &str, other: &str| -> bool {
        eval(&hud, &format!("return CampaignUI.RetrieveDiplomacyDetails('{faction}').{list}.{other} ~= nil"))
    };
    assert!(has("test_faction_a", "Protectorates", "test_faction_b"), "A is B's patron");
    assert!(!has("test_faction_a", "ProtectorOf", "test_faction_b"));
    assert!(has("test_faction_b", "ProtectorOf", "test_faction_a"), "B is A's protectorate");
    assert!(!has("test_faction_b", "Protectorates", "test_faction_a"));
    {
        let mut st = hud._scripts.state_mut();
        st.model.world.set_stance(A, C, Stance::War).unwrap();
        st.model.world.faction_details.entry(C).or_default().out_of_game = Some(true);
    }
    assert!(!has("test_faction_a", "AtWar", "test_faction_c"), "C is out of the game");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// `RetrieveDiplomacyDetails(f)`'s TradeRights reads `f`'s own record about the other faction
/// (`+0x788` on the `[f + 0x528]` entry, 0x009B2869): a flag on one side only lists the other
/// faction for that side alone.
#[test]
fn diplomacy_details_trade_rights_read_the_faction_s_own_record() {
    let hud = hud();
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        w.set_stance(A, B, Stance::Neutral).unwrap();
        w.relationships.entry((A, B)).or_default().trade_agreement = true;
        w.relationships.entry((B, A)).or_default().trade_agreement = false;
    }
    let has = |faction: &str, other: &str| -> bool {
        eval(&hud, &format!("return CampaignUI.RetrieveDiplomacyDetails('{faction}').TradeRights.{other} ~= nil"))
    };
    assert!(has("test_faction_a", "test_faction_b"), "A's record holds the agreement");
    assert!(!has("test_faction_b", "test_faction_a"), "B's record does not");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// `RetrieveRemainingMilitaryAccessTurns` (0x009F4380): -1 for the record whose stance is 4
/// (`IsStanceProtectorate` 0x00B5DCC0), the protectorate's about its patron; the patron's own
/// record keeps its turns. Bug: the -1 went to the patron's record.
#[test]
fn a_protectorate_gives_its_patron_access_for_good() {
    let hud = hud();
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        w.set_stance(B, A, Stance::Protectorate).unwrap();
        w.set_stance(A, B, Stance::Patron).unwrap();
        w.relationships.entry((A, B)).or_default().military_access_turns = 5;
        w.relationships.entry((B, A)).or_default().military_access_turns = 0;
    }
    let turns: (i32, i32) = eval(&hud, "return CampaignUI.RetrieveRemainingMilitaryAccessTurns('test_faction_b', 'test_faction_a')");
    assert_eq!(turns, (-1, 5), "B (the protectorate) gives A for good; A gives B 5 turns");
    let turns: (i32, i32) = eval(&hud, "return CampaignUI.RetrieveRemainingMilitaryAccessTurns('test_faction_a', 'test_faction_b')");
    assert_eq!(turns, (5, -1));
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// `RetrieveExistingTreaties(a, b)` (0x00B750D0 over b's record about a): "protectorate_of_player"
/// when b is a's protectorate (stance 4); else one "protectorate_of_non_player <name>" line for b's
/// patron, named by its display name (faction +0x518). The trade line reads b's record (+0x788).
#[test]
fn existing_treaties_name_the_protectorate_ties() {
    let hud = hud();
    let treaties = |a: &str, b: &str| -> String { eval(&hud, &format!("return CampaignUI.RetrieveExistingTreaties('{a}', '{b}')")) };
    hud._scripts.state_mut().model.world.set_stance(B, A, Stance::Protectorate).unwrap();
    assert_eq!(treaties("test_faction_a", "test_faction_b"), "Your protectorate", "B is the player's protectorate");
    assert_eq!(treaties("test_faction_b", "test_faction_a"), "", "A (the patron) is no one's protectorate");
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        w.set_stance(B, A, Stance::Allied).unwrap();
        w.set_stance(B, C, Stance::Protectorate).unwrap();
        w.relationships.entry((B, A)).or_default().trade_agreement = true;
        w.relationships.entry((A, B)).or_default().trade_agreement = false;
    }
    assert_eq!(treaties("test_faction_a", "test_faction_b"), "Protectorate of Cland\n\nAlliance\n\nTrade agreement");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The exe's record holds its patron's faction pointer (record +4 at `0x00B75255`), so the patron
/// always resolves; ours keys by id, and a patron id with no faction (a broken model) leaves its
/// line out and is logged once, never named "".
#[test]
fn a_patron_missing_from_the_world_is_logged_once_and_left_out() {
    let hud = hud();
    let treaties = || -> String { eval(&hud, "return CampaignUI.RetrieveExistingTreaties('test_faction_a', 'test_faction_b')") };
    hud._scripts.state_mut().model.world.factions.get_mut(&B).unwrap().diplomacy.insert(FactionId(99), Stance::Protectorate);
    assert_eq!(treaties(), "");
    assert_eq!(treaties(), "");
    let errors = hud.errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("patron"), "{errors:?}");
}

/// `RetrieveFactionListForDiplomacy`'s TradingTooltip: the `_can` / `_cannot` text followed by the trade
/// agreement explanation (`0x00BC7910(player, faction, 0)` at `0x009F35CC`, no counts); a faction in
/// the campaign's `diplomacy_hidden_factions` (the exe's britain in "episodic_3", `0x009F2F77`) is left
/// out.
#[test]
fn the_diplomacy_trade_tooltip_explains_and_hidden_factions_are_left_out() {
    let mut loc = Localisation::new();
    let entries = [("trade_status_tooltip_cannot", "Cannot trade. "), ("trade_agreement_tooltip_no_route_you", "No route.")];
    loc.add(&LocFile {
        version: 1,
        entries: entries
            .iter()
            .map(|(k, t)| LocEntry { key: format!("random_localisation_strings_string_{k}"), text: (*t).into(), flag: false })
            .collect(),
    });
    let hud = test_hud_setup(GameDatabase::test_fixture(), HudSetup { loc, ..HudSetup::default() });
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        for (id, key) in [(B, "test_faction_b"), (C, "test_faction_c")] {
            let mut f = w.factions[&A].clone();
            f.id = id;
            f.key = key.into();
            w.factions.insert(id, f);
            w.faction_details.entry(id).or_default().out_of_game = Some(false);
        }
        for (f, cap) in [(A, 1), (B, 2)] {
            w.faction_details.entry(f).or_default().capital = Some(ntw_sim::campaign::RegionId(cap));
        }
        // B can reach A, A cannot reach B: "no route you", and no agreement is possible.
        let rules = std::sync::Arc::make_mut(&mut st.model.rules);
        let maker: ntw_sim::seam::Maker<ntw_sim::campaign::seams::TradeRouteCheck> =
            std::sync::Arc::new(|_| std::sync::Arc::new(|_, a, _| if a == B { 1 } else { 0 }));
        rules.seams.trade_route_check.apply("test_mod", &maker);
        rules.features.diplomacy_hidden_factions = vec!["test_faction_c".into()];
    }
    let (tip, c_listed): (String, bool) = eval(
        &hud,
        "local t = CampaignUI.RetrieveFactionListForDiplomacy() return t.test_faction_b.TradingTooltip, t.test_faction_c ~= nil",
    );
    assert_eq!(tip, "Cannot trade. No route.");
    assert!(!c_listed, "C is hidden by the campaign's features");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The negotiation panel's route counts line (`0x00446C80`): each `%d` in turn (the shipped text).
#[test]
fn the_route_counts_fill_each_number_in_turn() {
    assert_eq!(fill_counts("You: %d/%d\nThem: %d/%d", [1, 3, 0, 2]), "You: 1/3\nThem: 0/2");
}

/// The trade agreement text (`0x00BC7910`): the reason, then the route counts line with each `%d`
/// filled (the negotiation panel's `counts`), then the route-shifted line (route check codes 4 / 6).
#[test]
fn the_trade_tooltip_adds_the_counts_and_shifted_lines() {
    let mut loc = Localisation::new();
    let entries = [
        (TRADE_TIP_SEA_ROUTES, "By sea. "),
        (TRADE_TIP_ROUTE_COUNT, "You: %d/%d, them: %d/%d. "),
        (TRADE_TIP_ROUTE_SHIFTED, "A route moves over land."),
    ];
    loc.add(&LocFile {
        version: 1,
        entries: entries
            .iter()
            .map(|(k, t)| LocEntry { key: format!("random_localisation_strings_string_{k}"), text: (*t).into(), flag: false })
            .collect(),
    });
    let hud = test_hud_setup(GameDatabase::test_fixture(), HudSetup { loc, ..HudSetup::default() });
    let text = |sea_routes, route_shifted| {
        let tip = TradeAgreementTooltip { reason: TRADE_TIP_SEA_ROUTES, sea_routes, route_shifted };
        trade_agreement_text(hud.host.inner(), Some(tip))
    };
    assert_eq!(text(None, false), "By sea. ");
    assert_eq!(text(Some([1, 3, 0, 2]), false), "By sea. You: 1/3, them: 0/2. ");
    assert_eq!(text(None, true), "By sea. A route moves over land.");
    assert_eq!(text(Some([2, 2, 1, 4]), true), "By sea. You: 2/2, them: 1/4. A route moves over land.");
    assert_eq!(trade_agreement_text(hud.host.inner(), None), "", "a side without a capital");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// `BuildPossibleActions` (`0x009B5220`) sets an entry's `Tooltip` only for a non-empty text (the
/// UniString length test `0x004F3500` at `0x009B58B4`): the trade button of a side without a capital
/// (the empty trade text) has none. Bug: it was set to "".
#[test]
fn an_empty_trade_tooltip_is_left_unset() {
    let hud = hud();
    hud._scripts.state_mut().model.apply(ntw_sim::campaign::CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    {
        let n = hud._scripts.state().model.negotiations.clone();
        let ui = hud.host.campaign_ui().unwrap();
        let mut state = ui.negotiation.borrow_mut();
        (state.seen_begun, state.seen_ended) = (n.begun, n.ended);
        state.target = n.current.map(|c| c.serial);
    }
    for f in [A, B] {
        hud._scripts.state_mut().model.world.faction_details.entry(f).or_default().capital = None;
    }
    let (listed, tooltip): (bool, Option<String>) = eval(
        &hud,
        "for _, e in ipairs(CampaignUI.BuildPossibleActions().Unilaterals) do if e.State == 'trade' then return true, e.Tooltip end end return false, nil",
    );
    assert!(listed, "the trade button is listed (not at war)");
    assert_eq!(tooltip, None, "no Tooltip for the empty text");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The number of values each call returns: `select('#', ...)` counts a returned nil, so 0 means the
/// call returned nothing.
fn result_counts(hud: &TestHud, args: &str) -> (i64, i64, i64) {
    let (one, two) = args.split_once(';').unwrap_or((args, args));
    eval(
        hud,
        &format!(
            "return select('#', CampaignUI.RetrieveDiplomacyDetails({one})), \
             select('#', CampaignUI.RetrieveExistingTreaties({two})), \
             select('#', CampaignUI.RetrieveRemainingMilitaryAccessTurns({two}))"
        ),
    )
}

/// `RetrieveDiplomacyDetails` (0x009F2750), `RetrieveExistingTreaties` (0x009F2B80) and
/// `RetrieveRemainingMilitaryAccessTurns` (0x009F4380) return nothing at all (`return 0`) unless
/// every key is in the faction registry (`FindCampaignFactionByKey` 0x00955B70), which never holds
/// the rebels (filled from the turn list, 0x009516A0) and finds nothing for an empty key. Bug: ours
/// returned nil (or two nils), found the rebels by their empty key, and gave the player's details
/// for a call without a key.
#[test]
fn diplomacy_reads_return_nothing_for_the_rebels_and_unknown_factions() {
    const REBELS: FactionId = FactionId(4);
    let hud = hud();
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        let mut f = w.factions[&A].clone();
        f.id = REBELS;
        f.key = String::new();
        w.factions.insert(REBELS, f);
    }
    assert_eq!(result_counts(&hud, "'test_faction_a'; 'test_faction_a', 'test_faction_b'"), (1, 1, 2), "real factions");
    assert_eq!(result_counts(&hud, "''; 'test_faction_a', ''"), (0, 0, 0), "the rebels");
    assert_eq!(result_counts(&hud, "''; '', 'test_faction_a'"), (0, 0, 0), "the rebels first");
    assert_eq!(result_counts(&hud, "'nobody'; 'test_faction_a', 'nobody'"), (0, 0, 0), "an unknown faction");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The exe's string reader (0x01055760) takes a string or a number; anything else is logged as a
/// script error (0x01095090, which returns: no Lua error) and the key stays empty, so the call
/// returns nothing. It reads from the top of the stack and stops at the first fault, so only the
/// last bad argument is reported; a missing one reads the called function below the arguments
/// (0x00FE8B50, 0x00FECED0). Each distinct fault is logged once. Bug: a call without a key gave the
/// player's details, a nil key to the two-faction calls raised a Lua error, and the first error of
/// a call hid every other one.
#[test]
fn a_missing_faction_key_returns_nothing_and_is_logged_once() {
    let hud = hud();
    assert_eq!(result_counts(&hud, "; 'test_faction_a'"), (0, 0, 0), "no key, one key");
    let errors = hud.errors();
    assert_eq!(errors.len(), 3, "one line per call: {errors:?}");
    assert!(errors.iter().all(|e| e.contains("Trying to retrieve type string, found type function (argument none)")), "{errors:?}");
    assert_eq!(result_counts(&hud, "; 'test_faction_a'"), (0, 0, 0), "the same faults");
    assert!(hud.errors().is_empty(), "logged once: {:?}", hud.errors());
    // The last argument is read first: the nil is reported, the table before it never read.
    assert_eq!(result_counts(&hud, "true; {}, nil"), (0, 0, 0), "a boolean, a table and a nil");
    let errors = hud.errors();
    assert_eq!(errors.len(), 3, "a new fault of each call: {errors:?}");
    assert!(errors[0].contains("RetrieveDiplomacyDetails: Trying to retrieve type string, found type boolean (argument 1)"), "{errors:?}");
    assert!(errors[1..].iter().all(|e| e.contains("found type nil (argument 2)")), "{errors:?}");
    assert_eq!(result_counts(&hud, "nil; nil, 'test_faction_a'"), (0, 0, 0), "nil keys");
    let errors = hud.errors();
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors[0].contains("found type nil (argument 1)") && errors[1..].iter().all(|e| e.contains("found type nil (argument 1)")), "{errors:?}");
    // A number is read as its string: no faction has the key "1", and nothing is logged.
    assert_eq!(result_counts(&hud, "1; 'test_faction_a', 2"), (0, 0, 0), "number keys");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// The reader takes the top of the stack, so the keys are the last arguments and any before them
/// are never read (0x009F2750, 0x009F2B80, 0x009F4380). Bug: ours read the first ones.
#[test]
fn faction_keys_are_the_last_arguments() {
    let hud = hud();
    {
        let mut st = hud._scripts.state_mut();
        let w = &mut st.model.world;
        w.set_stance(A, B, Stance::Patron).unwrap();
        w.relationships.entry((A, B)).or_default().military_access_turns = 5;
        w.relationships.entry((B, A)).or_default().military_access_turns = 2;
    }
    let extra = "'nobody', 'test_faction_a'; {}, 'test_faction_a', 'test_faction_b'";
    assert_eq!(result_counts(&hud, extra), (1, 1, 2), "an unknown key and a table before the keys");
    let a_lists: bool = eval(&hud, "return CampaignUI.RetrieveDiplomacyDetails('test_faction_b', 'test_faction_a').Protectorates.test_faction_b ~= nil");
    assert!(a_lists, "the last argument: A's details, A the patron of B");
    let turns: (i32, i32) = eval(&hud, "return CampaignUI.RetrieveRemainingMilitaryAccessTurns('test_faction_b', 'test_faction_a', 'test_faction_b')");
    assert_eq!(turns, (5, -1), "a = the next-to-last argument (A gives B 5 turns), b = the last (B, the protectorate, gives A access for good)");
    assert_eq!(result_counts(&hud, "'test_faction_a', 'nobody'; 'test_faction_a', 'test_faction_b', 'nobody'"), (0, 0, 0), "an unknown last key");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}
