//! # ntw_sim: the deterministic game "model"
//!
//! This crate is the authoritative simulation of NapoleonRust (see `docs/DESIGN.md` §1 and §3.3).
//! It is plain Rust with **no Bevy, no I/O, no threads and no hash-map iteration**, so that the
//! same inputs always give exactly the same outputs (the original runs a deterministic lockstep
//! simulation, DESIGN §1).
//!
//! Every formula cites where it came from, using these tags:
//! - **CONFIRMED**: read directly from the original executable's code (address given).
//! - **INFERRED**: a reasonable reading of the evidence, not proven.
//! - **UNKNOWN / PLACEHOLDER**: the original behaviour has not been found yet; the code here is
//!   our own simple stand-in and must be replaced when the real behaviour is recovered.
//!
//! Gameplay numbers (morale thresholds, fatigue values, unit stats) are **never** hard-coded here.
//! They come from the player's own game data at runtime and are passed in as plain structs
//! ([`battle::morale::KvMorale`], [`battle::fatigue::KvFatigue`], ...).
//!
//! Spec references: "W1" = `analysis/worker1/WORKER1_REPORT.md`, "W3" = `analysis/worker3/WORKER3_REPORT.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod battle;
pub mod calendar;
pub mod campaign;
pub mod fixed;
pub mod limits;
mod fnv;
pub(crate) mod msvc_sort;
pub mod rng;
pub mod unit_kind;
