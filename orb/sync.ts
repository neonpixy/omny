#!/usr/bin/env npx tsx
/**
 * Orb Sync — generates typed courtier interfaces, factories, and registry
 * from the SDK's generated ops.ts and the courtier name mappings.
 *
 * The Orb is one link in the chain:
 *   Rust → generate.py → SDK ops/types → orb/sync.ts → typed courtier access
 *
 * Usage:
 *   npx tsx orb/sync.ts
 *   npm run orb
 */

import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'

// ── Paths ────────────────────────────────────────────────────

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SDK_OPS = resolve(ROOT, '..', 'Library', 'sdk', 'src', 'generated', 'ops.ts')
const COURT_OPS = resolve(ROOT, 'court', 'operations.ts')
const COURTIERS_TS = resolve(ROOT, 'courtiers', 'courtiers.ts')
const COURTIERS_RS = resolve(ROOT, 'courtiers', 'src')
const CHANCELLOR_MODULES = resolve(ROOT, 'chancellor', 'src', 'modules')
const OUT_DIR = resolve(ROOT, 'orb', 'generated')

// ── Types ────────────────────────────────────────────────────

interface SdkOp {
  method: string       // camelCase method name (e.g., "soulProfile")
  daemonOp: string     // full daemon op (e.g., "crown.soul_profile")
  returns: string      // return type (e.g., "CrownState", "void", "unknown")
}

interface CourtierDef {
  name: string                    // courtier name (e.g., "chamberlain")
  displayName: string             // PascalCase (e.g., "Chamberlain")
  ops: SdkOp[]                    // all ops assigned to this courtier
  daemonNamespaces: Set<string>   // which daemon namespaces contribute
}

// ── Daemon → Courtier mapping ────────────────────────────────
// Maps SDK/crate namespaces to courtier names. Used to rewrite daemonOp
// strings so generated bridge calls match what Rust courtiers register.

const DAEMON_TO_COURTIER: Record<string, string> = {
  // Primary — one courtier per ABC crate
  advisor: 'sage',
  bulwark: 'warden',
  crown: 'chamberlain',
  fortune: 'treasurer',
  globe: 'envoy',
  hall: 'clerk',
  idea: 'bard',
  ideas: 'bard',
  jail: 'marshal',
  kingdom: 'magistrate',
  lingo: 'interpreter',
  magic: 'artificer',
  nexus: 'ambassador',
  oracle: 'mentor',
  polity: 'tribune',
  quest: 'champion',
  regalia: 'tailor',
  sentinal: 'keeper',
  undercroft: 'watchman',
  vault: 'castellan',
  world: 'ranger',
  yoke: 'chronicler',
  zeitgeist: 'scout',
  // Sub-namespaces — absorbed into parent courtiers
  appcatalog: 'watchman',
  commerce: 'treasurer',
  device: 'watchman',
  discovery: 'envoy',
  export: 'ambassador',
  exporter: 'ambassador',
  formula: 'interpreter',
  importer: 'ambassador',
  omnibus: 'ranger',
  physical: 'ranger',
  // Courtier-owned composed namespaces
  editor: 'artificer',
  collab: 'vizier',
  config: 'ranger',
  // Legacy aliases
  identity: 'chamberlain',
  network: 'envoy',
  gospel: 'envoy',
  health: 'envoy',
  tower: 'envoy',
  // Identity — courtier names map to themselves (Rust catalogs use these)
  ambassador: 'ambassador',
  artificer: 'artificer',
  bard: 'bard',
  castellan: 'castellan',
  chamberlain: 'chamberlain',
  champion: 'champion',
  chronicler: 'chronicler',
  clerk: 'clerk',
  envoy: 'envoy',
  interpreter: 'interpreter',
  keeper: 'keeper',
  magistrate: 'magistrate',
  marshal: 'marshal',
  mentor: 'mentor',
  ranger: 'ranger',
  sage: 'sage',
  scout: 'scout',
  tailor: 'tailor',
  treasurer: 'treasurer',
  tribune: 'tribune',
  vizier: 'vizier',
  warden: 'warden',
  watchman: 'watchman',
}

// Infrastructure namespaces — not exposed through courtiers
const SKIP_NAMESPACES = new Set(['bridge', 'contacts', 'email', 'pager', 'phone', 'daemon', 'op', 'events'])

