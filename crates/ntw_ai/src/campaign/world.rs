//! [`AiWorld`]: the campaign AI's read-only snapshot of the campaign model.
//!
//! The AI reads the model only through this snapshot, so a different model version (e.g. the
//! campaign-play turn loop's) needs only a new `from_model` adapter, not a new AI.

use std::collections::{BTreeMap, BTreeSet};

use ntw_sim::campaign::rules::TaxClass;
use ntw_sim::campaign::commands::UnitTypeCounts;
use ntw_sim::campaign::{CampaignModel, CharacterId, FactionId, ForceId, RegionId, SlotRef, Stance};

use super::data::{AiUnitInfo, CampaignAiData};

/// PROVISIONAL: an enemy army this close (campaign map units) threatens a settlement.
pub const THREAT_RADIUS: f64 = 25.0;
/// PROVISIONAL: armies this close to a settlement count as its defenders.
pub const GARRISON_RADIUS: f64 = 5.0;
/// PROVISIONAL: a faction's neighbours are the owners of the nearest this-many other settlements
/// of each of its regions (no region adjacency on `main` yet).
pub const NEIGHBOURS_PER_REGION: usize = 4;

/// A faction.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiFaction {
    /// Faction key.
    pub key: String,
    /// Treasury.
    pub treasury: i32,
    /// Regions owned.
    pub regions: usize,
    /// Gross income per turn (taxes + other, `ntw_sim::campaign::economy`) when the model has
    /// rules.
    pub income: Option<i32>,
    /// Unit upkeep per turn when the model has rules.
    pub upkeep: Option<i32>,
    /// Current `taxes_levels` keys of the lower and upper classes.
    pub tax: [String; 2],
    /// The faction's technologies and their `FACTION_TECHNOLOGY_MANAGER` `techs[]` #1 state
    /// (`ntw_sim::campaign::research::state`): 0 researched, 2 available, 4 not yet available.
    /// `None`: the snapshot was built without the model's details (then the AI does no research).
    pub technologies: Option<BTreeMap<String, u32>>,
    /// The faction's stored manager / personality keys (`World::ai_keys`); `None`: the
    /// PROVISIONAL naming rule ([`super::FactionAiConfig::resolve`]).
    pub ai_keys: Option<ntw_sim::campaign::FactionAiKeys>,
}

/// A school: a slot of an owned region whose standing building gives `research_points` and which
/// the owner holds (`ntw_sim::campaign::research`, CONFIRMED the school's own test). CONFIRMED: a
/// school researches one technology at a time, named by the faction's `techs[]` #3 researcher (the
/// slot's id).
#[derive(Debug, Clone, PartialEq)]
pub struct AiSchool {
    /// Slot index in `Region::slots`.
    pub slot: usize,
    /// `REGION_SLOT` #2 id (0 when the file did not hold one: such a school cannot be named as a
    /// researcher and the model never uses it).
    pub id: u32,
    /// Points per research step for a technology of thread 0 military, 1 industry, 2
    /// enlightenment (`ResearchParts::rate`, CONFIRMED formula).
    pub rate: [f32; 3],
    /// The technology this school is researching now, if any (`CampaignModel::researching_at`).
    pub researching: Option<String>,
}

/// A technology of the `technologies` table the AI can plan (`ntw_sim::campaign::rules`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiTech {
    /// Research cost in points (`technologies` #3).
    pub cost: i32,
    /// Thread the technology's school rate is taken from: 0 military, 1 industry, 2 enlightenment
    /// (`ntw_sim::campaign::research::thread_of`, INFERRED from the key prefix there).
    pub thread: usize,
}

