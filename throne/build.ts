/**
 * Castle Build Switchboard
 *
 * Replaces Vite. Orchestrates esbuild compilation, import map generation,
 * CSS assembly, and dist/ output for Throne + Castle + Programs.
 *
 * Usage:
 *   npx tsx build.ts           # dev build
 *   npx tsx build.ts --watch   # dev build + watch mode
 *   npx tsx build.ts --prod    # production (minified, sourcemaps)
 */

import { createHash } from 'crypto'
import * as esbuild from 'esbuild'
import { solidPlugin } from 'esbuild-plugin-solid'
import {
  mkdirSync, writeFileSync, readFileSync, copyFileSync,
  readdirSync, existsSync, rmSync,
} from 'fs'
import { join, resolve, dirname } from 'path'
import { execFileSync, spawn } from 'child_process'
import { discoverPrograms } from '../sceptor/discovery'
import { generateBrowserImportMap } from '../sceptor/injector'
import type { ProgramManifest } from '../cornerstone/types'

// ── Paths ────────────────────────────────────────────────────────────
const ROOT       = resolve(dirname(new URL(import.meta.url).pathname))
const OMNY       = resolve(ROOT, '..')
const LIBRARY    = resolve(OMNY, '..', 'Library')
const APPS       = resolve(OMNY, '..', 'Apps')
const DIST       = resolve(ROOT, 'dist')

// ── Flags ────────────────────────────────────────────────────────────
const args     = process.argv.slice(2)
const isWatch  = args.includes('--watch')
const isProd   = args.includes('--prod')

// ── Shared esbuild settings ─────────────────────────────────────────
const common: esbuild.BuildOptions = {
  bundle: true,
  format: 'esm',
  platform: 'browser',
  target: 'es2022',
  minify: isProd,
  sourcemap: isProd ? 'linked' : true,
  logLevel: 'info',
}

// ── WGSL text import plugin ─────────────────────────────────────────
// Crystal imports `./shader.wgsl` — load as text string.
const wgslRawPlugin: esbuild.Plugin = {
  name: 'wgsl-raw',
  setup(build) {
    build.onLoad({ filter: /\.wgsl$/ }, (args) => ({
      contents: readFileSync(args.path, 'utf-8'),
      loader: 'text',
    }))
  },
}

// ── Browser exports resolve plugin ──────────────────────────────────
// esbuild ignores the package.json `exports` field when resolving entry
// points, falling back to the legacy `module` field. Packages like
// solid-js set `module` to their server build, so browser builds get
// the wrong code. This plugin reads the `exports` field and resolves
// using the `browser` condition — matching what a browser bundler should do.
const browserExportsPlugin: esbuild.Plugin = {
  name: 'browser-exports',
  setup(build) {
    build.onResolve({ filter: /^[@a-z]/ }, (args) => {
      if (args.kind !== 'entry-point') return null
      try {
        const parts = args.path.split('/')
        const pkgDir = join(ROOT, 'node_modules', ...parts)
        const pkgPath = join(pkgDir, 'package.json')
        if (!existsSync(pkgPath)) return null

        const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'))
        const exports = pkg.exports?.['.']
        if (!exports?.browser) return null

        const browser = exports.browser
        const resolved = typeof browser === 'string'
          ? browser
          : browser.import ?? browser.default
        if (!resolved) return null

        return { path: resolve(pkgDir, resolved) }
      } catch {
        return null
      }
    })
  },
}

// ── Helpers ──────────────────────────────────────────────────────────

function ensureDir(dir: string) {
  mkdirSync(dir, { recursive: true })
}


function cleanDist() {
  if (existsSync(DIST)) rmSync(DIST, { recursive: true })
  ensureDir(DIST)
}

// ── Library package entry points ─────────────────────────────────────
const LIBRARY_PACKAGES: Record<string, string> = {
  ui:      resolve(LIBRARY, 'ui/src/lib/index.ts'),
  crystal: resolve(LIBRARY, 'crystal/src/index.ts'),
  editor:  resolve(LIBRARY, 'editor/src/index.ts'),
  fx:      resolve(LIBRARY, 'fx/src/lib/index.ts'),
  net:     resolve(LIBRARY, 'sdk/src/index.ts'),
}

