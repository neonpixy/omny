import type { ProgramManifest } from '../cornerstone/types'
import type { DiscoveredProgram } from './discovery'

/** Known Library packages and their import paths */
const PACKAGE_MAP: Record<string, string> = {
  ui:      '@omnidea/ui',
  crystal: '@omnidea/crystal',
  editor:  '@omnidea/editor',
  fx:      '@omnidea/fx',
  net:     '@omnidea/net',
}

/** Resolved import paths for a program */
export interface ImportMap {
  /** Package name → resolved path */
  packages: Record<string, string>
  /** @castle/cornerstone always included */
  cornerstone: string
}

/**
 * Build import map entries for a program based on its manifest.
 * Programs only get access to packages they declare.
 *
 * In dev (Vite), these are aliases in vite.config.ts.
 * In production, Sceptor injects these as importmap or Vite aliases.
 */
export function buildImportMap(manifest: ProgramManifest): ImportMap {
  const packages: Record<string, string> = {}

  for (const pkg of manifest.packages) {
    const mapped = PACKAGE_MAP[pkg]
    if (mapped) {
      packages[mapped] = mapped
    }
  }

  return {
    packages,
    cornerstone: '@castle/cornerstone',
  }
}

/** Browser import map shape: global imports + per-program scopes */
export interface BrowserImportMap {
  imports: Record<string, string>
  scopes: Record<string, Record<string, string>>
}

/** Global import mappings — Throne, libraries, and vendor deps all resolve through these */
const GLOBAL_IMPORTS: Record<string, string> = {
  'solid-js':                     '/lib/solid-js/solid.js',
  'solid-js/web':                 '/lib/solid-js/web.js',
  'solid-js/store':               '/lib/solid-js/store.js',
  '@solidjs/router':              '/lib/@solidjs/router.js',
  '@tauri-apps/api/core':         '/lib/@tauri-apps/api-core.js',
  '@tauri-apps/api/event':        '/lib/@tauri-apps/api-event.js',
  '@tauri-apps/api/window':       '/lib/@tauri-apps/api-window.js',
  '@castle/cornerstone':          '/lib/@castle/cornerstone.js',
  '@vizier':                       '/lib/@vizier/vizier.js',
  '@omnidea/ui':                  '/lib/@omnidea/ui.js',
  '@omnidea/crystal':             '/lib/@omnidea/crystal.js',
  '@omnidea/editor':              '/lib/@omnidea/editor.js',
  '@omnidea/fx':                  '/lib/@omnidea/fx.js',
  '@omnidea/net':                 '/lib/@omnidea/net.js',
  // Editor ecosystem (TipTap + ProseMirror + Yjs + solid-tiptap)
  // Built as one code-split bundle by buildEditorEcosystem() in build.ts.
  // Paths match EDITOR_ECOSYSTEM_ENTRIES in build.ts.
  '@tiptap/pm/commands':                       '/lib/editor/pm-commands.js',
  '@tiptap/pm/dropcursor':                     '/lib/editor/pm-dropcursor.js',
  '@tiptap/pm/gapcursor':                      '/lib/editor/pm-gapcursor.js',
  '@tiptap/pm/history':                        '/lib/editor/pm-history.js',
  '@tiptap/pm/keymap':                         '/lib/editor/pm-keymap.js',
  '@tiptap/pm/model':                          '/lib/editor/pm-model.js',
  '@tiptap/pm/schema-list':                    '/lib/editor/pm-schema-list.js',
  '@tiptap/pm/state':                          '/lib/editor/pm-state.js',
  '@tiptap/pm/tables':                         '/lib/editor/pm-tables.js',
  '@tiptap/pm/transform':                      '/lib/editor/pm-transform.js',
  '@tiptap/pm/view':                           '/lib/editor/pm-view.js',
  '@tiptap/core':                              '/lib/editor/tiptap-core.js',
  '@tiptap/core/jsx-runtime':                  '/lib/editor/tiptap-core-jsx.js',
  '@tiptap/starter-kit':                       '/lib/editor/tiptap-starter-kit.js',
  '@tiptap/extension-image':                   '/lib/editor/tiptap-ext-image.js',
  '@tiptap/extension-table':                   '/lib/editor/tiptap-ext-table.js',
  '@tiptap/extension-table-row':               '/lib/editor/tiptap-ext-table-row.js',
  '@tiptap/extension-table-cell':              '/lib/editor/tiptap-ext-table-cell.js',
  '@tiptap/extension-table-header':            '/lib/editor/tiptap-ext-table-header.js',
  '@tiptap/extension-task-list':               '/lib/editor/tiptap-ext-task-list.js',
  '@tiptap/extension-task-item':               '/lib/editor/tiptap-ext-task-item.js',
  '@tiptap/extension-placeholder':             '/lib/editor/tiptap-ext-placeholder.js',
  '@tiptap/extension-collaboration':           '/lib/editor/tiptap-ext-collab.js',
  '@tiptap/extension-collaboration-cursor':    '/lib/editor/tiptap-ext-collab-cursor.js',
  'yjs':                                       '/lib/editor/yjs.js',
  'y-prosemirror':                             '/lib/editor/y-prosemirror.js',
  'y-protocols/awareness':                     '/lib/editor/y-protocols-awareness.js',
  'y-protocols/sync':                          '/lib/editor/y-protocols-sync.js',
  'lib0/encoding':                             '/lib/editor/lib0-encoding.js',
  'lib0/decoding':                             '/lib/editor/lib0-decoding.js',
  'solid-tiptap':                              '/lib/editor/solid-tiptap.js',
}

