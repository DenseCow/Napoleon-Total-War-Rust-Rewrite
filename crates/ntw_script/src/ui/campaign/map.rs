//! The campaign map's HUD parts: the radar and theatres, the settlement labels and the Lists panel.

use super::*;

/// A theatre's on-screen name: loc `campaign_map_playable_areas_onscreen_name_<key>` (CONFIRMED
/// key, e.g. "... _1244818741" = "Europe"), else the area key.
pub(super) fn theatre_name(inner: &Inner, row: &ntw_data::CampaignMapPlayableArea) -> String {
    loc(inner, &format!("campaign_map_playable_areas_onscreen_name_{}", row.id)).unwrap_or_else(|| row.area.clone())
}


impl UiScriptHost {
    /// The campaign HUD's [`UiScriptHost::hover`].
    pub fn campaign_hover(&self, node: Option<NodeId>) {
        self.hover(node);
    }
}

impl UiScriptHost {
    /// The game's view of the map for the settlement labels: the camera position (any values that
    /// change when the view changes), the settlements on screen with their screen positions, and
    /// the settlement under the pointer. The root layout's pulse then updates the labels
    /// (Labels.UpdateEntityLabels, original script).
    pub fn campaign_set_view(&self, camera: (f32, f32, f32), settlements: Vec<(RegionId, f32, f32)>, over: Option<RegionId>) {
        let Some(ui) = self.campaign_ui() else { return };
        ui.camera.set(camera);
        *ui.visible.borrow_mut() = settlements;
        ui.over.set(over);
    }

    /// The point the campaign camera looks at, in map units (x, y with y north), and its zoom
    /// (`CampaignUI.CameraTarget`, which the radar follows).
    pub fn campaign_set_camera_target(&self, x: f32, y: f32, zoom: f32) {
        if let Some(ui) = self.campaign_ui() {
            ui.camera_target.set((x, zoom, y));
        }
    }

    /// The campaign theatre's bounds in map units (min, max), when the game has the map loaded
    /// (otherwise the HUD reads them from the map's regions.esf itself).
    pub fn campaign_set_theatre_bounds(&self, min: (f32, f32), max: (f32, f32)) {
        if let Some(ui) = self.campaign_ui() {
            let _ = ui.theatre_bounds.set(Some((min, max)));
        }
    }

    /// The radar's camera outline in UI units: the ground points under the screen's four corners
    /// (map units, top-left, top-right, bottom-right, bottom-left), placed on the component
    /// AttachRadarView named with UpdateRadarView's mapping (x: offset by the theatre's corner,
    /// scaled by map width / theatre width; y: north up). None until the radar scripts set both.
    /// Also the clip rectangle of that component.
    pub fn campaign_radar_outline(&self, ground: [(f32, f32); 4]) -> Option<([(f32, f32); 4], UiRect)> {
        let ui = self.campaign_ui()?;
        let (node, mapping) = *ui.radar_view.borrow();
        let ((mw, mh), (ox, oy), (tw, th)) = mapping?;
        let w = self.world();
        let n = w.get(node?)?;
        if !n.visible || tw <= 0.0 || th <= 0.0 {
            return None;
        }
        let r = n.rect;
        let clip = w.clip_rect(node?).map_or(r, |c| c.intersect(&r));
        let pt = |(x, y): (f32, f32)| (r.x + (x - ox) / tw * mw, r.y + (1.0 - (y - oy) / th) * mh);
        Some((ground.map(pt), clip))
    }
}

pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // AttachRadarView(component): the component the engine draws the camera's view outline on
    // (0x009DD050 → 0x009C1AB0, CONFIRMED: stores it); UpdateRadarView({map_dimensions = {w, h},
    // theatre_offset = {x, y}, theatre_dimensions = {w, h}}) (0x009FAE60 → 0x0099BE80, CONFIRMED
    // keys): how map units map onto it. The outline itself is the four screen corners of the
    // camera projected onto the ground (0x00A27D50, CONFIRMED), see `campaign_radar_outline`.
    f!("AttachRadarView", |_l, inner, ui, c: Value| {
        ui.radar_view.borrow_mut().0 = super::super::host::node_of(&c);
        Ok(())
    });
    f!("UpdateRadarView", |_l, inner, ui, t: Option<Table>| {
        let Some(t) = t else { return Ok(()) };
        let pair = |k: &str, a: &str, b: &str| -> Option<(f32, f32)> {
            let s: Table = t.get(k).ok()?;
            Some((s.get(a).ok()?, s.get(b).ok()?))
        };
        if let (Some(map), Some(off), Some(dim)) =
            (pair("map_dimensions", "w", "h"), pair("theatre_offset", "x", "y"), pair("theatre_dimensions", "w", "h"))
        {
            ui.radar_view.borrow_mut().1 = Some((map, off, dim));
        }
        Ok(())
    });
    f!("TheatreList", |lua, inner, ui, no_sea_trade: Option<bool>| {
        let t = lua.create_table()?;
        if let Some(row) = ui.theatre(&inner)
            && !(no_sea_trade.unwrap_or(false) && row.sea_trade)
        {
            let e = lua.create_table()?;
            e.set("Address", ui.entity(TAG_THEATRE, 0))?;
            e.set("Id", row.area.as_str())?;
            e.set("Name", theatre_name(&inner, &row))?;
            e.set("Key", row.id.as_str())?;
            e.set("SeaTrade", row.sea_trade)?;
            t.set(1, e)?;
        }
        Ok(t)
    });
    // TheatreMapDimensions(theatre) → x, y, width, height of the theatre in map units (the theatre
    // bounds' min corner and size, CONFIRMED 0x009F9760; then a u32 from the theatre's record, not
    // given: UNKNOWN). Nothing for an unknown theatre.
    f!("TheatreMapDimensions", |_l, inner, ui, theatre: Option<String>| {
        let ok = match (ui.theatre(&inner), theatre) {
            (Some(row), Some(t)) => row.id == t || row.area == t,
            _ => false,
        };
        Ok(match (ok, ui.theatre_bounds(&inner)) {
            (true, Some(((x0, y0), (x1, y1)))) => Variadic::from_iter([x0, y0, x1 - x0, y1 - y0]),
            _ => Variadic::new(),
        })
    });
    // SetCameraTarget(x, y): centre the camera on a map position (the radar's map_overlay.lua sends
    // the clicked point; CONFIRMED call; a theatre key alone, from campaign_hud.lua, is ignored).
    f!("SetCameraTarget", |_l, inner, ui, (x, y): (Value, Option<f32>)| {
        if let (Value::Number(_) | Value::Integer(_), Some(y)) = (&x, y) {
            let x = match x {
                Value::Number(n) => n as f32,
                Value::Integer(i) => i as f32,
                _ => 0.0,
            };
            ui.push(CampaignRequest::CameraTo(x, y));
        }
        Ok(())
    });
    // RegionsInTheatre(theatre, faction, sub_key, mode) → {Map, Overlay, Radar, [i] = region}
    // (0x009EFC30, CONFIRMED keys and per-mode fields; called by template.map_image.lua's
    // InitRegionMap with mode = diplomacy_status 0, diplomacy_attitude_* 1, region_list 2,
    // public_order 3, ownership / radar 4).
    // * Map / Overlay / Radar: the theatre's `campaign_map_playable_areas` row's three pictures,
    //   each as "<campaign map folder>/<file>" (CONFIRMED format "%S/%S"; the folder string,
    //   "data/campaign_maps/<map>", INFERRED).
    // * one entry per region of the theatre: PaletteEntry (the lookup picture's palette index whose
    //   colour is the region's `regions` DB colour, -1 if none: CONFIRMED 0x00A97680 /
    //   0x00A9D850), Key, Name, Address, Owner (faction name), OwnerKey, TaxExempt (region +0xE4,
    //   CONFIRMED 0x00AAF270); modes 0 and 4 add OwnerRGB {r, g, b} (the owner's colour × 255,
    //   CONFIRMED; the primary colour INFERRED); mode 1, for regions the faction does not own,
    //   OwnerAttitude (the owner's attitude total towards the faction) and
    //   FactionAttitudeTowardsOwner (the faction's towards the owner) (CONFIRMED total 0x00B0DB60,
    //   direction INFERRED from the names); mode 3 adds OrderRGB {r, g, b, a} and PublicOrder.
    // PROVISIONAL: every region of the model is in the (single) theatre of a Napoleon campaign
    // map; the exe also drops regions a position test hides (0x008E0500, UNKNOWN). Mode 1's
    // OwnerStatus / *RelationshipDetails texts and mode 3's Lower / Upper detail tables are not
    // given yet; mode 3's colour is a PLACEHOLDER (the exe's 0x00A727D0 is not decoded).
    f!("RegionsInTheatre", |lua, inner, ui, (theatre, faction, _sub, mode): (Option<String>, Option<String>, Value, Option<i32>)| {
        let mode = mode.unwrap_or(4);
        let theatre = theatre.unwrap_or_else(|| ui.home_theatre().to_owned());
        let faction = faction.unwrap_or_else(|| ui.link.human.clone());
        let out = lua.create_table()?;
        let folder = ui.map_folder();
        let row = ui.playable_area(&inner, &theatre);
        let mut lookup = None;
        if let Some(row) = &row {
            let file = |f: &Option<String>| f.as_deref().filter(|s| !s.is_empty()).map(|s| format!("{folder}/{s}"));
            if let Some(p) = file(&row.map) {
                out.set("Map", p)?;
            }
            if let Some(p) = file(&row.lookup) {
                out.set("Overlay", p.clone())?;
                lookup = Some(p);
            }
            if let Some(p) = file(&row.radar) {
                out.set("Radar", p)?;
            }
        }
        let palette = lookup.map(|p| ui.palette_index(&inner, &p)).unwrap_or_default();
        let m = ui.model();
        let me = m.faction_by_key(&faction).map(|f| f.id);
        let mut regions: Vec<_> = m.world.regions.values().collect();
        regions.sort_by_key(|r| r.id);
        for (n, r) in regions.into_iter().enumerate() {
            let e = lua.create_table()?;
            let colour = ui.link.db.region(&r.key).map(|rec| rec.colour());
            let entry = colour.and_then(|c| palette.get(&c).copied()).unwrap_or(-1);
            e.set("PaletteEntry", entry)?;
            e.set("Key", r.key.as_str())?;
            e.set("Name", region_name(&inner.loc, &r.key))?;
            e.set("Address", region_value(&ui, r.id))?;
            let owner = m.world.factions.get(&r.owner);
            let owner_key = owner.map(|f| f.key.clone()).unwrap_or_default();
            e.set("Owner", faction_name(&inner, &ui.link.db, &owner_key))?;
            e.set("OwnerKey", owner_key.as_str())?;
            e.set("TaxExempt", r.tax_exempt)?;
            if mode == 0 || mode == 4 {
                let [cr, cg, cb] = ui.link.db.faction(&owner_key).map_or([128, 128, 128], |f| f.primary_colour());
                let rgb = lua.create_table()?;
                rgb.set("r", cr)?;
                rgb.set("g", cg)?;
                rgb.set("b", cb)?;
                e.set("OwnerRGB", rgb)?;
            }
            if mode == 1
                && let Some(me) = me
                && me != r.owner
            {
                if let Some(rel) = m.world.relationships.get(&(r.owner, me)) {
                    e.set("OwnerAttitude", rel.attitude_total())?;
                }
                if let Some(rel) = m.world.relationships.get(&(me, r.owner)) {
                    e.set("FactionAttitudeTowardsOwner", rel.attitude_total())?;
                }
            }
            if mode == 3 {
                let order = ntw_sim::campaign::economy::public_order(&m, r.id).worst();
                // PLACEHOLDER colour scale: red when rioting, green when content.
                let t = ((order + 10.0) / 20.0).clamp(0.0, 1.0);
                let rgb = lua.create_table()?;
                rgb.set("r", ((1.0 - t) * 255.0).round())?;
                rgb.set("g", (t * 255.0).round())?;
                rgb.set("b", 0)?;
                rgb.set("a", 160)?;
                e.set("OrderRGB", rgb)?;
                e.set("PublicOrder", order)?;
            }
            out.set(n + 1, e)?;
        }
        Ok(out)
    });
    f!("HomeTheatre", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.theatre(&inner).map(|r| r.id)));
    // Map labels. CameraPosition() / CameraTarget() → x, y, z (CONFIRMED descriptions; Labels.lua
    // only compares them to notice camera moves). RetrieveVisibleEnitityDetails() → {Settlements =
    // {{Address, ScreenPos = {X, Y}}...}, Resources = {...}} (CONFIRMED names read by Labels.lua):
    // the settlements on screen, at the screen position the game reports with
    // `campaign_set_view` (`0x009F4520`: the settlements in the view frustum, HUD panels or not;
    // UI_FIDELITY.md §11.1). PROVISIONAL: no resource icons yet.
    f!("CameraPosition", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.camera.get()));
    // CameraTarget() → the point the camera looks at (map x), the zoom (camera +0x138), the map y,
    // then the key of the theatre holding that point (0x009DFEC0, CONFIRMED order; the radar's
    // UpdateCamera reads all four). Reported by the game with `campaign_set_camera_target`.
    // PROVISIONAL: the theatre is the campaign's one whenever the point is inside its bounds.
    f!("CameraTarget", |_l, inner, ui, _a: Variadic<Value>| {
        let (x, zoom, y) = ui.camera_target.get();
        let inside = ui.theatre_bounds(&inner).is_some_and(|((x0, y0), (x1, y1))| x >= x0 && x <= x1 && y >= y0 && y <= y1);
        let theatre = if inside { ui.theatre(&inner).map(|r| r.id) } else { None };
        Ok((x, zoom, y, theatre))
    });
    f!("RetrieveVisibleEnitityDetails", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let list = lua.create_table()?;
        // Both tables stay empty while the player has the labels off (`0x009F4520` reads preference
        // 0x44 first, CONFIRMED).
        if !campaign_labels_shown(&inner) {
            t.set("Settlements", list)?;
            t.set("Resources", lua.create_table()?)?;
            return Ok(t);
        }
        // The fog of war: a settlement the player has never seen has no label. This is
        // `CampaignModel::knows` (visible now OR explored), **not** `sees`: the original draws
        // the labels from the shroud's *explored* tree (CHARACTERS_FIDELITY.md §10; INFERRED --
        // no Ghidra output for it was kept, review 0-G), so a
        // settlement that has been seen and is now only explored keeps its label while the
        // terrain renderer dims it. Filtering with `sees` dropped those, leaving dimmed ground
        // with no label on it. INFERRED, from the name and the explored tree.
        let human = ui.model().faction_by_key(&ui.link.human).map(|h| h.id);
        let mut n = 0;
        for (r, x, y) in ui.visible.borrow().iter() {
            let seen = human.is_none_or(|h| {
                ui.model()
                    .world
                    .regions
                    .get(r)
                    .is_some_and(|reg| ui.model().knows(h, (reg.settlement.position.0.to_f32(), reg.settlement.position.1.to_f32())))
            });
            if !seen {
                continue;
            }
            let e = lua.create_table()?;
            e.set("Address", region_value(&ui, *r))?;
            let p = lua.create_table()?;
            p.set("X", *x)?;
            p.set("Y", *y)?;
            e.set("ScreenPos", p)?;
            e.set("Selected", ui.selection.get() == CampaignSelection::Settlement(*r))?;
            e.set("Over", ui.over.get() == Some(*r))?;
            n += 1;
            list.set(n, e)?;
        }
        t.set("Settlements", list)?;
        t.set("Resources", lua.create_table()?)?;
        Ok(t)
    });
    // ToggleLabels(): "Toggles that labels under settlements on/off" (CONFIRMED description,
    // `0x009F9CA0`): flips the preference `ui_show_campaign_labels` (preference index 0x44,
    // registered at `0x004057E6`, default true), which RetrieveVisibleEnitityDetails reads.
    f!("ToggleLabels", |_l, inner, _ui, _a: Variadic<Value>| {
        let shown = campaign_labels_shown(&inner);
        inner.prefs.borrow_mut().set(CAMPAIGN_LABELS_PREF, if shown { "false" } else { "true" });
        Ok(())
    });
    // ShouldShowLabelBottomRow(): "Out: Selected settlement, mouse over settlement" (CONFIRMED
    // description): the labels whose bottom row (wealth, population) is shown.
    f!("ShouldShowLabelBottomRow", |_l, inner, ui, _a: Variadic<Value>| {
        let sel = match ui.selection.get() {
            CampaignSelection::Settlement(r) => region_value(&ui, r),
            _ => Value::Nil,
        };
        Ok((sel, ui.over.get().map(|r| region_value(&ui, r)).unwrap_or(Value::Nil)))
    });
    // The CampaignSettlement(address) object's LabelDetails() (CONFIRMED names read by
    // template.city_info_bar.lua): {Name, IsCapital, FactionRGB = {R, G, B}, PopulationGrowthString,
    // Region = {Name, Key, Address, Region, Wealth, WealthChange, PopulationChange, ReligionKey}}.
    // CONFIRMED placement from the bottom-row function (`city_info_bar.lua:110`, lines 125-131):
    // `change_rates[details.Region.PopulationChange + 1]`, the same for `Region.WealthChange`,
    // `tostring(details.Region.Wealth)`, `religion:SetState(details.Region.ReligionKey)`,
    // `pop_text:SetStateText(details.PopulationGrowthString)` and
    // `CampaignUI.RegionsPublicOrders(details.Region.Region)`. So the changes are 0-based indices
    // into the template's `change_rates`, and `Region.Region` is the region's address (INFERRED: it
    // is what RegionsPublicOrders takes). IsCapital from the owner's capital (CAMPAIGN_DATA.md §3).
    // PROVISIONAL: both changes index 0, no religion.
    f!("__LabelDetails", |lua, inner, ui, a: Value| {
        let Some(r) = entity_of(&a, TAG_REGION).map(|r| RegionId(r as u32)) else { return Ok(Value::Nil) };
        let (key, owner, gdp, pop) = {
            let m = ui.model();
            let Some(reg) = m.world.regions.get(&r) else { return Ok(Value::Nil) };
            let owner = m.world.factions.get(&reg.owner).map(|f| f.key.clone()).unwrap_or_default();
            (reg.key.clone(), owner, reg.gdp, reg.population)
        };
        let t = lua.create_table()?;
        t.set("Name", ui.settlement_name(&inner, r))?;
        let rt = lua.create_table()?;
        rt.set("Name", region_name(&inner.loc, &key))?;
        rt.set("Key", key.as_str())?;
        rt.set("Address", region_value(&ui, r))?;
        rt.set("Region", region_value(&ui, r))?;
        rt.set("Wealth", gdp)?;
        rt.set("WealthChange", 0)?;
        rt.set("PopulationChange", 0)?;
        rt.set("ReligionKey", "")?;
        t.set("Region", rt)?;
        t.set("IsCapital", ui.model().world.is_capital(r))?;
        let c = ui.link.db.faction(&owner).map_or([128, 128, 128], |f| f.primary_colour());
        let rgb = lua.create_table()?;
        rgb.set("R", c[0])?;
        rgb.set("G", c[1])?;
        rgb.set("B", c[2])?;
        t.set("FactionRGB", rgb)?;
        t.set("Population", pop)?;
        t.set("PopulationGrowthString", "")?;
        Ok(Value::Table(t))
    });
    // RegionsPublicOrders(region) → upper, lower: the region's public order for the two classes
    // (CONFIRMED name, registered at 0x00429E90; CONFIRMED two results, upper first, from
    // `city_info_bar.lua:131-133`, which feeds them to `upper_order` / `lower_order` through the
    // template's StateFromOrder). Values as the lists panel's UpperOrder / LowerOrder.
    f!("RegionsPublicOrders", |_l, inner, ui, a: Value| {
        let Some(r) = entity_of(&a, TAG_REGION).map(|r| RegionId(r as u32)) else { return Ok((Value::Nil, Value::Nil)) };
        let m = ui.model();
        if !m.world.regions.contains_key(&r) {
            return Ok((Value::Nil, Value::Nil));
        }
        let po = ntw_sim::campaign::economy::public_order(&m, r);
        Ok((Value::Number(po.upper as f64), Value::Number(po.lower as f64)))
    });
    // The lists panel (entity_lists.lua, the "button_lists" HUD button): CONFIRMED function names
    // and the fields its row templates read (row_template_army / _naval / _region / _agent).
    // RetrieveFactionMilitaryForceLists(faction, armies) → one character details table per
    // commander ([`character_details`]: Address, Name, Location, Soldiers, ActionPoints,
    // ActionPointsPerTurn, CommandedUnit, ShowAsCharacter, ...).
    // RetrieveFactionRegionList(faction) → {{Address, Name, Settlement, SettlementAddress,
    // IsCapital, Wealth, WealthChange, Population, PopulationNumber, PopulationChange, UpperOrder,
    // LowerOrder, UpperTax, LowerTax}...}
    // RetrieveFactionAgentsList(faction) → {{Address, Name, Location, AgentType, ActionPoints,
    // ActionPointsPerTurn}...}
    // PROVISIONAL: a character without a name in the model shows his agent type; Location is the
    // region of the nearest settlement; changes 0; tax rates from the faction's levels.
    // The fog of war (INFERRED, as for the labels): another faction's force is listed only when the
    // player knows its commander ([`agents::knows_character`], CONFIRMED: the hidden-flag test and
    // the exposed lists), so an army hidden in the fog stays off the list.
    f!("RetrieveFactionMilitaryForceLists", |lua, inner, ui, (faction, armies): (Option<String>, Option<bool>)| {
        let key = faction.unwrap_or_else(|| ui.link.human.clone());
        let armies = armies.unwrap_or(true);
        let rows: Vec<CharacterId> = {
            let m = ui.model();
            let Some(f) = m.faction_by_key(&key).map(|f| f.id) else { return lua.create_table() };
            let me = m.faction_by_key(&ui.link.human).map(|h| h.id);
            m.world
                .forces
                .values()
                .filter(|x| x.faction == f && x.is_navy != armies)
                .filter(|x| Some(f) == me || x.commander.is_some_and(|c| me.is_some_and(|me| ntw_sim::campaign::agents::knows_character(&m, me, c))))
                .filter_map(|x| x.commander.filter(|c| m.world.characters.contains_key(c)))
                .collect()
        };
        // Each row is the commander's character details table (`0x009AD250`: it carries Soldiers,
        // ShowAsCharacter and CommandedUnit, CONFIRMED -- see `character_details`).
        let out = lua.create_table()?;
        for (i, c) in rows.iter().enumerate() {
            out.set(i + 1, character_details(lua, &inner, &ui, *c)?)?;
        }
        Ok(out)
    });
    f!("RetrieveFactionRegionList", |lua, inner, ui, faction: Option<String>| {
        let key = faction.unwrap_or_else(|| ui.link.human.clone());
        let rows: Vec<(RegionId, String, u32, u32, f32, f32)> = {
            let m = ui.model();
            let Some(f) = m.faction_by_key(&key).map(|f| f.id) else { return lua.create_table() };
            m.world
                .regions
                .values()
                .filter(|r| r.owner == f)
                .map(|r| {
                    let po = ntw_sim::campaign::economy::public_order(&m, r.id);
                    (r.id, r.key.clone(), r.gdp, r.population, po.upper, po.lower)
                })
                .collect()
        };
        let (upper_tax, lower_tax) = {
            let m = ui.model();
            m.faction_by_key(&key).map_or((0, 0), |f| (m.rules.tax_rate(&f.tax_upper), m.rules.tax_rate(&f.tax_lower)))
        };
        let out = lua.create_table()?;
        for (i, (r, rkey, gdp, pop, upper, lower)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Address", region_value(&ui, *r))?;
            e.set("Name", region_name(&inner.loc, rkey))?;
            e.set("Settlement", ui.settlement_name(&inner, *r))?;
            e.set("SettlementAddress", region_value(&ui, *r))?;
            e.set("IsCapital", ui.model().world.is_capital(*r))?;
            e.set("Wealth", *gdp)?;
            e.set("WealthChange", 0)?;
            e.set("Population", *pop)?;
            e.set("PopulationNumber", *pop)?;
            e.set("PopulationChange", 0)?;
            e.set("UpperOrder", *upper)?;
            e.set("LowerOrder", *lower)?;
            e.set("UpperTax", upper_tax)?;
            e.set("LowerTax", lower_tax)?;
            out.set(i + 1, e)?;
        }
        Ok(out)
    });
    // InitialiseRegionInfoDetails(region) → the region details panel's table (handler
    // `0x009E69B0`, CONFIRMED name registered at `0x00429075`; the table is
    // [`region_info::region_info_details`]). The root's ShowRegionInfo opens `region_info` with it
    // and region_details.lua's InitialiseFromDetails fills the panel from it.
    // With no argument the handler takes the HUD's current region (`g_pCampaignUiManager` +0x74);
    // no shipped script calls it so, and ours answers nil, logged once.
    f!("InitialiseRegionInfoDetails", |lua, inner, ui, a: Option<Value>| {
        let Some(r) = a.as_ref().and_then(|a| entity_of(a, TAG_REGION)).map(|r| RegionId(r as u32)) else {
            inner.log_once("InitialiseRegionInfoDetails without a region", || {
                format!("UNKNOWN CampaignUI.InitialiseRegionInfoDetails({a:?}): no region given, answered nil (logged once)")
            });
            return Ok(Value::Nil);
        };
        super::region_info::region_info_details(lua, &inner, &ui, r)
    });
    f!("RetrieveFactionAgentsList", |lua, inner, ui, faction: Option<String>| {
        let key = faction.unwrap_or_else(|| ui.link.human.clone());
        let rows: Vec<AgentRow> = {
            let m = ui.model();
            let Some(f) = m.faction_by_key(&key).map(|f| f.id) else { return lua.create_table() };
            let commanders: std::collections::HashSet<CharacterId> = m.world.forces.values().filter_map(|x| x.commander).collect();
            m.world
                .characters
                .values()
                .filter(|c| c.faction == f && !commanders.contains(&c.id) && c.kind != CharacterKind::Minister)
                .filter(|c| !matches!(c.kind, CharacterKind::General | CharacterKind::Admiral | CharacterKind::Colonel | CharacterKind::Captain))
                .map(|c| (c.id, c.kind, c.movement_points, c.max_movement_points, (c.position.0.to_f32(), c.position.1.to_f32())))
                .collect()
        };
        let out = lua.create_table()?;
        for (i, (c, kind, ap, max_ap, pos)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Address", character_value(&ui, *c))?;
            e.set("Name", character_name(&inner, &ui, *c).unwrap_or_else(|| character_type_name(&inner, &ui, *c)))?;
            e.set("AgentType", kind.esf_name())?;
            e.set("Location", ui.location_name(&inner, *pos))?;
            e.set("ActionPoints", *ap)?;
            e.set("ActionPointsPerTurn", *max_ap)?;
            out.set(i + 1, e)?;
        }
        Ok(out)
    });
    Ok(())
}

/// The preference that shows the settlement labels (`ui_show_campaign_labels`, index 0x44,
/// registered at `0x004057E6` with default true, CONFIRMED).
const CAMPAIGN_LABELS_PREF: &str = "ui_show_campaign_labels";

/// Whether the settlement labels are on (the preference, true when the file lacks it).
fn campaign_labels_shown(inner: &Inner) -> bool {
    inner.prefs.borrow().get_bool(CAMPAIGN_LABELS_PREF).unwrap_or(true)
}
