import { isValidNamespace, isValidDaemonNamespace } from '../court/operations'
import { isValidIntent } from '../court/intents'
import { isCourtierName } from '../courtiers/courtiers'
import type { ProgramManifest } from '../cornerstone/types'

const KNOWN_PACKAGES = ['ui', 'crystal', 'editor', 'fx', 'net']

export interface ValidationResult {
  valid: boolean
  errors: string[]
  warnings: string[]
}

export function validateManifest(manifest: ProgramManifest): ValidationResult {
  const errors: string[] = []
  const warnings: string[] = []

  // Required fields
  if (!manifest.name) errors.push('Missing required field: name')
  if (!manifest.slug) errors.push('Missing required field: slug')
  if (!manifest.entry) errors.push('Missing required field: entry')

  // Validate court namespaces — accepts courtier names, daemon namespaces, or FFI namespaces
  for (const ns of manifest.court) {
    if (!isCourtierName(ns) && !isValidNamespace(ns) && !isValidDaemonNamespace(ns)) {
      errors.push(`Unknown Court namespace: "${ns}"`)
    }
  }

  // Validate packages
  for (const pkg of manifest.packages) {
    if (!KNOWN_PACKAGES.includes(pkg)) {
      errors.push(`Unknown package: "${pkg}"`)
    }
  }

  // Validate intents
  for (const intent of manifest.intents.sends) {
    if (!isValidIntent(intent)) {
      errors.push(`Unknown intent (sends): "${intent}"`)
    }
  }
  for (const intent of manifest.intents.receives) {
    if (!isValidIntent(intent)) {
      errors.push(`Unknown intent (receives): "${intent}"`)
    }
  }

  // Warnings
  if (manifest.court.length === 0 && manifest.intents.sends.length === 0) {
    warnings.push('Program declares no Court capabilities and sends no intents — is this intentional?')
  }

  return { valid: errors.length === 0, errors, warnings }
}
