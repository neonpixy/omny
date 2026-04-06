import { readFileSync, existsSync, readdirSync, statSync } from 'fs'
import { join, resolve } from 'path'
import type { ProgramManifest } from '../cornerstone/types'
import { validateManifest } from '../crownsguard/validator'

/** A discovered program with its resolved paths */
export interface DiscoveredProgram {
  manifest: ProgramManifest
  dir: string
  entryPath: string
  valid: boolean
  errors: string[]
}

/**
 * Scan a directory for program.json files and return validated manifests.
 * Default: scans Apps/ (two levels up from sceptor/).
 */
export function discoverPrograms(appsDir?: string): DiscoveredProgram[] {
  const dir = appsDir ?? resolve(__dirname, '../../../../Apps')

  if (!existsSync(dir)) return []

  const entries = readdirSync(dir)
  const programs: DiscoveredProgram[] = []

  for (const entry of entries) {
    if (entry.startsWith('_') || entry.startsWith('.')) continue

    const programDir = join(dir, entry)
    if (!statSync(programDir).isDirectory()) continue

    const manifestPath = join(programDir, 'program.json')
    if (!existsSync(manifestPath)) continue

    try {
      const raw = readFileSync(manifestPath, 'utf-8')
      const manifest: ProgramManifest = JSON.parse(raw)
      const result = validateManifest(manifest)
      const entryPath = resolve(programDir, manifest.entry)

      programs.push({
        manifest,
        dir: programDir,
        entryPath,
        valid: result.valid,
        errors: result.errors,
      })
    } catch {
      programs.push({
        manifest: { name: entry, slug: entry, icon: '', description: '', entry: '', court: [], packages: [], intents: { sends: [], receives: [] } },
        dir: programDir,
        entryPath: '',
        valid: false,
        errors: [`Failed to parse program.json in ${entry}/`],
      })
    }
  }

  return programs
}

/** Get a single program by slug */
export function findProgram(slug: string, appsDir?: string): DiscoveredProgram | undefined {
  return discoverPrograms(appsDir).find(p => p.manifest.slug === slug)
}

/** Get dock programs sorted by section and position */
export function dockPrograms(appsDir?: string): DiscoveredProgram[] {
  return discoverPrograms(appsDir)
    .filter(p => p.valid && p.manifest.dock)
    .sort((a, b) => {
      const sa = a.manifest.dock!.section
      const sb = b.manifest.dock!.section
      if (sa !== sb) return sa.localeCompare(sb)
      return a.manifest.dock!.position - b.manifest.dock!.position
    })
}
