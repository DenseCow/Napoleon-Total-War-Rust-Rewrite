//! The region details panel's table: `CampaignUI.InitialiseRegionInfoDetails(region)` (handler
//! `0x009E69B0`), which starts from the region info table (`0x009AF570`). Every value comes from the
//! campaign model ([`economy`], [`population`]); nothing here computes a game rule.
//!
//! CONFIRMED layout (`0x009AF570`, then `0x009E69B0`; key strings at `0x0136BC08`..`0x0136E94C` and
//! `0x0136CA20`..`0x0136CBC4`):
//! - Address; Settlement (settlement virtual `+0x30`); Name (`0x00A8D5D0`, region `+0x29C`);
//!   Theatre; OwningFactionKey; PopulationNumber (int) and Population (`"%d"` of it, `0x009FB8F0`);
//!   PopulationChange (int); UpperOrder, LowerOrder (ints, replaced below by tables); Wealth (int,
//!   region `+0xCC` town wealth + `+0xBC` GDP); WealthChange (int, region `+0xC4`); UpperTax,
//!   LowerTax (floats, the effective rates `0x00AB55E0` / `0x00A8C360`, 0 for the rebels);
//!   ReligionKey, ReligionIcon (the region's strongest religion, the first of equals, `0x00A8C430`);
//!   Taxed (bool, not tax exempt); ActiveUpperClass, ActiveLowerClass (the government's class keys,
//!   `0x00A975F0`); with a governorship (region `+0x248`) UpperTaxPercentage, LowerTaxPercentage (the
//!   levels' rates × 0.01), BuildingPercentage (`0x00BC7540`), GovernorPercentage (`0x00BC7580`),
//!   TechnologyPercentage (`0x00BC75F0`) and AdministrationCostPercentage (`0x00BC73C0`); NextTownName
//!   and TurnsUntilNextTown when a next town exists (`0x00A673A0`).
//! - Then Governor (the governorship's character, `BuildCharacterDetailsInfoTable`), Effects
//!   (`0x009ACD10`: {Icon, Tooltip} per effect of region `+0x1CC`), the pip tables UpperOrder,
//!   LowerOrder, PopulationGrowth, RegionWealth and TownWealth, ReligiousBreakdown and TaxExempt.
//! - A pip table holds Total, PipValue, IsPercentage and its factors as the array part (the pip
//!   group's `init_pips` walks it with `ipairs`): {Pip, Total, Predicted, Tooltip, PredictedTooltip,
//!   EqualPipPredictedTooltip, Positive}, RegionWealth's factors also Lost and LostTooltip.

use super::*;
use ntw_sim::campaign::economy::{self, ClassPublicOrder};
use ntw_sim::campaign::population;

/// The happiness slots' factor keys, in slot order (`0x008C9110` → `0x014579A4`, CONFIRMED).
const HAPPINESS_FACTORS: [&str; 13] = [
    "happiness_gov_type",
    "happiness_tax_rate",
    "happiness_religion",
    "happiness_events",
    "happiness_culture",
    "happiness_industrialisation",
    "happiness_characters",
    "happiness_war",
    "happiness_reform",
    "happiness_bankruptcy",
    "happiness_resistance",
    "happiness_gentlemen",
    "happiness_alignment",
];

/// The repression slots' factor keys (`0x008EA460` → `0x014579D8`, CONFIRMED).
const REPRESSION_FACTORS: [&str; 6] = [
    "repression_gov_type",
    "repression_buildings",
    "repression_characters",
    "repression_policing",
    "repression_garrison",
    "repression_military_crackdown",
];

/// The garrison's repression slot: its predicted value names what the prediction assumes
/// (`0x008E02E0`: `random_localisation_strings` #0x1E0 / #0x1E1).
const GARRISON_SLOT: usize = 4;

/// Replaces the first `%S` of a factor's text with its value text, as `0x00453EB0` formats the
/// `public_order_factors` texts (CONFIRMED: each shipped text holds one `%S`).
fn format_s(fmt: &str, arg: &str) -> String {
    match fmt.find("%S") {
        Some(i) => format!("{}{arg}{}", &fmt[..i], &fmt[i + 2..]),
        None => fmt.to_owned(),
    }
}

