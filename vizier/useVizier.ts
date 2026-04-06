// ─── useVizier ───
// The one hook programs use for collaboration.
// Manages Y.Doc lifecycle, provider connections, session persistence,
// presence tracking, and auto-restore on Crown unlock.

import { createSignal, createEffect, onCleanup } from 'solid-js'
import { useCastle } from '@castle/cornerstone'
import * as DocManager from './doc-manager'
import * as ProviderManager from './provider-manager'
import * as SessionStore from './session-store'
import type { OmninetProvider } from './provider'
import type { CollabPeerInfo } from './types'

export function useVizier() {
  const castle = useCastle()

  // Reactive set of active collab doc IDs (for this session)
  const [collabIds, setCollabIds] = createSignal<Set<string>>(new Set())
  // Reactive peer lists per doc
  const [peerMap, setPeerMap] = createSignal<Map<string, CollabPeerInfo[]>>(new Map())

  let presenceUnsubs = new Map<string, () => void>()

  // ── Helpers ──────────────────────────────────────────────────────

  function addCollabId(docId: string) {
    setCollabIds(prev => { const next = new Set(prev); next.add(docId); return next })
  }

  function removeCollabId(docId: string) {
    setCollabIds(prev => { const next = new Set(prev); next.delete(docId); return next })
  }

  function updatePeers(docId: string, peers: CollabPeerInfo[]) {
    setPeerMap(prev => { const next = new Map(prev); next.set(docId, peers); return next })
  }

  function clearPeers(docId: string) {
    setPeerMap(prev => { const next = new Map(prev); next.delete(docId); return next })
  }

  async function fetchPeers(docId: string) {
    try {
      const result = await castle.court('vizier.peers', { idea_id: docId }) as { peers?: CollabPeerInfo[] }
      updatePeers(docId, result?.peers ?? [])
    } catch {
      updatePeers(docId, [])
    }
  }

  function subscribePresence(docId: string) {
    // Unsubscribe existing
    presenceUnsubs.get(docId)?.()

    const unsub = castle.on('vizier.presence_update', (raw: unknown) => {
      const data = raw as { idea_id: string }
      if (data.idea_id === docId) fetchPeers(docId)
    })
    presenceUnsubs.set(docId, unsub)
  }

  function unsubscribePresence(docId: string) {
    presenceUnsubs.get(docId)?.()
    presenceUnsubs.delete(docId)
  }

  // ── Auto-restore on Crown unlock ────────────────────────────────
  // Y.Doc state is restored automatically in DocManager.acquire()
  // (called when the editor mounts via acquireDoc). This effect only
  // sets collabIds and reconnects providers.

  createEffect(() => {
    const identity = castle.identity()
    if (identity?.state !== 'unlocked') return

    const sessions = SessionStore.getSessions()
    if (sessions.length === 0) return

    const docIds: string[] = []
    for (const session of sessions) {
      addCollabId(session.docId)
      docIds.push(session.docId)
    }

    ProviderManager.reconnectAll(docIds, {
      court: castle.court,
      on: castle.on,
    }).then(() => {
      for (const id of docIds) {
        fetchPeers(id)
        subscribePresence(id)
      }
    }).catch(() => {
      // Reconnection failed — clean up so UI doesn't show false "Sharing"
      for (const id of docIds) {
        removeCollabId(id)
        SessionStore.removeSession(id)
      }
    })
  })

  // ── Persist on beforeunload ─────────────────────────────────────

  const handleBeforeUnload = () => DocManager.persistAll()

  if (typeof window !== 'undefined') {
    window.addEventListener('beforeunload', handleBeforeUnload)
  }

  onCleanup(() => {
    if (typeof window !== 'undefined') {
      window.removeEventListener('beforeunload', handleBeforeUnload)
    }
    // Persist all docs before teardown
    DocManager.persistAll()
    // Clean up presence subscriptions
    for (const unsub of presenceUnsubs.values()) unsub()
    presenceUnsubs.clear()
  })

  // ── Public API ──────────────────────────────────────────────────

  return {
    /**
     * Start collaborating on a document (as the host).
     * Acquires the Y.Doc, connects the provider, persists the session.
     */
    async startCollab(docId: string): Promise<void> {
      if (collabIds().has(docId)) return

      await ProviderManager.connect(docId, {
        court: castle.court,
        on: castle.on,
      })

      addCollabId(docId)
      SessionStore.addSession(docId, castle.program.slug)
      fetchPeers(docId)
      subscribePresence(docId)
    },

    /**
     * Stop collaborating on a document.
     * Disconnects the provider, persists final state, cleans up.
     */
    async stopCollab(docId: string): Promise<void> {
      unsubscribePresence(docId)
      clearPeers(docId)
      removeCollabId(docId)

      DocManager.persist(docId)
      ProviderManager.destroy(docId)
      SessionStore.removeSession(docId)
    },

    /**
     * Join a collaboration session (as a peer).
     * Same as startCollab but semantic — the host already exists.
     */
    async joinCollab(docId: string): Promise<void> {
      if (collabIds().has(docId)) return

      // For a fresh join, the Y.Doc starts empty — content arrives via sync
      await ProviderManager.connect(docId, {
        court: castle.court,
        on: castle.on,
      })

      addCollabId(docId)
      SessionStore.addSession(docId, castle.program.slug)
      fetchPeers(docId)
      subscribePresence(docId)
    },

    /** Check if a document is currently being collaborated on. */
    isCollaborating(docId: string): boolean {
      return collabIds().has(docId)
    },

    /** Get peers for a document. */
    getPeers(docId: string): CollabPeerInfo[] {
      return peerMap().get(docId) ?? []
    },

    /**
     * Acquire a Y.Doc for the given document.
     * Also ensures a disconnected provider exists for cursor rendering.
     * The editor always gets both a Y.Doc and a provider.
     */
    acquireDoc(docId: string): Y.Doc {
      const doc = DocManager.acquire(docId)
      // Ensure a provider exists (disconnected) so CollaborationCursor
      // always has an Awareness to render cursors. When collab starts,
      // startCollab() connects this same provider.
      ProviderManager.ensure(docId, { court: castle.court, on: castle.on })
      return doc
    },

    /** Release a Y.Doc reference (call on editor unmount). */
    releaseDoc(docId: string): void {
      DocManager.release(docId)
    },

    /**
     * Get the provider for a document.
     * Always returns a provider after acquireDoc has been called
     * (may be disconnected if collab hasn't started yet).
     */
    getProvider(docId: string): OmninetProvider | undefined {
      return ProviderManager.getProvider(docId)
    },

    /** Set title via Y.Doc metadata (convenience wrapper). */
    setTitle(docId: string, title: string): void {
      ProviderManager.getProvider(docId)?.setTitle(title)
    },

    /** Observe title changes via Y.Doc metadata (convenience wrapper). */
    onTitleChange(docId: string, callback: (title: string, isLocal: boolean) => void): () => void {
      const provider = ProviderManager.getProvider(docId)
      if (!provider) return () => {}
      return provider.onTitleChange(callback)
    },
  }
}