// ── Parse SDK ops.ts ─────────────────────────────────────────

function parseSdkOps(content: string): Map<string, SdkOp[]> {
  const namespaces = new Map<string, SdkOp[]>()

  // Match each namespace block: export const <name> = { ... } as const;
  // The SDK generator produces consistent output — every method follows:
  //   methodName: (input: Record<string, unknown> = {}) =>
  //     exec<ReturnType>("daemon.op_name", input),
  const nsPattern = /export const (\w+) = \{([\s\S]*?)\} as const;/g
  let nsMatch: RegExpExecArray | null

  while ((nsMatch = nsPattern.exec(content)) !== null) {
    let nsName = nsMatch[1]
    const body = nsMatch[2]

    // Handle JS reserved word escaping: _export → export
    if (nsName.startsWith('_')) {
      nsName = nsName.slice(1)
    }

    const ops: SdkOp[] = []

    // Match each method: the SDK's internal dispatch function call
    // Pattern: methodName: (input: ...) => \n    dispatch<Type>("op.name", input),
    const methodPattern = /(\w+):\s*\(input:\s*Record<string,\s*unknown>\s*=\s*\{\}\)\s*=>\s*\n\s*\w+<(.+?)>\("(.+?)",\s*input\)/g
    let methodMatch: RegExpExecArray | null

    while ((methodMatch = methodPattern.exec(body)) !== null) {
      ops.push({
        method: methodMatch[1],
        returns: methodMatch[2],
        daemonOp: methodMatch[3],
      })
    }

    if (ops.length > 0) {
      namespaces.set(nsName, ops)
    }
  }

  return namespaces
}

// ── Parse court/operations.ts DaemonOperations ───────────────

function parseDaemonOps(content: string): Map<string, SdkOp[]> {
  const namespaces = new Map<string, SdkOp[]>()

  // Find the DaemonOperations block
  const daemonStart = content.indexOf('export const DaemonOperations')
  if (daemonStart === -1) return namespaces

  const daemonContent = content.slice(daemonStart)

  // Scan for all 'namespace.op_name': entries and group by namespace
  const entryPattern = /'(\w+)\.(\w+)':\s*\{[^}]*\}/g
  let entryMatch: RegExpExecArray | null

  while ((entryMatch = entryPattern.exec(daemonContent)) !== null) {
    const nsName = entryMatch[1]
    const opPart = entryMatch[2]
    // Convert snake_case to camelCase
    const method = opPart.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase())

    if (!namespaces.has(nsName)) {
      namespaces.set(nsName, [])
    }

    namespaces.get(nsName)!.push({
      method,
      daemonOp: `${nsName}.${opPart}`,
      returns: 'unknown',
    })
  }

  return namespaces
}

// ── Parse Rust catalogs ─────────────────────────────────────

interface RustCatalogEntry {
  op: string
  description: string
}

interface RustCatalogEvent {
  event: string
  description: string
}

interface RustCatalog {
  calls: RustCatalogEntry[]
  events: RustCatalogEvent[]
}

/** Parse a Rust file's catalog() method for CallDescriptor and EventDescriptor entries */
function parseRustCatalog(content: string): RustCatalog {
  const calls: RustCatalogEntry[] = []
  const events: RustCatalogEvent[] = []

  // Find the catalog() method body
  const catalogStart = content.indexOf('fn catalog(')
  if (catalogStart === -1) return { calls, events }

  const catalogBody = content.slice(catalogStart)

  // Extract CallDescriptor::new("op.name", "description")
  const callPattern = /\.with_call\(CallDescriptor::new\("([^"]+)",\s*"([^"]+)"\)\)/g
  let match: RegExpExecArray | null
  while ((match = callPattern.exec(catalogBody)) !== null) {
    calls.push({ op: match[1], description: match[2] })
  }

  // Extract EventDescriptor::new("event.name", "description")
  const eventPattern = /\.with_emitted_event\(EventDescriptor::new\("([^"]+)",\s*"([^"]+)"\)\)/g
  while ((match = eventPattern.exec(catalogBody)) !== null) {
    events.push({ event: match[1], description: match[2] })
  }

  return { calls, events }
}

