//! [`GameDatabase`]: every implemented table, loaded and indexed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ntw_formats::pack::{PackError, Vfs};
use ntw_formats::projectile_fx::{ExplosionRow, ExplosionTable, ImpactRow, ImpactTable, TrailRow, TrailTable, PROJECTILE_IMPACTS, PROJECTILE_TRAILS, PROJECTILES_EXPLOSIONS};
use ntw_sim::battle::attributes::{UnitAttributes, UnitCapabilities};
use ntw_sim::battle::fatigue::KvFatigue;
use ntw_sim::battle::morale::KvMorale;

use crate::kv::{self, KvRules, KvTable, SimKvRules};
use crate::record::{DbRecord, Table};
use crate::schemas::*;
use crate::DataError;

/// Where a [`GameDatabase`] came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataSource {
    /// Loaded from the player's install (the `data` folder path).
    Install(PathBuf),
    /// Loaded from packs mounted in a [`Vfs`] by the caller.
    Vfs,
    /// [`GameDatabase::test_fixture`]: made-up placeholder numbers, **not game data**.
    TestFixture,
}

/// All loaded game tables.
///
/// # Eager loading
/// [`from_install`](Self::from_install) reads and decodes every implemented table at once.
/// They add up to well under 1 MB and decode in milliseconds, so loading lazily (as the
/// original does) would only add complexity. Eager loading also means a bad or modded
/// table is reported at start-up, not in the middle of a battle.
///
/// # Lookups and foreign keys
/// `units.key` → `unit_stats_land.key` → `unit_stats_land.projectile` → `projectiles.key`.
/// Artillery has no direct projectile. Its `gun_type` leads to `gun_type_to_projectiles`
/// instead; see [`gun_projectiles`](Self::gun_projectiles).
#[derive(Debug, Clone)]
pub struct GameDatabase {
    source: DataSource,
    /// `units`.
    pub units: Table<UnitRecord>,
    /// `unit_stats_land`.
    pub unit_stats_land: Table<UnitStatsLand>,
    /// `projectiles`.
    pub projectiles: Table<Projectile>,
    /// `gun_type_to_projectiles` (several rows per gun type).
    pub gun_type_to_projectiles: Table<GunTypeProjectile>,
    /// `projectiles_explosions`, keyed by `projectiles.explosion`: the groups a detonation plays.
    pub projectile_explosions: ExplosionTable,
    /// `projectile_impacts`, keyed by the ball class `projectiles` names in column 33.
    pub projectile_impacts: ImpactTable,
    /// `projectile_trails`, keyed by the trail texture `projectiles` names in **column 6**. This is
    /// the colour and geometry of a shot's trail; the trail's *effect group* is column 32.
    pub projectile_trails: TrailTable,
    /// `unit_to_unit_abilities_junctions` (several rows per unit).
    pub unit_abilities: Table<UnitToUnitAbility>,
    /// `unit_class_to_unit_ability_junctions`.
    pub unit_class_abilities: Table<UnitClassToUnitAbility>,
    /// `technology_effects_junction`.
    pub technology_effects: Table<TechnologyEffect>,
    /// `effect_bonus_value_unit_ability_junctions`.
    pub effect_abilities: Table<EffectUnitAbility>,
    /// `effect_bonus_value_shot_type_junctions`.
    pub effect_shot_types: Table<EffectShotType>,
    /// `battle_weather_types`.
    pub battle_weather_types: Table<BattleWeatherType>,
    /// `battle_climate_weather_descriptions`.
    pub battle_climate_weather: Table<BattleClimateWeather>,
    /// `fatigue_effects` (several rows per threshold).
    pub fatigue_effects: Table<FatigueEffect>,
    /// `unit_stats_land_experience_bonuses` (10 rows, one per experience level).
    pub unit_stats_land_experience_bonuses: Table<UnitStatsLandExperienceBonuses>,
    /// `unit_stats_naval_experience_bonuses` (the naval twin of the table above, same ranks).
    pub unit_stats_naval_experience_bonuses: Table<UnitStatsNavalExperienceBonuses>,
    /// `factions`.
    pub factions: Table<FactionRecord>,
    /// `regions`.
    pub regions: Table<RegionRecord>,
    /// `building_levels`.
    pub building_levels: Table<BuildingLevel>,
    /// `technologies`.
    pub technologies: Table<Technology>,
    /// Campaign-map tables (tunables, buildings, recruitment, taxes, agents, map slots).
    pub campaign: crate::campaign::CampaignTables,
    /// `_kv_rules`, with the exe's int/float handling.
    pub kv_rules: KvRules,
    /// `_kv_rules` converted for the simulation (ints truncated like the exe, floats kept).
    pub kv_rules_sim: SimKvRules,
    /// `_kv_morale`, converted (truncated) for the simulation.
    pub kv_morale: KvMorale,
    /// `_kv_fatigue`, converted (truncated) for the simulation.
    pub kv_fatigue: KvFatigue,
    /// The raw `_kv_morale` rows (including keys the exe does not read).
    pub kv_morale_raw: KvTable,
    /// The raw `_kv_fatigue` rows.
    pub kv_fatigue_raw: KvTable,
    /// Gun type → its projectiles' keys in the exe's shot order, built at load from
    /// `gun_type_to_projectiles` and `projectiles` ([`index_gun_shots`]); whoever replaces those two
    /// tables rebuilds it (a stale index can only miss or misorder shots, never panic).
    gun_shots: HashMap<String, Vec<String>>,
}

/// A unit with its foreign keys already followed. Returned by [`GameDatabase::land_unit`].
#[derive(Debug, Clone, Copy)]
pub struct LandUnitView<'a> {
    /// The `units` row.
    pub unit: &'a UnitRecord,
    /// The `unit_stats_land` row with the same key.
    pub stats: &'a UnitStatsLand,
}

