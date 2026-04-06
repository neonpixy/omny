/**
 * Court Courtiers — courtier namespace utilities.
 *
 * Since courtiers now register under their own names (chamberlain, bard, etc.),
 * no translation is needed. Operations use courtier names end-to-end.
 *
 * This module preserves backward-compat helpers for any code that still
 * references legacy daemon namespaces (network, discovery, etc.).
 */

/** All known courtier names. */
export const CourtierNames = new Set([
  // Inner Court
  'chamberlain', 'keeper', 'castellan', 'clerk', 'bard',
  // Outer Court
  'artificer', 'envoy', 'sage', 'treasurer',
  // Governance Court
  'magistrate', 'tribune', 'warden', 'marshal',
  // Royal Staff
  'interpreter', 'tailor', 'ambassador', 'chronicler',
  'mentor', 'champion', 'scout', 'watchman', 'ranger',
  // Browser Services
  'vizier',
])

/** Legacy daemon namespace → courtier name (for backward compat only). */
export const LegacyDaemonToCourtier: Record<string, string> = {
  // Legacy aliases that programs might still use
  crown: 'chamberlain',
  sentinal: 'keeper',
  vault: 'castellan',
  hall: 'clerk',
  idea: 'bard',
  magic: 'artificer',
  editor: 'artificer',
  globe: 'envoy',
  network: 'envoy',
  discovery: 'envoy',
  gospel: 'envoy',
  health: 'envoy',
  tower: 'envoy',
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
  identity: 'chamberlain',
}

/** Check if a namespace is a known courtier name. */
export function isCourtierName(ns: string): boolean {
  return CourtierNames.has(ns)
}

/**
 * Resolve a namespace to its courtier name.
 * If it's already a courtier name, returns as-is.
 * If it's a legacy daemon namespace, returns the courtier name.
 * Otherwise returns the input unchanged.
 */
export function resolveCourtierName(ns: string): string {
  if (CourtierNames.has(ns)) return ns
  return LegacyDaemonToCourtier[ns] ?? ns
}