/** Parse all Rust files in a directory and merge their catalogs */
function parseRustCatalogsFromDir(dir: string): { ops: Map<string, SdkOp[]>, events: RustCatalogEvent[] } {
  const ops = new Map<string, SdkOp[]>()
  const events: RustCatalogEvent[] = []

  const files = readdirSync(dir).filter(f => f.endsWith('.rs') && f !== 'lib.rs' && f !== 'mod.rs')

  for (const file of files) {
    const content = readFileSync(resolve(dir, file), 'utf-8')
    const catalog = parseRustCatalog(content)

    for (const call of catalog.calls) {
      const dot = call.op.indexOf('.')
      if (dot === -1) continue
      const ns = call.op.slice(0, dot)
      const opPart = call.op.slice(dot + 1)
      // Convert snake_case and dots to camelCase (fortune.balance.get → balanceGet)
      const method = opPart
        .replace(/[._]([a-z])/g, (_, c: string) => c.toUpperCase())

      if (!ops.has(ns)) ops.set(ns, [])
      ops.get(ns)!.push({ method, daemonOp: call.op, returns: 'unknown' })
    }

    events.push(...catalog.events)
  }

  return { ops, events }
}

// ── Merge namespaces (SDK ops win on conflicts) ──────────────

function mergeNamespaces(
  sdk: Map<string, SdkOp[]>,
  daemon: Map<string, SdkOp[]>,
): Map<string, SdkOp[]> {
  const merged = new Map(sdk)

  for (const [nsName, ops] of daemon) {
    const existing = merged.get(nsName)
    if (!existing) {
      merged.set(nsName, ops)
    } else {
      // Add daemon ops that don't already exist in SDK (by daemon op name)
      const existingOps = new Set(existing.map(o => o.daemonOp))
      for (const op of ops) {
        if (!existingOps.has(op.daemonOp)) {
          existing.push(op)
        }
      }
    }
  }

  return merged
}

// ── Group ops by courtier ────────────────────────────────────

function groupByCourtier(namespaces: Map<string, SdkOp[]>): Map<string, CourtierDef> {
  const courtiers = new Map<string, CourtierDef>()

  for (const [nsName, ops] of namespaces) {
    if (SKIP_NAMESPACES.has(nsName)) continue

    const courtierName = DAEMON_TO_COURTIER[nsName]
    if (!courtierName) {
      console.warn(`  WARNING: no courtier mapping for namespace "${nsName}" (${ops.length} ops skipped)`)
      continue
    }

    let def = courtiers.get(courtierName)
    if (!def) {
      def = {
        name: courtierName,
        displayName: courtierName[0].toUpperCase() + courtierName.slice(1),
        ops: [],
        daemonNamespaces: new Set(),
      }
      courtiers.set(courtierName, def)
    }

    def.daemonNamespaces.add(nsName)
    // Rewrite daemonOp namespace from crate name to courtier name
    // e.g., crown.soul_profile → chamberlain.soul_profile
    const rewritten = ops.map(op => {
      const dot = op.daemonOp.indexOf('.')
      if (dot === -1) return op
      return { ...op, daemonOp: `${courtierName}.${op.daemonOp.slice(dot + 1)}` }
    })
    def.ops.push(...rewritten)
  }

  // Deduplicate by normalized daemonOp. SDK ops use underscores (fortune.cash_generate_serial),
  // Rust catalogs use dots (treasurer.cash.generate_serial). After courtier rewrite both start
  // with the courtier name — normalize the op part (dots→underscores) for comparison.
  // Merge: SDK typed returns + Rust catalog daemonOp (the string handlers actually match on).
  for (const def of courtiers.values()) {
    const byNormOp = new Map<string, SdkOp>()
    for (const op of def.ops) {
      const firstDot = op.daemonOp.indexOf('.')
      const normKey = firstDot === -1 ? op.daemonOp
        : op.daemonOp.slice(0, firstDot + 1) + op.daemonOp.slice(firstDot + 1).replace(/\./g, '_')

      const existing = byNormOp.get(normKey)
      if (!existing) {
        byNormOp.set(normKey, op)
      } else {
        // Merge best of both: typed return from SDK, dot-notation daemonOp from Rust catalog
        const bestReturn = existing.returns !== 'unknown' ? existing.returns : op.returns
        const hasDots = (s: string) => s.indexOf('.', s.indexOf('.') + 1) !== -1
        const bestDaemonOp = hasDots(op.daemonOp) ? op.daemonOp
          : hasDots(existing.daemonOp) ? existing.daemonOp
          : existing.daemonOp
        byNormOp.set(normKey, { ...existing, returns: bestReturn, daemonOp: bestDaemonOp })
      }
    }
    def.ops = [...byNormOp.values()]
  }

  // Deduplicate: when multiple sub-namespaces produce the same method name,
  // prefix with the daemon namespace to disambiguate.
  // e.g., exporter.registryCount + importer.registryCount → exporterRegistryCount + importerRegistryCount
  for (const def of courtiers.values()) {
    const seen = new Map<string, SdkOp[]>()
    for (const op of def.ops) {
      const existing = seen.get(op.method)
      if (existing) {
        existing.push(op)
      } else {
        seen.set(op.method, [op])
      }
    }

    // Rename collisions by prefixing with daemon namespace
    const renamed: SdkOp[] = []
    for (const [method, ops] of seen) {
      if (ops.length === 1) {
        renamed.push(ops[0])
      } else {
        for (const op of ops) {
          const ns = op.daemonOp.split('.')[0]
          const prefixed = ns + method[0].toUpperCase() + method.slice(1)
          renamed.push({ ...op, method: prefixed })
        }
      }
    }

    def.ops = renamed
  }

  // Sort ops within each courtier for stable output
  for (const def of courtiers.values()) {
    def.ops.sort((a, b) => a.method.localeCompare(b.method))
  }

  return courtiers
}

