import { describe, it, expect } from 'vitest'
import {
  CourtierNames,
  LegacyDaemonToCourtier,
  isCourtierName,
  resolveCourtierName,
} from './courtiers'

describe('CourtierNames', () => {
  it('contains all 22 courtier positions', () => {
    expect(CourtierNames.size).toBe(22)
  })

  it('includes inner court', () => {
    for (const name of ['chamberlain', 'keeper', 'castellan', 'clerk', 'bard']) {
      expect(CourtierNames.has(name)).toBe(true)
    }
  })

  it('includes outer court', () => {
    for (const name of ['artificer', 'envoy', 'sage', 'treasurer']) {
      expect(CourtierNames.has(name)).toBe(true)
    }
  })

  it('includes governance court', () => {
    for (const name of ['magistrate', 'tribune', 'warden', 'marshal']) {
      expect(CourtierNames.has(name)).toBe(true)
    }
  })

  it('includes royal staff', () => {
    for (const name of ['interpreter', 'tailor', 'ambassador', 'chronicler', 'mentor', 'champion', 'scout', 'watchman', 'ranger']) {
      expect(CourtierNames.has(name)).toBe(true)
    }
  })

  it('includes browser services', () => {
    expect(CourtierNames.has('vizier')).toBe(true)
  })
})

describe('isCourtierName', () => {
  it('returns true for courtier names', () => {
    expect(isCourtierName('chamberlain')).toBe(true)
    expect(isCourtierName('bard')).toBe(true)
    expect(isCourtierName('envoy')).toBe(true)
  })

  it('returns false for legacy daemon names', () => {
    expect(isCourtierName('crown')).toBe(false)
    expect(isCourtierName('idea')).toBe(false)
    expect(isCourtierName('globe')).toBe(false)
  })

  it('returns false for infrastructure names', () => {
    expect(isCourtierName('daemon')).toBe(false)
    expect(isCourtierName('config')).toBe(false)
    expect(isCourtierName('omnibus')).toBe(false)
  })
})

describe('resolveCourtierName', () => {
  it('passes through courtier names unchanged', () => {
    expect(resolveCourtierName('chamberlain')).toBe('chamberlain')
    expect(resolveCourtierName('bard')).toBe('bard')
    expect(resolveCourtierName('envoy')).toBe('envoy')
  })

  it('resolves legacy crate names to courtier names', () => {
    expect(resolveCourtierName('crown')).toBe('chamberlain')
    expect(resolveCourtierName('sentinal')).toBe('keeper')
    expect(resolveCourtierName('vault')).toBe('castellan')
    expect(resolveCourtierName('hall')).toBe('clerk')
    expect(resolveCourtierName('idea')).toBe('bard')
    expect(resolveCourtierName('magic')).toBe('artificer')
    expect(resolveCourtierName('globe')).toBe('envoy')
    expect(resolveCourtierName('advisor')).toBe('sage')
  })

  it('resolves absorbed namespace aliases', () => {
    expect(resolveCourtierName('network')).toBe('envoy')
    expect(resolveCourtierName('discovery')).toBe('envoy')
    expect(resolveCourtierName('gospel')).toBe('envoy')
    expect(resolveCourtierName('health')).toBe('envoy')
    expect(resolveCourtierName('tower')).toBe('envoy')
    expect(resolveCourtierName('editor')).toBe('artificer')
    expect(resolveCourtierName('identity')).toBe('chamberlain')
  })

  it('returns unknown namespaces unchanged', () => {
    expect(resolveCourtierName('daemon')).toBe('daemon')
    expect(resolveCourtierName('config')).toBe('config')
    expect(resolveCourtierName('unknown')).toBe('unknown')
  })
})

describe('LegacyDaemonToCourtier', () => {
  it('maps all 22 primary crate names', () => {
    const primaryMappings: Record<string, string> = {
      crown: 'chamberlain',
      sentinal: 'keeper',
      vault: 'castellan',
      hall: 'clerk',
      idea: 'bard',
      magic: 'artificer',
      globe: 'envoy',
      advisor: 'sage',
      kingdom: 'magistrate',
      polity: 'tribune',
      bulwark: 'warden',
      jail: 'marshal',
      fortune: 'treasurer',
      lingo: 'interpreter',
      regalia: 'tailor',
      nexus: 'ambassador',
      yoke: 'chronicler',
      oracle: 'mentor',
      quest: 'champion',
      zeitgeist: 'scout',
      undercroft: 'watchman',
      world: 'ranger',
      collab: 'vizier',
    }

    for (const [daemon, courtier] of Object.entries(primaryMappings)) {
      expect(LegacyDaemonToCourtier[daemon]).toBe(courtier)
    }
  })
})
