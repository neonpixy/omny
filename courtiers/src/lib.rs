//! # courtiers
//!
//! The Castle's courtiers — one per ABC, managed by the Chancellor.
//! Each courtier wraps an Omninet crate and exposes its operations
//! as Phone handlers through the DaemonModule trait.

pub mod castellan;
pub mod chamberlain;
pub mod bard;
pub mod clerk;
pub mod keeper;
pub mod artificer;
pub mod envoy;
pub mod sage;
pub mod magistrate;
pub mod tribune;
pub mod warden;
pub mod marshal;
pub mod treasurer;
pub mod interpreter;
pub mod tailor;
pub mod ambassador;
pub mod chronicler;
pub mod mentor;
pub mod champion;
pub mod scout;
pub mod watchman;
pub mod ranger;
pub mod vizier;

use prerogative::DaemonModule;

/// All courtier modules in dependency order.
///
/// Called by Chancellor's `all_modules()` to integrate courtiers
/// into the boot sequence. Infrastructure modules register first.
pub fn all_courtiers() -> Vec<Box<dyn DaemonModule>> {
    vec![
        // Inner Court
        Box::new(chamberlain::ChamberlainModule),
        Box::new(castellan::CastellanModule),
        Box::new(keeper::KeeperModule),
        Box::new(clerk::ClerkModule),
        Box::new(bard::BardModule),

        // Outer Court
        Box::new(artificer::ArtificerModule),
        Box::new(envoy::EnvoyModule),
        Box::new(treasurer::TreasurerModule),
        Box::new(sage::SageModule),

        // Governance Court
        Box::new(magistrate::MagistrateModule),
        Box::new(tribune::TribuneModule),
        Box::new(warden::WardenModule),
        Box::new(marshal::MarshalModule),

        // Royal Staff
        Box::new(interpreter::InterpreterModule),
        Box::new(tailor::TailorModule),
        Box::new(ambassador::AmbassadorModule),
        Box::new(chronicler::ChroniclerModule),
        Box::new(mentor::MentorModule),
        Box::new(champion::ChampionModule),
        Box::new(scout::ScoutModule),
        Box::new(watchman::WatchmanModule),
        Box::new(ranger::RangerModule),

        // Browser Services
        Box::new(vizier::VizierModule),
    ]
}