// ── Collect referenced types ─────────────────────────────────

function collectReferencedTypes(courtiers: Map<string, CourtierDef>): Set<string> {
  const types = new Set<string>()
  const builtins = new Set(['unknown', 'void', 'number', 'string', 'boolean', 'Uint8Array'])

  for (const def of courtiers.values()) {
    for (const op of def.ops) {
      // Handle array types like "Type[]"
      const baseType = op.returns.replace('[]', '')
      if (!builtins.has(baseType) && !baseType.includes('{')) {
        types.add(baseType)
      }
    }
  }

  return types
}

// ── Generate interfaces.ts ───────────────────────────────────

function generateInterfaces(courtiers: Map<string, CourtierDef>, types: Set<string>): string {
  const lines: string[] = [
    '// AUTO-GENERATED by orb/sync.ts — do not edit manually.',
    '// Source: @omnidea/net SDK ops.ts + courtiers/courtiers.ts',
    '',
  ]

  if (types.size > 0) {
    const sorted = [...types].sort()
    lines.push(`import type { ${sorted.join(', ')} } from '../../../Library/sdk/src/generated/types.js'`)
    lines.push('')
  }

  // Sort courtiers for stable output
  const sorted = [...courtiers.values()].sort((a, b) => a.name.localeCompare(b.name))

  for (const def of sorted) {
    const nsNote = [...def.daemonNamespaces].sort().join(', ')
    lines.push(`// ── ${def.displayName} (${nsNote}) ──`)
    lines.push('')
    lines.push(`export interface ${def.displayName}Court {`)

    for (const op of def.ops) {
      lines.push(`  ${op.method}(input?: Record<string, unknown>): Promise<${op.returns}>`)
    }

    lines.push('}')
    lines.push('')
  }

  return lines.join('\n')
}

// ── Generate factories.ts ────────────────────────────────────

function generateFactories(courtiers: Map<string, CourtierDef>): string {
  const lines: string[] = [
    '// AUTO-GENERATED by orb/sync.ts — do not edit manually.',
    '// Source: @omnidea/net SDK ops.ts + courtiers/courtiers.ts',
    '',
    "import type { CourtBridge } from './registry.js'",
  ]

  // Import all courtier interfaces
  const sorted = [...courtiers.values()].sort((a, b) => a.name.localeCompare(b.name))
  const interfaceNames = sorted.map(d => `${d.displayName}Court`)
  lines.push(`import type { ${interfaceNames.join(', ')} } from './interfaces.js'`)
  lines.push('')

  for (const def of sorted) {
    lines.push(`export function create${def.displayName}(bridge: CourtBridge): ${def.displayName}Court {`)
    lines.push('  return {')

    for (const op of def.ops) {
      lines.push(`    ${op.method}: (input = {}) => bridge('${op.daemonOp}', input) as Promise<${op.returns}>,`)
    }

    lines.push('  }')
    lines.push('}')
    lines.push('')
  }

  return lines.join('\n')
}

