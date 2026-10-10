//! The real DB table layouts, written as typed records.
//!
//! # Where the layouts come from
//! * **Column order, column types and version guards:** Worker 1 `DB_BUILDERS.md`. These were
//!   decompiled from the exe's row readers, so they are authoritative (CONFIRMED).
//! * **i32 or f32** for each 4-byte column: Worker 2 `schemas_battle.md` and Worker 3
//!   `DB_CAMPAIGN_TABLES.md`, judged from the values. The exe copies 4 raw bytes and does not say.
//! * **Field names:** Worker 2 and Worker 3. The files have no column names, so **every
//!   name is INFERRED**.
//!
//! Each field's doc comment begins with `#col @offset conf`:
//! * `#col` is the column number in the file;
//! * `@offset` is the BUILDER struct offset in the exe (`@?` if not stored directly);
//! * `conf` is how sure we are of the *name*: H = high, M = medium, L = guess.
//!
//! A column whose meaning is unknown is named `unknown_<offset>`. A version-guarded column
//! is marked `{v>=N}`; in older table versions it holds its default value.
//!
//! **Optional strings** are `Option<String>`. `None` means the "absent" flag was 0, which
//! the exe treats exactly like an empty string.

use crate::record::db_record;

db_record! {
    /// One row of `units` (exe reader 0x00E85B20, 25 columns, file v4).
    /// The general, campaign-side description of every land and naval unit.
    pub struct UnitRecord in "units", key = key {
        /// #0 @0x00 H: unit key (foreign key target for `unit_stats_land.key`).
        key: String,
        /// #1 @0x0C H: English development name, e.g. "German Fusiliers".
        dev_name: String,
        /// #2 @0x18 H: category: infantry / cavalry / artillery / naval_*.
        category: String,
        /// #3 @0x24 H: class, e.g. infantry_line, cavalry_heavy.
        unit_class: String,
        /// #4 @0x30 M: the battle army-setup cost (custom and multiplayer battles, the `0x00ED49A0` base; `UNIT_RECORD`
        /// +0x2C). Not the campaign recruitment cost (#7).
        recruitment_cost: i32,
        /// #5 @0x34 M {v>=1}: second cost (MP / custom battle?). In v0 the exe copies #4 here.
        secondary_cost: i32 => since 1 copy,
        /// #6 @0x38: recruitment time in turns (record +0x34, CONFIRMED by 0-B: see `ntw_sim::campaign::UnitRules::turns`).
        unknown_38: i32,
        /// #7 @0x3C: the campaign recruitment cost before effects (`UNIT_RECORD` +0x38, read by the
        /// recruitable entry pricing `0x00B0D220`, CONFIRMED; `ntw_sim::campaign::UnitRules::campaign_cost`).
        unknown_3c: i32,
        /// #8 @0x40 M: upkeep.
        upkeep: i32,
        /// #9 @0x44: the path cost a unit recruited through a commander covers per turn on its way to him
        /// (`UNIT_RECORD` +0x40, CONFIRMED by `0x00E91388` and its reader `0x00B41F60`; 23..55;
        /// `ntw_sim::campaign::UnitRules::travel_speed`).
        unknown_44: i32,
        /// #10 @0x48 L: unit card group / commander type.
        unit_card_group: Option<String>,
        /// #11 @0x54 M: campaign model.
        campaign_model: String,
        /// #12 @0x60 M: second model.
        model_2: String,
        /// #13 @0x6C H: icon / info key.
        info_key: String,
        /// #14 @0x78 L: recruitment scope ("global").
        recruitment_scope: Option<String>,
        /// #15 @0x84: the most units of this type a faction may hold and have queued together, 0 = no limit
        /// (`UNIT_RECORD` +0x68, copied by `0x00E91320`; the campaign test `0x008F68B0`, CONFIRMED; 142 of the
        /// 442 vanilla units have one, e.g. `Cav_Heavy_French_Grenadiers_a_Cheval` 4). The custom battle
        /// army setup shows it as "Cap". `ntw_sim::campaign::UnitRules::unit_cap`.
        unit_cap: i32,
        /// #16 @0x88 H: multiplayer category, e.g. mp_infantry.
        mp_category: String,
        /// #17 @0x94: UNKNOWN flag.
        unknown_94: bool,
        /// #18 @0x95: UNKNOWN flag.
        unknown_95: bool,
        /// #19 @0x96: UNKNOWN flag.
        unknown_96: bool,
        /// #20 @0x98: UNKNOWN (0, 2, 5, 7, ...).
        unknown_98: i32,
        /// #21 @0x9C: UNKNOWN flag.
        unknown_9c: bool,
        /// #22 @0xA0 {v>=2}: UNKNOWN (0, 1, 2, 4, ...).
        unknown_a0: i32 => since 2,
        /// #23 @0xA4 M {v>=3}: AI / unit role, e.g. line_infantry.
        ai_role: Option<String> => since 3,
        /// #24 @0xB0 {v>=4}: UNKNOWN flag.
        unknown_b0: bool => since 4,
    }
}

