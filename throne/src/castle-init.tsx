/**
 * Castle bootstrap — wires Castle's Sceptor and Orb into Throne.
 *
 * Loads program manifests from the build-generated registry, builds
 * routes + dock items, wraps each route in a CastleContext.Provider
 * with a scoped runtime empowered by the Orb, and provides the mounter
 * for program lifecycle.
 */
import { onCleanup } from 'solid-js'
import type { ProgramManifest } from '../../cornerstone/types'
import type { DiscoveredProgram } from '../../sceptor/discovery'
import { buildRoutes, defaultRoute, type ProgramRoute } from '../../sceptor/router'
import { mountProgram, unmountProgram } from '../../sceptor/mounter'
import { empower } from '../../orb/empower'
// CastleContext MUST come from the import-map-resolved module, not a relative path.
// Programs import useCastle() from @castle/cornerstone via import map — if Throne
// bundles its own copy, the Provider and useContext see different context objects.
import { CastleContext } from '@castle/cornerstone'
import { manifests } from './_registry'

// ---------------------------------------------------------------------------
// 1. Build DiscoveredProgram list from registry manifests
// ---------------------------------------------------------------------------

const discovered: DiscoveredProgram[] = manifests.map((manifest) => ({
  manifest,
  dir: manifest.slug,
  entryPath: manifest.entry,
  valid: true,
  errors: [],
}))

// ---------------------------------------------------------------------------
// 2. Build routes
// ---------------------------------------------------------------------------

/**
 * Program loader via dynamic import.
 * Each program is compiled to dist/programs/{slug}.js by build.ts.
 * The browser import map scopes what each program can resolve.
 */
function programLoader(program: DiscoveredProgram): () => Promise<Record<string, unknown>> {
  const slug = program.manifest.slug
  return () => import(`/programs/${slug}.js`)
}

const rawRoutes: ProgramRoute[] = buildRoutes(discovered, programLoader)

// ---------------------------------------------------------------------------
// 2b. Wrap each route's component in CastleContext.Provider
// ---------------------------------------------------------------------------

/** Build a slug → manifest lookup for the provider wrapper */
const manifestBySlug = new Map<string, ProgramManifest>(
  discovered.map(p => [p.manifest.slug, p.manifest]),
)

/**
 * Wrap a lazy program component so it renders inside a CastleContext.Provider.
 * mountProgram() returns an existing runtime if already active, or creates a
 * new scoped one. onCleanup unmounts when the route is left.
 */
function wrapWithCastle(route: ProgramRoute): ProgramRoute {
  const LazyComponent = route.component
  const manifest = manifestBySlug.get(route.slug)

  if (!manifest) return route

  return {
    ...route,
    component: ((props: any) => {
      const runtime = empower(mountProgram(manifest), manifest)
      onCleanup(() => unmountProgram(manifest.slug))

      return (
        <CastleContext.Provider value={runtime}>
          <LazyComponent {...props} />
        </CastleContext.Provider>
      )
    }) as any,
  }
}

export const routes: ProgramRoute[] = rawRoutes.map(wrapWithCastle)
export const defaultPath = defaultRoute(rawRoutes)

// ---------------------------------------------------------------------------
// 3. Dock items (programs with dock config, sorted by section + position)
// ---------------------------------------------------------------------------

export interface DockEntry {
  slug: string
  path: string
  icon: string
  label: string
  section: string
  position: number
}

export const dockItems: DockEntry[] = discovered
  .filter(p => p.valid && p.manifest.dock)
  .sort((a, b) => {
    const sa = a.manifest.dock!.section
    const sb = b.manifest.dock!.section
    if (sa !== sb) return sa.localeCompare(sb)
    return a.manifest.dock!.position - b.manifest.dock!.position
  })
  .map(p => ({
    slug: p.manifest.slug,
    path: `/${p.manifest.slug}`,
    icon: `ri-${p.manifest.icon.replace(/-?(fill|line)$/, '')}-line`,
    label: p.manifest.name,
    section: p.manifest.dock!.section,
    position: p.manifest.dock!.position,
  }))

// ---------------------------------------------------------------------------
// 4. Re-exports for Throne components
// ---------------------------------------------------------------------------

export { mountProgram, unmountProgram, CastleContext }
export { discovered as programs }
