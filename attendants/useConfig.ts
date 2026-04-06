import { createSignal } from 'solid-js'
import { useCastle } from '../cornerstone/useCastle'
import type { DaemonConfig } from './types'

/**
 * Provider hook for daemon configuration.
 * Replaces _shared/stores/config.ts — all access goes through Castle.
 *
 * Must be called within a Castle-mounted component.
 */
export function useConfig() {
  const castle = useCastle()

  // --- Config resource (auto-invalidated by config.changed) ---
  const config = castle.resource<DaemonConfig>('config.get')

  // Connected when the resource has loaded successfully
  const connected = () => config.state() === 'ready'

  // --- Pending restart ---
  const [pendingRestart, setPendingRestart] = createSignal(false)

  // --- Actions ---

  async function set(section: string, key: string, value: unknown): Promise<boolean> {
    // Optimistic update: patch the config locally
    const rollback = config.mutate((prev) => {
      if (!prev) return prev!
      const sectionData = prev[section as keyof DaemonConfig]
      if (!sectionData || typeof sectionData !== 'object') return prev
      return {
        ...prev,
        [section]: { ...sectionData, [key]: value },
      }
    })

    try {
      const result = await castle.court('config.set', { section, key, value }) as
        { success?: boolean; needs_restart?: boolean } | null

      if (result?.needs_restart) {
        setPendingRestart(true)
      }

      return result?.success ?? true
    } catch {
      // Rollback optimistic update on failure
      rollback()
      return false
    }
  }

  async function restart(): Promise<void> {
    // Save current page for post-restart navigation
    sessionStorage.setItem('omny:lastPage', location.pathname)
    setPendingRestart(false)

    try {
      await castle.court('daemon.restart', {})
    } catch {
      // Daemon may disconnect before responding — that's expected
    }

    // After restart, daemon needs Crown unlock. Check state and navigate.
    try {
      const state = await castle.court('chamberlain.state', {}) as
        { exists?: boolean; unlocked?: boolean } | null

      if (state?.exists && !state?.unlocked) {
        location.href = '/crown-unlock'
      } else if (!state?.exists) {
        location.href = '/crown-setup'
      }
    } catch {
      // Can't reach daemon after restart — safeguard to unlock
      location.href = '/crown-unlock'
    }
  }

  return {
    config,
    connected,
    set,
    pendingRestart,
    clearRestart: () => setPendingRestart(false),
    restart,
  }
}
