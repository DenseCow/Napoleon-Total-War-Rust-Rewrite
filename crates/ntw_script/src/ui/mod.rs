//! UI scripting: the live UI component tree ([`world`]) and the host that runs the original
//! UI `.luac` scripts against it ([`host`]). Evidence: `analysis/frontend/UI_SCRIPTING.md`.
//! The campaign HUD's engine functions are in [`campaign`] (`analysis/campaign/CAMPAIGN_UI.md`),
//! the battle HUD's in [`battle`].

pub mod battle;
pub mod campaign;
pub mod army_file;
mod army_setup;
mod battle_setup;
mod credits;
pub mod frontend;
pub mod host;
mod image;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod world;

pub use campaign::{CampaignLink, CampaignRequest, CampaignSelection};
pub use host::{CustomArmy, FrontEndFacts, UiFrame, UiRequest, UiScriptHost};
pub use world::{NodeId, PointerEvent, RUNTIME_IMAGE_PREFIX, RuntimeImage, UiNode, UiRect, UiWorld};