db_record! {
    /// One row of `unit_stats_land` (exe reader 0x00E84B00, 89 columns, file v5): the battle
    /// stats of a land unit (`LAND_UNIT_RECORD`). Napoleon has no separate weapon or armour
    /// tables; those stats are columns here.
    pub struct UnitStatsLand in "unit_stats_land", key = key {
        /// #0 @0x00 H: unit key (= `units.key`).
        key: String,
        /// #1 @0x0C H: number of men.
        num_men: i32,
        /// #2 @0x10 H: number of mounts.
        num_mounts: i32,
        /// #3 @0x14 H: number of guns (engines).
        num_guns: i32,
        /// #4 @0x18 H: officer (FK battle_personalities).
        officer: String,
        /// #5 @0x24 H: musician (FK battle_personalities).
        musician: Option<String>,
        /// #6 @0x30 H: standard bearer (FK battle_personalities).
        standard_bearer: Option<String>,
        /// #7 @0x3C M: animation culture set ("euroline").
        animation_culture_set: String,
        /// #8 @0x48 H: soldier entity (FK battle_entities).
        man_entity: String,
        /// #9 @0x54 M: soldier skeleton / animation type.
        man_animation_type: String,
        /// #10 @0x60 M: weapon animation group.
        weapon_anim_group: String,
        /// #11 @0x6C M: armour.
        armour: i32,
        /// #12 @0x70 H: armour type (leather, plate, ...).
        armour_type: String,
        /// #13 @0x7C: UNKNOWN (always 1).
        unknown_7c: i32,
        /// #14 @0x80 H: mount (FK mounts).
        mount: Option<String>,
        /// #15 @0x8C H: mount entity (FK battle_entities).
        mount_entity: Option<String>,
        /// #16 @0x98 H: mount type.
        mount_type: Option<String>,
        /// #17 @0xBC L: numeric text, mount-related ("0", "6").
        mount_text_a: String,
        /// #18 @0xC8 L: numeric text ("0", "1").
        mount_text_b: String,
        /// #19 @0xD4 H: gun-train entity (FK battle_entities).
        gun_train_entity: Option<String>,
        /// #20 @0xE0 H: ammo caisson entity (FK battle_entities).
        ammo_caisson_entity: Option<String>,
        /// #21 @0xEC H: limber model.
        limber_model: Option<String>,
        /// #22 @0xF8 H: engine (gun) model.
        engine_model: Option<String>,
        /// #23 @0x1E0 M: is artillery / has engine.
        is_artillery: bool,
        /// #24 @0x11C H: gun type (FK gun_types).
        gun_type: Option<String>,
        /// #25 @0x128 M: missile weapon class ("musket").
        missile_weapon_class: Option<String>,
        /// #26 @0x134 H: accuracy.
        accuracy: i32,
        /// #27 @0x138 H: reload skill.
        reload_skill: i32,
        /// #28 @0x13C: UNKNOWN text (always "0").
        unknown_13c: String,
        /// #29 @0x148 H: firing mechanism (matchlock / flintlock / ...; see the `misfire_*` kv keys).
        firing_mechanism: String,
        /// #30 @0x154 H: projectile (FK projectiles).
        projectile: Option<String>,
        /// #31 @0x160 M: ammunition.
        ammunition: i32,
        /// #32 @0x164 M: firing animation set.
        firing_animation_set: Option<String>,
        /// #33 @0x170 H: melee weapon type.
        melee_weapon_type: String,
        /// #34 @0x17C H: melee attack.
        melee_attack: i32,
        /// #35 @0x180 H: charge bonus.
        charge_bonus: i32,
        /// #36 @0x184 H: melee defence.
        melee_defence: i32,
        /// #37 @0x188: UNKNOWN (always 0; shield?). Could also be f32 0.0.
        unknown_188: i32,
        /// #38 @0x18C M: melee animation category.
        melee_animation_category: String,
        /// #39 @0x198: UNKNOWN text ("0").
        unknown_198: String,
        /// #40 @0x1A4 H: drill set.
        drill_set: String,
        /// #41 @0x1B0 H: training level (FK entity_training_levels).
        training_level: String,
        /// #42 @0x1BC H: morale.
        morale: i32,
        /// #43 @0x1C0 L: INFERRED default formation depth (ranks): line infantry 3, skirmishers 2,
        /// cavalry 3, mobs and Ottoman infantry 4, revolutionary columns 8, artillery 1.
        default_ranks: i32,
        /// #44 @0x1C4 M: file spacing, close order (m).
        spacing_file_close: f32,
        /// #45 @0x1C8 M: rank spacing, close order (m).
        spacing_rank_close: f32,
        /// #46 @0x1CC M: file spacing, loose order (m).
        spacing_file_loose: f32,
        /// #47 @0x1D0 M: rank spacing, loose order (m).
        spacing_rank_loose: f32,
        /// #48 @0x1D4 L: artillery-only spacing A.
        artillery_spacing_a: f32,
        /// #49 @0x1D8 L: artillery-only spacing B.
        artillery_spacing_b: f32,
        /// #50 @0x1DC: UNKNOWN (art 7, cav 10, inf 20).
        unknown_1dc: f32,
        /// #51 @0x1E4: UNKNOWN (always 0).
        unknown_1e4: i32,
        /// #52 @0x1E8: UNKNOWN flag.
        unknown_1e8: bool,
        /// #53 @0x1E9: UNKNOWN flag.
        unknown_1e9: bool,
        /// #54 @0x1EA: UNKNOWN flag (true for line infantry).
        unknown_1ea: bool,
        /// #55 @0x1EB: UNKNOWN flag.
        unknown_1eb: bool,
        /// #56 @0x1EC: UNKNOWN flag.
        unknown_1ec: bool,
        /// #57 @0x1ED: UNKNOWN flag (artillery).
        unknown_1ed: bool,
        /// #58 @0x1EE: UNKNOWN flag.
        unknown_1ee: bool,
        /// #59 @0x1EF: UNKNOWN flag (artillery).
        unknown_1ef: bool,
        /// #60 @0x1F0: UNKNOWN flag.
        unknown_1f0: bool,
        /// #61 @0x1F1: UNKNOWN flag.
        unknown_1f1: bool,
        /// #62 @0x1F2: UNKNOWN flag.
        unknown_1f2: bool,
        /// #63 @0x1F3: UNKNOWN flag.
        unknown_1f3: bool,
        /// #64 @0x1F4: UNKNOWN flag.
        unknown_1f4: bool,
        /// #65 @0x1F5: UNKNOWN flag.
        unknown_1f5: bool,
        /// #66 @0x1F8: UNKNOWN (always 50).
        unknown_1f8: f32,
        /// #67 @0x1FC: UNKNOWN (75..100).
        unknown_1fc: f32,
        /// #68 @0x200: UNKNOWN (100..150).
        unknown_200: f32,
        /// #69 @0x204: UNKNOWN flag.
        unknown_204: bool,
        /// #70 @0x205: the militia flag (`UNIT_RECORD` +0x136, CONFIRMED by 0-G; counts twice in a garrison, 0x008B1C30).
        unknown_205: bool,
        /// #71 @0x206: UNKNOWN flag (true for cavalry and infantry).
        unknown_206: bool,
        /// #72 @0x207: UNKNOWN flag.
        unknown_207: bool,
        /// #73 @0x208: UNKNOWN flag.
        unknown_208: bool,
        /// #74 @0x209: UNKNOWN flag.
        unknown_209: bool,
        /// #75 @0x20A: UNKNOWN flag.
        unknown_20a: bool,
        /// #76 @0x20B: UNKNOWN flag.
        unknown_20b: bool,
        /// #77 @0x20C: UNKNOWN flag (line infantry).
        unknown_20c: bool,
        /// #78 @0x20D: UNKNOWN flag.
        unknown_20d: bool,
        /// #79 @0x20E: UNKNOWN flag.
        unknown_20e: bool,
        /// #80 @0x20F: UNKNOWN flag (line infantry).
        unknown_20f: bool,
        /// #81 @0x210: UNKNOWN flag.
        unknown_210: bool,
        /// #82 @0x211: UNKNOWN flag.
        unknown_211: bool,
        /// #83 @0x212 {v>=1}: UNKNOWN flag.
        unknown_212: bool => since 1,
        /// #84 @0x213 {v>=2}: UNKNOWN flag.
        unknown_213: bool => since 2,
        /// #85 @0x214 H {v>=3}: second musician (FK battle_personalities).
        second_musician: Option<String> => since 3,
        /// #86 @0x220 H {v>=4}: destroyed caisson model.
        destroyed_caisson_model: Option<String> => since 4,
        /// #87 @0x22C H {v>=4}: caisson destruction animation.
        caisson_destruction_anim: Option<String> => since 4,
        /// #88 @0x238 {v>=5}: guerrilla deployment (set on the 23 Spanish guerrilla units and the French
        /// contra-guerrillas; the default deployment places these units as a second group 25 m ahead,
        /// unit `+0x1C5` in `0x005BA640`, CONFIRMED; fidelity 0-D §5.2).
        unknown_238: bool => since 5,
    }
}

