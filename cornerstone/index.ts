export { useCastle } from './useCastle'
export { CastleContext } from './context'
export type { CastleRuntime, ProgramManifest, CourtOperation, Intent, Pipeline, CastleResource, ResourceOptions, ResourceState } from './types'
export type { TypedCastleRuntime, CourtierMap, CourtierName } from '../orb/generated/registry'

// Provider hooks — convenience wrappers around castle.resource()
export { useIdeas, useConfig, useDaemonHealth, useVizier } from '../attendants'
export type { ManifestEntry, IdeaPackage, Digit, DaemonConfig, DaemonStatus, CollabPeerInfo } from '../attendants'