/**
 * Generate a browser import map with global imports and per-program scopes.
 *
 * Global imports: all known packages resolve for Throne and library code.
 * Scopes: each program only gets @castle/cornerstone + its declared @omnidea/* packages.
 * Programs can't reach solid-js, @tauri-apps, or @blocksuite directly — those resolve
 * through the library layer. The browser enforces this via referrer-scoped resolution.
 */
export function generateBrowserImportMap(programs: DiscoveredProgram[]): BrowserImportMap {
  const scopes: Record<string, Record<string, string>> = {}

  for (const program of programs) {
    if (!program.valid) continue

    const { slug, packages } = program.manifest
    const scopeKey = `/programs/${slug}.js`
    const scopeMap: Record<string, string> = {
      '@castle/cornerstone': '/lib/@castle/cornerstone.js',
    }

    for (const pkg of packages) {
      const specifier = `@omnidea/${pkg}`
      if (specifier in GLOBAL_IMPORTS) {
        scopeMap[specifier] = GLOBAL_IMPORTS[specifier]
      }
    }

    scopes[scopeKey] = scopeMap
  }

  return { imports: { ...GLOBAL_IMPORTS }, scopes }
}

/**
 * @deprecated Use generateBrowserImportMap() — Vite is being replaced by Castle-native resolution.
 *
 * Produce Vite alias entries for a program's allowed imports.
 * Used by Throne's vite.config.ts to scope what each program can resolve.
 */
export function viteAliases(
  manifest: ProgramManifest,
  libraryRoot: string,
): Record<string, string> {
  const aliases: Record<string, string> = {}

  const PACKAGE_DIRS: Record<string, string> = {
    ui:      'ui/src/lib',
    crystal: 'crystal/src',
    editor:  'editor/src',
    fx:      'fx/src/lib',
    net:     'sdk/dist',
  }

  for (const pkg of manifest.packages) {
    const dir = PACKAGE_DIRS[pkg]
    if (dir) {
      aliases[`@omnidea/${pkg}`] = `${libraryRoot}/${dir}`
    }
  }

  return aliases
}

/**
 * Check if a program is allowed to import a given package.
 */
export function canImport(manifest: ProgramManifest, packageName: string): boolean {
  // cornerstone is always allowed
  if (packageName === '@castle/cornerstone') return true

  // Strip @omnidea/ prefix if present
  const bare = packageName.startsWith('@omnidea/')
    ? packageName.slice('@omnidea/'.length)
    : packageName

  return manifest.packages.includes(bare)
}
