//! Headless Java client and smoke scenarios for Pumpkin compatibility tests.
//!
//! The client speaks the same protocol version as this workspace
//! (`CURRENT_MC_VERSION`). Scenarios start a scratch server, so they are
//! ignored by the default test run and invoked from `.github/workflows/compat.yml`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod client;
pub mod error;
pub mod pumpkin_proc;
pub mod scenario;
pub mod trace;
pub mod vanilla;

pub use error::Error;
pub use pumpkin_proc::{PumpkinProcess, pumpkin_binary};
pub use scenario::{
    SmokeReport, login_spawn, run_config_packet_checks, run_milestone_diff, run_pumpkin_smoke,
};
pub use trace::{milestone_diff, milestones_match};
pub use vanilla::VanillaProcess;