// ── Vendor deps to pre-bundle ────────────────────────────────────────
// Each maps a bare specifier to the output path under dist/lib/
const VENDOR_ENTRIES: { specifier: string; outfile: string; external?: string[] }[] = [
  { specifier: 'solid-js',          outfile: 'solid-js/solid.js' },
  { specifier: 'solid-js/web',      outfile: 'solid-js/web.js',      external: ['solid-js'] },
  { specifier: 'solid-js/store',    outfile: 'solid-js/store.js',    external: ['solid-js'] },
  { specifier: '@solidjs/router',   outfile: '@solidjs/router.js',   external: ['solid-js', 'solid-js/web'] },
  { specifier: '@tauri-apps/api/core',  outfile: '@tauri-apps/api-core.js' },
  { specifier: '@tauri-apps/api/event', outfile: '@tauri-apps/api-event.js', external: ['@tauri-apps/api/core'] },
  { specifier: '@tauri-apps/api/window', outfile: '@tauri-apps/api-window.js', external: ['@tauri-apps/api/core', '@tauri-apps/api/event'] },
  // TipTap + Yjs built as one code-split bundle via buildEditorEcosystem()
]

// ── 1. Discover Programs ────────────────────────────────────────────

function discover(): { programs: ReturnType<typeof discoverPrograms>; manifests: ProgramManifest[] } {
  const programs = discoverPrograms(APPS)
  const valid = programs.filter(p => p.valid)
  const invalid = programs.filter(p => !p.valid)

  if (invalid.length > 0) {
    console.warn(`\n  Skipping ${invalid.length} invalid program(s):`)
    for (const p of invalid) {
      console.warn(`    - ${p.manifest.slug}: ${p.errors.join(', ')}`)
    }
    console.warn('')
  }

  console.log(`  Found ${valid.length} program(s): ${valid.map(p => p.manifest.slug).join(', ')}`)
  return { programs: valid, manifests: valid.map(p => p.manifest) }
}

// ── 2. Compile Library packages ──────────────────────────────────────

async function buildLibraryPackages() {
  console.log('\n  Building Library packages...')
  const outDir = join(DIST, 'lib', '@omnidea')
  ensureDir(outDir)

  for (const [name, entry] of Object.entries(LIBRARY_PACKAGES)) {
    // Skip if entry doesn't exist (e.g., net might only have dist/)
    const actualEntry = existsSync(entry) ? entry : entry.replace('/src/', '/dist/').replace('.ts', '.js')
    if (!existsSync(actualEntry)) {
      console.warn(`    Skipping @omnidea/${name}: entry not found at ${entry}`)
      continue
    }

    await esbuild.build({
      ...common,
      entryPoints: [actualEntry],
      outfile: join(outDir, `${name}.js`),
      external: ['solid-js', 'solid-js/*', '@tiptap/*', 'solid-tiptap', 'yjs', 'y-prosemirror', 'y-protocols/*', 'lib0/*', '@castle/cornerstone', '@vizier'],
      plugins: [wgslRawPlugin, solidPlugin({ solid: { generate: 'dom' } })],
    })
    console.log(`    @omnidea/${name} -> dist/lib/@omnidea/${name}.js`)
  }
}

// ── 3a. Compile editor ecosystem (TipTap + ProseMirror + Yjs) ───────
//
// Instead of bundling each package individually with fragile cross-references,
// we use esbuild code splitting: one build, multiple entry points, shared chunks.
// esbuild deduplicates shared code (ProseMirror, lib0, etc.) into chunk files
// that all entry points reference. No module identity issues, no sub-path
// resolution failures, no default export re-export problems.