/// For each gun type, its projectiles' keys in the exe's shot order (see
/// [`GameDatabase::gun_projectiles`]): built once at load, so a volley's shot lookup is a few hash
/// lookups, no table scan. Keys, not row numbers, and every lookup is `Table::get` (the table's
/// one "first row of a key" rule), so a `projectiles` table replaced later is read as it is (a
/// missing key is skipped) and never indexed out of bounds. The sort key is the shot type enum
/// value (`ntw_sim`'s one lookup), an unknown name 0 as the exe's `0x00F59030`.
fn index_gun_shots(guns: &Table<GunTypeProjectile>, projectiles: &Table<Projectile>) -> HashMap<String, Vec<String>> {
    let mut by_gun: HashMap<&str, Vec<&Projectile>> = HashMap::new();
    // `gun_type_to_projectiles` rows naming no `projectiles` row: skipped, logged once per load.
    let mut missing: Vec<&str> = Vec::new();
    for g in guns.iter() {
        match projectiles.get(&g.projectile) {
            Some(p) => by_gun.entry(g.gun_type.as_str()).or_default().push(p),
            None => missing.push(&g.projectile),
        }
    }
    if let Some(first) = missing.first() {
        eprintln!("WARN ntw_data: {} gun_type_to_projectiles rows name no projectile (first: {first}); those shots are skipped", missing.len());
    }
    by_gun
        .into_iter()
        .map(|(gun, mut shots)| {
            shots.sort_by_key(|p| ntw_sim::battle::attributes::shot_type_value(&p.shot_type).unwrap_or(0));
            (gun.to_owned(), shots.into_iter().map(|p| p.key.clone()).collect())
        })
        .collect()
}

fn load<T: DbRecord>(vfs: &Vfs) -> Result<Table<T>, DataError> {
    Table::from_bytes(&vfs.read(&T::path())?)
}

/// Like [`load`] for a table the game can run without (only pictures or text depend on it): a
/// **missing** table becomes an empty one with a `WARN` line instead of failing the whole
/// database load. A table that is present but does not read (corrupt, wrong version, a bad mod
/// override) still fails the load like any other, so a data error is not hidden.
fn load_optional<T: DbRecord>(vfs: &Vfs) -> Result<Table<T>, DataError> {
    match load(vfs) {
        Err(DataError::Pack(PackError::NotFound(path))) => {
            eprintln!("WARN ntw_data: optional table {} not found ({path}); using an empty table", T::TABLE);
            Ok(Table::default())
        }
        other => other,
    }
}

fn load_campaign(vfs: &Vfs) -> Result<crate::campaign::CampaignTables, DataError> {
    Ok(crate::campaign::CampaignTables {
        variables: load(vfs)?,
        variable_overrides: load(vfs)?,
        building_effects: load(vfs)?,
        building_units: load(vfs)?,
        building_upgrades: load(vfs)?,
        government_relations: load(vfs)?,
        attitude_thresholds: load(vfs)?,
        naval_stats: load(vfs)?,
        chains: load(vfs)?,
        chain_slots: load(vfs)?,
        building_chains: load(vfs)?,
        tax_levels: load(vfs)?,
        tax_keys: load(vfs)?,
        tax_effects: load(vfs)?,
        government_effects: load(vfs)?,
        agents: load(vfs)?,
        unit_factions: load(vfs)?,
        map_slots: load(vfs)?,
        map_towns: load(vfs)?,
        slot_art: load(vfs)?,
        slot_templates_models: load(vfs)?,
        trade_nodes: load(vfs)?,
        tech_requirements: load(vfs)?,
        tech_factions: load(vfs)?,
        unit_techs: load(vfs)?,
        building_techs: load(vfs)?,
        building_faction_variants: load(vfs)?,
        building_culture_variants: load(vfs)?,
        government_types: load(vfs)?,
        religion_relations: load(vfs)?,
        commodity_demand: load(vfs)?,
        ground_types: load(vfs)?,
        agent_cultures: load(vfs)?,
        historical_characters: load(vfs)?,
        effects: crate::effects::EffectTables {
            bonus_basic: load(vfs)?,
            bonus_unit_category: load(vfs)?,
            bonus_unit_class: load(vfs)?,
            bonus_pop_class: load(vfs)?,
            bonus_agent: load(vfs)?,
            bonus_religion: load(vfs)?,
            bonus_chain: load(vfs)?,
            religion_conversion: load(vfs)?,
            technology: load(vfs)?,
            building_factionwide: load(vfs)?,
            trait_levels: load(vfs)?,
            trait_level: load(vfs)?,
            ancillary: load(vfs)?,
            trait_attribute: load(vfs)?,
            ancillary_attribute: load(vfs)?,
            ministerial: load(vfs)?,
            ministerial_effectiveness: load(vfs)?,
            difficulty: load(vfs)?,
        },
        characters: crate::characters::CharacterTables {
            attributes: load_optional(vfs)?,
            traits: load(vfs)?,
            trait_info: load(vfs)?,
            antitraits: load(vfs)?,
            trait_agents: load(vfs)?,
            ancillaries: load(vfs)?,
            ancillary_agents: load(vfs)?,
            ancillary_excluded: load(vfs)?,
            ancillary_subcultures: load(vfs)?,
            subcultures: load(vfs)?,
        },
    })
}

fn load_kv(vfs: &Vfs, name: &'static str) -> Result<KvTable, DataError> {
    KvTable::from_bytes(name, &vfs.read(&KvTable::path(name))?)
}

impl GameDatabase {
    /// Loads everything from an install's `data` folder (read-only), mounting its packs in
    /// game load order.
    pub fn from_install(data_dir: impl AsRef<Path>) -> Result<Self, DataError> {
        let data_dir = data_dir.as_ref();
        let vfs = Vfs::open_install(data_dir)?;
        let mut db = Self::from_vfs(&vfs)?;
        db.source = DataSource::Install(data_dir.to_path_buf());
        Ok(db)
    }

