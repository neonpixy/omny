import { createSignal, createEffect, untrack } from 'solid-js'
import type { Accessor } from 'solid-js'
import type { ProgramManifest, CastleRuntime, CrownState, CastleResource, ResourceOptions, ResourceState } from '../cornerstone/types'
import { createRuntimeGuard } from '../crownsguard/runtime-guard'
import { Pipelines } from '../court/pipelines'
import { isValidIntent } from '../court/intents'
import { invalidationEvents, opsInvalidatedBy } from '../court/invalidation'
import { resolveCourtierName } from '../courtiers/courtiers'

/**
 * Build a scoped CastleRuntime for a specific program.
 * Every call is checked against the program's manifest by Crownsguard
 * before it reaches the bridge (window.omninet).
 */
export function createScopedRuntime(manifest: ProgramManifest): CastleRuntime {
  const guard = createRuntimeGuard(manifest)
  const [identity, setIdentity] = createSignal<CrownState>({ exists: false, unlocked: false })

  // Intent listeners keyed by intent name
  const intentListeners = new Map<string, Set<(payload: unknown) => void>>()

  // Subscribe to real Crown events from Chancellor and refetch state
  const crownEvents = ['chamberlain.created', 'chamberlain.unlocked', 'chamberlain.locked', 'chamberlain.deleted'] as const
  if (typeof window !== 'undefined' && window.omninet) {
    const refetchIdentity = () => {
      window.omninet.run({ method: 'chamberlain.state', params: {} })
        .then((data: unknown) => setIdentity(data as CrownState))
        .catch(() => { /* Crown state unavailable — leave current value */ })
    }
    // Fetch current state on mount (crown may already be unlocked)
    refetchIdentity()
    for (const event of crownEvents) {
      window.omninet.on(event, refetchIdentity)
    }
  }

  // Resource registry for dedup: key is `${op}::${JSON.stringify(input)}`
  interface ResourceEntry {
    resource: CastleResource<unknown>
    unsubscribes: (() => void)[]
  }
  const resourceRegistry = new Map<string, ResourceEntry>()

  const runtime: CastleRuntime = {
    court(op: string, input?: Record<string, unknown>): Promise<unknown> {
      // Crownsguard validates namespace access against the manifest (courtier names)
      try {
        guard.checkOperation(op)
      } catch (e) {
        return Promise.reject(e)
      }

      // Ops use courtier names directly — no translation needed
      return window.omninet.run({ method: op, params: input ?? {} })
    },

    pipeline(name: string, input?: Record<string, unknown>): Promise<unknown> {
      const pipeline = Pipelines[name]
      if (!pipeline) {
        return Promise.reject(new Error(`Unknown pipeline: "${name}"`))
      }

      guard.checkPipeline(pipeline.requires)

      // Execute pipeline steps in sequence
      return executePipeline(pipeline.steps, input ?? {}, runtime)
    },

    send(intent: string, payload: Record<string, unknown>): void {
      guard.checkSendIntent(intent)

      if (!isValidIntent(intent)) {
        throw new Error(`Unknown intent: "${intent}"`)
      }

      // Broadcast via Chancellor
      window.omninet.run({
        method: 'equipment.broadcast',
        params: { channel: `intent:${intent}`, payload: { ...payload, _sender: manifest.slug } }
      })
    },

    onIntent(intent: string, handler: (payload: unknown) => void): () => void {
      guard.checkReceiveIntent(intent)

      if (!intentListeners.has(intent)) {
        intentListeners.set(intent, new Set())

        // Subscribe via Chancellor
        window.omninet.on(`intent:${intent}`, (data: unknown) => {
          const listeners = intentListeners.get(intent)
          if (listeners) {
            for (const fn of listeners) fn(data)
          }
        })
      }

      intentListeners.get(intent)!.add(handler)

      return () => {
        intentListeners.get(intent)?.delete(handler)
      }
    },

    identity,

    on(event: string, handler: (data: unknown) => void): () => void {
      // Scope events to program's allowed namespaces (courtier names)
      const namespace = resolveCourtierName(event.split('.')[0])
      if (manifest.court.length > 0 && !manifest.court.includes(namespace)) {
        throw new Error(
          `Program "${manifest.slug}" cannot listen to "${event}" — ` +
          `add "${namespace}" to court in program.json`
        )
      }

      // Events use courtier names directly — no translation needed
      return window.omninet.on(event, handler)
    },

    resource<T = unknown>(op: string, options?: ResourceOptions<T>): CastleResource<T> {
      const resolvedInput = typeof options?.input === 'function'
        ? options.input()
        : (options?.input ?? {})
      const key = `${op}::${JSON.stringify(resolvedInput)}`

      // Dedup: return existing resource if already created for this op+input
      const existing = resourceRegistry.get(key)
      if (existing) return existing.resource as CastleResource<T>

      // Create reactive signals
      const [data, setData] = createSignal<T | undefined>(options?.initialValue)
      const [state, setState] = createSignal<ResourceState>('idle')
      const [error, setError] = createSignal<Error | undefined>(undefined)

      const unsubscribes: (() => void)[] = []
      let inFlightPromise: Promise<T> | null = null

      // Core fetch: calls court() which goes through Crownsguard automatically
      const doFetch = (overrideInput?: Record<string, unknown>): Promise<T> => {
        // Dedup in-flight requests
        if (inFlightPromise && !overrideInput) return inFlightPromise

        const fetchInput = overrideInput
          ?? (typeof options?.input === 'function' ? options.input() : (options?.input ?? {}))

        setState(data() !== undefined ? 'stale' : 'loading')

        const promise = runtime.court(op, fetchInput as Record<string, unknown>)
          .then((result: unknown) => {
            setData(() => result as T)
            setState('ready')
            setError(undefined)
            inFlightPromise = null
            return result as T
          })
          .catch((e: unknown) => {
            const err = e instanceof Error ? e : new Error(String(e))
            setError(err)
            setState('error')
            // Keep previous data for stale-while-revalidate
            inFlightPromise = null
            throw err
          })

        inFlightPromise = promise
        return promise
      }

      // Subscribe to invalidation events via runtime.on()
      // (goes through Crownsguard namespace check automatically)
      if (!options?.manualInvalidation) {
        for (const event of invalidationEvents()) {
          const invalidatedOps = opsInvalidatedBy(event)
          if (invalidatedOps.has(op)) {
            try {
              const unsub = runtime.on(event, () => { doFetch().catch(() => {}) })
              unsubscribes.push(unsub)
            } catch {
              // Namespace not in manifest — skip this event silently
            }
          }
        }
      }

      // Build the CastleResource accessor with properties
      const accessor = (() => data()) as CastleResource<T>
      accessor.state = state as Accessor<ResourceState>
      accessor.error = error
      accessor.loading = () => state() === 'loading' || state() === 'stale'
      accessor.refetch = (newInput?: Record<string, unknown>) => doFetch(newInput)
      accessor.mutate = (updater: T | ((prev: T | undefined) => T)) => {
        const prev = data()
        const next = typeof updater === 'function'
          ? (updater as (prev: T | undefined) => T)(prev)
          : updater
        setData(() => next)
        return () => setData(() => prev)  // rollback function
      }

      // Store in registry for dedup
      resourceRegistry.set(key, { resource: accessor as CastleResource<unknown>, unsubscribes })

      // Reactive input: if options.input is a function, track it and refetch on change
      if (typeof options?.input === 'function') {
        const inputFn = options.input as () => Record<string, unknown>
        let first = true
        createEffect(() => {
          const newInput = inputFn()
          if (first) { first = false; return }
          untrack(() => doFetch(newInput))
        })
      }

      // Handle enabled option
      const enabled = options?.enabled
      if (enabled === undefined || enabled === true) {
        // Fetch immediately
        doFetch()
      } else if (typeof enabled === 'function') {
        // Reactive enabled: watch and fetch when it becomes true
        createEffect(() => {
          if ((enabled as Accessor<boolean>)()) {
            untrack(() => doFetch())
          }
        })
      }
      // enabled === false: don't fetch until manual refetch()

      return accessor
    },

    program: { name: manifest.name, slug: manifest.slug },
  }

  return runtime
}