/** All editor ecosystem specifiers that need their own import map entry */
const EDITOR_ECOSYSTEM_ENTRIES: Record<string, string> = {
  // ProseMirror sub-paths (via @tiptap/pm)
  '@tiptap/pm/commands':    'pm-commands',
  '@tiptap/pm/dropcursor':  'pm-dropcursor',
  '@tiptap/pm/gapcursor':   'pm-gapcursor',
  '@tiptap/pm/history':     'pm-history',
  '@tiptap/pm/keymap':      'pm-keymap',
  '@tiptap/pm/model':       'pm-model',
  '@tiptap/pm/schema-list': 'pm-schema-list',
  '@tiptap/pm/state':       'pm-state',
  '@tiptap/pm/tables':      'pm-tables',
  '@tiptap/pm/transform':   'pm-transform',
  '@tiptap/pm/view':        'pm-view',
  // TipTap core + extensions
  '@tiptap/core':                            'tiptap-core',
  '@tiptap/core/jsx-runtime':                'tiptap-core-jsx',
  '@tiptap/starter-kit':                     'tiptap-starter-kit',
  '@tiptap/extension-image':                 'tiptap-ext-image',
  '@tiptap/extension-table':                 'tiptap-ext-table',
  '@tiptap/extension-table-row':             'tiptap-ext-table-row',
  '@tiptap/extension-table-cell':            'tiptap-ext-table-cell',
  '@tiptap/extension-table-header':          'tiptap-ext-table-header',
  '@tiptap/extension-task-list':             'tiptap-ext-task-list',
  '@tiptap/extension-task-item':             'tiptap-ext-task-item',
  '@tiptap/extension-placeholder':           'tiptap-ext-placeholder',
  '@tiptap/extension-collaboration':         'tiptap-ext-collab',
  '@tiptap/extension-collaboration-cursor':  'tiptap-ext-collab-cursor',
  // Yjs ecosystem
  'yjs':                    'yjs',
  'y-prosemirror':          'y-prosemirror',
  'y-protocols/awareness':  'y-protocols-awareness',
  'y-protocols/sync':       'y-protocols-sync',
  'lib0/encoding':          'lib0-encoding',
  'lib0/decoding':          'lib0-decoding',
  // Solid binding
  'solid-tiptap':           'solid-tiptap',
}

async function buildEditorEcosystem() {
  console.log('\n  Building editor ecosystem (code-split)...')
  const outDir = join(DIST, 'lib', 'editor')
  ensureDir(outDir)

  // Build all entry points together with code splitting.
  // esbuild extracts shared dependencies (prosemirror-model, lib0, etc.)
  // into chunk files, so all entries share the same module instances.
  const entryPoints: Record<string, string> = {}
  for (const [specifier, name] of Object.entries(EDITOR_ECOSYSTEM_ENTRIES)) {
    entryPoints[name] = specifier
  }

  const result = await esbuild.build({
    ...common,
    entryPoints,
    outdir: outDir,
    splitting: true,
    chunkNames: 'chunks/[name]-[hash]',
    // Only externalize things OUTSIDE this ecosystem
    external: ['solid-js', 'solid-js/*'],
    nodePaths: [
      join(ROOT, 'node_modules'),
      join(LIBRARY, 'node_modules'),
    ],
    // @tiptap/y-tiptap is a full copy of y-prosemirror (not a re-export).
    // Both define their own `new PluginKey('y-sync')`. ProseMirror uses
    // identity comparison on PluginKeys, so two copies = cursor plugin
    // can't find sync plugin's state. Alias to y-prosemirror so there's
    // only one ySyncPluginKey instance in the entire build.
    alias: {
      '@tiptap/y-tiptap': 'y-prosemirror',
    },
    metafile: true,
  })

  // Report output
  const outputs = Object.keys(result.metafile!.outputs)
    .filter(f => f.endsWith('.js') && !f.endsWith('.map'))
  const entries = outputs.filter(f => !f.includes('/chunks/'))
  const chunks = outputs.filter(f => f.includes('/chunks/'))
  const totalKB = outputs.reduce((sum, f) => {
    const bytes = result.metafile!.outputs[f]?.bytes ?? 0
    return sum + bytes
  }, 0) / 1024

  console.log(`    ${entries.length} entries + ${chunks.length} shared chunks (${totalKB.toFixed(0)} KB total)`)
}

