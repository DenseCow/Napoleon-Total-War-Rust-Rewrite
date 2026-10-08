//! SpeedTree 4 (SpeedTreeRT, statically linked in `Napoleon.exe`): the `.spt` tree description
//! reader and, built on it, our own recreation of the SpeedTreeRT geometry generator.
//!
//! Notes and evidence: `analysis/speedtree/SPEEDTREE.md`. Tags: CONFIRMED (read in the exe's
//! parser and/or checked on every shipped file), INFERRED, UNKNOWN.
//!
//! The `.spt` files live under `rigidmodels\vegetation\battle\<climate>\` and are read through
//! the [`Vfs`](crate::pack::Vfs), so mods that add trees work the same way.

pub mod frond;
pub mod generate;
pub mod params;
pub mod rng;
pub mod spline;
pub mod spt;
pub mod wind;

pub use spline::BezierSpline;
pub use spt::*;
