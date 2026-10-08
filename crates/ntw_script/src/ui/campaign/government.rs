//! The government screen: governorships, ministers, prestige, missions and trade.

use super::*;

/// `CampaignUI.RetrieveGovernorshipDetails(key)` (0x009F41D0 → 0x009BC8B0 and the finance summary
/// 0x009BB910, CONFIRMED key names) → `{Governor, UpperTaxRate, LowerTaxRate, TaxIncomeUpper,
/// TaxIncomeLower, UpperTaxEffects, LowerTaxEffects, UpperEffects, LowerEffects, Theatre,
/// LowestPublicOrder = {Name, Value, Address, IsUpper}, AutomanageTaxes, AutomanageConstruction,
/// and the finance fields}` for the player's governorship. Tax rates are the `taxes_levels` index
/// (0 minimal .. 4 extortionate) of the faction's current levels; the effect lists are {Icon, Tooltip}
/// entries (government_screens.lua AddTaxEffects). PROVISIONAL: one governorship per
/// theatre (the key is not checked), the class incomes halve the tax total, the effect
/// lists empty, automanage off.
fn governorship_details(lua: &Lua, inner: &Inner, ui: &CampaignUi) -> mlua::Result<Value> {
    use ntw_sim::campaign::details::TAX_LEVELS;
    let m = ui.model();
    let Some(f) = m.faction_by_key(&ui.link.human) else { return Ok(Value::Nil) };
    let fid = f.id;
    let index = |k: &str| TAX_LEVELS.iter().position(|l| *l == k).unwrap_or(2);
    let (upper, lower) = (index(&f.tax_upper), index(&f.tax_lower));
    let income = ntw_sim::campaign::economy::faction_income(&m, fid);
    let governor = m.world.faction_details.get(&fid).and_then(|d| d.posts.iter().find(|p| p.governorship.is_some()).and_then(|p| p.holder));
    // The lowest public order of the governed regions (the worse class).
    let lowest = m
        .world
        .regions
        .values()
        .filter(|r| r.owner == fid)
        .map(|r| {
            let po = ntw_sim::campaign::economy::public_order(&m, r.id);
            (r.id, r.key.clone(), po.worst(), po.upper < po.lower)
        })
        .min_by(|a, b| a.2.total_cmp(&b.2));
    drop(m);
    let t = lua.create_table()?;
    if let Some(c) = governor {
        t.set("Governor", character_details(lua, inner, ui, c)?)?;
    }
    t.set("UpperTaxRate", upper)?;
    t.set("LowerTaxRate", lower)?;
    t.set("TaxIncomeUpper", income.taxes / 2)?;
    t.set("TaxIncomeLower", income.taxes - income.taxes / 2)?;
    t.set("UpperTaxEffects", lua.create_table()?)?;
    t.set("LowerTaxEffects", lua.create_table()?)?;
    t.set("UpperEffects", lua.create_table()?)?;
    t.set("LowerEffects", lua.create_table()?)?;
    t.set("Theatre", ui.theatre(inner).map(|r| theatre_name(inner, &r)))?;
    if let Some((id, key, value, is_upper)) = lowest {
        let l = lua.create_table()?;
        l.set("Name", region_name(&inner.loc, &key))?;
        l.set("Value", value.round() as i32)?;
        l.set("Address", region_value(ui, id))?;
        l.set("IsUpper", is_upper)?;
        t.set("LowestPublicOrder", l)?;
    }
    t.set("AutomanageTaxes", false)?;
    t.set("AutomanageConstruction", false)?;
    t.set("TaxIncomeTotal", income.taxes)?;
    t.set("FactionTaxIncomeUpper", income.taxes / 2)?;
    t.set("FactionTaxIncomeLower", income.taxes - income.taxes / 2)?;
    t.set("Trade", income.trade)?;
    t.set("ArmyUpkeep", income.upkeep)?;
    t.set("NavyUpkeep", 0)?;
    t.set("Policing", 0)?;
    t.set("OtherIncome", income.other)?;
    t.set("OtherIncomeTooltip", "")?;
    t.set("AnnualIncome", income.revenue() - income.upkeep)?;
    Ok(Value::Table(t))
}


pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // PROVISIONAL governorship: one per theatre, keyed by the radar's theatre id ("europe").
    f!("GovernorshipList", |lua, inner, ui, _a: Variadic<Value>| {
        let (_, id) = theatre_of(&ui.link.campaign);
        let key = ui.theatre(&inner).map(|r| r.id).unwrap_or_default();
        let e = lua.create_table()?;
        e.set("Key", id)?;
        e.set("TheatreKey", key)?;
        e.set("Name", loc(&inner, &format!("governorships_onscreen_{id}")).unwrap_or_else(|| id.to_owned()))?;
        let t = lua.create_table()?;
        t.set(1, e)?;
        Ok(t)
    });
    // InitialiseGovernmentDetails() → the government screen's details (0x009E54D0, CONFIRMED
    // names): {DatabaseKey (government_types key), Government, Religion, ReligionIcon, Home
    // ("%S, %S": the capital's settlement and region), Treasury, Population, Prosperity, Prestige,
    // HadElectionInThisTurn, ...} and, from 0x009B1D40, ElectedMinisters (bool), Ministers
    // {<post key> = character details, FactionLeader = character details} and MinisterPool.
    // PROVISIONAL: Prosperity = summed region GDP, Prestige 0, Popularity / NextElections /
    // Governorships not given, the minister pool empty, ElectedMinisters false.
    f!("InitialiseGovernmentDetails", |lua, inner, ui, _a: Variadic<Value>| {
        let (gov, treasury, pop, gdp, capital, religion, posts, leader) = {
            let m = ui.model();
            let Some(f) = m.faction_by_key(&ui.link.human) else { return Ok(Value::Nil) };
            let owned: Vec<&ntw_sim::campaign::Region> = m.world.regions.values().filter(|r| r.owner == f.id).collect();
            let d = m.world.faction_details.get(&f.id);
            (
                f.government_key.clone(),
                f.treasury,
                owned.iter().map(|r| r.population).sum::<u32>(),
                owned.iter().map(|r| r.gdp).sum::<u32>(),
                m.world.capital(f.id).or_else(|| owned.first().map(|r| r.id)),
                d.map(|d| d.religion.clone()).unwrap_or_default(),
                d.map(|d| d.posts.iter().filter(|p| p.key != "faction_leader").filter_map(|p| Some((p.key.clone(), p.holder?))).collect::<Vec<_>>())
                    .unwrap_or_default(),
                m.world.faction_leader(f.id),
            )
        };
        let t = lua.create_table()?;
        t.set("DatabaseKey", gov.as_str())?;
        t.set("Name", faction_name(&inner, &ui.link.db, &ui.link.human))?;
        t.set("Government", loc(&inner, &format!("government_types_onscreen_{gov}")).unwrap_or_else(|| gov.clone()))?;
        t.set("Religion", loc(&inner, &format!("religions_onscreen_{religion}")).unwrap_or_else(|| religion.clone()))?;
        t.set("ReligionIcon", ui.religion_icon(&inner, &religion))?;
        if let Some(c) = capital {
            let key = ui.model().world.regions.get(&c).map(|r| r.key.clone()).unwrap_or_default();
            t.set("Home", format!("{}, {}", ui.settlement_name(&inner, c), region_name(&inner.loc, &key)))?;
        }
        t.set("Treasury", treasury)?;
        t.set("Population", pop)?;
        t.set("Prosperity", gdp)?;
        t.set("Prestige", 0)?;
        // Ministers are keyed by the post's `ministerial_positions` number (CONFIRMED: the record's
        // +0xC int is the key; head_of_government 1, finance 2, justice 3, army 4, navy 5,
        // governors 6.., government_screens.lua reads Ministers[1..5]).
        let numbers: HashMap<String, i32> = small_table(&inner, "db/ministerial_positions_tables/ministerial_positions", "s,i")
            .into_iter()
            .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(1)?.as_i32()?)))
            .collect();
        let ministers = lua.create_table()?;
        for (post, c) in posts {
            let Some(n) = numbers.get(&post) else { continue };
            ministers.set(*n, character_details(lua, &inner, &ui, c)?)?;
        }
        if let Some(c) = leader {
            ministers.set("FactionLeader", character_details(lua, &inner, &ui, c)?)?;
        }
        t.set("Ministers", ministers)?;
        t.set("MinisterPool", lua.create_table()?)?;
        t.set("ElectedMinisters", false)?;
        t.set("HadElectionInThisTurn", false)?;
        // Popularity "%d %%" and PopularityStatus "up" / "down" / "falling_fast" (0x008DEA60 /
        // 0x008DEB10, CONFIRMED shape: clamp(clamp(a + b, -20, 20) + round(mean of two values) + 10,
        // 0, 100); the status compares it with the last stored value, -10 or worse = falling_fast).
        // PROVISIONAL: the inputs are not decoded; we use the capital's public order (mean of the
        // two classes) as the mean term and 0 for a + b, and the status is always "up";
        // NextElections 0.
        let popularity = capital.map_or(50, |c| {
            let po = ntw_sim::campaign::economy::public_order(&ui.model(), c);
            (((po.lower + po.upper) / 2.0).round() as i32 + 10).clamp(0, 100)
        });
        t.set("Popularity", format!("{popularity} %"))?;
        t.set("PopularityStatus", "up")?;
        t.set("NextElections", 0)?;
        // The finance summary on the same table (0x009BB910, CONFIRMED names): TaxIncomeTotal,
        // AnnualIncome, FactionTaxIncomeUpper / Lower, Trade, ArmyUpkeep, NavyUpkeep, Policing,
        // OtherIncome, OtherIncomeTooltip; values from the model's income (`faction_income`).
        // PROVISIONAL: the upkeep is all shown as army upkeep, policing 0, the class split of the
        // taxes halves the total, AnnualIncome = revenue - upkeep.
        let income = {
            let m = ui.model();
            m.faction_by_key(&ui.link.human).map(|f| ntw_sim::campaign::economy::faction_income(&m, f.id)).unwrap_or_default()
        };
        t.set("TaxIncomeTotal", income.taxes)?;
        t.set("FactionTaxIncomeUpper", income.taxes / 2)?;
        t.set("FactionTaxIncomeLower", income.taxes - income.taxes / 2)?;
        t.set("Trade", income.trade)?;
        t.set("ArmyUpkeep", income.upkeep)?;
        t.set("NavyUpkeep", 0)?;
        t.set("Policing", 0)?;
        t.set("OtherIncome", income.other)?;
        t.set("OtherIncomeTooltip", "")?;
        t.set("AnnualIncome", income.revenue() - income.upkeep)?;
        Ok(Value::Table(t))
    });
    // PrestigeDetails() → {factions = {[i] = {name, key, flag_path, faction_colour {r, g, b},
    // is_ally, is_enemy, is_neighbouring, history = {[j] = {enlightenment, military, naval,
    // economics, overall}}}}, scale} (0x009EF290 → 0x009A5470, CONFIRMED keys). PROVISIONAL:
    // prestige is not in the model: the major powers are listed with one all-zero history entry,
    // scale 1; is_neighbouring false.
    f!("PrestigeDetails", |lua, inner, ui, _a: Variadic<Value>| {
        use ntw_sim::campaign::Stance;
        let m = ui.model();
        let me = m.faction_by_key(&ui.link.human).map(|f| f.id);
        let rows: Vec<(String, Stance)> = m
            .world
            .factions
            .values()
            .filter(|f| m.world.faction_details.get(&f.id).and_then(|d| d.major).unwrap_or(false))
            .map(|f| {
                let stance = me.and_then(|me| m.world.factions.get(&me)?.diplomacy.get(&f.id).copied()).unwrap_or_default();
                (f.key.clone(), stance)
            })
            .collect();
        drop(m);
        let factions = lua.create_table()?;
        for (i, (key, stance)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("name", faction_name(&inner, &ui.link.db, key))?;
            e.set("key", key.as_str())?;
            let rec = ui.link.db.faction(key);
            e.set("flag_path", rec.map(|r| r.flag_path.clone()).unwrap_or_default())?;
            let [r, g, b] = rec.map_or([128, 128, 128], |r| r.primary_colour());
            let c = lua.create_table()?;
            c.set("r", r)?;
            c.set("g", g)?;
            c.set("b", b)?;
            e.set("faction_colour", c)?;
            e.set("is_ally", *stance == Stance::Allied)?;
            e.set("is_enemy", *stance == Stance::War)?;
            e.set("is_neighbouring", false)?;
            let h = lua.create_table()?;
            let entry = lua.create_table()?;
            for k in ["enlightenment", "military", "naval", "economics", "overall"] {
                entry.set(k, 0)?;
            }
            h.set(1, entry)?;
            e.set("history", h)?;
            factions.set(i + 1, e)?;
        }
        let t = lua.create_table()?;
        t.set("factions", factions)?;
        t.set("scale", 1)?;
        Ok(Value::Table(t))
    });
    // MissionsDetails() → {active_missions = {...}, expired_missions = {}} for the player
    // (0x009EDA20 → 0x009B3BD0, CONFIRMED keys). Each active mission: Title, Activity (""),
    // Description, Issuer (""), Reward, Penalty ("No penalty"), Objective, RemainingTime, Year,
    // and location {X, Y} / Location when the mission has a place. The exe copies texts the
    // mission keeps (+0xA0 title, +0xAC description, +0xB8 objective, +0xD0 reward); ours come
    // from the loc the scripts' mission keys use: `mission_text_text_<key>_heading` / `_text`
    // (CONFIRMED keys exist), the objective `mission_activities_description_<activity>` (kind →
    // activity INFERRED), the reward the money as text (PROVISIONAL). RemainingTime = turns -
    // elapsed (INFERRED); the location is the target settlement's position.
    f!("MissionsDetails", |lua, inner, ui, _a: Variadic<Value>| {
        const ACTIVITY: [&str; 16] = [
            "capture_city",
            "protectorate_region_capture",
            "build",
            "recruit",
            "make_alliance",
            "capture_fort",
            "engage_faction",
            "blockade_port",
            "spy_on_city",
            "research",
            "assassination",
            "make_trade_agreement",
            "engage_faction",
            "engage_faction",
            "capture_city",
            "spy_on_city",
        ];
        let m = ui.model();
        let Some(me) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(Value::Nil) };
        let year = m.calendar.date.year;
        let rows: Vec<(ntw_sim::campaign::details::CampaignMission, Option<(f32, f32)>)> = m
            .world
            .missions
            .get(&me)
            .map(|list| {
                list.iter()
                    .map(|mi| {
                        let pos = mi
                            .settlement
                            .as_ref()
                            .or(mi.region.as_ref())
                            .and_then(|t| t.found())
                            .and_then(|r| m.world.regions.get(r))
                            .map(|r| (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32()));
                        (mi.clone(), pos)
                    })
                    .collect()
            })
            .unwrap_or_default();
        drop(m);
        let active = lua.create_table()?;
        for (i, (mi, pos)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            let text = |suffix: &str| loc(&inner, &format!("mission_text_text_{}_{suffix}", mi.script_key));
            e.set("Title", text("heading").unwrap_or_else(|| mi.script_key.clone()))?;
            e.set("Activity", "")?;
            e.set("Description", text("text").unwrap_or_default())?;
            e.set("Issuer", "")?;
            e.set("Reward", if mi.reward_money > 0 { mi.reward_money.to_string() } else { String::new() })?;
            e.set("Penalty", "No penalty")?;
            let activity = ACTIVITY.get(mi.kind as usize).copied().unwrap_or("capture_city");
            e.set("Objective", loc(&inner, &format!("mission_activities_description_{activity}")).unwrap_or_default())?;
            e.set("RemainingTime", mi.turns.saturating_sub(mi.elapsed))?;
            e.set("Year", year)?;
            if let Some((x, y)) = pos {
                let l = lua.create_table()?;
                l.set("X", *x)?;
                l.set("Y", *y)?;
                e.set("location", l.clone())?;
                e.set("Location", l)?;
            }
            active.set(i + 1, e)?;
        }
        let t = lua.create_table()?;
        t.set("active_missions", active)?;
        t.set("expired_missions", lua.create_table()?)?;
        Ok(Value::Table(t))
    });
    // RegionsOwnedByFactionOrByProtectorates(faction) → a sequence of {Address, Name,
    // OwnedByProtectorate}: the faction's regions, then its protectorates' (0x009F0DA0, CONFIRMED
    // keys; the protectorates are the factions it has stance 4 with, our Patron, INFERRED).
    f!("RegionsOwnedByFactionOrByProtectorates", |lua, inner, ui, key: Option<String>| {
        let m = ui.model();
        let Some(f) = m.faction_by_key(&key.unwrap_or_else(|| ui.link.human.clone())) else { return Ok(Value::Nil) };
        let protectorates: Vec<_> =
            f.diplomacy.iter().filter(|(_, s)| **s == ntw_sim::campaign::Stance::Patron).map(|(id, _)| *id).collect();
        let mut rows: Vec<(RegionId, String, bool)> =
            m.world.regions.values().filter(|r| r.owner == f.id).map(|r| (r.id, r.key.clone(), false)).collect();
        rows.extend(m.world.regions.values().filter(|r| protectorates.contains(&r.owner)).map(|r| (r.id, r.key.clone(), true)));
        drop(m);
        let t = lua.create_table()?;
        for (i, (id, key, prot)) in rows.into_iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Address", region_value(&ui, id))?;
            e.set("Name", region_name(&inner.loc, &key))?;
            e.set("OwnedByProtectorate", prot)?;
            t.set(i + 1, e)?;
        }
        Ok(Value::Table(t))
    });
    // TradeInfo() → {prices, price_changes, supply, export} for the player (0x009F9F00: table
    // "trade_info_table"; routes from 0x00A299D0 with key, name, type, value, pips = {pip = {type,
    // tooltip, icon}}; prices / price_changes per commodity from 0x00A28FB0; CONFIRMED names).
    // government_screens.lua lists `supply` as imports and `export` as exports, `type` indexing
    // {Sea, Land, Blockaded, SeaRaided, Piracy, LandRaided, Banditry}. PROVISIONAL: every trade
    // partner (`economy::trade_partners`) is listed in both lists as a sea route worth the pair's
    // trade value, without pips; commodity prices are not given.
    f!("TradeInfo", |lua, inner, ui, _a: Variadic<Value>| {
        let m = ui.model();
        let Some(me) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(Value::Nil) };
        let partners: Vec<(String, i32)> = ntw_sim::campaign::economy::trade_partners(&m, me)
            .into_iter()
            .filter_map(|p| Some((m.world.factions.get(&p)?.key.clone(), ntw_sim::campaign::economy::trade_pair_value(&m, me, p))))
            .collect();
        drop(m);
        let list = || -> mlua::Result<Table> {
            let t = lua.create_table()?;
            for (i, (key, value)) in partners.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("key", key.as_str())?;
                e.set("name", faction_name(&inner, &ui.link.db, key))?;
                e.set("type", 1)?;
                e.set("value", *value)?;
                e.set("pips", lua.create_table()?)?;
                t.set(i + 1, e)?;
            }
            Ok(t)
        };
        let t = lua.create_table()?;
        t.set("prices", lua.create_table()?)?;
        t.set("price_changes", lua.create_table()?)?;
        t.set("supply", list()?)?;
        t.set("export", list()?)?;
        Ok(Value::Table(t))
    });
    // RetrieveGovernorshipDetails(key) → see `governorship_details`.
    f!("RetrieveGovernorshipDetails", |lua, inner, ui, _key: Value| governorship_details(lua, &inner, &ui));
    // SetGovernorshipTaxRate(governorship, upper, rate): the tax slider (CONFIRMED call shape in
    // government_screens.lua: false = lower classes, true = upper). INFERRED: `rate` is the
    // `taxes_levels` index 0..4; it becomes the model's SetTaxLevel for the player's faction.
    f!("SetGovernorshipTaxRate", |_l, inner, ui, (_g, upper, rate): (Value, bool, f64)| {
        let m = ui.model();
        let Some(faction) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(()) };
        drop(m);
        let i = rate.round().clamp(0.0, 4.0) as usize;
        let class = if upper { ntw_sim::campaign::rules::TaxClass::Upper } else { ntw_sim::campaign::rules::TaxClass::Lower };
        let level = ntw_sim::campaign::details::TAX_LEVELS[i].to_owned();
        ui.push(CampaignRequest::Command(CampaignCommand::SetTaxLevel { faction, class, level }));
        Ok(())
    });
    Ok(())
}
