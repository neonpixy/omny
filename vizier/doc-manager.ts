// ─── DocManager ───
// Singleton Y.Doc registry. One Y.Doc per document ID, lives independently
// of component mount/unmount. This is the architectural fix for the content
// duplication bug — no more create/cache/destroy/restore dance.

import * as Y from 'yjs'

interface DocEntry {
  doc: Y.Doc
  refs: number
}

const docs = new Map<string, DocEntry>()

const STORAGE_PREFIX = 'vizier:doc:'

/**
 * Acquire a Y.Doc for the given document ID.
 * Returns the existing instance if one exists, or creates a new one.
 * Increments the reference count.
 *
 * When creating a new doc, automatically restores persisted state from
 * localStorage if available. This ensures the Y.Doc has content BEFORE
 * the editor mounts, preventing a race condition where the editor calls
 * setContent() with daemon HTML and then a later restore doubles it.
 */
export function acquire(docId: string): Y.Doc {
  const existing = docs.get(docId)
  if (existing) {
    existing.refs++
    return existing.doc
  }

  const doc = new Y.Doc()
  docs.set(docId, { doc, refs: 1 })

  // Auto-restore from localStorage — the doc gets persisted state
  // immediately, so ydoc.getXmlFragment('default').length > 0 is
  // true before anyone else touches the doc.
  const stored = localStorage.getItem(STORAGE_PREFIX + docId)
  if (stored) {
    try {
      const binary = atob(stored)
      const bytes = new Uint8Array(binary.length)
      for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i)
      }
      Y.applyUpdate(doc, bytes)
    } catch {
      // Corrupted state — doc is still usable, just empty
    }
  }

  return doc
}

/**
 * Release a Y.Doc. Decrements the reference count.
 * When refs reach zero, the doc is destroyed and removed.
 */
export function release(docId: string): void {
  const entry = docs.get(docId)
  if (!entry) return

  entry.refs--
  if (entry.refs <= 0) {
    entry.doc.destroy()
    docs.delete(docId)
  }
}

/** Check if a Y.Doc exists for the given ID. */
export function has(docId: string): boolean {
  return docs.has(docId)
}

/** Get a Y.Doc without incrementing refs (for read-only access). */
export function get(docId: string): Y.Doc | undefined {
  return docs.get(docId)?.doc
}

/**
 * Persist a Y.Doc's state to localStorage.
 * Called periodically and on beforeunload.
 */
export function persist(docId: string): void {
  const entry = docs.get(docId)
  if (!entry) return

  try {
    const state = Y.encodeStateAsUpdate(entry.doc)
    const binary = String.fromCharCode(...state)
    localStorage.setItem(STORAGE_PREFIX + docId, btoa(binary))
  } catch {
    // localStorage full or unavailable — non-fatal
  }
}

/**
 * Restore a Y.Doc from localStorage.
 * Calls acquire(), which handles auto-restore for new docs.
 * Safe to call even if the doc already exists — acquire() only
 * applies persisted state when first creating the doc.
 */
export function restore(docId: string): Y.Doc | undefined {
  const stored = localStorage.getItem(STORAGE_PREFIX + docId)
  if (!stored) return undefined
  return acquire(docId)
}

/** Remove persisted state for a doc from localStorage. */
export function removePersisted(docId: string): void {
  localStorage.removeItem(STORAGE_PREFIX + docId)
}

/** Persist all active docs to localStorage. Called on beforeunload. */
export function persistAll(): void {
  for (const docId of docs.keys()) {
    persist(docId)
  }
}

/** Destroy all docs and clear the registry. */
export function destroyAll(): void {
  for (const [, entry] of docs) {
    entry.doc.destroy()
  }
  docs.clear()
}
