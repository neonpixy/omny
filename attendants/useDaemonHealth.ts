import { createSignal, onCleanup } from 'solid-js'
import { useCastle } from '../cornerstone/useCastle'
import type { DaemonStatus } from './types'

/**
 * Provider hook for daemon health and network status.
 * Replaces inline fetching in Home.tsx — all access goes through Castle.
 *
 * Must be called within a Castle-mounted component.
 */
export function useDaemonHealth() {
  const castle = useCastle()

  // --- Daemon status resource ---
  const status = castle.resource<DaemonStatus>('daemon.status')

  // --- Store stats resource ---
  const storeStats = castle.resource<{ event_count: number }>('envoy.store_stats')

  // --- Version resource ---
  const version = castle.resource<{ daemon: string; op_count: number }>('daemon.version')

  // --- Peer count (polled, no invalidation event for discovery changes) ---
  const [peerCount, setPeerCount] = createSignal(0)

  // Initial fetch
  castle.court('envoy.peer_count', {})
    .then((r: unknown) => setPeerCount((r as { count?: number })?.count ?? 0))
    .catch(() => {})

  // Poll every 15 seconds
  const pollPeers = setInterval(async () => {
    try {
      const r = await castle.court('envoy.peer_count', {}) as { count?: number }
      setPeerCount(r?.count ?? 0)
    } catch { /* silent — daemon may be restarting */ }
  }, 15_000)

  onCleanup(() => clearInterval(pollPeers))

  return {
    status,
    peerCount,
    storeStats,
    version,
  }
}
