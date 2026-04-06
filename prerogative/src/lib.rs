//! # prerogative
//!
//! The Chancellor's prerogative — shared authority types for the Castle.
//! Both `chancellor` (infrastructure) and `courtiers` depend on this crate.

pub mod api_json;
pub mod daemon_module;
pub mod editor_types;
pub mod state;

// Re-exports for convenience
pub use daemon_module::DaemonModule;
pub use editor_types::EditorSession;
pub use state::DaemonState;
