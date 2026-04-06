import { lazy } from 'solid-js'
import type { ProgramManifest } from '../cornerstone/types'
import type { DiscoveredProgram } from './discovery'

/** A route entry for the Solid.js router */
export interface ProgramRoute {
  path: string
  slug: string
  component: ReturnType<typeof lazy>
  manifest: ProgramManifest
}

/**
 * Wrap a dynamic import as a lazy Solid component.
 * Catches load failures to avoid silent blank pages.
 */
function lazyProgram(
  loader: () => Promise<Record<string, unknown>>,
  exportName: string,
) {
  return lazy(() =>
    loader()
      .then(m => ({ default: (m[exportName] ?? m.default) as any }))
      .catch(e => {
        console.error(`Failed to load program "${exportName}":`, e)
        return { default: () => null }
      })
  )
}

/**
 * Build routes from discovered programs.
 * Each program gets a route at /{slug}.
 *
 * The loader function is provided by Throne — it knows where the
 * program entry files live relative to the Vite root.
 */
export function buildRoutes(
  programs: DiscoveredProgram[],
  loader: (program: DiscoveredProgram) => () => Promise<Record<string, unknown>>,
): ProgramRoute[] {
  return programs
    .filter(p => p.valid)
    .map(p => ({
      path: `/${p.manifest.slug}`,
      slug: p.manifest.slug,
      component: lazyProgram(loader(p), p.manifest.entry.replace(/^\.\//, '').replace(/\.tsx?$/, '')),
      manifest: p.manifest,
    }))
}

/**
 * Build a route map for quick slug-to-route lookup.
 */
export function routeMap(routes: ProgramRoute[]): Map<string, ProgramRoute> {
  const map = new Map<string, ProgramRoute>()
  for (const route of routes) {
    map.set(route.slug, route)
  }
  return map
}

/** Get the default route (first dock program or '/home') */
export function defaultRoute(routes: ProgramRoute[]): string {
  const home = routes.find(r => r.slug === 'home')
  if (home) return home.path

  const firstDock = routes.find(r => r.manifest.dock !== undefined)
  if (firstDock) return firstDock.path

  return routes[0]?.path ?? '/'
}
