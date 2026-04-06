/** Invalidation rules: daemon events that should trigger resource refetches. */

const InvalidationRules: { event: string; invalidates: string[] }[] = [
  { event: 'chamberlain.created',   invalidates: ['chamberlain.state'] },
  { event: 'chamberlain.unlocked',  invalidates: ['chamberlain.state'] },
  { event: 'chamberlain.locked',    invalidates: ['chamberlain.state'] },
  { event: 'chamberlain.deleted',   invalidates: ['chamberlain.state'] },
  { event: 'bard.created',          invalidates: ['bard.list', 'bard.search'] },
  { event: 'bard.saved',            invalidates: ['bard.list', 'bard.load', 'bard.search'] },
  { event: 'bard.deleted',          invalidates: ['bard.list', 'bard.search'] },
  { event: 'artificer.editor_saved',  invalidates: ['bard.list', 'bard.load'] },
  { event: 'artificer.editor_closed', invalidates: ['bard.list'] },
  { event: 'castellan.unlocked',    invalidates: ['castellan.status'] },
  { event: 'castellan.locked',      invalidates: ['castellan.status'] },
  { event: 'config.changed',        invalidates: ['config.get'] },
]

// Pre-computed lookup: event -> set of ops that should be invalidated
const _lookup = new Map<string, ReadonlySet<string>>()
for (const rule of InvalidationRules) {
  const existing = _lookup.get(rule.event)
  if (existing) {
    const merged = new Set(existing)
    for (const op of rule.invalidates) merged.add(op)
    _lookup.set(rule.event, merged)
  } else {
    _lookup.set(rule.event, new Set(rule.invalidates))
  }
}

const _empty: ReadonlySet<string> = new Set()

/** Returns the set of operations invalidated by a given event, or empty set. */
export function opsInvalidatedBy(event: string): ReadonlySet<string> {
  return _lookup.get(event) ?? _empty
}

/** All events that have invalidation rules. */
export function invalidationEvents(): ReadonlyArray<string> {
  return [..._lookup.keys()]
}

export { InvalidationRules }