db_record! {
    /// One row of `projectiles` (exe reader 0x00F3EF70, 35 columns, file v1).
    pub struct Projectile in "projectiles", key = key {
        /// #0 @0x00 H: projectile key.
        key: String,
        /// #1 @0x0C H: category (missile / artillery).
        category: String,
        /// #2 @0x24 H: shot type (FK projectile_shot_type_enum).
        shot_type: String,
        /// #3 @0x30 H: missile type (FK projectiles_missile_type_enum).
        missile_type: String,
        /// #4 @0x18 M: projectile model.
        model: Option<String>,
        /// #5 @0x3C M: explosion (FK projectiles_explosions).
        explosion: Option<String>,
        /// #6 @0x48 L: trail texture — the **foreign key into `projectile_trails`** (`alpha`,
        /// `alpha_bullet`, `alpha_shrapnel`, `e3_rocket`, `none`). This is the bridge round 2
        /// could not find: CONFIRMED on the install, all five distinct values over all 144 rows
        /// are that table's five keys
        /// (`ntw_data/tests/effects_data.rs::the_trail_group_and_the_trail_table_are_joined_by_column_six`).
        /// NOT to be confused with column 32's `trail`, which names the trail's effect group.
        trail_texture: Option<String>,
        /// #7 @0x54 M: spin type.
        spin_type: String,
        /// #8 @0x60 H: projectiles per shot (canister 35..65).
        projectiles_per_shot: i32,
        /// #9 @0x64 M: sub-projectile (shrapnel).
        sub_projectile: Option<String>,
        /// #10 @0x70 L: trajectory class.
        trajectory_class: String,
        /// #11 @0x7C H: effective range (m).
        effective_range: i32,
        /// #12 @0x80 M: minimum range.
        minimum_range: i32,
        /// #13 @0x84 L: maximum elevation.
        max_elevation: i32,
        /// #14 @0x88 M: muzzle velocity.
        muzzle_velocity: f32,
        /// #15 @0x8C L: marksmanship / accuracy modifier.
        accuracy_modifier: f32,
        /// #16 @0x90 L: spread.
        spread: f32,
        /// #17 @0x94 L: damage.
        damage: f32,
        /// #18 @0x98 L: penetration class.
        penetration: Option<String>,
        /// #19 @0xA4 M: incendiary chance class (kv `projectile_incendiary_chance_*`).
        incendiary_chance: Option<String>,
        /// #20 @0xB0 M: impact behaviour.
        impact_behaviour: Option<String>,
        /// #21 @0xBC L: bounces?
        bounces: bool,
        /// #22 @0xBD: UNKNOWN flag.
        unknown_bd: bool,
        /// #23 @0xC0: UNKNOWN (0.1).
        unknown_c0: f32,
        /// #24 @0xC4 M: reload time (s).
        reload_time: i32,
        /// #25 @0xC8 L: shots per volley.
        shots_per_volley: i32,
        /// #26 @0xCC L: size class.
        size_class: Option<String>,
        /// #27 @0xD8: UNKNOWN (0.1 / 0.2).
        unknown_d8: f32,
        /// #28 @0xDC: UNKNOWN (0).
        unknown_dc: f32,
        /// #29 @0xEC H: weapon class (small_arm / gun).
        weapon_class: String,
        /// #30 @0xF8 H: calibre size.
        calibre: String,
        /// #31 @0xE0 H: the effect **group** the shot plays when it is fired (`MusketFire`,
        /// `LandGunFire_large`, `CannonFire`, ...). CONFIRMED on the install:
        /// `ntw_data/tests/effects_data.rs::projectiles_name_their_own_fire_group_and_trail`
        /// checks every set value is a `SCRIPTED_EFFECT_GROUP` of `effects\landbattle.xml`. This is
        /// the firing muzzle flash and smoke, so it replaces the name-guessing in
        /// `battle::fx::fire_group` (which was PROVISIONAL). Also pinned there: the exact tally
        /// (120 of 144 rows set it).
        fire_effect: Option<String>,
        /// #32 @0x104 M: the trail's **effect group** in `effects\landbattle.xml`
        /// (`shrapnel_trail`, `carcass_trail`, `congreve_rocket`, `quicklime_trail`,
        /// `ship_explosion_fragment_trail`) — set on 9 of the 144 rows, and CONFIRMED on the install
        /// that all seven distinct values are `SCRIPTED_EFFECT_GROUP`s of that file.
        ///
        /// Round 2 called this a dead end because the values are not `projectile_trails` keys.
        /// They were never meant to be: **column 6** (`trail_texture`) is that table's foreign key.
        /// A shot's trail is the pair — this group draws the particles, column 6's row gives the
        /// colour and geometry. Both halves CONFIRMED by
        /// `ntw_data/tests/effects_data.rs::the_trail_group_and_the_trail_table_are_joined_by_column_six`.
        trail: Option<String>,
        /// #33 @0x110 M: the impact **ball class** (`musket_ball`, `default_ball`, `naval_grape`),
        /// a foreign key to `projectile_impacts`. CONFIRMED on the install: every set value is one
        /// of that table's 8 row keys.
        impact_ball: Option<String>,
        /// #34 @0x11C H {v>=1}: weapon family (musket_flintlock / cannon / howitzer).
        weapon_family: Option<String> => since 1,
    }
}