// ── Generate registry.ts ─────────────────────────────────────

function generateRegistry(courtiers: Map<string, CourtierDef>): string {
  const sorted = [...courtiers.values()].sort((a, b) => a.name.localeCompare(b.name))

  const lines: string[] = [
    '// AUTO-GENERATED by orb/sync.ts — do not edit manually.',
    '// Source: @omnidea/net SDK ops.ts + courtiers/courtiers.ts',
    '',
    "import type { CastleRuntime } from '../../cornerstone/types'",
  ]

  // Import interfaces
  const interfaceNames = sorted.map(d => `${d.displayName}Court`)
  lines.push(`import type { ${interfaceNames.join(', ')} } from './interfaces.js'`)

  // Import factories
  const factoryNames = sorted.map(d => `create${d.displayName}`)
  lines.push(`import { ${factoryNames.join(', ')} } from './factories.js'`)
  lines.push('')

  // CourtBridge type
  lines.push('/** Bridge function — calls runtime.court() internally */')
  lines.push('export type CourtBridge = (op: string, input?: Record<string, unknown>) => Promise<unknown>')
  lines.push('')

  // CourtierName union
  const nameUnion = sorted.map(d => `'${d.name}'`).join(' | ')
  lines.push('/** All courtier names */')
  lines.push(`export type CourtierName = ${nameUnion}`)
  lines.push('')

  // CourtierMap interface
  lines.push('/** Maps courtier names to their typed interfaces */')
  lines.push('export interface CourtierMap {')
  for (const def of sorted) {
    lines.push(`  ${def.name}: ${def.displayName}Court`)
  }
  lines.push('}')
  lines.push('')

  // TypedCastleRuntime
  lines.push('/** Castle runtime with typed courtier access for declared courtiers */')
  lines.push('export type TypedCastleRuntime<C extends (keyof CourtierMap)[]> =')
  lines.push('  CastleRuntime & Pick<CourtierMap, C[number]>')
  lines.push('')

  // courtierFactories
  lines.push('/** Factory registry — Sceptor uses this to mount typed courtiers */')
  lines.push('export const courtierFactories: Record<CourtierName, (bridge: CourtBridge) => CourtierMap[CourtierName]> = {')
  for (const def of sorted) {
    lines.push(`  ${def.name}: create${def.displayName},`)
  }
  lines.push('}')
  lines.push('')

  // Re-export interfaces
  lines.push('// Re-export all interfaces')
  lines.push(`export type { ${interfaceNames.join(', ')} } from './interfaces.js'`)
  lines.push('')

  return lines.join('\n')
}

// ── Generate DaemonOperations for court/operations.ts ────────

