// ─── OmninetProvider ───
// Bridges Yjs collaboration with the Omnidea daemon relay via Castle.
// Implements y-protocols sync handshake and awareness protocol.
// Binary Yjs data is base64-encoded for JSON transport.

import * as Y from 'yjs'
import {
  Awareness,
  encodeAwarenessUpdate,
  applyAwarenessUpdate,
  removeAwarenessStates,
} from 'y-protocols/awareness'
import * as syncProtocol from 'y-protocols/sync'
import * as encoding from 'lib0/encoding'
import * as decoding from 'lib0/decoding'

import type { SyncMessage, ProviderState } from './types'

// ── Castle interface types ──────────────────────────────────────────

export interface CastleCourtFn {
  (op: string, input?: Record<string, unknown>): Promise<unknown>
}

export interface CastleOnFn {
  (event: string, handler: (data: unknown) => void): () => void
}

// ── Base64 helpers ──────────────────────────────────────────────────

function toBase64(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.byteLength; i++) {
    binary += String.fromCharCode(bytes[i])
  }
  return btoa(binary)
}

function fromBase64(str: string): Uint8Array {
  const binary = atob(str)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i)
  }
  return bytes
}

// ── Provider ────────────────────────────────────────────────────────

export class OmninetProvider {
  ydoc: Y.Doc
  awareness: Awareness
  ideaId: string

  private _synced = false
  get synced(): boolean { return this._synced }

  private _state: ProviderState = 'disconnected'
  get state(): ProviderState { return this._state }

  private court: CastleCourtFn
  private on: CastleOnFn
  private unsubSync?: () => void
  private unsubAwareness?: () => void
  private connected = false

  // Bound handlers for clean removal
  private _onDocUpdate: (update: Uint8Array, origin: unknown) => void
  private _onAwarenessUpdate: (
    changes: { added: number[]; updated: number[]; removed: number[] },
    origin: unknown,
  ) => void

  constructor(
    ydoc: Y.Doc,
    ideaId: string,
    castle: { court: CastleCourtFn; on: CastleOnFn },
  ) {
    this.ydoc = ydoc
    this.ideaId = ideaId
    this.court = castle.court
    this.on = castle.on
    this.awareness = new Awareness(ydoc)

    this._onDocUpdate = this.handleDocUpdate.bind(this)
    this._onAwarenessUpdate = this.handleAwarenessUpdate.bind(this)
  }

  // ── Connect ─────────────────────────────────────────────────────

  async connect(): Promise<void> {
    if (this.connected) return
    this._state = 'connecting'

    try {
      // 1. Subscribe to remote sync messages
      this.unsubSync = this.on('vizier.sync_message', (raw: unknown) => {
        const msg = raw as SyncMessage
        if (msg.idea_id !== this.ideaId) return

        const bytes = fromBase64(msg.data)
        const decoder = decoding.createDecoder(bytes)
        const encoder = encoding.createEncoder()

        const messageType = syncProtocol.readSyncMessage(
          decoder,
          encoder,
          this.ydoc,
          'remote',
        )

        if (messageType === syncProtocol.messageYjsSyncStep2) {
          this._synced = true
        }

        if (encoding.hasContent(encoder)) {
          this.court('vizier.sync', {
            idea_id: this.ideaId,
            data: toBase64(encoding.toUint8Array(encoder)),
          }).catch(() => {})
        }
      })

      // 2. Subscribe to remote awareness updates
      this.unsubAwareness = this.on('vizier.awareness_update', (raw: unknown) => {
        const msg = raw as SyncMessage
        if (msg.idea_id !== this.ideaId) return

        const update = fromBase64(msg.data)
        applyAwarenessUpdate(this.awareness, update, 'remote')
      })

      // 3. Join the relay session
      await this.court('vizier.join', { idea_id: this.ideaId })

      // 4. Observe local Y.Doc updates and broadcast
      this.ydoc.on('update', this._onDocUpdate)

      // 5. Observe local awareness changes and broadcast
      this.awareness.on('update', this._onAwarenessUpdate)

      // 6. Initiate sync handshake (SyncStep1)
      const syncEncoder = encoding.createEncoder()
      syncProtocol.writeSyncStep1(syncEncoder, this.ydoc)
      await this.court('vizier.sync', {
        idea_id: this.ideaId,
        data: toBase64(encoding.toUint8Array(syncEncoder)),
      })

      // 7. Broadcast initial awareness state
      const awarenessUpdate = encodeAwarenessUpdate(
        this.awareness,
        [this.ydoc.clientID],
      )
      await this.court('vizier.awareness', {
        idea_id: this.ideaId,
        data: toBase64(awarenessUpdate),
      })

      // 8. Mark connected
      this.connected = true
      this._state = 'connected'
    } catch (err) {
      this._state = 'error'
      throw err
    }
  }

  // ── Event handlers ──────────────────────────────────────────────

  private handleDocUpdate(update: Uint8Array, origin: unknown): void {
    if (origin === 'remote') return
    if (!this.connected) return

    const encoder = encoding.createEncoder()
    syncProtocol.writeUpdate(encoder, update)
    const msg = encoding.toUint8Array(encoder)

    this.court('vizier.sync', {
      idea_id: this.ideaId,
      data: toBase64(msg),
    }).catch(() => {})
  }

  private handleAwarenessUpdate(
    { added, updated, removed }: { added: number[]; updated: number[]; removed: number[] },
    origin: unknown,
  ): void {
    if (origin === 'remote') return
    if (!this.connected) return

    const changedClients = added.concat(updated, removed)
    const update = encodeAwarenessUpdate(this.awareness, changedClients)

    this.court('vizier.awareness', {
      idea_id: this.ideaId,
      data: toBase64(update),
    }).catch(() => {})
  }

  // ── Shared metadata (title, etc.) ────────────────────────────────

  private get meta(): Y.Map<string> {
    return this.ydoc.getMap('meta')
  }

  /** Set the document title (broadcasts to peers via Y.Doc sync). */
  setTitle(title: string): void {
    this.ydoc.transact(() => {
      this.meta.set('title', title)
    }, 'local')
  }

  /** Observe ALL title changes (local + remote). Returns unsubscribe. */
  onTitleChange(callback: (title: string, isLocal: boolean) => void): () => void {
    const meta = this.meta
    const handler = (_event: unknown, transaction: Y.Transaction) => {
      const title = meta.get('title')
      if (title !== undefined) callback(title, transaction.origin === 'local')
    }
    meta.observe(handler as Parameters<typeof meta.observe>[0])
    return () => meta.unobserve(handler as Parameters<typeof meta.unobserve>[0])
  }

  // ── Disconnect ──────────────────────────────────────────────────

  disconnect(): void {
    if (!this.connected) return
    this.connected = false

    this.ydoc.off('update', this._onDocUpdate)
    this.awareness.off('update', this._onAwarenessUpdate)

    this.unsubSync?.()
    this.unsubSync = undefined
    this.unsubAwareness?.()
    this.unsubAwareness = undefined

    removeAwarenessStates(this.awareness, [this.ydoc.clientID], 'local')
    this.court('vizier.leave', { idea_id: this.ideaId }).catch(() => {})

    this._synced = false
    this._state = 'disconnected'
  }

  // ── Destroy ─────────────────────────────────────────────────────

  destroy(): void {
    this.disconnect()
  }
}