db_record! {
    /// One row of `gun_type_to_projectiles` (exe reader 0x00DD29F0): which projectiles a gun
    /// type can fire. A gun type has several rows, so look them up with
    /// [`crate::GameDatabase::gun_projectiles`].
    pub struct GunTypeProjectile in "gun_type_to_projectiles", key = gun_type {
        /// #0 @0x00 H: gun type (FK gun_types).
        gun_type: String,
        /// #1 @0x0C H: projectile (FK projectiles).
        projectile: String,
        /// #2 @0x18 M: muzzle flash effect.
        muzzle_flash: String,
    }
}

db_record! {
    /// One row of `factions` (exe reader 0x00F70CB0, 48 columns, file v3).
    ///
    /// The three faction colours are stored twice (exe offsets 0x9C.. and 0xC0..); the
    /// copies matched in every row checked (W3).
    pub struct FactionRecord in "factions", key = key {
        /// #0 @0x00 H: faction key, e.g. "france".
        key: String,
        /// #1 @0x0C L: id / UI order (hash-like for newer rows).
        id: i32,
        /// #2 @0x10 H: subculture (FK cultures_subcultures).
        subculture: String,
        /// #3 @0x1C H: category (playable / minor / non-expansionist / rebel).
        category: String,
        /// #4 @0x28 H: screen name (loc `factions_screen_name_<key>` overrides it).
        screen_name: String,
        /// #5 @0x34 H: screen adjective.
        screen_adjective: String,
        /// #6 @0x40 H: character names group (FK names_groups).
        character_names_group: String,
        /// #7 @0x4C L: unit/uniform model faction.
        model_faction: String,
        /// #8 @0x58 L {v>=1}: culture-specific variant ("Ottoman").
        culture_variant: Option<String> => since 1,
        /// #9 @0x64: UNKNOWN flag.
        unknown_64: bool,
        /// #10 @0x65: UNKNOWN flag.
        unknown_65: bool,
        /// #11 @0x66: UNKNOWN flag.
        unknown_66: bool,
        /// #12 @0x6C H: unit-card icon folder.
        unit_icon_path: String,
        /// #13 @0x78 H: flag folder.
        flag_path: String,
        /// #14 @0x84 H: republic flag folder.
        republic_flag_path: Option<String>,
        /// #15 @0x90 H: rebel flag folder.
        rebel_flag_path: Option<String>,
        /// #16 @0x9C H: primary colour, red (0..255 as float).
        primary_r: f32,
        /// #17 @0xA0 H: primary colour, green.
        primary_g: f32,
        /// #18 @0xA4 H: primary colour, blue.
        primary_b: f32,
        /// #19 @0xC0 H: primary colour copy, red.
        primary_copy_r: f32,
        /// #20 @0xC4 H: primary colour copy, green.
        primary_copy_g: f32,
        /// #21 @0xC8 H: primary colour copy, blue.
        primary_copy_b: f32,
        /// #22 @0xA8 H: secondary colour, red.
        secondary_r: f32,
        /// #23 @0xAC H: secondary colour, green.
        secondary_g: f32,
        /// #24 @0xB0 H: secondary colour, blue.
        secondary_b: f32,
        /// #25 @0xCC H: secondary colour copy, red.
        secondary_copy_r: f32,
        /// #26 @0xD0 H: secondary colour copy, green.
        secondary_copy_g: f32,
        /// #27 @0xD4 H: secondary colour copy, blue.
        secondary_copy_b: f32,
        /// #28 @0xB4 H: tertiary colour, red.
        tertiary_r: f32,
        /// #29 @0xB8 H: tertiary colour, green.
        tertiary_g: f32,
        /// #30 @0xBC H: tertiary colour, blue.
        tertiary_b: f32,
        /// #31 @0xD8 H: tertiary colour copy, red.
        tertiary_copy_r: f32,
        /// #32 @0xDC H: tertiary colour copy, green.
        tertiary_copy_g: f32,
        /// #33 @0xE0 H: tertiary colour copy, blue.
        tertiary_copy_b: f32,
        /// #34 @0xE4 H: faction group, e.g. france_group.
        faction_group: String,
        /// #35 @0xFC H: rebel faction key.
        rebel_faction: Option<String>,
        /// #36 @0x108 L: extra colour, red (173 or 0).
        extra_r: f32,
        /// #37 @0x10C L: extra colour, green.
        extra_g: f32,
        /// #38 @0x110 L: extra colour, blue.
        extra_b: f32,
        /// #39 @0x114 L: voice / actor id ("24").
        voice_id: Option<String>,
        /// #40 @0x120 H: language / voice code ("Fr", "Uk", ...).
        language_code: String,
        /// #41 @0x67: UNKNOWN flag.
        unknown_67: bool,
        /// #42 @0x68: UNKNOWN flag.
        unknown_68: bool,
        /// #43 @0x12C L: ship names group.
        ship_names_group: String,
        /// #44 @0x138 L: secondary names group.
        secondary_names_group: String,
        /// #45 @0x144 H {v>=2}: attack description (loc `factions_attack_desc_<key>`).
        attack_description: String => since 2,
        /// #46 @0x150 H {v>=2}: defend description.
        defend_description: String => since 2,
        /// #47 @0x160 {v>=3}: UNKNOWN optional string (W3 read it as a bool; the exe reads an optional string).
        unknown_160: Option<String> => since 3,
    }
}