/// The engine's `%f` (`0x004F16F0` → `0x004F3140`): six decimals (`_fcvt_s`), then up to five
/// trailing zeros dropped ("0.300000" → "0.3", "1.000000" → "1.0").
fn ca_float(x: f32) -> String {
    let mut s = format!("{x:.6}");
    for _ in 0..5 {
        if s.ends_with('0') {
            s.pop();
        } else {
            break;
        }
    }
    s
}

/// How a factor's values read (the handler's two kinds of pip table).
#[derive(Clone, Copy)]
enum FactorUnits {
    /// Whole points (public order, town wealth): `"%d"`, `"%d (%S)"`, `"%d / %d (%S)"`
    /// (`0x009E6D79`..`0x009E6FCE`).
    Points,
    /// Hundredths of a percent shown as percentages (population growth, `0x00A88EF0`): `"%f%%"`,
    /// `"%f%% (%S)"`, `"%f%% / %f%% (%S)"` (`0x009E7FB9`..`0x009E8545`).
    Percent,
}

/// One pip-table factor (`0x00887960`, CONFIRMED): Positive unless the value is not above 0 and the
/// value or the prediction is negative; Total is the value's magnitude and Predicted the prediction's
/// magnitude minus the value's (`0x009E7064`, `0x009E862B`); Pip is the `public_order_factors` picture of
/// that sign; the tooltips are the sign's text (`public_order_factors_{positive,negative}_tooltip_<key>`,
/// the record's `+0x24` / `+0x28`) with the value, the prediction and both (see [`FactorUnits`]), the `%S`
/// being `extra`. `None` when both are 0 (the handler skips it). A factor without a text has empty
/// tooltips.
fn factor_entry(lua: &Lua, inner: &Inner, ui: &CampaignUi, key: &str, (value, predicted): (i32, i32), extra: &str, units: FactorUnits) -> mlua::Result<Option<Table>> {
    let positive = !((value < 0 || predicted < 0) && value < 1);
    let sign = if positive { "positive" } else { "negative" };
    let pip = ui.order_factor_pip(inner, key, positive);
    let text = loc(inner, &format!("public_order_factors_{sign}_tooltip_{key}"));
    pip_entry(lua, (value, predicted), positive, pip, text.as_deref(), extra, units)
}

/// A pip-table factor's fields from its sign, picture and text (see [`factor_entry`]). `None` when
/// both values are 0.
fn pip_entry(lua: &Lua, (value, predicted): (i32, i32), positive: bool, pip: Option<String>, text: Option<&str>, extra: &str, units: FactorUnits) -> mlua::Result<Option<Table>> {
    let (total, pred) = (value.unsigned_abs(), predicted.unsigned_abs());
    if total == 0 && pred == 0 {
        return Ok(None);
    }
    let tip = |arg: String| text.map_or_else(String::new, |t| format_s(t, &arg));
    let change = pred.wrapping_sub(total) as i32;
    let e = lua.create_table()?;
    e.set("Pip", pip)?;
    match units {
        FactorUnits::Points => {
            e.set("Total", total)?;
            e.set("Predicted", change)?;
            e.set("Tooltip", tip(total.to_string()))?;
            e.set("PredictedTooltip", tip(format!("{pred} ({extra})")))?;
            e.set("EqualPipPredictedTooltip", tip(format!("{total} / {pred} ({extra})")))?;
        }
        FactorUnits::Percent => {
            let pct = |n: i64| (f64::from(0.01f32) * n as f64) as f32;
            let (cur, next) = (pct(i64::from(total)), pct(i64::from(pred)));
            e.set("Total", cur)?;
            e.set("Predicted", pct(i64::from(change)))?;
            e.set("Tooltip", tip(format!("{}%", ca_float(cur))))?;
            e.set("PredictedTooltip", tip(format!("{}% ({extra})", ca_float(next))))?;
            e.set("EqualPipPredictedTooltip", tip(format!("{}% / {}% ({extra})", ca_float(cur), ca_float(next))))?;
        }
    }
    e.set("Positive", positive)?;
    Ok(Some(e))
}