/** Get the import map entries for the editor ecosystem */
function editorEcosystemImports(): Record<string, string> {
  const imports: Record<string, string> = {}
  for (const [specifier, name] of Object.entries(EDITOR_ECOSYSTEM_ENTRIES)) {
    imports[specifier] = `/lib/editor/${name}.js`
  }
  return imports
}

// ── 3b. Compile vendor deps ─────────────────────────────────────────

async function buildVendorDeps() {
  console.log('\n  Building vendor deps...')

  for (const vendor of VENDOR_ENTRIES) {
    const outfile = join(DIST, 'lib', vendor.outfile)
    ensureDir(dirname(outfile))

    try {
      await esbuild.build({
        ...common,
        entryPoints: [vendor.specifier],
        outfile,
        external: vendor.external ?? [],
        plugins: [browserExportsPlugin],
        // Vendor deps need node_modules resolution
        nodePaths: [
          join(ROOT, 'node_modules'),
          join(LIBRARY, 'node_modules'),
        ],
      })
      console.log(`    ${vendor.specifier} -> dist/lib/${vendor.outfile}`)
    } catch (err) {
      console.warn(`    Skipping ${vendor.specifier}: ${(err as Error).message.split('\n')[0]}`)
    }
  }
}

// ── 4. Compile Castle cornerstone ────────────────────────────────────

async function buildCornerstone() {
  console.log('\n  Building Castle cornerstone...')
  const outDir = join(DIST, 'lib', '@castle')
  ensureDir(outDir)

  await esbuild.build({
    ...common,
    entryPoints: [resolve(OMNY, 'cornerstone/index.ts')],
    outfile: join(outDir, 'cornerstone.js'),
    external: ['solid-js', 'solid-js/*', '@vizier'],
    plugins: [solidPlugin({ solid: { generate: 'dom' } })],
  })
  console.log('    @castle/cornerstone -> dist/lib/@castle/cornerstone.js')
}

// ── 4b. Compile Vizier ──────────────────────────────────────────────

async function buildVizier() {
  console.log('\n  Building Vizier...')
  const outDir = join(DIST, 'lib', '@vizier')
  ensureDir(outDir)

  await esbuild.build({
    ...common,
    entryPoints: [resolve(OMNY, 'vizier/index.ts')],
    outfile: join(outDir, 'vizier.js'),
    external: [
      'solid-js', 'solid-js/*',
      '@castle/cornerstone',
      'yjs', 'y-prosemirror', 'y-protocols/*', 'lib0/*',
    ],
    plugins: [solidPlugin({ solid: { generate: 'dom' } })],
  })
  console.log('    @vizier -> dist/lib/@vizier/vizier.js')
}

// ── 5. Generate _registry.ts ─────────────────────────────────────────

function generateRegistry(manifests: ProgramManifest[]) {
  console.log('\n  Generating _registry.ts...')

  const lines = [
    '// AUTO-GENERATED by build.ts — do not edit',
    '',
    'import type { ProgramManifest } from \'../../cornerstone/types\'',
    '',
    'export const manifests: ProgramManifest[] = ',
    JSON.stringify(manifests, null, 2),
    '',
  ]

  writeFileSync(resolve(ROOT, 'src/_registry.ts'), lines.join('\n'))
  console.log(`    ${manifests.length} program manifest(s) written to src/_registry.ts`)
}

// ── 6. Compile programs ──────────────────────────────────────────────