function generateDaemonOpsSection(
  allOps: Map<string, SdkOp[]>,
  allEvents: RustCatalogEvent[],
): string {
  const lines: string[] = [
    '// ---------------------------------------------------------------------------',
    '// Daemon Operations — auto-generated from Rust catalogs by orb/sync.ts.',
    '// These are the ops programs call via castle.court().',
    '// Source: courtiers/src/*.rs + chancellor/src/modules/*.rs',
    '// ---------------------------------------------------------------------------',
    '',
    "export const DaemonOperations: Record<string, Record<string, CourtOperation>> = {",
  ]

  // Sort namespaces for stable output
  const sorted = [...allOps.entries()].sort(([a], [b]) => a.localeCompare(b))

  for (const [ns, ops] of sorted) {
    lines.push(`  ${ns}: {`)
    // Sort ops within namespace and deduplicate
    const seen = new Set<string>()
    const sortedOps = [...ops].sort((a, b) => a.daemonOp.localeCompare(b.daemonOp))
    for (const op of sortedOps) {
      if (seen.has(op.daemonOp)) continue
      seen.add(op.daemonOp)
      // Extract description from the op — use snake_case op part as fallback
      const dot = op.daemonOp.indexOf('.')
      const desc = op.daemonOp.slice(dot + 1).replace(/_/g, ' ')
      lines.push(`    '${op.daemonOp}': { description: '${desc}', namespace: '${ns}' },`)
    }
    lines.push('  },')
  }

  lines.push('}')
  lines.push('')

  // DaemonOperationCount
  lines.push('/** Total daemon operation count across all namespaces */')
  lines.push('export const DaemonOperationCount = Object.values(DaemonOperations).reduce(')
  lines.push('  (sum, ns) => sum + Object.keys(ns).length, 0')
  lines.push(')')
  lines.push('')

  // DaemonEvents
  const eventsByNs = new Map<string, RustCatalogEvent[]>()
  for (const event of allEvents) {
    const dot = event.event.indexOf('.')
    if (dot === -1) continue
    const ns = event.event.slice(0, dot)
    if (!eventsByNs.has(ns)) eventsByNs.set(ns, [])
    eventsByNs.get(ns)!.push(event)
  }

  lines.push('// ---------------------------------------------------------------------------')
  lines.push('// Daemon Events — push notifications from daemon modules.')
  lines.push('// Programs subscribe via castle.on(event, handler).')
  lines.push('// ---------------------------------------------------------------------------')
  lines.push('')
  lines.push("export const DaemonEvents: Record<string, { description: string; namespace: string }[]> = {")

  const sortedEventNs = [...eventsByNs.entries()].sort(([a], [b]) => a.localeCompare(b))
  for (const [ns, events] of sortedEventNs) {
    lines.push(`  ${ns}: [`)
    for (const e of events) {
      lines.push(`    { description: '${e.description}', namespace: '${ns}' },`)
    }
    lines.push('  ],')
  }
  lines.push('}')
  lines.push('')

  // DaemonEventNames
  lines.push('/** All event names, flattened */')
  lines.push('export const DaemonEventNames: string[] = [')
  const allEventNames = allEvents.map(e => e.event).sort()
  // Chunk into lines of ~3
  for (let i = 0; i < allEventNames.length; i += 3) {
    const chunk = allEventNames.slice(i, i + 3).map(n => `'${n}'`).join(', ')
    lines.push(`  ${chunk},`)
  }
  lines.push(']')
  lines.push('')

  // Lookup helpers
  lines.push('// ---------------------------------------------------------------------------')
  lines.push('// Daemon lookup helpers')
  lines.push('// ---------------------------------------------------------------------------')
  lines.push('')
  lines.push('/** Check if a daemon operation exists (SDK/FFI ops OR catalog-composed ops) */')
  lines.push('export function isValidDaemonOp(op: string): boolean {')
  lines.push("  const dot = op.indexOf('.')")
  lines.push('  if (dot === -1) return false')
  lines.push('  const ns = op.slice(0, dot)')
  lines.push('  return (ns in DaemonOperations && op in DaemonOperations[ns]) ||')
  lines.push('         (ns in Operations && op in Operations[ns])')
  lines.push('}')
  lines.push('')
  lines.push('/** Get all daemon operation names in a namespace */')
  lines.push('export function daemonOpsInNamespace(ns: string): string[] {')
  lines.push('  return ns in DaemonOperations ? Object.keys(DaemonOperations[ns]) : []')
  lines.push('}')
  lines.push('')
  lines.push('/** Check if a namespace has daemon operations */')
  lines.push('export function isValidDaemonNamespace(ns: string): boolean {')
  lines.push('  return ns in DaemonOperations')
  lines.push('}')
  lines.push('')

  return lines.join('\n')
}

// ── Main ─────────────────────────────────────────────────────