/// A class's public order pip table (`0x00A98F50` / `0x00A98EA0` → `0x008E02E0`, CONFIRMED): Total
/// is the class's public order, PipValue 1, not a percentage; the 13 happiness factors then the 6
/// repression factors, those with a value now or predicted; each factor's prediction is the same
/// class's slot in the projection (`predicted`, `0x00A727D0`).
fn order_pips(lua: &Lua, inner: &Inner, ui: &CampaignUi, class: Option<&ClassPublicOrder>, predicted: Option<&ClassPublicOrder>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("Total", class.map_or(0, ClassPublicOrder::total))?;
    t.set("PipValue", 1)?;
    t.set("IsPercentage", false)?;
    let Some(c) = class else { return Ok(t) };
    let empty = ClassPublicOrder::default();
    let p = predicted.unwrap_or(&empty);
    let predicted_word = loc_required(inner, "random_localisation_strings_string_predicted");
    let mut n = 0;
    let slots = HAPPINESS_FACTORS.iter().zip(c.happiness.iter().zip(p.happiness)).map(|(k, (v, q))| (*k, *v, q, false));
    let repression = REPRESSION_FACTORS.iter().zip(c.repression.iter().zip(p.repression)).enumerate();
    let slots = slots.chain(repression.map(|(i, (k, (v, q)))| (*k, *v, q, i == GARRISON_SLOT)));
    for (key, value, predicted, garrison) in slots {
        let extra = if garrison {
            // 0x1E1 when the prediction is not above the current value, else 0x1E0.
            let k = if predicted <= value { "predicted_with_units_removed" } else { "predicted_with_units_added" };
            loc_required(inner, &format!("random_localisation_strings_string_{k}"))
        } else {
            predicted_word.clone()
        };
        if let Some(e) = factor_entry(lua, inner, ui, key, (value, predicted), &extra, FactorUnits::Points)? {
            n += 1;
            t.set(n, e)?;
        }
    }
    Ok(t)
}

/// The town wealth pip table (`0x00A995A0`, CONFIRMED): Total the region's growth (+0xD8), PipValue 50,
/// not a percentage; per `economy::TOWN_WEALTH_FACTORS` slot with a value now or predicted (`now` /
/// `next`: `economy::region_wealth` without and with the prediction) whose key the table has: one
/// factor, or two (now, then predicted, each against 0) when the two have opposite signs; the tax slot
/// of a tax-exempt region counts as 0 now and never splits. A factor (`0x00A4E250`) is positive when its
/// first non-zero value is not negative; its pip is the table's column 1 and its text the sign's
/// `town_wealth_growth_factors_{positive,negative}_tooltip_<key>`; values in whole points with the
/// "Predicted" word.
fn town_wealth_pips(lua: &Lua, inner: &Inner, ui: &CampaignUi, growth: i32, now: &[i32; 10], next: &[i32; 10], tax_exempt: bool) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("Total", growth)?;
    t.set("PipValue", 50)?;
    t.set("IsPercentage", false)?;
    let predicted_word = loc_required(inner, "random_localisation_strings_string_predicted");
    let mut n = 0;
    for (i, key) in economy::TOWN_WEALTH_FACTORS.iter().enumerate() {
        let (cur, pred) = (now[i], next[i]);
        if cur == 0 && pred == 0 {
            continue;
        }
        let Some(pip) = ui.town_factor_pip(inner, key) else { continue };
        let exempt_tax = i == 5 && tax_exempt;
        let pairs = if (cur < 0) != (pred < 0) && !exempt_tax { vec![(cur, 0), (0, pred)] } else { vec![(if exempt_tax { 0 } else { cur }, pred)] };
        for (a, b) in pairs {
            let positive = if a != 0 { a >= 0 } else { b >= 0 };
            let sign = if positive { "positive" } else { "negative" };
            let text = loc(inner, &format!("town_wealth_growth_factors_{sign}_tooltip_{key}"));
            if let Some(e) = pip_entry(lua, (a, b), positive, Some(pip.clone()), text.as_deref(), &predicted_word, FactorUnits::Points)? {
                n += 1;
                t.set(n, e)?;
            }
        }
    }
    Ok(t)
}

/// The population growth pip table (`0x00A99000`, CONFIRMED): Total the current growth in percent,
/// PipValue 0.01, a percentage; the seven growth factors (`population::FACTOR_KEYS`) as whole
/// hundredths, now and after the projected round, those with a value; the extra word is "Predicted"
/// (`random_localisation_strings` #0x41).
fn growth_pips(lua: &Lua, inner: &Inner, ui: &CampaignUi, p: &population::Projection) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("Total", p.current.growth)?;
    t.set("PipValue", 0.01f32)?;
    t.set("IsPercentage", true)?;
    let predicted_word = loc_required(inner, "random_localisation_strings_string_predicted");
    let mut n = 0;
    for (i, key) in population::FACTOR_KEYS.iter().enumerate() {
        let pair = (population::as_hundredths(p.current.factors[i]), population::as_hundredths(p.predicted.factors[i]));
        if let Some(e) = factor_entry(lua, inner, ui, key, pair, &predicted_word, FactorUnits::Percent)? {
            n += 1;
            t.set(n, e)?;
        }
    }
    Ok(t)
}

