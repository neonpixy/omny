import type { ProgramManifest } from '../cornerstone/types'
import { resolveCourtierName } from '../courtiers/courtiers'

/**
 * Creates a scoped guard that checks every Court call against the manifest.
 * Wraps the bridge — any unauthorized call throws instead of reaching Chancellor.
 */
export function createRuntimeGuard(manifest: ProgramManifest) {
  const allowedNamespaces = new Set(manifest.court)
  const allowedSendIntents = new Set(manifest.intents.sends)
  const allowedReceiveIntents = new Set(manifest.intents.receives)

  return {
    /** Check if a Court operation is allowed */
    checkOperation(op: string): void {
      const rawNamespace = op.split('.')[0]
      // Accept both courtier names and daemon names — translate daemon→courtier for validation
      const namespace = resolveCourtierName(rawNamespace)
      if (!allowedNamespaces.has(namespace)) {
        throw new Error(
          `Program "${manifest.slug}" is not authorized for "${op}" — ` +
          `add "${namespace}" to court in program.json`
        )
      }
    },

    /** Check if sending an intent is allowed */
    checkSendIntent(intent: string): void {
      if (!allowedSendIntents.has(intent)) {
        throw new Error(
          `Program "${manifest.slug}" cannot send intent "${intent}" — ` +
          `add it to intents.sends in program.json`
        )
      }
    },

    /** Check if receiving an intent is allowed */
    checkReceiveIntent(intent: string): void {
      if (!allowedReceiveIntents.has(intent)) {
        throw new Error(
          `Program "${manifest.slug}" cannot receive intent "${intent}" — ` +
          `add it to intents.receives in program.json`
        )
      }
    },

    /** Check if a pipeline's requirements are met */
    checkPipeline(requires: string[]): void {
      for (const raw of requires) {
        const ns = resolveCourtierName(raw)
        if (!allowedNamespaces.has(ns)) {
          throw new Error(
            `Program "${manifest.slug}" cannot run this pipeline — ` +
            `requires "${ns}" which is not in court`
          )
        }
      }
    }
  }
}