/** Execute pipeline steps in sequence, resolving $ references */
async function executePipeline(
  steps: { id: string; op: string; input: Record<string, unknown> }[],
  initialInput: Record<string, unknown>,
  runtime: CastleRuntime,
): Promise<unknown> {
  const results: Record<string, unknown> = {}

  for (const step of steps) {
    const resolved = resolveRefs(step.input, initialInput, results)

    if (step.op.startsWith('intent:')) {
      runtime.send(step.op.slice(7), resolved as Record<string, unknown>)
      results[step.id] = { result: resolved }
    } else {
      const result = await runtime.court(step.op, resolved as Record<string, unknown>)
      results[step.id] = { result }
    }
  }

  return results
}

/** Resolve $var and $step.result references in pipeline inputs */
function resolveRefs(
  input: Record<string, unknown>,
  initial: Record<string, unknown>,
  results: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = {}

  for (const [key, value] of Object.entries(input)) {
    if (typeof value === 'string' && value.startsWith('$')) {
      out[key] = resolveRef(value, initial, results)
    } else {
      out[key] = value
    }
  }

  return out
}

function resolveRef(
  ref: string,
  initial: Record<string, unknown>,
  results: Record<string, unknown>,
): unknown {
  const path = ref.slice(1) // strip $
  const parts = path.split('.')

  // Try initial input first (e.g., $title, $idea_id)
  if (parts.length === 1 && parts[0] in initial) {
    return initial[parts[0]]
  }

  // Try step results (e.g., $idea.result.id)
  let current: unknown = results
  for (const part of parts) {
    if (current == null || typeof current !== 'object') return undefined
    current = (current as Record<string, unknown>)[part]
  }

  return current
}

/** Extend Window to include the omninet bridge */
declare global {
  interface Window {
    omninet: {
      run: (input: string | { method: string; params: Record<string, unknown> }) => Promise<unknown>
      platform: (op: string, input?: string | Record<string, unknown>) => Promise<unknown>
      capabilities: () => Promise<string>
      on: (event: string, handler: (data: unknown) => void) => () => void
      off: (event: string, handler: (data: unknown) => void) => void
    }
  }
}
