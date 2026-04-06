/**
 * Empower — mounts the Orb's typed courtiers onto a CastleRuntime.
 *
 * Called by Throne's castle-init after Sceptor creates the scoped runtime.
 * Each courtier declared in the manifest gets a typed interface mounted
 * on the runtime, routing through runtime.court() for full validation.
 */

import type { CastleRuntime, ProgramManifest } from '../cornerstone/types'
import { courtierFactories } from './generated/registry.js'
import type { CourtierName } from './generated/registry.js'

/**
 * Empower a CastleRuntime with typed courtier access from the Orb.
 *
 * Only courtiers declared in the manifest's `court` array are mounted.
 * Each typed method routes through runtime.court() — Crownsguard validates,
 * Court translates, Chancellor delivers. No bypass.
 */
export function empower(runtime: CastleRuntime, manifest: ProgramManifest): CastleRuntime {
  const bridge = runtime.court.bind(runtime)

  for (const name of manifest.court) {
    const factory = courtierFactories[name as CourtierName]
    if (factory) {
      ;(runtime as any)[name] = factory(bridge)
    }
  }

  return runtime
}