/// A region.
#[derive(Debug, Clone, PartialEq)]
pub struct AiRegion {
    /// Id.
    pub id: RegionId,
    /// Key.
    pub key: String,
    /// Owner.
    pub owner: FactionId,
    /// Settlement position (map units).
    pub position: (f64, f64),
    /// Building level key per slot (`Region::slots` order).
    pub buildings: Vec<Option<String>>,
    /// Units being recruited.
    pub recruiting: usize,
    /// Free land recruitment points (`recruitment_points` effects minus queued land units).
    /// `None` when the snapshot was built without the model's rules (then v1's one-per-turn rule
    /// alone applies).
    pub recruit_capacity: Option<u32>,
    /// The region's recruitable entries as the model prices and flags them for the owner now
    /// ([`AiRecruitable`]), in unit key order. Filled only for the regions of the faction the snapshot
    /// was built for ([`AiWorld::from_model_for`]) and only with the model's rules; empty otherwise, and a
    /// model without rules recruits nothing.
    pub recruitable: Vec<AiRecruitable>,
    /// What the model lets the owner build here now: its one option rule
    /// (`CampaignModel::region_construction_options`, researched levels only, as `can_build` checks) for
    /// every slot, the walls and the road ([`SlotRef`]); empty for a model without rules, which permits
    /// nothing.
    pub build_options: Vec<BuildOption>,
    /// True if something is being built.
    pub constructing: bool,
    /// PROVISIONAL value: 1 + buildings + population in millions.
    pub value: f64,
    /// The region's GDP (`Region::gdp`), the stand-in for the region value base's fields.
    pub gdp: u32,
    /// Public order of the lower and upper classes (`ntw_sim::campaign::economy::public_order`)
    /// when the model has rules.
    pub public_order: Option<[f32; 2]>,
    /// The region's schools: slots whose standing building gives `research_points` and that the
    /// owner holds (`ntw_sim::campaign::research`, CONFIRMED the school's own test). `None`: the
    /// snapshot was built without the model's rules, so the AI cannot research here.
    pub schools: Option<Vec<AiSchool>>,
    /// The original's own stored base value of the region (`World::region_base_values`); `None`:
    /// the formula.
    pub base_value: Option<i32>,
}

/// A unit the region's owner can recruit there now, as the model's recruitable entry: the price the
/// queue command charges (`ntw_sim::campaign::economy::recruitment_cost_in`, `0x00B0D220`; it already holds
/// the faction's difficulty handicap, which the campaign start adds to the faction's effects, `0x008DD090`)
/// and the entry's flags (`CampaignModel::recruitable_entry_flags`, `0x00B69BA0`; any flag makes the command
/// refuse the unit).
#[derive(Debug, Clone, PartialEq)]
pub struct AiRecruitable {
    /// Unit key.
    pub unit_key: String,
    /// What recruiting it costs.
    pub cost: i32,
    /// The entry's unavailability flags (0: the command accepts it).
    pub flags: u32,
    /// How many more the faction may queue before its unit cap flags the entry
    /// (`UnitTypeCounts::cap_room`); `None` without a cap.
    pub cap_room: Option<usize>,
    /// A ship (it goes to the region's naval queue).
    pub naval: bool,
    /// How many more items the queue of its kind takes (`CampaignModel::recruitment_queue_room`).
    pub queue_room: usize,
}

/// Something a region can build now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOption {
    /// The slot.
    pub slot: SlotRef,
    /// The building level to build.
    pub level_key: String,
    /// The building level standing there now (none for an empty slot).
    pub from: Option<String>,
    /// Cost the model charges.
    pub cost: i32,
}

/// A unit in an army.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUnit {
    /// Unit key.
    pub key: String,
    /// Men.
    pub men: u32,
    /// Full strength.
    pub max_men: u32,
}

/// An army or navy.
#[derive(Debug, Clone, PartialEq)]
pub struct AiArmy {
    /// Id.
    pub id: ForceId,
    /// Owner.
    pub faction: FactionId,
    /// Commander (forces move with their commander).
    pub commander: Option<CharacterId>,
    /// Position: the commander's, or the settlement of the region it garrisons.
    pub position: (f64, f64),
    /// Commander's movement points left (0 without a commander).
    pub movement_points: i32,
    /// Units.
    pub units: Vec<AiUnit>,
    /// Navy.
    pub is_navy: bool,
}

/// The snapshot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiWorld {
    /// Factions.
    pub factions: BTreeMap<FactionId, AiFaction>,
    /// Regions.
    pub regions: BTreeMap<RegionId, AiRegion>,
    /// Armies and navies with a known position.
    pub armies: BTreeMap<ForceId, AiArmy>,
    /// Stances, both directions as stored.
    pub stances: BTreeMap<(FactionId, FactionId), Stance>,
    /// Movement points per map unit (`road_level_0_action_point_cost` on `main`).
    pub move_cost_per_unit: f32,
    /// `taxes_levels`: (key, rate %), lowest rate first.
    pub tax_levels: Vec<(String, i32)>,
    /// Public order a tax level gives its class: `[class][level key]`, class 0 lower, 1 upper (the
    /// `happy_*` / `repression_*` effects of that class's `taxes_effects` row, the same rule as
    /// `ntw_sim::campaign::economy::public_order`).
    pub tax_order: [BTreeMap<String, f32>; 2],
    /// The technologies the model knows (`technologies` rows), for the RESEARCH_TECHNOLOGY
    /// behaviour. `None` when the snapshot was built without the model's rules.
    pub technologies: Option<BTreeMap<String, AiTech>>,
}

