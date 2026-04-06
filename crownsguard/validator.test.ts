import { describe, it, expect } from 'vitest'
import { validateManifest } from './validator'
import type { ProgramManifest } from '../cornerstone/types'

function manifest(overrides: Partial<ProgramManifest> = {}): ProgramManifest {
  return {
    name: 'Test',
    slug: 'test',
    entry: './Test.tsx',
    court: [],
    packages: [],
    intents: { sends: [], receives: [] },
    ...overrides,
  } as ProgramManifest
}

describe('validateManifest', () => {
  it('validates a minimal valid manifest', () => {
    const result = validateManifest(manifest())
    expect(result.valid).toBe(true)
    expect(result.errors).toHaveLength(0)
  })

  it('rejects missing required fields', () => {
    const result = validateManifest(manifest({ name: '', slug: '', entry: '' }))
    expect(result.valid).toBe(false)
    expect(result.errors).toContain('Missing required field: name')
    expect(result.errors).toContain('Missing required field: slug')
    expect(result.errors).toContain('Missing required field: entry')
  })

  // Courtier names (original)
  it('accepts original courtier names in court', () => {
    const result = validateManifest(manifest({ court: ['chamberlain', 'castellan', 'bard', 'vizier'] }))
    expect(result.valid).toBe(true)
    expect(result.errors).toHaveLength(0)
  })

  // Courtier names (new courtiers)
  it('accepts new courtier names in court', () => {
    const result = validateManifest(manifest({ court: ['keeper', 'clerk', 'artificer', 'envoy'] }))
    expect(result.valid).toBe(true)
    expect(result.errors).toHaveLength(0)
  })

  // Daemon namespaces (still valid during migration)
  it('accepts daemon namespaces in court', () => {
    const result = validateManifest(manifest({ court: ['daemon', 'config', 'editor', 'health'] }))
    expect(result.valid).toBe(true)
    expect(result.errors).toHaveLength(0)
  })

  // Mixed
  it('accepts mix of courtier names and daemon namespaces', () => {
    const result = validateManifest(manifest({ court: ['chamberlain', 'daemon', 'bard', 'config'] }))
    expect(result.valid).toBe(true)
    expect(result.errors).toHaveLength(0)
  })

  // Unknown namespace
  it('rejects unknown namespaces', () => {
    const result = validateManifest(manifest({ court: ['nonexistent'] }))
    expect(result.valid).toBe(false)
    expect(result.errors[0]).toContain('Unknown Court namespace: "nonexistent"')
  })

  // Packages
  it('validates known packages', () => {
    const result = validateManifest(manifest({ packages: ['ui', 'editor', 'crystal'] }))
    expect(result.valid).toBe(true)
  })

  it('rejects unknown packages', () => {
    const result = validateManifest(manifest({ packages: ['nonexistent'] }))
    expect(result.valid).toBe(false)
    expect(result.errors[0]).toContain('Unknown package: "nonexistent"')
  })

  // Intents
  it('validates known intents', () => {
    const result = validateManifest(manifest({
      intents: { sends: ['share-content'], receives: ['edit-content'] },
    }))
    expect(result.valid).toBe(true)
  })

  it('rejects unknown intents', () => {
    const result = validateManifest(manifest({
      intents: { sends: ['nonexistent'], receives: [] },
    }))
    expect(result.valid).toBe(false)
    expect(result.errors[0]).toContain('Unknown intent (sends): "nonexistent"')
  })

  // Warnings
  it('warns when no court and no intents', () => {
    const result = validateManifest(manifest())
    expect(result.warnings.length).toBeGreaterThan(0)
    expect(result.warnings[0]).toContain('no Court capabilities')
  })
})