impl FactionRecord {
    /// Primary colour as RGB bytes (each float truncated to 0..=255).
    pub fn primary_colour(&self) -> [u8; 3] {
        [self.primary_r, self.primary_g, self.primary_b].map(|c| c as u8)
    }
    /// Secondary colour as RGB bytes.
    pub fn secondary_colour(&self) -> [u8; 3] {
        [self.secondary_r, self.secondary_g, self.secondary_b].map(|c| c as u8)
    }
    /// Tertiary colour as RGB bytes.
    pub fn tertiary_colour(&self) -> [u8; 3] {
        [self.tertiary_r, self.tertiary_g, self.tertiary_b].map(|c| c as u8)
    }
}

db_record! {
    /// One row of `regions` (exe reader 0x00F3F950, 6 columns, file v1).
    pub struct RegionRecord in "regions", key = key {
        /// #0 @0x00 H: region key, e.g. "eur_france".
        key: String,
        /// #1 @0x0C H: continent (FK regions_continents).
        continent: String,
        /// #2 @0x1A M: map colour red. The exe keeps only the low byte; see [`RegionRecord::colour`].
        colour_r: i32,
        /// #3 @0x19 M: map colour green (low byte kept).
        colour_g: i32,
        /// #4 @0x18 M: map colour blue (low byte kept).
        colour_b: i32,
        /// #5 @0x1C H {v>=1}: battle name (loc `regions_battle_name_<key>` overrides it).
        battle_name: String => since 1,
    }
}

impl RegionRecord {
    /// The map colour exactly as the exe stores it: only the low byte of each i32 (CONFIRMED).
    pub fn colour(&self) -> [u8; 3] {
        [self.colour_r as u8, self.colour_g as u8, self.colour_b as u8]
    }
}

db_record! {
    /// One row of `campaign_map_playable_areas` (record `CAMPAIGN_MAP_PLAYABLE_AREA_RECORD`, read
    /// by `CampaignUI.RegionsInTheatre` 0x009EFC30; 9 columns, file v0; layout read from the
    /// shipped table, which has one row per campaign map).
    pub struct CampaignMapPlayableArea in "campaign_map_playable_areas", key = id {
        /// #0 @0x00 H: the theatre key, a number written as text (e.g. "1244818741"; CONFIRMED:
        /// the theatre's name in regions.esf, TheatreList's `Key`, loc
        /// `campaign_map_playable_areas_onscreen_name_<key>`).
        id: String,
        /// #1 @0x0C H: the theatre is a sea-trade theatre (CONFIRMED: TheatreList returns it as
        /// `SeaTrade` and leaves such theatres out on request; false in every shipped row).
        sea_trade: bool,
        /// #2 @0x10 H: the theatre map picture in the campaign map folder (e.g. "europe_map.tga";
        /// CONFIRMED: RegionsInTheatre returns it as `Map`).
        map: Option<String>,
        /// #3 @0x1C H: the 8-bit region lookup picture, one palette entry per region
        /// ("europe_lookup.tga"; CONFIRMED: returned as `Overlay`).
        lookup: Option<String>,
        /// #4 @0x28 H: the radar picture ("stratradar_europe.tga"; CONFIRMED: returned as `Radar`).
        radar: Option<String>,
        /// #5 @0x34 H: the area key (e.g. "europe_main"; CONFIRMED: TheatreList's `Id`).
        area: String,
        /// #6 @0x40 M: width of the pictures in pixels (605 for Europe; matches the files).
        width: i32,
        /// #7 @0x44 M: height of the pictures in pixels (300).
        height: i32,
        /// #8 @0x48 L: UNKNOWN u32 (0 in every shipped row; TheatreMapDimensions returns it fifth).
        unknown_8: i32,
    }
}

db_record! {
    /// One row of `slots` (exe reader 0x00F07130, 6 columns, file v1): a slot type and what kind
    /// of slot it is. The flags' meanings are INFERRED from the shipped rows (farm: horses, sheep;
    /// town: town_UNUSED; port: port; resource: gold, iron, timber, horses, sheep); the building
    /// browser's slot classes read four flags of the slot type (0x00A8B9B0 town, 0x00A8B810 port,
    /// 0x00A8B4A0 farm, +0x3F resource; CONFIRMED tests, order INFERRED).
    pub struct SlotTypeRecord in "slots", key = key {
        /// #0 @0x00 H: slot type key, e.g. "settlement_4_slot", "port", "timber".
        key: String,
        /// #1 @0x0C M: a farm slot.
        farm: bool,
        /// #2 @0x0D M: a town slot.
        town: bool,
        /// #3 @0x0E M: a port slot.
        port: bool,
        /// #4 @0x0F M: a resource slot.
        resource: bool,
        /// #5 @0x10 L {v>=1}: UNKNOWN (set on the settlement_N_slot types and port).
        unknown_10: bool => since 1,
    }
}