    /// Loads everything from an already-mounted [`Vfs`] (e.g. with extra mod packs).
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, DataError> {
        let kv_rules = KvRules { table: load_kv(vfs, "_kv_rules")? };
        kv_rules.check_complete()?;
        let kv_morale_raw = load_kv(vfs, "_kv_morale")?;
        let kv_fatigue_raw = load_kv(vfs, "_kv_fatigue")?;
        let projectiles: Table<Projectile> = load(vfs)?;
        // The explosion table's row keys are `projectiles.explosion`'s own values, so the rows can
        // be cut without knowing the table's numeric column layout (ntw_formats::projectile_fx).
        let projectile_explosions = ExplosionTable::read(&vfs.read(PROJECTILES_EXPLOSIONS)?)
            .map_err(|error| DataError::ProjectileFx { table: "projectiles_explosions", error })?;
        let projectile_impacts = ImpactTable::read(&vfs.read(PROJECTILE_IMPACTS)?)
            .map_err(|error| DataError::ProjectileFx { table: "projectile_impacts", error })?;
        // Regular table: key, blend mode, ten floats, next key (ntw_formats::projectile_fx).
        let projectile_trails = TrailTable::read(&vfs.read(PROJECTILE_TRAILS)?)
            .map_err(|error| DataError::ProjectileFx { table: "projectile_trails", error })?;
        let gun_type_to_projectiles = load(vfs)?;
        let gun_shots = index_gun_shots(&gun_type_to_projectiles, &projectiles);
        Ok(Self {
            source: DataSource::Vfs,
            units: load(vfs)?,
            unit_stats_land: load(vfs)?,
            projectiles,
            gun_type_to_projectiles,
            projectile_explosions,
            projectile_impacts,
            projectile_trails,
            unit_abilities: load(vfs)?,
            unit_class_abilities: load(vfs)?,
            technology_effects: load(vfs)?,
            effect_abilities: load(vfs)?,
            effect_shot_types: load(vfs)?,
            battle_weather_types: load(vfs)?,
            battle_climate_weather: load(vfs)?,
            fatigue_effects: load(vfs)?,
            unit_stats_land_experience_bonuses: load(vfs)?,
            unit_stats_naval_experience_bonuses: load(vfs)?,
            factions: load(vfs)?,
            regions: load(vfs)?,
            building_levels: load(vfs)?,
            technologies: load(vfs)?,
            campaign: load_campaign(vfs)?,
            kv_rules_sim: SimKvRules::try_from(&kv_rules)?,
            kv_rules,
            kv_morale: KvMorale::try_from(&kv_morale_raw)?,
            kv_fatigue: KvFatigue::try_from(&kv_fatigue_raw)?,
            kv_morale_raw,
            kv_fatigue_raw,
            gun_shots,
        })
    }

    /// Where this data came from.
    pub fn source(&self) -> &DataSource {
        &self.source
    }

    /// True for [`test_fixture`](Self::test_fixture) data (made-up numbers).
    pub fn is_test_fixture(&self) -> bool {
        self.source == DataSource::TestFixture
    }

    /// The `units` row for `key`.
    pub fn unit(&self, key: &str) -> Option<&UnitRecord> {
        self.units.get(key)
    }

    /// The `unit_stats_land` row for `key` (the same key as in `units`).
    pub fn unit_stats(&self, key: &str) -> Option<&UnitStatsLand> {
        self.unit_stats_land.get(key)
    }

    /// The `unit_stats_land_experience_bonuses` row for an experience rank (0..9), by its key.
    pub fn experience_bonuses(&self, rank: u8) -> Option<&UnitStatsLandExperienceBonuses> {
        self.unit_stats_land_experience_bonuses.get(&rank.to_string())
    }

    /// The per-tick fatigue bonus column (`+0x20`) of `unit_stats_land_experience_bonuses` in
    /// **file order**, which is the order the exe indexes it by (rank 0..9; CONFIRMED read in
    /// `0x00670F40`). Empty when the table did not load, which makes the lookup give 0.
    pub fn experience_fatigue_bonuses(&self) -> Vec<i32> {
        self.unit_stats_land_experience_bonuses.iter().map(|r| r.fatigue_bonus).collect()
    }

    /// The `unit_stats_naval_experience_bonuses` row for an experience rank (0..9), by its key —
    /// the naval twin of [`GameDatabase::experience_bonuses`], read by `0x00ED49A0`'s naval
    /// branch (`this+0xA0 != 0`).
    pub fn naval_experience_bonuses(&self, rank: u8) -> Option<&UnitStatsNavalExperienceBonuses> {
        self.unit_stats_naval_experience_bonuses.get(&rank.to_string())
    }

    /// The experience-adjusted **cost** of a naval unit: `row+0x1C + ROUND(base * row+0x20)`, the
    /// naval branch of `0x00ED49A0` (CONFIRMED — note the base is *not* added, it is scaled).
    /// `base` is the cost the caller starts from (the recruit or upkeep cost, chosen by the exe's
    /// flag) and `rank` the unit's experience level. Without the row the exe returns `base`.
    pub fn naval_experience_adjusted_cost(&self, rank: u8, base: i32) -> i32 {
        match self.naval_experience_bonuses(rank) {
            Some(r) => r.unknown_1c + (base as f32 * r.unknown_20).round() as i32,
            None => base,
        }
    }

    /// The land twin of [`GameDatabase::naval_experience_adjusted_cost`]: `row+0x24 +
    /// ROUND(base * row+0x28)`, the land branch of `0x00ED49A0` (CONFIRMED). This is the cost the
    /// campaign's auto-build (`0x0045CB50` → `0x0045D170`) pays and the unit info panel shows as
    /// "XpAdjustedCost" (`0x005CD340`) — a campaign cost, NOT a battle stat.
    pub fn experience_adjusted_cost(&self, rank: u8, base: i32) -> i32 {
        match self.experience_bonuses(rank) {
            Some(r) => r.unknown_24 + (base as f32 * r.unknown_28).round() as i32,
            None => base,
        }
    }

    /// The land XP-cost rows as `(rank, flat, multiplier)`, i.e. what a campaign rules loader would
    /// copy into the campaign's cost rules (`+0x24` / `+0x28`, the two columns `0x00ED49A0` reads;
    /// not wired into `ntw_campaign` yet). In table order, which is rank 0..9; a row whose key is not a rank
    /// number is left out, since it cannot be looked up as one.
    pub fn experience_cost_rows(&self) -> Vec<(u8, i32, f32)> {
        self.unit_stats_land_experience_bonuses
            .iter()
            .filter_map(|r| Some((r.rank.parse::<u8>().ok()?, r.unknown_24, r.unknown_28)))
            .collect()
    }

    /// The naval XP-cost rows as `(rank, flat, multiplier)`, i.e. what a campaign rules loader
    /// copies into the campaign's cost rules (`+0x1C` / `+0x20`; `ntw_campaign::rules` loads them).
    pub fn naval_experience_cost_rows(&self) -> Vec<(u8, i32, f32)> {
        self.unit_stats_naval_experience_bonuses
            .iter()
            .filter_map(|r| Some((r.rank.parse::<u8>().ok()?, r.unknown_1c, r.unknown_20)))
            .collect()
    }

    /// The `projectiles` row for `key`.
    pub fn projectile(&self, key: &str) -> Option<&Projectile> {
        self.projectiles.get(key)
    }

