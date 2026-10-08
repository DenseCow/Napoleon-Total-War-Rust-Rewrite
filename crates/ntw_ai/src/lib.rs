//! # ntw_ai: the battle AI and the campaign AI
//!
//! - [`battle`]: controls an army in an `ntw_sim` battle (call [`battle::BattleAi::update`]
//!   once per tick before `Battle::step`).
//! - [`campaign`]: per-faction turn decisions ([`campaign::take_turn`]).
//! - [`tables`]: raw readers for the AI's DB tables.
//!
//! The AI only reads the simulation and gives orders through the simulation's public command
//! functions, so it stays deterministic and in sync for replays and multiplayer.
//! See `analysis/ai/AI_RESEARCH.md` for what is known about the original's AI and what is still
//! PROVISIONAL here.

#![forbid(unsafe_code)]

pub mod battle;
pub mod campaign;
pub mod tables;