async function buildPrograms(programs: ReturnType<typeof discoverPrograms>) {
  console.log('\n  Building programs...')
  const outDir = join(DIST, 'programs')
  ensureDir(outDir)

  for (const program of programs) {
    const { manifest, entryPath } = program

    if (!existsSync(entryPath)) {
      console.warn(`    Skipping ${manifest.slug}: entry not found at ${entryPath}`)
      continue
    }

    await esbuild.build({
      ...common,
      entryPoints: [entryPath],
      outfile: join(outDir, `${manifest.slug}.js`),
      external: [
        '@omnidea/*', '@castle/*', '@vizier',
        'solid-js', 'solid-js/*', '@solidjs/*',
      ],
      plugins: [solidPlugin({ solid: { generate: 'dom' } })],
    })
    console.log(`    ${manifest.slug} -> dist/programs/${manifest.slug}.js`)
  }
}

// ── 7. Compile Throne shell ──────────────────────────────────────────

async function buildThrone() {
  console.log('\n  Building Throne shell...')

  await esbuild.build({
    ...common,
    entryPoints: [resolve(ROOT, 'src/main.tsx')],
    outfile: join(DIST, 'throne.js'),
    external: [
      'solid-js', 'solid-js/*', '@solidjs/*',
      '@omnidea/*', '@castle/*', '@vizier', '@tauri-apps/*',
    ],
    plugins: [solidPlugin({ solid: { generate: 'dom' } })],
    // Throne only bundles its OWN code (Shell, chrome, bridge, castle-init).
    // All @omnidea/*, @castle/*, solid-js are external — resolved by the
    // browser import map to pre-compiled library bundles in dist/lib/.
  })
  console.log('    throne -> dist/throne.js')
}

// ── 8. Import map generation ─────────────────────────────────────────

function buildImportMap(programs: ReturnType<typeof discoverPrograms>): string {
  const importMap = generateBrowserImportMap(programs)
  return JSON.stringify(importMap, null, 2)
}

// ── 9. CSS assembly ──────────────────────────────────────────────────

function buildCSS() {
  console.log('\n  Building CSS...')

  // Run UnoCSS CLI (hardcoded args, no user input)
  const unoOut = join(DIST, 'uno.css')
  try {
    execFileSync('npx', [
      'unocss',
      'src/**/*.{tsx,ts}',
      '../../Apps/**/*.{tsx,ts}',
      '../../Library/ui/src/**/*.{tsx,ts}',
      '--out-file', unoOut,
    ], {
      cwd: ROOT,
      stdio: 'pipe',
    })
    console.log('    UnoCSS generated')
  } catch {
    console.warn('    UnoCSS generation failed, continuing without utilities')
    writeFileSync(unoOut, '/* UnoCSS not generated */\n')
  }

  // Concatenate CSS sources
  const cssSources = [
    { path: resolve(LIBRARY, 'ui/src/lib/theme.css'),      label: 'theme.css' },
    { path: resolve(LIBRARY, 'ui/src/lib/utilities.css'),   label: 'utilities.css' },
    { path: resolve(LIBRARY, 'editor/src/editor.css'),     label: 'editor.css' },
    { path: resolve(ROOT, 'src/app.css'),                   label: 'app.css' },
    { path: resolve(ROOT, 'src/chrome.css'),                label: 'chrome.css' },
    { path: unoOut,                                         label: 'uno.css' },
  ]

  let combined = ''
  for (const src of cssSources) {
    if (existsSync(src.path)) {
      combined += `/* === ${src.label} === */\n`
      combined += readFileSync(src.path, 'utf-8')
      combined += '\n\n'
    } else {
      console.warn(`    CSS not found: ${src.label}`)
    }
  }

  writeFileSync(join(DIST, 'throne.css'), combined)
  console.log('    throne.css assembled')
}

// ── 10. Copy Remix Icon fonts ────────────────────────────────────────

function copyRemixIcon() {
  console.log('\n  Copying Remix Icon fonts...')
  const remixSrc = resolve(LIBRARY, 'node_modules/remixicon/fonts')
  const remixDest = join(DIST, 'remixicon')

  if (!existsSync(remixSrc)) {
    console.warn('    remixicon fonts not found, skipping')
    return
  }

  ensureDir(remixDest)
  for (const file of readdirSync(remixSrc)) {
    copyFileSync(join(remixSrc, file), join(remixDest, file))
  }
  console.log('    Remix Icon fonts copied')
}

