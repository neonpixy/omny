import { createSignal } from 'solid-js'
import type { ProgramManifest, CastleRuntime } from '../cornerstone/types'
import { createScopedRuntime } from './scoper'

export type ProgramState = 'idle' | 'mounting' | 'active' | 'suspended' | 'error'

/** A mounted program instance */
export interface MountedProgram {
  manifest: ProgramManifest
  runtime: CastleRuntime
  state: ProgramState
  mountedAt: number
  error?: string
}

const [programs, setPrograms] = createSignal<Map<string, MountedProgram>>(new Map())

/** Get all currently mounted programs */
export function mountedPrograms(): Map<string, MountedProgram> {
  return programs()
}

/** Get a specific mounted program */
export function getProgram(slug: string): MountedProgram | undefined {
  return programs().get(slug)
}

/**
 * Mount a program — create its scoped runtime and register it.
 * Returns the CastleRuntime to inject via CastleContext.Provider.
 */
export function mountProgram(manifest: ProgramManifest): CastleRuntime {
  const existing = programs().get(manifest.slug)
  if (existing && existing.state === 'active') {
    return existing.runtime
  }

  const runtime = createScopedRuntime(manifest)

  const mounted: MountedProgram = {
    manifest,
    runtime,
    state: 'active',
    mountedAt: Date.now(),
  }

  setPrograms((prev: Map<string, MountedProgram>) => {
    const next = new Map(prev)
    next.set(manifest.slug, mounted)
    return next
  })

  return runtime
}

/** Unmount a program — clean up its runtime and listeners */
export function unmountProgram(slug: string): void {
  setPrograms((prev: Map<string, MountedProgram>) => {
    const next = new Map(prev)
    next.delete(slug)
    return next
  })
}

/**
 * Suspend a program — mark it inactive but keep its state.
 * Suspended programs don't receive events.
 */
export function suspendProgram(slug: string): void {
  setPrograms((prev: Map<string, MountedProgram>) => {
    const current = prev.get(slug)
    if (!current) return prev

    const next = new Map(prev)
    next.set(slug, { ...current, state: 'suspended' })
    return next
  })
}

/** Resume a suspended program */
export function resumeProgram(slug: string): void {
  setPrograms((prev: Map<string, MountedProgram>) => {
    const current = prev.get(slug)
    if (!current || current.state !== 'suspended') return prev

    const next = new Map(prev)
    next.set(slug, { ...current, state: 'active' })
    return next
  })
}

/** Mark a program as errored */
export function errorProgram(slug: string, error: string): void {
  setPrograms((prev: Map<string, MountedProgram>) => {
    const current = prev.get(slug)
    if (!current) return prev

    const next = new Map(prev)
    next.set(slug, { ...current, state: 'error', error })
    return next
  })
}

/** Check if a program is currently active */
export function isActive(slug: string): boolean {
  const p = programs().get(slug)
  return p?.state === 'active'
}
