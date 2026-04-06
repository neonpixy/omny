// ─── Vizier Types ───
// Shared types for the collaboration engine.

/** Information about a connected peer. */
export interface CollabPeerInfo {
  crown_id: string
  display_name: string
  color: string
}

/** A persisted session entry for restore after restart. */
export interface PersistedSession {
  docId: string
  joinedAt: number
  programSlug: string
}

/** Wire format for sync/awareness messages over Castle. */
export interface SyncMessage {
  idea_id: string
  data: string   // base64-encoded binary
  author: string
}

/** Provider connection state. */
export type ProviderState = 'disconnected' | 'connecting' | 'connected' | 'error'
