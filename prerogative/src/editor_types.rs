//! Editor types shared between chancellor modules.

use std::collections::HashMap;
use uuid::Uuid;

/// Key for looking up a specific field within a session: (digit_id, field_name).
pub type FieldKey = (Uuid, String);

/// In-memory editing session for one open .idea document.
/// Stores the latest markdown per field for .idea persistence
/// (updated by `editor.set_content` from the browser's markdown debounce).
pub struct EditorSession {
    /// Latest markdown text per (digit_id, field_name), updated by the
    /// browser's `editor.set_content` debounce. Used for .idea persistence.
    pub field_texts: HashMap<FieldKey, String>,
    /// Whether any edits have been made since last save.
    pub dirty: bool,
}