db_record! {
    /// One row of `building_levels` (exe reader 0x00DD2470, 24 columns, file v0).
    /// Uses the exe layout, which corrects W3's guess at columns 3 and 16-17.
    pub struct BuildingLevel in "building_levels", key = key {
        /// #0 @0x00 H: building level key, e.g. "fFort1_wooden_artillery_fort".
        key: String,
        /// #1 @0x0C H: chain (FK building_chains).
        chain: String,
        /// #2 @0x18 H: level index in the chain (0..4).
        level: i32,
        /// #3 @0x1C: UNKNOWN string (usually empty).
        unknown_1c: String,
        /// #4 @0x28 M: construction time in turns.
        construction_turns: i32,
        /// #5 @0x2C M: construction cost.
        cost: i32,
        /// #6 @0x30: UNKNOWN (0 in shipped data; could be f32).
        unknown_30: i32,
        /// #7 @0x34: UNKNOWN (0).
        unknown_34: i32,
        /// #8 @0x38: UNKNOWN (0).
        unknown_38: i32,
        /// #9 @0x3C: UNKNOWN (0).
        unknown_3c: i32,
        /// #10 @0x40: UNKNOWN (0).
        unknown_40: i32,
        /// #11 @0x44: UNKNOWN (0).
        unknown_44: i32,
        /// #12 @0x48: UNKNOWN (0).
        unknown_48: i32,
        /// #13 @0x4C: UNKNOWN (0).
        unknown_4c: i32,
        /// #14 @0x50: UNKNOWN (0).
        unknown_50: i32,
        /// #15 @0x54: UNKNOWN (0).
        unknown_54: i32,
        /// #16 @0x58: UNKNOWN string (usually empty).
        unknown_58: String,
        /// #17 @0x68: UNKNOWN string (usually empty).
        unknown_68: String,
        /// #18 @0x64: UNKNOWN (0).
        unknown_64: i32,
        /// #19 @0x74 M: unique / "great building" flag.
        is_great_building: bool,
        /// #20 @0x78 M: military prestige.
        prestige_military: i32,
        /// #21 @0x7C M: naval prestige.
        prestige_naval: i32,
        /// #22 @0x80 M: economic prestige.
        prestige_economic: i32,
        /// #23 @0x84 M: cultural prestige.
        prestige_cultural: i32,
    }
}