    /// The `factions` row for `key`.
    pub fn faction(&self, key: &str) -> Option<&FactionRecord> {
        self.factions.get(key)
    }

    /// The `regions` row for `key`.
    pub fn region(&self, key: &str) -> Option<&RegionRecord> {
        self.regions.get(key)
    }

    /// The `building_levels` row for `key`.
    pub fn building_level(&self, key: &str) -> Option<&BuildingLevel> {
        self.building_levels.get(key)
    }

    /// The `technologies` row for `key`.
    pub fn technology(&self, key: &str) -> Option<&Technology> {
        self.technologies.get(key)
    }

    /// Follows `stats.projectile` to its `projectiles` row (infantry and cavalry firearms).
    pub fn unit_projectile(&self, stats: &UnitStatsLand) -> Option<&Projectile> {
        self.projectile(stats.projectile.as_deref()?)
    }

    /// The projectile a unit fires: its own [`unit_projectile`](Self::unit_projectile), else
    /// (artillery) the first of its gun type's projectiles in the exe's order
    /// ([`gun_projectiles`](Self::gun_projectiles): by shot type), the shot a battle loads at the
    /// start (INFERRED that the loaded shot is the gun record's first). The one rule for battle,
    /// its effects and sounds and AI strength; the unit card's Range has its own exe rule
    /// ([`unit_card_range`](Self::unit_card_range)). Allocates nothing (called per volley).
    pub fn primary_projectile(&self, stats: &UnitStatsLand) -> Option<&Projectile> {
        self.unit_projectile(stats).or_else(|| self.gun_shots(stats).next())
    }

    /// The unit attribute flags from the boolean `unit_stats_land` columns (record offsets and
    /// columns CONFIRMED from the record constructor `0x00E8CFF0`/`0x00E8F1D0`, names INFERRED; see
    /// `ntw_sim::battle::attributes` and BATTLE_FIDELITY.md §5).
    pub fn unit_attributes(stats: &UnitStatsLand) -> UnitAttributes {
        UnitAttributes {
            skirmisher: stats.unknown_1e9,        // col 53
            marksmen: stats.unknown_1ec,          // col 56
            col63: stats.unknown_1f3,             // col 63
            col64: stats.unknown_1f4,             // col 64
            steadfast: stats.unknown_206,         // col 71
            impetuous: stats.unknown_207,         // col 72
            frightens_horses: stats.unknown_208,  // col 73
            frightens_enemy: stats.unknown_209,   // col 74
            inspires: stats.unknown_20a,          // col 75
            good_stamina: stats.unknown_20b,      // col 76
            climate_exempt_2c: stats.unknown_20d, // col 78
            climate_exempt_30: stats.unknown_20e, // col 79
        }
    }

    /// The unit card's capability block for a unit built from the database (not a battle file),
    /// with every technology-gated ability available ([`GameDatabase::unit_capabilities_with`]
    /// without technology state).
    pub fn unit_capabilities(&self, unit_key: &str, unit_class: &str) -> UnitCapabilities {
        self.unit_capabilities_with(unit_key, unit_class, None)
    }

    /// The unit card's capability block for a unit built from the database: the abilities of
    /// `unit_to_unit_abilities_junctions` and of its class in `unit_class_to_unit_ability_junctions`
    /// (drill names set the firing drill). INFERRED source (BATTLE_FIDELITY.md §22).
    ///
    /// Technologies: an ability that some technology enables (`technology_effects_junction` →
    /// `effect_bonus_value_unit_ability_junctions`, e.g. `military1_fire_and_advance` →
    /// `enable_fire_and_advance` → `fire_and_advance`) is kept only when one of the `researched`
    /// technologies has that effect (INFERRED rule). Enabling effects that no technology has
    /// (`enable_square_formation`, `enable_fire_by_rank`, … left over from Empire) gate nothing.
    /// `None` = no technology state (a battle outside a campaign): everything the unit lists is
    /// kept (PROVISIONAL).
    pub fn unit_capabilities_with(&self, unit_key: &str, unit_class: &str, researched: Option<&[String]>) -> UnitCapabilities {
        let enabled = researched.map(|t| self.technology_unlocks(t).0);
        let keep = |a: &str| match &enabled {
            Some(on) => !self.ability_needs_technology(a) || on.iter().any(|x| x.eq_ignore_ascii_case(a)),
            None => true,
        };
        let own = self.unit_abilities.iter().filter(|r| r.unit.eq_ignore_ascii_case(unit_key)).map(|r| r.ability.as_str());
        let class =
            self.unit_class_abilities.iter().filter(|r| r.class.eq_ignore_ascii_case(unit_class)).map(|r| r.ability.as_str());
        let names: Vec<&str> = own.chain(class).filter(|a| keep(a)).collect();
        UnitCapabilities::from_names(names, std::iter::empty())
    }

    /// The abilities and shot types the `researched` technologies enable: their effects
    /// (`technology_effects_junction`) looked up in `effect_bonus_value_unit_ability_junctions` and
    /// `effect_bonus_value_shot_type_junctions` (CONFIRMED tables, bonus value `enable`).
    pub fn technology_unlocks(&self, researched: &[String]) -> (Vec<String>, Vec<String>) {
        let effects: Vec<&str> = self
            .technology_effects
            .iter()
            .filter(|r| researched.iter().any(|t| t.eq_ignore_ascii_case(&r.technology)))
            .map(|r| r.effect.as_str())
            .collect();
        let has = |e: &str| effects.iter().any(|x| x.eq_ignore_ascii_case(e));
        let abilities = self.effect_abilities.iter().filter(|r| has(&r.effect)).map(|r| r.ability.clone()).collect();
        let shots = self.effect_shot_types.iter().filter(|r| has(&r.effect)).map(|r| r.shot_type.clone()).collect();
        (abilities, shots)
    }

    /// True if some technology enables shot type `name`: a technology effect
    /// (`technology_effects_junction`) that `effect_bonus_value_shot_type_junctions` maps to it.
    /// An enabling effect no technology has (`enable_canister_shot`, from Empire) gates nothing
    /// (INFERRED).
    pub fn shot_type_needs_technology(&self, name: &str) -> bool {
        self.effect_shot_types.iter().any(|r| r.shot_type.eq_ignore_ascii_case(name) && self.effect_of_some_technology(&r.effect))
    }

