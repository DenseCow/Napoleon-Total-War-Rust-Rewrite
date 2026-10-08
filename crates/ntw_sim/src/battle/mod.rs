//! The land-battle model (DESIGN §3.3 "battle").
//!
//! - [`speed`]: the battle speed multipliers (W1 §12.2).
//! - [`morale`]: the 8-state morale machine and its modifiers (W1 §12.3, §12.4).
//! - [`fatigue`]: the 6-state fatigue machine (W1 §12.5).
//! - [`ground`]: ground types (movement speed) and heights (slope fatigue) under the units.
//! - [`melee`]: real-time melee hit number, kill chance and blow outcome (W1 §12.9).
//! - [`orders`]: extra unit orders for the AI and scripts (halt, fire at will, attack unit).
//! - [`missile`]: missile range, accuracy, dispersion, chance to hit and impact (W1 §12.6, §12.10).
//! - [`shooting`]: missile units pick targets, reload and fire volleys in the model (W1 §12.6, §12.10).
//! - [`abilities`]: skirmish mode, deployable defences, special abilities, shot types and the
//!   script morale modes (the scripts' unit-controller orders).
//! - [`cloth`]: the verlet solve of a standard bearer's flag cloth (`VerletItems\*.logic`), stepped
//!   from the bearer's pole bone. Display state, but a stateful `f32` integrator, so it lives in
//!   the model crate; the reader is `ntw_formats::verlet`.
//! - [`unit_scale`]: the unit-size option's four steps and the men it leaves a unit with.
//! - [`attributes`]: the unit attribute flags (`unit_stats_land` boolean columns).
//! - [`strength`]: unit strength as the morale code reads it (rally test).
//! - [`rules`]: the `kv_rules` table and C-style arithmetic helpers.
//! - [`autoresolve`]: campaign autoresolve kill rates (W1 §12.7).
//! - [`victory`]: when a battle ends (victory conditions, time limit) and the results totals.
//! - [`model`]: the `Battle` / `LandUnit` skeleton and the 0.1 s tick (W1 §5 item 6).

pub mod abilities;
pub mod attributes;
pub mod autoresolve;
pub mod casualties;
pub mod cloth;
pub mod fatigue;
pub mod garrison;
pub mod ground;
pub mod melee;
pub mod missile;
pub mod orders;
pub mod model;
pub mod morale;
pub mod rules;
pub mod shooting;
pub mod speed;
pub mod strength;
pub mod unit_scale;
pub mod victory;

/// Length of one battle model tick in seconds. W1 §12.2 (INFERRED high; battle time = `tick * 0.1`, 0x00600A00).
pub const TICK_SECONDS: f32 = 0.1;
