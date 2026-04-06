// ─── ProviderManager ───
// Manages OmninetProvider instances per document. Providers are created
// when collab starts and survive component mount/unmount. They are only
// destroyed when collab explicitly ends.

import { OmninetProvider } from './provider'
import type { CastleCourtFn, CastleOnFn } from './provider'
import * as DocManager from './doc-manager'

interface CastleHandle {
  court: CastleCourtFn
  on: CastleOnFn
}

const providers = new Map<string, OmninetProvider>()

/**
 * Ensure a provider exists for the given document (disconnected).
 * Creates the provider if it doesn't exist, but does NOT connect it.
 * Used by acquireDoc so the editor always has a provider for cursors.
 */
export function ensure(
  docId: string,
  castle: CastleHandle,
): OmninetProvider {
  let provider = providers.get(docId)
  if (!provider) {
    const ydoc = DocManager.acquire(docId)
    provider = new OmninetProvider(ydoc, docId, castle)
    providers.set(docId, provider)
  }
  return provider
}

/**
 * Connect a provider for the given document.
 * Creates the provider if it doesn't exist, then connects it.
 */
export async function connect(
  docId: string,
  castle: CastleHandle,
): Promise<OmninetProvider> {
  const provider = ensure(docId, castle)

  if (provider.state !== 'connected') {
    await provider.connect()
  }

  return provider
}

/**
 * Disconnect a provider without destroying it.
 * The provider stays in the map for potential reconnect.
 */
export function disconnect(docId: string): void {
  const provider = providers.get(docId)
  if (provider) provider.disconnect()
}

/**
 * Full teardown — disconnect, destroy, and remove from map.
 * Also releases the Y.Doc reference.
 */
export function destroy(docId: string): void {
  const provider = providers.get(docId)
  if (provider) {
    provider.destroy()
    providers.delete(docId)
    DocManager.release(docId)
  }
}

/** Get an existing provider (or undefined). */
export function getProvider(docId: string): OmninetProvider | undefined {
  return providers.get(docId)
}

/** Check if a provider exists for the given doc. */
export function has(docId: string): boolean {
  return providers.has(docId)
}

/**
 * Reconnect all providers from a list of doc IDs.
 * Used on Crown unlock to restore active sessions.
 */
export async function reconnectAll(
  docIds: string[],
  castle: CastleHandle,
): Promise<void> {
  const results = await Promise.allSettled(
    docIds.map(id => connect(id, castle)),
  )

  for (let i = 0; i < results.length; i++) {
    if (results[i].status === 'rejected') {
      console.warn(`[vizier] Failed to reconnect session ${docIds[i]}`)
    }
  }
}

/** Disconnect all active providers. */
export function disconnectAll(): void {
  for (const provider of providers.values()) {
    provider.disconnect()
  }
}

/** Destroy all providers and clear the map. */
export function destroyAll(): void {
  for (const [docId, provider] of providers) {
    provider.destroy()
    DocManager.release(docId)
  }
  providers.clear()
}