    /// True if some technology enables ability `name` (as [`GameDatabase::shot_type_needs_technology`]).
    pub fn ability_needs_technology(&self, name: &str) -> bool {
        self.effect_abilities.iter().any(|r| r.ability.eq_ignore_ascii_case(name) && self.effect_of_some_technology(&r.effect))
    }

    fn effect_of_some_technology(&self, effect: &str) -> bool {
        self.technology_effects.iter().any(|t| t.effect.eq_ignore_ascii_case(effect))
    }

    /// The effect **group** a projectile plays when it is fired — the muzzle flash, the smoke and
    /// the rest of the firing group, straight from `projectiles` column 31 (CONFIRMED group names
    /// of `effects\landbattle.xml`; see
    /// `ntw_data/tests/effects_data.rs::projectiles_name_their_own_fire_group_and_trail`).
    ///
    /// `None` for the rows with no group (grenades, fragments, arrows, the tutorial ball). A group
    /// the installed `effects\landbattle.xml` does not have is still returned — the caller checks
    /// it against the library, because a mod may add the group.
    pub fn fire_effect<'a>(&self, projectile: &'a Projectile) -> Option<&'a str> {
        projectile.fire_effect.as_deref()
    }

    /// The `projectiles_explosions` row a projectile detonates with: the fragment group, the air
    /// burst and the ground scorch, all CONFIRMED group names. `None` when the projectile has no
    /// `explosion` foreign key, or names a key the table does not have.
    pub fn explosion_effects<'a>(&'a self, projectile: &'a Projectile) -> Option<&'a ExplosionRow> {
        self.projectile_explosions.get(projectile.explosion.as_deref()?)
    }

    /// The `projectile_impacts` row a projectile's ball class plays on impact. `None` when the
    /// projectile names no ball class.
    pub fn impact_effects<'a>(&'a self, projectile: &'a Projectile) -> Option<&'a ImpactRow> {
        self.projectile_impacts.get(projectile.impact_ball.as_deref()?)
    }

    /// The `projectile_trails` row a projectile's trail is drawn with: its blend mode and its
    /// colour, reached through `projectiles`' **column 6** (`trail_texture`). `None` when the
    /// projectile names no trail texture.
    ///
    /// This is **not** [`Self::trail_group`]. Column 32 names the trail's *effect group* in
    /// `effects\landbattle.xml`; a shot's trail is both. CONFIRMED on the install by
    /// `ntw_data/tests/effects_data.rs::the_trail_group_and_the_trail_table_are_joined_by_column_six`:
    /// all five column-6 values over all 144 rows are this table's five keys.
    pub fn trail_row<'a>(&'a self, projectile: &'a Projectile) -> Option<&'a TrailRow> {
        self.projectile_trails.get(projectile.trail_texture.as_deref()?)
    }

    /// The effect group a projectile's trail plays, from `projectiles`' column 32. `None` for the
    /// 135 rows that have no trail group — which is not the same as "no trail", because column 6
    /// is set on all 144: see [`Self::trail_row`].
    pub fn trail_group<'a>(&self, projectile: &'a Projectile) -> Option<&'a str> {
        projectile.trail.as_deref()
    }

    /// The projectiles a unit's gun type can fire (artillery), in the exe's order: the gun type
    /// record (`GUN_TYPE_RECORD` ctor `0x00F41750`) collects them in table order and then sorts
    /// them by shot type enum value, ascending (`0x00F25CF0`, key: the projectile record's shot
    /// type record +0x10, the value `0x00F59030` gives the `shot_type` name, an unknown name 0),
    /// keeping table order among equal shot types (CONFIRMED for up to 32 shots, an insertion
    /// sort; a gun with more is UNKNOWN, ours stays stable). Empty if the unit has no gun type.
    /// From the index built at load (`index_gun_shots`).
    pub fn gun_projectiles(&self, stats: &UnitStatsLand) -> Vec<&Projectile> {
        self.gun_shots(stats).collect()
    }

    /// The gun's projectiles in the exe's order, from the load-time index (no table scan).
    fn gun_shots<'a>(&'a self, stats: &UnitStatsLand) -> impl Iterator<Item = &'a Projectile> + use<'a> {
        let rows = stats.gun_type.as_deref().and_then(|g| self.gun_shots.get(g)).map_or(&[][..], Vec::as_slice);
        rows.iter().filter_map(|k| self.projectiles.get(k))
    }

    /// The unit card's `Range` (the card snapshot's +0x5C, built by `0x008DF190`): for a unit
    /// with a gun type, the longest effective range among its gun's projectiles (the gun type
    /// record's +0x60, an unsigned maximum from 0 over the projectile records' +0x60, which the
    /// projectile record ctor `0x00F45970` fills from `projectiles` column 11, effective range);
    /// else its own projectile's effective range; else 0 (all CONFIRMED). The exe reads both as
    /// unsigned, so ours does too: a negative modded range is a huge value that wins the maximum,
    /// never a negative card Range (the gun maximum's `CMOVNC` at `0x00F41893` and the unsigned-to-float
    /// conversion at `0x008DF588`, CONFIRMED, UI_FIDELITY.md "Unit card Range"; the shipped ranges
    /// are all non-negative).
    /// PROVISIONAL: the exe stores the card Range as an f32, which rounds values above 2^24; ours
    /// keeps the exact integer (only a modded range that large differs).
    /// PROVISIONAL: a gun type key with no `gun_types` row (the exe then has no record and falls
    /// back to the projectile) counts as a gun type here, as we do not load `gun_types`.
    pub fn unit_card_range(&self, stats: &UnitStatsLand) -> u32 {
        // `as u32` reinterprets the exe's unsigned field (two's complement), not a clamp.
        if stats.gun_type.as_deref().is_some_and(|g| !g.is_empty()) {
            self.gun_shots(stats).map(|p| p.effective_range as u32).max().unwrap_or(0)
        } else {
            self.unit_projectile(stats).map_or(0, |p| p.effective_range as u32)
        }
    }

    /// A land unit with its foreign keys followed: unit → stats (its projectile: [`primary_projectile`](Self::primary_projectile)).
    /// `None` if the key is not a land unit (naval units have no `unit_stats_land` row).
    pub fn land_unit(&self, key: &str) -> Option<LandUnitView<'_>> {
        let unit = self.unit(key)?;
        let stats = self.unit_stats(key)?;
        Some(LandUnitView { unit, stats })
    }

    /// A tiny database of **made-up placeholder numbers** for tests and for running the
    /// app without an install. Nothing in it is Creative Assembly data. All keys start with
    /// `fixture_`, and the numbers were picked to be obviously artificial.
    pub fn test_fixture() -> Self {
        let units = vec![
            UnitRecord {
                key: "fixture_line_infantry".into(),
                dev_name: "Fixture Line Infantry (made up)".into(),
                category: "infantry".into(),
                unit_class: "infantry_line".into(),
                recruitment_cost: 111,
                secondary_cost: 111,
                upkeep: 11,
                ..Default::default()
            },
            UnitRecord {
                key: "fixture_foot_artillery".into(),
                dev_name: "Fixture Foot Artillery (made up)".into(),
                category: "artillery".into(),
                unit_class: "artillery_foot".into(),
                recruitment_cost: 222,
                secondary_cost: 222,
                upkeep: 22,
                ..Default::default()
            },
        ];
        let stats = vec![
            UnitStatsLand {
                key: "fixture_line_infantry".into(),
                num_men: 123,
                accuracy: 12,
                reload_skill: 34,
                ammunition: 5,
                melee_attack: 7,
                charge_bonus: 8,
                melee_defence: 9,
                morale: 10,
                firing_mechanism: "flintlock".into(),
                projectile: Some("fixture_ball".into()),
                spacing_file_close: 1.25,
                spacing_rank_close: 1.5,
                ..Default::default()
            },
            UnitStatsLand {
                key: "fixture_foot_artillery".into(),
                num_men: 21,
                num_guns: 3,
                is_artillery: true,
                accuracy: 21,
                gun_type: Some("fixture_gun".into()),
                morale: 4,
                ..Default::default()
            },
        ];
        let projectiles = vec![
            Projectile {
                key: "fixture_ball".into(),
                category: "missile".into(),
                projectiles_per_shot: 1,
                effective_range: 99,
                muzzle_velocity: 123.5,
                damage: 1.25,
                reload_time: 17,
                ..Default::default()
            },
            Projectile {
                key: "fixture_round_shot".into(),
                category: "artillery".into(),
                projectiles_per_shot: 1,
                effective_range: 999,
                damage: 12.5,
                ..Default::default()
            },
        ];
        let guns = vec![GunTypeProjectile {
            gun_type: "fixture_gun".into(),
            projectile: "fixture_round_shot".into(),
            muzzle_flash: String::new(),
        }];
        let factions = vec![FactionRecord {
            key: "fixture_faction".into(),
            screen_name: "Fixtureland (made up)".into(),
            category: "playable".into(),
            primary_r: 1.0,
            primary_g: 2.0,
            primary_b: 3.0,
            ..Default::default()
        }];
        // Two experience ranks per table, with the REAL shapes: a recruit costs its base, a veteran
        // pays a flat premium plus a multiplier of it (0x00ED49A0).
        let experience_bonuses = vec![
            UnitStatsLandExperienceBonuses { rank: "0".into(), fatigue_bonus: 0, unknown_24: 0, unknown_28: 1.0, ..Default::default() },
            UnitStatsLandExperienceBonuses { rank: "9".into(), fatigue_bonus: -3, unknown_24: 360, unknown_28: 1.9, ..Default::default() },
        ];
        let naval_experience_bonuses = vec![
            UnitStatsNavalExperienceBonuses { rank: "0".into(), unknown_1c: 0, unknown_20: 1.0, ..Default::default() },
            UnitStatsNavalExperienceBonuses { rank: "9".into(), unknown_1c: 255, unknown_20: 1.45, ..Default::default() },
        ];
        let regions = vec![RegionRecord {
            key: "fixture_region".into(),
            continent: "fixture_continent".into(),
            colour_r: 0x101,
            colour_g: 2,
            colour_b: 3,
            battle_name: "Fixtureville (made up)".into(),
        }];
        let building_levels = vec![BuildingLevel {
            key: "fixture_barracks_1".into(),
            chain: "fixture_barracks".into(),
            construction_turns: 3,
            cost: 333,
            ..Default::default()
        }];
        let technologies = vec![Technology {
            key: "fixture_tech".into(),
            research_cost: 444,
            ..Default::default()
        }];

        // Made-up kv values: every key gets (its index + 0.75), so truncation is visible.
        let made_up = |name: &'static str, keys: &mut dyn Iterator<Item = &str>| {
            KvTable::from_entries(name, keys.enumerate().map(|(i, k)| (k.to_owned(), i as f32 + 0.75)).collect())
        };
        // _kv_rules gets (index + 1) + 0.75 instead, so every integer (all divisors included) is
        // non-zero and fixture battles can still divide by armour and defence factors.
        let kv_rules = KvRules {
            table: KvTable::from_entries(
                "_kv_rules",
                kv::KV_RULES_KEYS.iter().enumerate().map(|(i, (k, _))| (k.to_string(), i as f32 + 1.75)).collect(),
            ),
        };
        let kv_morale_raw = made_up("_kv_morale", &mut kv::KV_MORALE_KEYS.iter().copied());
        let kv_fatigue_raw = made_up("_kv_fatigue", &mut kv::KV_FATIGUE_KEYS.iter().copied());
        // The key lists come from the same macros as the conversions, so these cannot fail.
        let kv_morale = KvMorale::try_from(&kv_morale_raw).unwrap_or_default();
        let kv_fatigue = KvFatigue::try_from(&kv_fatigue_raw).unwrap_or_default();

        let projectiles = Table::from_rows(1, projectiles);
        let gun_type_to_projectiles = Table::from_rows(0, guns);
        let gun_shots = index_gun_shots(&gun_type_to_projectiles, &projectiles);
        Self {
            source: DataSource::TestFixture,
            units: Table::from_rows(4, units),
            unit_stats_land: Table::from_rows(5, stats),
            projectiles,
            gun_type_to_projectiles,
            // The fixture ships no effect tables; the battle effects fall back to their
            // PROVISIONAL name rules, which is what the empty tables ask for.
            projectile_explosions: ExplosionTable::default(),
            projectile_impacts: ImpactTable::default(),
            projectile_trails: TrailTable::default(),
            unit_abilities: Table::from_rows(0, Vec::new()),
            unit_class_abilities: Table::from_rows(0, Vec::new()),
            technology_effects: Table::from_rows(0, Vec::new()),
            effect_abilities: Table::from_rows(0, Vec::new()),
            effect_shot_types: Table::from_rows(0, Vec::new()),
            battle_weather_types: Table::from_rows(0, Vec::new()),
            battle_climate_weather: Table::from_rows(0, Vec::new()),
            fatigue_effects: Table::from_rows(0, Vec::new()),
            unit_stats_land_experience_bonuses: Table::from_rows(0, experience_bonuses),
            unit_stats_naval_experience_bonuses: Table::from_rows(0, naval_experience_bonuses),
            factions: Table::from_rows(3, factions),
            regions: Table::from_rows(1, regions),
            building_levels: Table::from_rows(0, building_levels),
            technologies: Table::from_rows(1, technologies),
            campaign: crate::campaign::CampaignTables::default(),
            // Complete by construction (same key list), so this cannot fail; a test checks it.
            kv_rules_sim: SimKvRules::try_from(&kv_rules).unwrap_or_default(),
            kv_rules,
            kv_morale,
            kv_fatigue,
            kv_morale_raw,
            kv_fatigue_raw,
            gun_shots,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `agent_attributes` only gives the character cards their pictures: an install (or mod set)
    /// without it still loads, with an empty table and no icons, instead of failing the campaign.
    #[test]
    fn a_missing_optional_table_is_empty_not_an_error() {
        use crate::characters::{AgentAttributeRecord, CharacterTables};
        let vfs = Vfs::new();
        assert!(load::<AgentAttributeRecord>(&vfs).is_err(), "the plain loader fails");
        let attributes = load_optional::<AgentAttributeRecord>(&vfs).expect("a missing table is not an error");
        assert!(attributes.is_empty());
        let c = CharacterTables { attributes, ..Default::default() };
        assert_eq!(c.attribute_icon("command_land"), "");
    }

    #[test]
    fn fixture_lookups_and_foreign_keys() {
        let db = GameDatabase::test_fixture();
        assert!(db.is_test_fixture());
        let inf = db.land_unit("fixture_line_infantry").unwrap();
        assert_eq!((inf.unit.recruitment_cost, inf.stats.num_men), (111, 123));
        assert_eq!(db.primary_projectile(inf.stats).unwrap().key, "fixture_ball");
        assert!(db.gun_projectiles(inf.stats).is_empty());

        let art = db.land_unit("fixture_foot_artillery").unwrap();
        assert!(db.unit_projectile(art.stats).is_none());
        let shots: Vec<_> = db.gun_projectiles(art.stats).iter().map(|p| p.key.as_str()).collect();
        assert_eq!(shots, ["fixture_round_shot"]);

        assert!(db.unit("missing").is_none());
        assert!(db.land_unit("missing").is_none());
        assert_eq!(db.faction("fixture_faction").unwrap().primary_colour(), [1, 2, 3]);
        assert_eq!(db.region("fixture_region").unwrap().colour(), [1, 2, 3]); // low byte only
        assert_eq!(db.building_level("fixture_barracks_1").unwrap().cost, 333);
        assert_eq!(db.technology("fixture_tech").unwrap().research_cost, 444);
    }

    /// A gun's shots are in shot type order, table order among equals (`0x00F41750` →
    /// `0x00F25CF0`); the battle's shot is the first of them; the unit card's Range is the
    /// gun's LONGEST effective range (`0x008DF190`, gun type +0x60), not the first shot's
    /// (regression: round 14 showed the first), and an own projectile's range without a gun.
    #[test]
    fn gun_shots_are_in_shot_type_order_and_the_card_range_is_the_longest() {
        let mut db = GameDatabase::test_fixture();
        let shot = |key: &str, shot_type: &str, range: i32| Projectile {
            key: key.into(),
            shot_type: shot_type.into(),
            effective_range: range,
            ..Default::default()
        };
        let row = |p: &str| GunTypeProjectile { gun_type: "fixture_gun".into(), projectile: p.into(), muzzle_flash: String::new() };
        // Table order: canister (3), round shot (0), shell (1), a second round shot (0).
        db.projectiles = Table::from_rows(1, vec![shot("can", "canister", 150), shot("ball", "round_shot", 600), shot("shell", "explosive_shell", 800), shot("ball2", "round_shot", 500)]);
        db.gun_type_to_projectiles = Table::from_rows(0, vec![row("can"), row("ball"), row("shell"), row("ball2")]);
        db.gun_shots = index_gun_shots(&db.gun_type_to_projectiles, &db.projectiles);
        let art = db.land_unit("fixture_foot_artillery").unwrap();
        let keys: Vec<&str> = db.gun_projectiles(art.stats).iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["ball", "ball2", "shell", "can"]);
        assert_eq!(db.primary_projectile(art.stats).unwrap().key, "ball");
        assert_eq!(db.unit_card_range(art.stats), 800, "the longest, not the first shot's 600");
        // The volley's lookup reads the load-time index, not the junction table (regression: a
        // full table scan per artillery volley).
        db.gun_type_to_projectiles = Table::from_rows(0, Vec::new());
        let art = db.land_unit("fixture_foot_artillery").unwrap();
        assert_eq!(db.primary_projectile(art.stats).unwrap().key, "ball");

        let inf = db.land_unit("fixture_line_infantry").unwrap();
        assert_eq!(db.unit_card_range(inf.stats), 0, "its own projectile is gone from this table");
        let fixture = GameDatabase::test_fixture();
        let inf = fixture.land_unit("fixture_line_infantry").unwrap();
        assert_eq!(fixture.unit_card_range(inf.stats), 99, "no gun: its own projectile's range");
    }

    /// Regression (polish): the index held row numbers into the public `projectiles` table, so
    /// replacing that table with a shorter one made a volley's lookup index out of bounds.
    #[test]
    fn a_replaced_projectiles_table_never_panics_the_gun_shot_lookup() {
        let mut db = GameDatabase::test_fixture();
        let art = db.land_unit("fixture_foot_artillery").unwrap().stats.clone();
        let keys = |db: &GameDatabase| db.gun_projectiles(&art).iter().map(|p| p.key.clone()).collect::<Vec<_>>();
        let shots = keys(&db);
        assert!(!shots.is_empty(), "the fixture's gun has shots");
        // Rows moved: each shot is found by its key, not the stale row.
        let mut rows = db.projectiles.rows().to_vec();
        rows.reverse();
        db.projectiles = Table::from_rows(1, rows);
        assert_eq!(keys(&db), shots);
        db.projectiles = Table::from_rows(1, Vec::new());
        assert!(db.gun_projectiles(&art).is_empty());
        assert!(db.primary_projectile(&art).is_none());
        assert_eq!(db.unit_card_range(&art), 0);
    }

    /// Regression (polish): the card Range reads the exe's unsigned field, so a negative modded
    /// range is never shown negative; as an unsigned value it wins the gun's maximum, as in the exe.
    #[test]
    fn a_negative_modded_range_is_read_unsigned_as_the_exe() {
        let mut db = GameDatabase::test_fixture();
        let shot = |key: &str, range: i32| Projectile { key: key.into(), shot_type: "round_shot".into(), effective_range: range, ..Default::default() };
        let row = |p: &str| GunTypeProjectile { gun_type: "fixture_gun".into(), projectile: p.into(), muzzle_flash: String::new() };
        db.projectiles = Table::from_rows(1, vec![shot("ball", 600), shot("modded", -1)]);
        db.gun_type_to_projectiles = Table::from_rows(0, vec![row("ball"), row("modded")]);
        db.gun_shots = index_gun_shots(&db.gun_type_to_projectiles, &db.projectiles);
        let art = db.land_unit("fixture_foot_artillery").unwrap();
        assert_eq!(db.unit_card_range(art.stats), u32::MAX);
    }

    /// `0x00ED49A0`: the experience-adjusted **cost** (the unit info panel's "XpAdjustedCost", what
    /// the campaign auto-build pays) is `flat + ROUND(base * multiplier)` — the base is scaled,
    /// not added — for both the land (`+0x24`/`+0x28`) and the naval (`+0x1C`/`+0x20`) row. A rank
    /// the table does not have leaves the cost alone (the exe's else branch).
    #[test]
    fn experience_adjusted_cost_scales_the_base() {
        let db = GameDatabase::test_fixture();
        // A recruit (flat 0, x1.0) pays exactly the base; the fixture's rank 9 is the real one.
        assert_eq!(db.experience_adjusted_cost(0, 100), 100);
        assert_eq!(db.experience_adjusted_cost(9, 100), 360 + 190); // 100 * 1.9 = 190
        assert_eq!(db.naval_experience_adjusted_cost(0, 100), 100);
        assert_eq!(db.naval_experience_adjusted_cost(9, 100), 255 + 145); // 100 * 1.45 = 145
        // Rounding is ROUND-half-away-from-zero (the exe's ROUND), not a truncation: 10 * 1.45.
        assert_eq!(db.naval_experience_adjusted_cost(9, 10), 255 + 15);
        // An unknown rank is left as it is.
        assert_eq!(db.experience_adjusted_cost(5, 100), 100);
        assert_eq!(db.naval_experience_adjusted_cost(5, 100), 100);
        // The per-tick fatigue column still comes out in file order.
        assert_eq!(db.experience_fatigue_bonuses(), vec![0, -3]);
    }

    /// The rank boundaries and the row accessors a campaign rules loader copies the tables through:
    /// rank 0 is a recruit (flat 0, ×1.0 → the base unchanged) and rank 9 the last row, and the two
    /// tables are read apart (`this+0xA0` in `0x00ED49A0` picks the naval one).
    #[test]
    fn experience_cost_rows_and_rank_boundaries() {
        let db = GameDatabase::test_fixture();
        assert_eq!(db.experience_cost_rows(), vec![(0, 0, 1.0), (9, 360, 1.9)]);
        assert_eq!(db.naval_experience_cost_rows(), vec![(0, 0, 1.0), (9, 255, 1.45)]);
        // The two rank boundaries of the shipped tables: 0 and 9 (ranks "0".."9", CONFIRMED).
        for rank in 0..=9u8 {
            let base = 200;
            let land = db.experience_adjusted_cost(rank, base);
            let naval = db.naval_experience_adjusted_cost(rank, base);
            if rank == 0 {
                assert_eq!((land, naval), (base, base));
            } else if rank == 9 {
                // The real rows: land +360 x1.9, naval +255 x1.45.
                assert_eq!(land, 360 + 380);
                assert_eq!(naval, 255 + 290);
            } else {
                // The fixture has no other rank, so the cost is left alone.
                assert_eq!((land, naval), (base, base));
            }
        }
        // A negative base (never happens in game data, but the arithmetic is plain): the flat term
        // still adds and the multiplier scales the base.
        assert_eq!(db.experience_adjusted_cost(9, -100), 360 - 190);
        assert_eq!(db.naval_experience_adjusted_cost(9, -100), 255 - 145);
        // Out of the 0..9 range the key lookup misses, so the cost is untouched.
        assert_eq!(db.experience_adjusted_cost(10, 100), 100);
        assert_eq!(db.experience_adjusted_cost(255, 100), 100);
    }

    #[test]
    fn fixture_kv_is_truncated() {
        let db = GameDatabase::test_fixture();
        // Index 1 + 0.75 = 1.75 -> 1.
        assert_eq!(db.kv_morale.ums_impetuous_threshold_lower, 1);
        assert_eq!(db.kv_fatigue.idle_in_building, 1);
        db.kv_rules.check_complete().unwrap();
        // _kv_rules: (index + 1) + 0.75.
        assert_eq!(db.kv_rules.int("relative_melee_fatigue_multiplier"), Some(2));
        assert_eq!(db.kv_rules.float("melee_height_delta_min"), Some(5.75));
    }

    #[test]
    fn fixture_sim_kv_rules_has_non_zero_divisors() {
        let r = GameDatabase::test_fixture().kv_rules_sim;
        assert_eq!(r.relative_melee_experience_multiplier, 1);
        assert_eq!(r.melee_height_delta_min, 5.75);
        for d in [
            r.armour_missile_penetrating_divisor,
            r.armour_missile_piercing_divisor,
            r.armour_melee_penetrating_divisor,
            r.armour_melee_piercing_divisor,
            r.defense_missile_penetrating_divisor,
            r.defense_missile_piercing_divisor,
            r.defense_melee_penetrating_divisor,
            r.defense_melee_piercing_divisor,
            r.projectile_damage_shield_divisor,
            r.projectile_damage_armour_divisor,
            r.projectile_damage_defense_divisor,
            r.melee_charge_factor_power_divisor,
        ] {
            assert_ne!(d, 0);
        }
        assert_ne!(r.relative_melee_height_delta_divisor, 0.0);
    }
}