function main() {
  console.log('Orb Sync — generating typed courtier layer')
  console.log(`  SDK ops: ${SDK_OPS}`)
  console.log(`  Courtier catalogs: ${COURTIERS_RS}`)
  console.log(`  Chancellor modules: ${CHANCELLOR_MODULES}`)
  console.log(`  Output: ${OUT_DIR}`)

  // ── Source 1: SDK ops (FFI-derived, with return types) ──
  const opsContent = readFileSync(SDK_OPS, 'utf-8')
  const sdkNamespaces = parseSdkOps(opsContent)

  let sdkOps = 0
  for (const ops of sdkNamespaces.values()) sdkOps += ops.length
  console.log(`\n  SDK: ${sdkNamespaces.size} namespaces, ${sdkOps} ops (typed returns)`)

  // ── Source 2: Rust catalogs (courtiers + chancellor infra) ──
  const courtierCatalogs = parseRustCatalogsFromDir(COURTIERS_RS)
  const chancellorCatalogs = parseRustCatalogsFromDir(CHANCELLOR_MODULES)

  // Merge Rust catalog ops
  const rustOps = new Map(courtierCatalogs.ops)
  for (const [ns, ops] of chancellorCatalogs.ops) {
    const existing = rustOps.get(ns)
    if (!existing) {
      rustOps.set(ns, ops)
    } else {
      const existingDaemonOps = new Set(existing.map(o => o.daemonOp))
      for (const op of ops) {
        if (!existingDaemonOps.has(op.daemonOp)) existing.push(op)
      }
    }
  }

  let rustOpCount = 0
  for (const ops of rustOps.values()) rustOpCount += ops.length
  const allEvents = [...courtierCatalogs.events, ...chancellorCatalogs.events]
  console.log(`  Rust catalogs: ${rustOps.size} namespaces, ${rustOpCount} ops, ${allEvents.length} events`)

  // ── Merge: SDK ops (typed returns) > Rust catalog ops (unknown returns) ──
  const namespaces = mergeNamespaces(sdkNamespaces, rustOps)

  let totalOps = 0
  for (const ops of namespaces.values()) totalOps += ops.length
  console.log(`  Merged: ${namespaces.size} namespaces, ${totalOps} total ops`)

  // ── Group by courtier ──
  const courtiers = groupByCourtier(namespaces)

  let courtierOps = 0
  for (const def of courtiers.values()) courtierOps += def.ops.length
  console.log(`  Mapped to ${courtiers.size} courtiers, ${courtierOps} typed operations`)
  console.log(`  Skipped ${totalOps - courtierOps} infrastructure ops`)

  // Collect referenced types
  const types = collectReferencedTypes(courtiers)
  console.log(`  Referenced ${types.size} SDK types`)

  // ── Generate Orb files ──
  mkdirSync(OUT_DIR, { recursive: true })

  const interfaces = generateInterfaces(courtiers, types)
  writeFileSync(resolve(OUT_DIR, 'interfaces.ts'), interfaces)
  console.log(`\n  Wrote interfaces.ts (${courtiers.size} interfaces)`)

  const factories = generateFactories(courtiers)
  writeFileSync(resolve(OUT_DIR, 'factories.ts'), factories)
  console.log(`  Wrote factories.ts (${courtiers.size} factories)`)

  const registry = generateRegistry(courtiers)
  writeFileSync(resolve(OUT_DIR, 'registry.ts'), registry)
  console.log(`  Wrote registry.ts (CourtierMap, courtierFactories)`)

  // ── Generate DaemonOperations in court/operations.ts ──
  // Read existing file, keep the SDK Operations section (top), replace everything below
  const existingCourt = readFileSync(COURT_OPS, 'utf-8')
  const marker = '// ---------------------------------------------------------------------------\n// Daemon Operations'
  const markerIdx = existingCourt.indexOf(marker)
  if (markerIdx === -1) {
    console.error('  ERROR: could not find DaemonOperations marker in court/operations.ts')
  } else {
    const sdkSection = existingCourt.slice(0, markerIdx)
    const daemonSection = generateDaemonOpsSection(rustOps, allEvents)
    writeFileSync(COURT_OPS, sdkSection + daemonSection)

    let daemonOpCount = 0
    for (const ops of rustOps.values()) daemonOpCount += ops.length
    console.log(`  Wrote court/operations.ts DaemonOperations (${daemonOpCount} ops, ${allEvents.length} events)`)
  }

  // Stats
  console.log('\n  Courtier breakdown:')
  const sortedByOps = [...courtiers.values()].sort((a, b) => b.ops.length - a.ops.length)
  for (const def of sortedByOps) {
    const ns = [...def.daemonNamespaces].sort().join(', ')
    console.log(`    ${def.displayName.padEnd(14)} ${String(def.ops.length).padStart(3)} ops  (${ns})`)
  }

  console.log('\n  Done. Orb is synced.')
}

main()
