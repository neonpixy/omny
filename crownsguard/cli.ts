import { readFileSync, existsSync } from 'fs'
import { resolve, join } from 'path'
import { validateManifest } from './validator'
import type { ProgramManifest } from '../cornerstone/types'

/**
 * CLI entry point: npx crownsguard check Apps/hearth/
 * Reads program.json, validates against Court, reports results.
 */
function main() {
  const args = process.argv.slice(2)

  if (args[0] !== 'check' || !args[1]) {
    console.error('Usage: crownsguard check <program-dir>')
    process.exit(1)
  }

  const programDir = resolve(args[1])
  const manifestPath = join(programDir, 'program.json')

  if (!existsSync(manifestPath)) {
    console.error(`No program.json found at ${manifestPath}`)
    process.exit(1)
  }

  let manifest: ProgramManifest
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf-8'))
  } catch (e) {
    console.error(`Failed to parse program.json: ${e}`)
    process.exit(1)
  }

  console.log(`Validating ${manifest.name} (${manifest.slug})...`)

  const result = validateManifest(manifest)

  if (result.errors.length > 0) {
    console.error('\nErrors:')
    for (const err of result.errors) console.error(`  ✗ ${err}`)
  }

  if (result.warnings.length > 0) {
    console.warn('\nWarnings:')
    for (const warn of result.warnings) console.warn(`  ⚠ ${warn}`)
  }

  if (result.valid) {
    console.log(`\n✓ ${manifest.name} passes Crownsguard validation`)
    process.exit(0)
  } else {
    console.error(`\n✗ ${manifest.name} failed validation (${result.errors.length} errors)`)
    process.exit(1)
  }
}

main()
