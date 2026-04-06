//! Self-registering daemon modules.
//!
//! Infrastructure modules live here. Domain modules live in the `courtiers` crate.
//! Both register through the DaemonModule trait during boot.

mod daemon_mod;
mod config_mod;
mod omnibus_mod;
mod events_mod;
mod op_mod;

use crate::daemon_module::DaemonModule;

/// All daemon modules in dependency order.
///
/// Infrastructure registers first, then courtiers (from courtiers crate),
/// then meta. The boot sequence iterates this list, calling `register()` on each.
pub fn all_modules() -> Vec<Box<dyn DaemonModule>> {
    let mut modules: Vec<Box<dyn DaemonModule>> = vec![
        // Core infrastructure (no deps)
        Box::new(daemon_mod::DaemonOpsModule),
        Box::new(config_mod::ConfigModule),
        Box::new(omnibus_mod::OmnibusModule),
        Box::new(events_mod::EventsModule),
    ];

    // Courtiers — the Castle's named positions (domain logic)
    modules.extend(courtiers::all_courtiers());

    // Meta (dep on phone — must be last)
    modules.push(Box::new(op_mod::OpModule));

    modules
}