/// Distance between two map points.
pub fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// The `cdir_unit_balances` group of a unit: `infantry`, `cavalry`, `artillery` or `navy`.
pub fn balance_group(info: &AiUnitInfo) -> &'static str {
    match info.category.as_str() {
        "cavalry" => "cavalry",
        "artillery" => "artillery",
        "infantry" => "infantry",
        _ => "navy",
    }
}

impl AiWorld {
    /// The snapshot of the campaign model. The regions' build options come from the model's own option
    /// rule (empty without rules: the model permits nothing then);
    /// when the model has rules (DB data), the recruitable units and free recruitment points come from
    /// its own validation helpers too, so every order the AI gives is one the model accepts. The
    /// recruitable entries are priced for every region ([`AiWorld::from_model_for`] prices one faction's).
    pub fn from_model(m: &CampaignModel) -> Self {
        Self::from_model_for(m, None)
    }

    /// [`AiWorld::from_model`], pricing the recruitable entries ([`AiRegion::recruitable`]) of `faction`'s
    /// regions only (each needs the region's effect set), or of every region with `None`.
    pub fn from_model_for(m: &CampaignModel, faction: Option<FactionId>) -> Self {
        let w = &m.world;
        let mut out = AiWorld { move_cost_per_unit: m.rules.road_cost(0), ..Default::default() };
        let has_rules = !m.rules.buildings.is_empty() || !m.rules.units.is_empty();
        // The effect sums, once for the whole snapshot.
        let fx = has_rules.then(|| ntw_sim::campaign::effects::Effects::compute(m));
        let new_levels = m.new_levels_by_slot_type();
        out.tax_levels = m.rules.tax_levels.iter().map(|(k, v)| (k.clone(), *v)).collect();
        out.tax_levels.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        for (ci, class) in [TaxClass::Lower, TaxClass::Upper].into_iter().enumerate() {
            for (level, _) in &out.tax_levels {
                let v: f32 = m
                    .rules
                    .effects
                    .tax_rows(class.key(), level)
                    .map_or(0.0, |list| list.iter().filter(|(k, _)| order_effect_applies(k, class)).map(|(_, v)| *v).sum());
                out.tax_order[ci].insert(level.clone(), v);
            }
        }
        let mut owned: BTreeMap<FactionId, usize> = BTreeMap::new();
        let mut garrison_pos: BTreeMap<ForceId, (f64, f64)> = BTreeMap::new();
        out.technologies = has_rules.then(|| {
            m.rules
                .technologies
                .iter()
                .map(|(k, t)| {
                    (
                        k.clone(),
                        AiTech { cost: t.cost, thread: ntw_sim::campaign::research::thread_of(k) },
                    )
                })
                .collect()
        });
        // Each owner's unit-type counts (what its unit caps are held against), counted once per snapshot.
        let mut unit_counts = BTreeMap::new();
        for r in w.regions.values() {
            *owned.entry(r.owner).or_default() += 1;
            let pos = (r.settlement.position.0.to_f64(), r.settlement.position.1.to_f64());
            if let Some(g) = r.garrison {
                garrison_pos.insert(g, pos);
            }
            let buildings: Vec<Option<String>> = r.slots.iter().map(|s| s.building.as_ref().map(|b| b.level_key.clone())).collect();
            let value = 1.0 + buildings.iter().flatten().count() as f64 + r.population as f64 / 1e6;
            let schools = has_rules.then(|| {
                r.slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| {
                        let p = m.research_parts(r.id, i)?;
                        Some(AiSchool {
                            slot: i,
                            id: s.id,
                            rate: [p.rate(0), p.rate(1), p.rate(2)],
                            researching: m.researching_at(r.id, i),
                        })
                    })
                    .collect()
            });
            // The model's option rule, with its permission and script-restriction tests (empty without rules).
            let build_options = build_options(m, &new_levels, r);
            let (recruit_capacity, recruitable) = if has_rules {
                let used = r
                    .recruitment_queue
                    .iter()
                    .filter(|i| !m.rules.units.get(&i.unit_key).is_some_and(|u| u.is_naval))
                    .count() as u32;
                let cap = m.recruitment_points_with(fx.as_ref().expect("has rules"), r.id, false).saturating_sub(used);
                let entries = if faction.is_none_or(|f| f == r.owner) {
                    recruitable_entries(m, r, unit_counts.entry(r.owner).or_insert_with(|| m.unit_type_counts(r.owner)))
                } else {
                    Vec::new()
                };
                (Some(cap), entries)
            } else {
                (None, Vec::new())
            };
            out.regions.insert(
                r.id,
                AiRegion {
                    id: r.id,
                    key: r.key.clone(),
                    owner: r.owner,
                    position: pos,
                    buildings,
                    recruiting: r.recruitment_queue.len(),
                    recruit_capacity,
                    recruitable,
                    build_options,
                    constructing: !r.construction.is_empty(),
                    value,
                    gdp: r.gdp,
                    public_order: has_rules.then(|| {
                        let po = ntw_sim::campaign::economy::public_order(m, r.id);
                        [po.lower, po.upper]
                    }),
                    schools,
                    base_value: w.region_base_values.get(&r.id).copied(),
                },
            );
        }
        for f in w.factions.values() {
            let inc = fx.as_ref().map(|fx| ntw_sim::campaign::economy::faction_income_with(m, fx, f.id));
            let technologies = w.faction_details.get(&f.id).map(|d| d.technologies.iter().cloned().collect());
            out.factions.insert(
                f.id,
                AiFaction {
                    key: f.key.clone(),
                    treasury: f.treasury,
                    regions: owned.get(&f.id).copied().unwrap_or(0),
                    income: inc.map(|i| i.taxes.saturating_add(i.other)),
                    upkeep: inc.map(|i| i.upkeep),
                    tax: [f.tax_lower.clone(), f.tax_upper.clone()],
                    technologies,
                    ai_keys: w.ai_keys.get(&f.id).cloned(),
                },
            );
            for (o, s) in &f.diplomacy {
                out.stances.insert((f.id, *o), *s);
            }
        }
        for fo in w.forces.values() {
            let (position, mp) = match fo.commander.and_then(|c| w.characters.get(&c)) {
                Some(ch) => ((ch.position.0.to_f64(), ch.position.1.to_f64()), ch.movement_points),
                None => match garrison_pos.get(&fo.id) {
                    Some(p) => (*p, 0),
                    None => continue, // position unknown
                },
            };
            out.armies.insert(
                fo.id,
                AiArmy {
                    id: fo.id,
                    faction: fo.faction,
                    commander: fo.commander,
                    position,
                    movement_points: mp,
                    units: fo.units.iter().map(|u| AiUnit { key: u.unit_key.clone(), men: u.men, max_men: u.max_men }).collect(),
                    is_navy: fo.is_navy,
                },
            );
        }
        out
    }

    /// Stance of `a` towards `b` (neutral when nothing is stored).
    pub fn stance(&self, a: FactionId, b: FactionId) -> Stance {
        self.stances.get(&(a, b)).copied().unwrap_or_default()
    }

    /// Strength of an army: sum of unit quality (`cdir_unit_qualities`, else the `units` #4 battle cost,
    /// else 100) x men / max men. PROVISIONAL (the original's strength analysers are UNKNOWN).
    pub fn army_strength(&self, a: &AiArmy, data: &CampaignAiData) -> f32 {
        a.units
            .iter()
            .map(|u| {
                let q = data.units.get(&u.key).map_or(100, |i| i.quality.unwrap_or(i.battle_cost.max(1)));
                q as f32 * u.men as f32 / u.max_men.max(1) as f32
            })
            .sum()
    }

    /// Total land strength of a faction.
    pub fn faction_strength(&self, f: FactionId, data: &CampaignAiData) -> f32 {
        self.armies.values().filter(|a| a.faction == f && !a.is_navy).map(|a| self.army_strength(a, data)).sum()
    }

    /// The [`NEIGHBOURS_PER_REGION`] nearest other settlements of region `r` (PROVISIONAL
    /// adjacency; ties by region id).
    pub fn nearest_regions(&self, r: RegionId) -> Vec<RegionId> {
        let Some(me) = self.regions.get(&r) else { return Vec::new() };
        let mut others: Vec<(f64, RegionId)> =
            self.regions.values().filter(|o| o.id != r).map(|o| (dist(me.position, o.position), o.id)).collect();
        others.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        others.into_iter().take(NEIGHBOURS_PER_REGION).map(|(_, id)| id).collect()
    }

    /// `f`'s region groups: connected blocks of its regions, two regions joined when one is among
    /// the other's [`Self::nearest_regions`]. Each group sorted by region id, groups by their
    /// first region. INFERRED meaning (the CAI's region groups, whose LOST / SPLIT / MERGED states
    /// the composite value analyser reads); PROVISIONAL adjacency.
    pub fn region_groups(&self, f: FactionId) -> Vec<Vec<RegionId>> {
        let own: Vec<RegionId> = self.regions.values().filter(|r| r.owner == f).map(|r| r.id).collect();
        let mut link: BTreeMap<RegionId, BTreeSet<RegionId>> = own.iter().map(|r| (*r, BTreeSet::new())).collect();
        for &r in &own {
            for o in self.nearest_regions(r) {
                if link.contains_key(&o) {
                    link.get_mut(&r).expect("own").insert(o);
                    link.get_mut(&o).expect("own").insert(r);
                }
            }
        }
        let mut seen = BTreeSet::new();
        let mut groups = Vec::new();
        for &start in &own {
            if !seen.insert(start) {
                continue;
            }
            let mut g = vec![start];
            let mut i = 0;
            while i < g.len() {
                for &n in &link[&g[i]] {
                    if seen.insert(n) {
                        g.push(n);
                    }
                }
                i += 1;
            }
            g.sort();
            groups.push(g);
        }
        groups
    }

    /// Factions owning one of the nearest settlements of `f`'s regions (PROVISIONAL adjacency).
    pub fn neighbour_factions(&self, f: FactionId) -> BTreeSet<FactionId> {
        let mut out = BTreeSet::new();
        for r in self.regions.values().filter(|r| r.owner == f) {
            let mut others: Vec<(f64, RegionId, FactionId)> = self
                .regions
                .values()
                .filter(|o| o.id != r.id)
                .map(|o| (dist(r.position, o.position), o.id, o.owner))
                .collect();
            others.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            for (_, _, owner) in others.into_iter().take(NEIGHBOURS_PER_REGION) {
                if owner != f {
                    out.insert(owner);
                }
            }
        }
        out
    }
}