// ── 11. Assemble index.html ──────────────────────────────────────────

function assembleHTML(importMapJSON: string) {
  console.log('\n  Assembling index.html...')

  // Format the import map content exactly as it appears between <script> tags.
  // CSP hash is computed on this exact string (content of the script element).
  const importMapContent = '\n' + importMapJSON.split('\n').map(line => '      ' + line).join('\n') + '\n    '

  // SHA-256 hash of the import map — allows this specific inline script
  // while blocking all other inline scripts (XSS protection).
  const hash = createHash('sha256').update(importMapContent, 'utf-8').digest('base64')
  const csp = [
    "default-src 'self'",
    `script-src 'self' 'sha256-${hash}'`,
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "font-src 'self' data:",
    "connect-src 'self' ipc: http://ipc.localhost",
  ].join('; ')

  // Write CSP into tauri.conf.json so Tauri injects the correct policy.
  // Tauri injects its own CSP header — if we only use a <meta> tag, Tauri's
  // default CSP (script-src 'self') would override it (most restrictive wins).
  // By writing the hash into the config, Tauri's injected CSP includes it.
  // Guard: only write if CSP changed, to avoid triggering Tauri's file watcher
  // and causing a rebuild loop in dev mode.
  const tauriConfPath = join(ROOT, 'src-tauri', 'tauri.conf.json')
  const tauriConf = JSON.parse(readFileSync(tauriConfPath, 'utf-8'))
  const currentCsp = tauriConf.app?.security?.csp
  if (currentCsp !== csp) {
    tauriConf.app.security = { csp }
    writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n')
    console.log(`    CSP updated in tauri.conf.json (sha256-${hash})`)
  } else {
    console.log(`    CSP unchanged (sha256-${hash})`)
  }

  // No CSP meta tag needed — Tauri handles it via the config.
  const html = `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Omny</title>
    <script type="importmap">${importMapContent}</script>
    <link rel="stylesheet" href="/throne.css">
    <link rel="stylesheet" href="/remixicon/remixicon.css">
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/throne.js"></script>
  </body>
</html>
`

  writeFileSync(join(DIST, 'index.html'), html)
  console.log('    index.html written')
}

// ── Full build ───────────────────────────────────────────────────────

async function build() {
  const start = performance.now()
  console.log(`\n  Castle Build${isProd ? ' (production)' : ''}${isWatch ? ' (watch)' : ''}\n`)

  // Clean dist/
  cleanDist()

  // 1. Discover programs
  const { programs, manifests } = discover()

  // 2-5. Compile in dependency order
  await buildEditorEcosystem()
  await buildVendorDeps()
  await buildLibraryPackages()
  await buildCornerstone()
  await buildVizier()

  // 6. Generate registry
  generateRegistry(manifests)

  // 7. Compile programs
  await buildPrograms(programs)

  // 8. Compile Throne
  await buildThrone()

  // 9. Generate import map
  const importMapJSON = buildImportMap(programs)

  // 10. Build CSS
  buildCSS()

  // 11. Copy fonts
  copyRemixIcon()

  // 12. Assemble HTML
  assembleHTML(importMapJSON)

  const elapsed = (performance.now() - start).toFixed(0)
  console.log(`\n  Build complete in ${elapsed}ms\n`)
}

// ── Watch mode ───────────────────────────────────────────────────────

