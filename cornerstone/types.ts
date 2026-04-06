import type { Accessor } from 'solid-js'

/** The state of the user's Crown identity */
export interface CrownState {
  exists: boolean
  unlocked: boolean
  crownId?: string
  displayName?: string
  online?: boolean
}

/** Resource lifecycle state */
export type ResourceState = 'idle' | 'loading' | 'ready' | 'error' | 'stale'

/** Options for castle.resource() */
export interface ResourceOptions<T = unknown> {
  input?: Record<string, unknown> | (() => Record<string, unknown>)
  initialValue?: T
  enabled?: boolean | Accessor<boolean>
  manualInvalidation?: boolean
}

/** A reactive, auto-invalidating data accessor returned by castle.resource() */
export interface CastleResource<T = unknown> {
  (): T | undefined               // Reactive accessor (call it like a signal)
  state: Accessor<ResourceState>
  error: Accessor<Error | undefined>
  loading: Accessor<boolean>
  refetch: (input?: Record<string, unknown>) => Promise<T>
  mutate: (updater: T | ((prev: T | undefined) => T)) => () => void  // Returns rollback fn
}

/** A program's manifest (program.json) */
export interface ProgramManifest {
  name: string
  slug: string
  icon: string
  description: string
  entry: string
  court: string[]
  packages: string[]
  intents: {
    sends: string[]
    receives: string[]
  }
  dock?: {
    position: number
    section: string
  }
}

/** The runtime interface every program gets via useCastle() */
export interface CastleRuntime {
  /** Call a Court operation (e.g., 'chamberlain.state', 'bard.create') */
  court: (op: string, input?: Record<string, unknown>) => Promise<unknown>

  /** Execute a named pipeline (e.g., 'bard.create-and-store') */
  pipeline: (name: string, input?: Record<string, unknown>) => Promise<unknown>

  /** Send an intent to other programs (e.g., 'share-content') */
  send: (intent: string, payload: Record<string, unknown>) => void

  /** Listen for intents from other programs */
  onIntent: (intent: string, handler: (payload: unknown) => void) => () => void

  /** Reactive Crown identity state */
  identity: Accessor<CrownState>

  /** Subscribe to Chancellor events */
  on: (event: string, handler: (data: unknown) => void) => () => void

  /** Create a reactive resource backed by a Court operation */
  resource: <T = unknown>(op: string, options?: ResourceOptions<T>) => CastleResource<T>

  /** This program's metadata */
  program: { name: string; slug: string }
}

/** Court operation descriptor */
export interface CourtOperation {
  description: string
  namespace: string
  input?: Record<string, string>
  returns?: string
}

/** Intent descriptor */
export interface Intent {
  description: string
  payload: Record<string, string>
  returns?: Record<string, string>
}

/** Pipeline descriptor */
export interface Pipeline {
  description: string
  requires: string[]
  steps: PipelineStep[]
  returns?: string
}

export interface PipelineStep {
  id: string
  op: string
  input: Record<string, unknown>
}