/// Region `r`'s recruitable entries for its owner, priced and flagged by the model's own functions (the ones
/// the queue command uses), in unit key order. The region's effect set is built once for all of them, and
/// `counts` (the owner's unit-type counts) once per snapshot.
fn recruitable_entries(m: &CampaignModel, r: &ntw_sim::campaign::Region, counts: &UnitTypeCounts<'_>) -> Vec<AiRecruitable> {
    let set = ntw_sim::campaign::economy::region_effect_set(m, r);
    let room = [m.recruitment_queue_room(r, false), m.recruitment_queue_room(r, true)];
    let mut out: Vec<AiRecruitable> = m
        .recruitable_units(r.id)
        .into_iter()
        .filter_map(|unit_key| {
            let unit = m.rules.units.get(&unit_key)?;
            let cost = ntw_sim::campaign::economy::recruitment_cost_in(&m.rules, &set, &unit_key, unit);
            let flags = m.recruitable_entry_flags(r, &unit_key, unit, cost, counts);
            let cap_room = counts.cap_room(&unit_key, unit);
            let queue_room = room[usize::from(unit.is_naval)];
            Some(AiRecruitable { unit_key, cost, flags, cap_room, naval: unit.is_naval, queue_room })
        })
        .collect();
    out.sort_by(|a, b| a.unit_key.cmp(&b.unit_key));
    out
}