async function watch() {
  // Initial build
  await build()

  console.log('  Entering watch mode...\n')

  // Set up esbuild contexts for incremental rebuilds
  const { programs } = discover()

  // Watch Throne
  const throneCtx = await esbuild.context({
    ...common,
    entryPoints: [resolve(ROOT, 'src/main.tsx')],
    outfile: join(DIST, 'throne.js'),
    external: ['solid-js', 'solid-js/*', '@solidjs/*', '@castle/*', '@vizier'],
    plugins: [wgslRawPlugin, solidPlugin({ solid: { generate: 'dom' } })],
    alias: {
      '@omnidea/ui':      resolve(LIBRARY, 'ui/src/lib'),
      '@omnidea/crystal':  resolve(LIBRARY, 'crystal/src/index.ts'),
      '@omnidea/editor':   resolve(LIBRARY, 'editor/src/index.ts'),
      '@omnidea/fx':       resolve(LIBRARY, 'fx/src/lib'),
      '@omnidea/net':      resolve(LIBRARY, 'sdk/src/index.ts'),
    },
  })
  await throneCtx.watch()
  console.log('  Watching Throne...')

  // Watch Castle cornerstone (must be a separate bundle so Throne and programs
  // share the same CastleContext object via the import map)
  const cornerstoneCtx = await esbuild.context({
    ...common,
    entryPoints: [resolve(OMNY, 'cornerstone/index.ts')],
    outfile: join(DIST, 'lib', '@castle', 'cornerstone.js'),
    external: ['solid-js', 'solid-js/*', '@vizier'],
    plugins: [solidPlugin({ solid: { generate: 'dom' } })],
  })
  await cornerstoneCtx.watch()
  console.log('  Watching @castle/cornerstone...')

  // Watch Vizier
  const vizierCtx = await esbuild.context({
    ...common,
    entryPoints: [resolve(OMNY, 'vizier/index.ts')],
    outfile: join(DIST, 'lib', '@vizier', 'vizier.js'),
    external: [
      'solid-js', 'solid-js/*',
      '@castle/cornerstone',
      'yjs', 'y-prosemirror', 'y-protocols/*', 'lib0/*',
    ],
    plugins: [solidPlugin({ solid: { generate: 'dom' } })],
  })
  await vizierCtx.watch()
  console.log('  Watching @vizier...')

  // Watch each program
  for (const program of programs) {
    if (!existsSync(program.entryPath)) continue

    const ctx = await esbuild.context({
      ...common,
      entryPoints: [program.entryPath],
      outfile: join(DIST, 'programs', `${program.manifest.slug}.js`),
      external: [
        '@omnidea/*', '@castle/*', '@vizier',
        'solid-js', 'solid-js/*', '@solidjs/*',
      ],
      plugins: [solidPlugin({ solid: { generate: 'dom' } })],
    })
    await ctx.watch()
    console.log(`  Watching ${program.manifest.slug}...`)
  }

  // Watch Library packages
  for (const [name, entry] of Object.entries(LIBRARY_PACKAGES)) {
    if (!existsSync(entry)) continue

    const ctx = await esbuild.context({
      ...common,
      entryPoints: [entry],
      outfile: join(DIST, 'lib', '@omnidea', `${name}.js`),
      external: ['solid-js', 'solid-js/*', '@tiptap/*', 'solid-tiptap', 'yjs', 'y-prosemirror', 'y-protocols/*', 'lib0/*', '@castle/cornerstone', '@vizier'],
      plugins: [wgslRawPlugin, solidPlugin({ solid: { generate: 'dom' } })],
    })
    await ctx.watch()
    console.log(`  Watching @omnidea/${name}...`)
  }

  // UnoCSS watch (separate process, hardcoded args)
  const unoProc = spawn('npx', [
    'unocss',
    'src/**/*.{tsx,ts}',
    '../../Apps/**/*.{tsx,ts}',
    '../../Library/ui/src/**/*.{tsx,ts}',
    '--watch', '--out-file', join(DIST, 'uno.css'),
  ], {
    cwd: ROOT,
    stdio: 'inherit',
  })
  unoProc.on('error', (err) => console.warn('  UnoCSS watch failed:', err.message))
  console.log('  Watching UnoCSS...')

  console.log('\n  Ready. Waiting for changes...\n')

  // Keep alive
  await new Promise(() => {})
}

// ── Entry point ──────────────────────────────────────────────────────

if (isWatch) {
  watch().catch(err => { console.error(err); process.exit(1) })
} else {
  build().catch(err => { console.error(err); process.exit(1) })
}
