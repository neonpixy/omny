import { describe, it, expect } from 'vitest'
import { createRuntimeGuard } from './runtime-guard'
import type { ProgramManifest } from '../cornerstone/types'

function manifest(court: string[], intents?: { sends?: string[]; receives?: string[] }): ProgramManifest {
  return {
    name: 'Test',
    slug: 'test',
    entry: './Test.tsx',
    court,
    packages: [],
    intents: { sends: intents?.sends ?? [], receives: intents?.receives ?? [] },
  } as ProgramManifest
}

describe('createRuntimeGuard', () => {
  describe('checkOperation', () => {
    it('allows operations in declared courtier namespaces', () => {
      const guard = createRuntimeGuard(manifest(['chamberlain', 'castellan']))
      expect(() => guard.checkOperation('chamberlain.state')).not.toThrow()
      expect(() => guard.checkOperation('chamberlain.profile')).not.toThrow()
      expect(() => guard.checkOperation('castellan.save')).not.toThrow()
    })

    it('blocks operations outside declared namespaces', () => {
      const guard = createRuntimeGuard(manifest(['chamberlain']))
      expect(() => guard.checkOperation('castellan.save')).toThrow(/not authorized/)
      expect(() => guard.checkOperation('bard.create')).toThrow(/not authorized/)
    })

    it('error message suggests the courtier name, not daemon name', () => {
      const guard = createRuntimeGuard(manifest(['chamberlain']))
      expect(() => guard.checkOperation('bard.create')).toThrow(/add "bard" to court/)
    })

    it('allows daemon namespaces when declared', () => {
      const guard = createRuntimeGuard(manifest(['daemon', 'config']))
      expect(() => guard.checkOperation('daemon.ping')).not.toThrow()
      expect(() => guard.checkOperation('config.get')).not.toThrow()
    })

    // ── New courtier tests ─────────────────────────────────

    it('allows keeper ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['keeper']))
      expect(() => guard.checkOperation('keeper.encrypt')).not.toThrow()
      expect(() => guard.checkOperation('keeper.decrypt')).not.toThrow()
    })

    it('allows clerk ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['clerk']))
      expect(() => guard.checkOperation('clerk.read')).not.toThrow()
      expect(() => guard.checkOperation('clerk.asset_import')).not.toThrow()
    })

    it('allows artificer ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['artificer']))
      expect(() => guard.checkOperation('artificer.render')).not.toThrow()
      expect(() => guard.checkOperation('artificer.project_swiftui')).not.toThrow()
    })

    it('allows envoy ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['envoy']))
      expect(() => guard.checkOperation('envoy.event_verify')).not.toThrow()
      expect(() => guard.checkOperation('envoy.relay_count')).not.toThrow()
    })

    // ── Sage (Advisor AI) courtier tests ────────────────────

    it('allows sage ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['sage']))
      expect(() => guard.checkOperation('sage.generate')).not.toThrow()
      expect(() => guard.checkOperation('sage.status')).not.toThrow()
    })

    it('accepts daemon names when courtier name is in court (advisor→sage)', () => {
      const guard = createRuntimeGuard(manifest(['sage']))
      expect(() => guard.checkOperation('advisor.generate')).not.toThrow()
      expect(() => guard.checkOperation('advisor.status')).not.toThrow()
    })

    // ── Governance Court courtier tests ─────────────────────

    it('allows magistrate ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['magistrate']))
      expect(() => guard.checkOperation('magistrate.create_charter')).not.toThrow()
      expect(() => guard.checkOperation('magistrate.cast_vote')).not.toThrow()
    })

    it('allows tribune ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['tribune']))
      expect(() => guard.checkOperation('tribune.review')).not.toThrow()
      expect(() => guard.checkOperation('tribune.would_violate')).not.toThrow()
    })

    it('allows warden ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['warden']))
      expect(() => guard.checkOperation('warden.trust_capabilities')).not.toThrow()
      expect(() => guard.checkOperation('warden.compute_drift')).not.toThrow()
    })

    it('allows marshal ops when declared', () => {
      const guard = createRuntimeGuard(manifest(['marshal']))
      expect(() => guard.checkOperation('marshal.check_admission')).not.toThrow()
      expect(() => guard.checkOperation('marshal.raise_flag')).not.toThrow()
    })

    // ── Daemon→courtier translation in guard ──────────────────

    it('accepts daemon names when courtier name is in court (sentinal→keeper)', () => {
      const guard = createRuntimeGuard(manifest(['keeper']))
      expect(() => guard.checkOperation('sentinal.encrypt')).not.toThrow()
      expect(() => guard.checkOperation('sentinal.decrypt')).not.toThrow()
    })

    it('accepts daemon names when courtier name is in court (hall→clerk)', () => {
      const guard = createRuntimeGuard(manifest(['clerk']))
      expect(() => guard.checkOperation('hall.read')).not.toThrow()
      expect(() => guard.checkOperation('hall.asset_list')).not.toThrow()
    })

    it('accepts daemon names when courtier name is in court (magic→artificer)', () => {
      const guard = createRuntimeGuard(manifest(['artificer']))
      expect(() => guard.checkOperation('magic.render')).not.toThrow()
      expect(() => guard.checkOperation('magic.session_create')).not.toThrow()
    })

    it('accepts daemon names when courtier name is in court (collab→vizier)', () => {
      const guard = createRuntimeGuard(manifest(['vizier']))
      expect(() => guard.checkOperation('collab.join')).not.toThrow()
      expect(() => guard.checkOperation('collab.sync')).not.toThrow()
    })

    it('accepts daemon names when courtier name is in court (kingdom→magistrate)', () => {
      const guard = createRuntimeGuard(manifest(['magistrate']))
      expect(() => guard.checkOperation('kingdom.create_charter')).not.toThrow()
      expect(() => guard.checkOperation('kingdom.cast_vote')).not.toThrow()
    })

    it('accepts daemon names when courtier name is in court (polity→tribune)', () => {
      const guard = createRuntimeGuard(manifest(['tribune']))
      expect(() => guard.checkOperation('polity.review')).not.toThrow()
      expect(() => guard.checkOperation('polity.axioms')).not.toThrow()
    })

    it('accepts daemon names when courtier name is in court (bulwark→warden)', () => {
      const guard = createRuntimeGuard(manifest(['warden']))
      expect(() => guard.checkOperation('bulwark.trust_capabilities')).not.toThrow()
      expect(() => guard.checkOperation('bulwark.age_tier')).not.toThrow()
    })

    it('accepts daemon names when courtier name is in court (jail→marshal)', () => {
      const guard = createRuntimeGuard(manifest(['marshal']))
      expect(() => guard.checkOperation('jail.check_admission')).not.toThrow()
      expect(() => guard.checkOperation('jail.accused_rights')).not.toThrow()
    })

    it('accepts absorbed legacy namespaces when envoy is in court', () => {
      const guard = createRuntimeGuard(manifest(['envoy']))
      expect(() => guard.checkOperation('health.relay')).not.toThrow()
      expect(() => guard.checkOperation('health.store_stats')).not.toThrow()
      expect(() => guard.checkOperation('network.post')).not.toThrow()
      expect(() => guard.checkOperation('discovery.peers')).not.toThrow()
      expect(() => guard.checkOperation('gospel.dump')).not.toThrow()
      expect(() => guard.checkOperation('tower.status')).not.toThrow()
    })

    it('blocks absorbed legacy namespaces when envoy is NOT in court', () => {
      const guard = createRuntimeGuard(manifest(['chamberlain']))
      expect(() => guard.checkOperation('health.relay')).toThrow(/not authorized/)
      expect(() => guard.checkOperation('network.post')).toThrow(/not authorized/)
      expect(() => guard.checkOperation('discovery.peers')).toThrow(/not authorized/)
    })
  })

  describe('checkSendIntent', () => {
    it('allows declared send intents', () => {
      const guard = createRuntimeGuard(manifest([], { sends: ['share-content'] }))
      expect(() => guard.checkSendIntent('share-content')).not.toThrow()
    })

    it('blocks undeclared send intents', () => {
      const guard = createRuntimeGuard(manifest([], { sends: [] }))
      expect(() => guard.checkSendIntent('share-content')).toThrow(/cannot send/)
    })
  })

  describe('checkReceiveIntent', () => {
    it('allows declared receive intents', () => {
      const guard = createRuntimeGuard(manifest([], { receives: ['edit-content'] }))
      expect(() => guard.checkReceiveIntent('edit-content')).not.toThrow()
    })

    it('blocks undeclared receive intents', () => {
      const guard = createRuntimeGuard(manifest([], { receives: [] }))
      expect(() => guard.checkReceiveIntent('edit-content')).toThrow(/cannot receive/)
    })
  })

  describe('checkPipeline', () => {
    it('allows pipelines when all requirements met', () => {
      const guard = createRuntimeGuard(manifest(['bard', 'castellan', 'clerk']))
      expect(() => guard.checkPipeline(['bard', 'castellan', 'clerk'])).not.toThrow()
    })

    it('blocks pipelines with unmet requirements', () => {
      const guard = createRuntimeGuard(manifest(['bard']))
      expect(() => guard.checkPipeline(['bard', 'castellan'])).toThrow(/requires "castellan"/)
    })

    it('accepts daemon names in pipeline requirements', () => {
      const guard = createRuntimeGuard(manifest(['keeper', 'clerk']))
      expect(() => guard.checkPipeline(['sentinal', 'hall'])).not.toThrow()
    })

    it('accepts governance daemon names in pipeline requirements', () => {
      const guard = createRuntimeGuard(manifest(['magistrate', 'tribune', 'warden', 'marshal']))
      expect(() => guard.checkPipeline(['kingdom', 'polity', 'bulwark', 'jail'])).not.toThrow()
    })
  })
})