/// What region `r` can build now: the model's one option rule
/// (`CampaignModel::region_construction_options`) for every slot, the walls and the road, keeping the
/// researched levels (the command's `can_build` refuses the others). `new_levels` is the model's
/// level-0 index, built once per snapshot.
fn build_options(m: &CampaignModel, new_levels: &BTreeMap<&str, Vec<&str>>, r: &ntw_sim::campaign::Region) -> Vec<BuildOption> {
    let mut out = Vec::new();
    for (slot, options) in m.region_construction_options(r.id, Some(new_levels)) {
        let from = r.construction_slot(slot).and_then(|(_, b)| b).map(|b| b.level_key.clone());
        for o in options.into_iter().filter(|o| o.tech) {
            out.push(BuildOption { slot, level_key: o.level_key, from: from.clone(), cost: o.cost });
        }
    }
    out
}

/// True if a `taxes_effects` key changes the public order of `class`: `repression_*`, or a
/// `happy_*` key for all classes or naming the class (the rule of
/// `ntw_sim::campaign::economy::public_order`, repeated here because it is private there).
pub fn order_effect_applies(effect: &str, class: TaxClass) -> bool {
    if effect.starts_with("repression_") {
        return true;
    }
    if !effect.starts_with("happy_") {
        return false;
    }
    let name = match class {
        TaxClass::Lower => "lower",
        TaxClass::Upper => "upper",
    };
    effect.ends_with("_all") || effect.ends_with("_all_classes") || effect.split('_').any(|p| p == name)
}
