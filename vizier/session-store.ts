// ─── SessionStore ───
// localStorage-backed persistence for active collaboration sessions.
// Survives Throne restart. On Crown unlock, Vizier reads this to
// restore sessions and reconnect providers.

import type { PersistedSession } from './types'
import * as DocManager from './doc-manager'

const SESSIONS_KEY = 'vizier:sessions'

/** Get all persisted sessions. */
export function getSessions(): PersistedSession[] {
  try {
    const raw = localStorage.getItem(SESSIONS_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

/** Add a session to the persisted list. */
export function addSession(docId: string, programSlug: string): void {
  const sessions = getSessions()
  // Don't add duplicates
  if (sessions.some(s => s.docId === docId)) return

  sessions.push({ docId, joinedAt: Date.now(), programSlug })
  localStorage.setItem(SESSIONS_KEY, JSON.stringify(sessions))
}

/** Remove a session from the persisted list and its doc state. */
export function removeSession(docId: string): void {
  const sessions = getSessions().filter(s => s.docId !== docId)
  localStorage.setItem(SESSIONS_KEY, JSON.stringify(sessions))
  DocManager.removePersisted(docId)
}

/** Clear all sessions and their persisted doc states. */
export function clear(): void {
  const sessions = getSessions()
  for (const s of sessions) {
    DocManager.removePersisted(s.docId)
  }
  localStorage.removeItem(SESSIONS_KEY)
}

/** Check if a session exists for the given doc. */
export function hasSession(docId: string): boolean {
  return getSessions().some(s => s.docId === docId)
}