/// A pip table without factors: `total`, `pip_value`, `percentage` as the handler sets them.
fn bare_pips(lua: &Lua, total: impl mlua::IntoLua, pip_value: impl mlua::IntoLua, percentage: bool) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("Total", total)?;
    t.set("PipValue", pip_value)?;
    t.set("IsPercentage", percentage)?;
    Ok(t)
}

/// `InitialiseRegionInfoDetails(region)`'s table (see the module). `Nil` for a region not in the
/// campaign.
/// What the region info table (`0x009AF570`) reads from the model, gathered under one borrow.
struct Facts {
    key: String,
    owner_key: String,
    population: u32,
    gdp: u32,
    town_wealth: u32,
    tax_exempt: bool,
    rebel: bool,
    religions: Vec<(String, f32)>,
    classes: (String, String),
    rates: Option<economy::RegionTaxRates>,
    governorship: bool,
    projection: population::Projection,
    wealth_next: economy::RegionWealth,
}

/// What the details (`0x009E69B0`) add: only `InitialiseRegionInfoDetails` computes these.
struct DetailFacts {
    town_wealth_growth: i32,
    governor: Option<CharacterId>,
    upper: Option<ClassPublicOrder>,
    lower: Option<ClassPublicOrder>,
    wealth_now: economy::RegionWealth,
    predicted_upper: Option<ClassPublicOrder>,
    predicted_lower: Option<ClassPublicOrder>,
}

pub(super) fn region_info_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, r: RegionId) -> mlua::Result<Value> {
    if !ui.model().world.regions.contains_key(&r) {
        inner.log_once("InitialiseRegionInfoDetails of a missing region", || {
            format!("CampaignUI.InitialiseRegionInfoDetails: region {} is not in the campaign, answered nil (logged once)", r.0)
        });
        return Ok(Value::Nil);
    }
    let Some((facts, Some(details))) = region_facts(ui, r, true) else { return Ok(Value::Nil) };
    let t = region_info_table(lua, inner, ui, r, &facts)?;
    region_details_extras(lua, inner, ui, &t, &facts, &details)?;
    Ok(Value::Table(t))
}

/// The region info table `0x009AF570` alone (the start of the details above), as the negotiation's
/// `TradeableRegions` rows carry it. `None` for a region not in the campaign or without a
/// population projection.
pub(super) fn region_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, r: RegionId) -> mlua::Result<Option<Table>> {
    match region_facts(ui, r, false) {
        Some((facts, _)) => region_info_table(lua, inner, ui, r, &facts).map(Some),
        None => Ok(None),
    }
}

/// The facts of the info table, and with `details` those of the details' additions.
fn region_facts(ui: &CampaignUi, r: RegionId, details: bool) -> Option<(Facts, Option<DetailFacts>)> {
    let m = ui.model();
    let reg = m.world.regions.get(&r)?;
    let governing = m.world.governing_faction(r).unwrap_or(reg.owner);
    // The governorship that lists the region (region +0x248) and its governor.
    let post = m
        .world
        .faction_details
        .get(&governing)
        .and_then(|d| d.posts.iter().find(|p| p.governorship.as_ref().is_some_and(|g| g.regions.contains(&r))));
    let fx = ntw_sim::campaign::effects::Effects::compute_for(&m, reg.owner);
    // The one-round projection (`0x00A727D0`), which the info table's PopulationChange reads too.
    let projection = population::project(&m, r)?;
    let extra = details.then(|| {
        let (upper, lower) = economy::governed_class_factors(&m, r);
        // The projection's public order: the predicted set, the projected religions and the
        // garrison as it stands. PROVISIONAL: the exe's garrison count (`0x00A190A0`) adds or
        // removes the units of an army the player has selected to move into or out of the
        // settlement (`0x00B148B0` / `0x00B14900`); the model's UI has no such selection here.
        let predicted = economy::public_order_factors_with(&m, reg, &projection.set, &projection.religions, economy::garrison_units(&m, reg));
        let (predicted_upper, predicted_lower) = economy::governed_classes(&m, reg, predicted);
        DetailFacts {
            town_wealth_growth: reg.town_wealth_growth,
            governor: post.and_then(|p| p.holder).filter(|c| m.world.characters.contains_key(c)),
            upper,
            lower,
            wealth_now: economy::region_wealth(&m, Some(&fx), reg, false),
            predicted_upper,
            predicted_lower,
        }
    });
    let facts = Facts {
        key: reg.key.clone(),
        owner_key: m.world.factions.get(&reg.owner).map(|f| f.key.clone()).unwrap_or_default(),
        population: reg.population,
        gdp: reg.gdp,
        town_wealth: reg.town_wealth,
        tax_exempt: reg.tax_exempt,
        rebel: m.is_rebel_faction(reg.owner),
        religions: reg.religions.clone(),
        classes: economy::government_classes(&m, governing),
        rates: economy::region_tax_rates(&m, &fx, reg),
        governorship: post.is_some(),
        projection,
        wealth_next: economy::region_wealth(&m, Some(&fx), reg, true),
    };
    Some((facts, extra))
}