db_record! {
    /// One row of `technologies` (exe reader 0x00F086C0, 13 columns, file v1).
    pub struct Technology in "technologies", key = key {
        /// #0 @0x00 H: technology key, e.g. "admin1_classical_economics".
        key: String,
        /// #1 @0x0C M: building level where it is researched.
        building_level: String,
        /// #2 @0x18 M: tree position / column (0..7).
        tree_column: i32,
        /// #3 @0x1C M: research cost.
        research_cost: i32,
        /// #4 @0x20 M: text key (the key repeated).
        text_key: String,
        /// #5 @0x2C: UNKNOWN (0..100; AI weight?).
        unknown_2c: i32,
        /// #6 @0x30: UNKNOWN.
        unknown_30: i32,
        /// #7 @0x34: UNKNOWN.
        unknown_34: i32,
        /// #8 @0x38: UNKNOWN.
        unknown_38: i32,
        /// #9 @0x3C: UNKNOWN flag.
        unknown_3c: bool,
        /// #10 @0x3D: UNKNOWN flag.
        unknown_3d: bool,
        /// #11 @0x40 H: icon file.
        icon: String,
        /// #12 @0x4C {v>=1}: UNKNOWN (0/1).
        unknown_4c: i32 => since 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::DbRecord;
    use ntw_formats::db::FieldType;

    /// Column counts and version guards, checked against DB_BUILDERS.md.
    #[test]
    fn column_counts_match_the_exe() {
        assert_eq!(UnitRecord::schema().fields.len(), 25);
        assert_eq!(UnitStatsLand::schema().fields.len(), 89);
        assert_eq!(Projectile::schema().fields.len(), 35);
        assert_eq!(GunTypeProjectile::schema().fields.len(), 3);
        assert_eq!(FactionRecord::schema().fields.len(), 48);
        assert_eq!(RegionRecord::schema().fields.len(), 6);
        assert_eq!(BuildingLevel::schema().fields.len(), 24);
        assert_eq!(Technology::schema().fields.len(), 13);
    }

    /// The exe layout strings from DB_BUILDERS.md §0, compared letter by letter
    /// (s string, o optional string, b bool, 4 four-byte value).
    #[test]
    fn layouts_match_the_tie_breaker() {
        fn layout<T: DbRecord>() -> String {
            T::schema()
                .fields
                .iter()
                .map(|f| match f.ty {
                    FieldType::Str => 's',
                    FieldType::OptStr => 'o',
                    FieldType::Bool => 'b',
                    FieldType::I32 | FieldType::F32 => '4',
                    FieldType::U16 => 'h',
                })
                .collect()
        }
        let four = |n: usize| "4".repeat(n);
        let bools = |n: usize| "b".repeat(n);
        // Transcribed from DB_BUILDERS.md §0, e.g. units = `s s s s 4 4 4 4 4 4 o s s s o 4 s b b b 4 b 4 o b`.
        assert_eq!(layout::<UnitRecord>(), "ssss444444ossso4sbbb4b4ob");
        assert_eq!(layout::<BuildingLevel>(), format!("ss4s{}ss4b4444", four(12)));
        assert_eq!(layout::<FactionRecord>(), format!("s4ssssssobbbssoo{}so444osbbsssso", four(18)));
        assert_eq!(layout::<Projectile>(), format!("ssssooos4os{}ooobb444o44ssoooo", four(7)));
        assert_eq!(
            layout::<UnitStatsLand>(),
            format!(
                "s444soossss4s4ooossooooboo44sso4os4444ssss{}{}444{}bbooob",
                four(10),
                bools(14),
                bools(14)
            )
        );
        assert_eq!(layout::<Technology>(), "ss44s4444bbs4");
        assert_eq!(layout::<RegionRecord>(), "ss444s");
    }
}

db_record! {
    /// One row of `unit_to_unit_abilities_junctions`: a special ability a land unit type has
    /// (838 rows: formations, deployables, rally, inspire, …). INFERRED to be where the unit card's
    /// abilities come from outside the battle files (BATTLE_FIDELITY.md §22).
    pub struct UnitToUnitAbility in "unit_to_unit_abilities_junctions", key = unit {
        /// #0 H: unit key (= `units.key`).
        unit: String,
        /// #1 H: ability key (the exe's special-ability enum names).
        ability: String,
    }
}

db_record! {
    /// One row of `unit_class_to_unit_ability_junctions`: an ability every unit of a class has
    /// (7 rows, all `fire_volley` for the infantry classes).
    pub struct UnitClassToUnitAbility in "unit_class_to_unit_ability_junctions", key = class {
        /// #0 H: unit class (`units` column 3).
        class: String,
        /// #1 H: ability key.
        ability: String,
    }
}

db_record! {
    /// One row of `technology_effects_junction` (106 rows): a technology's effect and its value.
    pub struct TechnologyEffect in "technology_effects_junction", key = technology {
        /// #0 H: technology key (`technologies.key`).
        technology: String,
        /// #1 H: effect key.
        effect: String,
        /// #2 H: effect value.
        value: f32,
    }
}

db_record! {
    /// One row of `effect_bonus_value_unit_ability_junctions` (16 rows): an effect that enables a
    /// unit ability (`enable_fire_and_advance` → `fire_and_advance`, drills such as `rank_fire`).
    pub struct EffectUnitAbility in "effect_bonus_value_unit_ability_junctions", key = effect {
        /// #0 H: effect key.
        effect: String,
        /// #1 H: bonus value (always `enable`).
        bonus: String,
        /// #2 H: ability key (the exe's special-ability enum names, or a drill name).
        ability: String,
    }
}

db_record! {
    /// One row of `effect_bonus_value_shot_type_junctions` (9 rows): an effect that enables a shot
    /// type (`enable_canister_shot` → `canister`).
    pub struct EffectShotType in "effect_bonus_value_shot_type_junctions", key = effect {
        /// #0 H: effect key.
        effect: String,
        /// #1 H: bonus value (always `enable`).
        bonus: String,
        /// #2 H: shot type (`projectile_shot_type_enum`).
        shot_type: String,
    }
}

db_record! {
    /// One row of `battle_weather_types` (10 rows; reader 0x00E54FD0). The battle's current weather
    /// record (`0x005B9510`) is one of these; the chance to hit reads its intensity (`missile::visibility`)
    /// and the reload and fatigue rules its kind (`0x005DBB30` rain, `0x005DBD60` snow).
    pub struct BattleWeatherType in "battle_weather_types", key = key {
        /// #0 @0x00: key (`dry`, `light_rain`, `heavy_snow`, `torrential_dust`, ...).
        key: String,
        /// #1 @0x0C: intensity: dry 0, light 1, heavy 2, torrential 3 (CONFIRMED read by `0x00700EB0`).
        intensity: i32,
        /// #2 (runtime `+0x10`): kind: 0 rain, 1 snow, 2 dust, 3 none (CONFIRMED tests).
        kind: i32,
        /// #3 @0x14: UNKNOWN (90..1000; particle count?).
        unknown_14: i32,
        /// #4 @0x18: UNKNOWN.
        unknown_18: f32,
        /// #5 @0x1C: UNKNOWN.
        unknown_1c: f32,
        /// #6 @0x20: UNKNOWN (0..9, a weather index).
        unknown_20: i32,
        /// #7 @0x24: UNKNOWN flag (false for dust).
        unknown_24: bool,
    }
}

db_record! {
    /// One row of `battle_climate_weather_descriptions` (660 rows; reader 0x00E54260): one weather of a
    /// climate in a season, with its pick weight. The campaign-battle weather pick `0x00F5B4D0` filters
    /// these and draws one by weight (BATTLE_FIDELITY.md §52 (3)).
    pub struct BattleClimateWeather in "battle_climate_weather_descriptions", key = key {
        /// #0 @0x00: key (`lc_am_desert_dry_summer`).
        key: String,
        /// #1 @0x0C: climate (`lc_am_desert`).
        climate: String,
        /// #2 @0x18: season (`season_summer` / `season_winter` / `season_spring` / `season_autumn`).
        season: String,
        /// #3 @0x24: weather (FK `battle_weather_types`).
        weather: String,
        /// #4 @0x30: pick weight (runtime `+0x18`, CONFIRMED use).
        weight: i32,
        /// #5 @0x34: percent chance of the picked weather's flag (runtime `+0x1C`; rain rows 15..60;
        /// meaning UNKNOWN, INFERRED: precipitation falling).
        flag_chance: i32,
        /// #6 @0x38: UNKNOWN.
        unknown_38: i32,
        /// #7 @0x3C: heat fatigue term (runtime `+0x24`, INFERRED §4.2).
        heat: i32,
        /// #8 @0x40: cold fatigue term (runtime `+0x28`, INFERRED §4.2).
        cold: i32,
        /// #9 @0x44: UNKNOWN.
        unknown_44: i32,
        /// #10 @0x48: UNKNOWN.
        unknown_48: i32,
    }
}

db_record! {
    /// One row of `unit_stats_land_experience_bonuses` (10 rows, ranks "0".."9"; row reader
    /// `0x00E85F40`, table getter `0x00E31490`, whose own `"Loading database: %s\n"` string names
    /// `unit_stats_land_experience_bonuses_table`; the row struct is
    /// `EMPIREUTILITY::UNIT_STATS_LAND_EXPERIENCE_BONUS_RECORD` from the `record_index` error
    /// string). The stat bonuses a veteran unit (chevrons) gets.
    ///
    /// CONFIRMED readers (offsets here are the BUILDER offsets, DB_BUILDERS.md §
    /// `unit_stats_land_experience_bonuses_tables`):
    /// * `+0x20` (col 6) — the per-tick fatigue bonus, read by the soldier fatigue accumulation
    ///   `0x00670F40` at the unit's experience byte (`unit+0xD48`). Rows are indexed **by position**,
    ///   not by key: `if (experience < count) rows[experience]`. Rank 0..4 = 0, 5/6 = −1, 7/8 = −2,
    ///   9 = −3 (so veterans tire more slowly).
    /// * `+0x24` (col 7) and `+0x28` (col 8) — the **experience-adjusted cost**: `0x00ED49A0`
    ///   returns `row+0x24 + ROUND(base * row+0x28)`, where `base` is `this+0x2C` or `this+0x30`
    ///   (which one is chosen by its `param_3`). Rank 9 = +360 and ×1.9. Those two fields are
    ///   `units` #4 and the late-era #5 (`UNIT_RECORD` +0x2C / +0x30), and the value is the battle
    ///   army-setup price of a veteran unit (custom and multiplayer battles: the generator
    ///   `0x004765F0` spends it against its budget through `0x0045CB50`; the unit info labels it
    ///   "XpAdjustedCost", `0x005CD340`). Not a battle stat, and not the campaign's recruitment cost
    ///   (that is `units` #7 with effects, `0x00B0D220`); see `GameDatabase::experience_adjusted_cost`.
    /// * `+0x0C..+0x1C` (cols 1..5) — five more per-rank bonuses; no reader found (UNKNOWN).
    pub struct UnitStatsLandExperienceBonuses in "unit_stats_land_experience_bonuses", key = rank {
        /// #0 @0x00 H: experience rank as string ("0".."9"); the exe's fatigue path uses the row
        /// *position*, which is this rank (the file order is 0..9).
        rank: String,
        /// #1 @0x0C: UNKNOWN bonus (rank 9 = 7).
        unknown_0c: i32,
        /// #2 @0x10: UNKNOWN bonus (rank 9 = 7).
        unknown_10: i32,
        /// #3 @0x14: UNKNOWN bonus (rank 9 = 18).
        unknown_14: i32,
        /// #4 @0x18: UNKNOWN bonus (rank 9 = 6).
        unknown_18: i32,
        /// #5 @0x1C: UNKNOWN bonus (rank 9 = 18).
        unknown_1c: i32,
        /// #6 @0x20: the per-tick fatigue bonus added to every soldier of a veteran unit
        /// (CONFIRMED reader `0x00670F40`); rank 9 = −3, ranks 0..4 = 0.
        fatigue_bonus: i32,
        /// #7 @0x24: the flat part of the `0x00ED49A0` XP-adjusted cost (rank 9 = 360).
        unknown_24: i32,
        /// #8 @0x28: the multiplier part of it (f32; rank 9 = 1.9, and
        /// `1.0 + rank/10` throughout).
        unknown_28: f32,
    }
}

db_record! {
    /// One row of `unit_stats_naval_experience_bonuses` (the naval twin of the land table above).
    /// The table getter is `0x00E31710`: its own `"Loading database: %s\n"` call names
    /// `unit_stats_naval_experience_bonuses_table` and it closes with
    /// `FUN_00e86c80("unit_stats_naval_experience_bonuses_table")` — the same address DB_BUILDERS.md
    /// § `unit_stats_naval_experience_bonuses_tables` gives as the table's **name getter**, so the
    /// name is CONFIRMED twice over. The row struct is
    /// `EMPIREUTILITY::UNIT_STATS_NAVAL_EXPERIENCE_BONUS_RECORD` (the `record_index` error string
    /// inside `0x00ED49A0`); row reader `0x00E862E0`, 7 columns.
    ///
    /// CONFIRMED reader: `0x00ED49A0`'s naval branch (`this+0xA0 != 0`) reads `row+0x1C` (col 5,
    /// flat) and `row+0x20` (col 6, f32 multiplier) as
    /// `return row+0x1C + ROUND(base * row+0x20)` — the naval counterpart of the land
    /// `+0x24`/`+0x28` pair. `base` is `this+0x2C` or `this+0x30`, the same two cost fields the land
    /// branch uses, and the result is the battle army-setup **experience-adjusted cost** (the unit
    /// info labels it "XpAdjustedCost"), not a battle stat and not the campaign's recruitment cost.
    /// `+0x0C..+0x18` (cols 1..4) have no reader yet (UNKNOWN).
    pub struct UnitStatsNavalExperienceBonuses in "unit_stats_naval_experience_bonuses", key = rank {
        /// #0 @0x00 H: experience rank as string (the land table's ranks "0".."9").
        rank: String,
        /// #1 @0x0C: UNKNOWN bonus.
        unknown_0c: i32,
        /// #2 @0x10: UNKNOWN bonus.
        unknown_10: i32,
        /// #3 @0x14: UNKNOWN bonus.
        unknown_14: i32,
        /// #4 @0x18: UNKNOWN bonus.
        unknown_18: i32,
        /// #5 @0x1C: the flat part of the naval `0x00ED49A0` cost bonus (CONFIRMED reader).
        unknown_1c: i32,
        /// #6 @0x20: the multiplier part of it (f32; CONFIRMED reader `0x00ED49A0`).
        unknown_20: f32,
    }
}

db_record! {
    /// One row of `fatigue_effects` (30 rows; reader 0x00F714E0): the stat changes of a fatigue
    /// threshold for a unit category. The exe reads them per unit through `0x00649520` (unit record
    /// `+0xE4 + 4 × fatigue level`) as multipliers at runtime `+0x10..+0x1C` (builder `0xC..0x18`
    /// + 4): speed `+0x10` (`0x006543D0`), charge `+0x14` (`0x006B30E0`), control `+0x18`
    /// (`0x006B8AA0`), attack `+0x1C` (`0x006A8290`). CONFIRMED readers; the column order is the
    /// builder order; the runtime value being `1 + delta` is INFERRED (the readers multiply by it).
    pub struct FatigueEffect in "fatigue_effects", key = threshold {
        /// #0: threshold key (`threshold_tired`, `threshold_very_tired`, `threshold_exhausted`).
        threshold: String,
        /// #1: unit category (`infantry`, `cavalry`, `dragoons`, `artillery`, `elephants`, `naval_*`).
        category: String,
        /// #2: speed change.
        speed: f32,
        /// #3: charge change.
        charge: f32,
        /// #4: control (missile chance-to-hit) change.
        control: f32,
        /// #5: melee attack change.
        attack: f32,
    }
}