/// `0x009AF570`: the region info table.
fn region_info_table(lua: &Lua, inner: &Inner, ui: &CampaignUi, r: RegionId, facts: &Facts) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("Address", region_value(ui, r))?;
    t.set("Settlement", ui.settlement_name(inner, r))?;
    // Region +0x29C is the text of the region's `CAMPAIGN_LOCALISATION` (REGION #41: the reader
    // `0x00A45950` loads it through `0x00872420` into the key / text pair at +0x290 / +0x29C), whose key is
    // `regions_onscreen_<region key>` in every region of the vanilla saves (CONFIRMED, 134 regions).
    t.set("Name", region_name(&inner.loc, &facts.key))?;
    // The region's theatre string (`0x00AAF350` → theatre +0x68), the same field `HomeTheatre`
    // (`0x009E5180`: faction +0x734 → +0x68) returns, so the theatre key (CONFIRMED: the
    // negotiation's InitRegionList lists a region only when its `Theatre` equals the panel's
    // `HomeTheatre(player)`). PROVISIONAL as `HomeTheatre`: the campaign's one theatre.
    t.set("Theatre", ui.theatre(inner).map(|a| a.id))?;
    t.set("OwningFactionKey", facts.owner_key.as_str())?;
    t.set("PopulationNumber", facts.population)?;
    t.set("Population", facts.population.to_string())?;
    // The projected growth's trend (`0x00A727D0` result +0xA4 = the grown copy's +0x40): 1 up, 2
    // unchanged, 3 down (`population::grow`).
    t.set("PopulationChange", facts.projection.predicted.trend)?;
    let wealth = i64::from(facts.gdp) + i64::from(facts.town_wealth);
    t.set("Wealth", wealth)?;
    // Region +0xC4: the trend of the predicted town wealth growth (`0x00AB4410`'s second recompute, the
    // predicted mode of `0x00A6AFC0`), `economy::wealth_trend`.
    t.set("WealthChange", economy::wealth_trend(facts.wealth_next.growth))?;
    let (upper_tax, lower_tax) = match facts.rates {
        Some(x) if !facts.rebel => (x.upper, x.lower),
        _ => (0.0, 0.0),
    };
    t.set("UpperTax", upper_tax)?;
    t.set("LowerTax", lower_tax)?;
    // The strongest religion; the first of equals stays (`0x00A8C430` replaces only on greater).
    let main = facts.religions.iter().fold(None::<&(String, f32)>, |best, x| match best {
        Some(b) if x.1 <= b.1 => Some(b),
        _ => Some(x),
    });
    if let Some((key, _)) = main {
        t.set("ReligionKey", key.as_str())?;
        t.set("ReligionIcon", ui.religion_icon(inner, key))?;
    }
    t.set("Taxed", !facts.tax_exempt)?;
    t.set("ActiveUpperClass", facts.classes.0.as_str())?;
    t.set("ActiveLowerClass", facts.classes.1.as_str())?;
    if facts.governorship && let Some(x) = facts.rates {
        t.set("UpperTaxPercentage", x.upper_level as f32 * 0.01)?;
        t.set("LowerTaxPercentage", x.lower_level as f32 * 0.01)?;
        t.set("BuildingPercentage", x.bonuses.building * 0.01)?;
        t.set("GovernorPercentage", x.bonuses.character * 0.01)?;
        t.set("TechnologyPercentage", x.bonuses.technology * 0.01)?;
        t.set("AdministrationCostPercentage", x.efficiency)?;
    }
    // PROVISIONAL: NextTownName / TurnsUntilNextTown are left out: towns emerging on the map are not in the
    // model (BACKLOG §0-E). Traced: the next town (`0x00A673A0`) is the region's town slot (region +0x120
    // list) not yet emerged with the lowest order (+0x214); the turns (`0x00AAFB40`) are
    // ceil((threshold − pop) / (projected pop − pop) − 0.001) while the population grows, else −1, the
    // threshold (`0x00A8D900`) being round(`POPULATION` #3 × (1 + `pop_growth_for_spawn`%)).
    Ok(t)
}

/// What `InitialiseRegionInfoDetails` (`0x009E69B0`) adds to the info table.
fn region_details_extras(lua: &Lua, inner: &Inner, ui: &CampaignUi, t: &Table, facts: &Facts, details: &DetailFacts) -> mlua::Result<()> {
    if let Some(c) = details.governor {
        t.set("Governor", character_details(lua, inner, ui, c)?)?;
    }
    // PROVISIONAL: the region's effect list (`0x009ACD10` over region +0x1CC: {Icon, Tooltip} per entry
    // of an effect record with a positive priority, equal records merged) is not in the model, and its
    // writer is not traced (BACKLOG §0-E): none.
    t.set("Effects", lua.create_table()?)?;
    t.set("UpperOrder", order_pips(lua, inner, ui, details.upper.as_ref(), details.predicted_upper.as_ref())?)?;
    t.set("LowerOrder", order_pips(lua, inner, ui, details.lower.as_ref(), details.predicted_lower.as_ref())?)?;
    t.set("PopulationGrowth", growth_pips(lua, inner, ui, &facts.projection)?)?;
    // RegionWealth (`0x00A992A0`, CONFIRMED): GDP + town wealth, PipValue 50; its factors are the GDP
    // breakdown's slots that have a `region_wealth_factors` record, and the shipped data has no such
    // table (`table_list wealth`: only `town_wealth_growth_factors`), so the original lists none either.
    let wealth = i64::from(facts.gdp) + i64::from(facts.town_wealth);
    t.set("RegionWealth", bare_pips(lua, wealth, 50, false)?)?;
    let (now, next) = (&details.wealth_now.factors, &facts.wealth_next.factors);
    t.set("TownWealth", town_wealth_pips(lua, inner, ui, details.town_wealth_growth, now, next, facts.tax_exempt)?)?;
    // ReligiousBreakdown (`0x00A99510` → `0x00A4E200`, CONFIRMED): each religion of the projection's
    // current breakdown (normalised, `0x00A72990`) with a share above 0, in the region's order:
    // Percentage (share × 100), Icon, Name, Key and Change ((projected share − share) × 100, the same
    // index of the grown copy's breakdown).
    let rb = lua.create_table()?;
    let mut n = 0;
    let p = &facts.projection;
    for (i, (key, share)) in p.current_religions.iter().enumerate().filter(|(_, (_, s))| *s > 0.0) {
        let next = p.religions.get(i).map_or(*share, |x| x.1);
        let e = lua.create_table()?;
        e.set("Percentage", share * 100.0)?;
        e.set("Icon", ui.religion_icon(inner, key))?;
        e.set("Name", loc_required(inner, &format!("religions_onscreen_{key}")))?;
        e.set("Key", key.as_str())?;
        e.set("Change", (next - share) * 100.0)?;
        n += 1;
        rb.set(n, e)?;
    }
    t.set("ReligiousBreakdown", rb)?;
    t.set("TaxExempt", facts.tax_exempt)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ca_float, format_s};

    #[test]
    fn the_engine_float_keeps_one_decimal_at_least() {
        assert_eq!([ca_float(0.29999998), ca_float(-0.37), ca_float(1.0), ca_float(0.25), ca_float(-0.07000002)], ["0.3", "-0.37", "1.0", "0.25", "-0.07"]);
    }

    #[test]
    fn a_factor_text_takes_its_value_at_the_first_percent_s() {
        assert_eq!(format_s("Tax burden: %S||Lower taxes.", "3 (Predicted)"), "Tax burden: 3 (Predicted)||Lower taxes.");
        assert_eq!(format_s("No value", "3"), "No value");
    }
}
