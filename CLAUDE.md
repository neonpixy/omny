# Omny — The Browser for Omnidea

## Architecture

Omny IS the Castle — a contract-driven system where programs are declarations, not wired-up apps. Inspired by Wyndsor (SASS composition engine).

```
Programs speak Court. Sceptor translates. Crownsguard validates.
Staff serves. Chancellor delivers. Library provides materials. Throne displays.
```

### Components

| Component | What | Where |
|-----------|------|-------|
| **Throne** | Desktop shell — Tauri 2.x, renders programs, owns the window | `throne/` |
| **Court** | The vocabulary — every SDK op, intent, and pipeline | `court/` |
| **Sceptor** | The switchboard — discovery, scoping, injection, mounting | `sceptor/` |
| **Crownsguard** | The enforcer — build-time CLI + runtime scope guard | `crownsguard/` |
| **Cornerstone** | The foundation — `useCastle()` hook, the ONE import | `cornerstone/` |
| **Attendants** | Domain hooks: `useIdeas()`, `useConfig()`, `useDaemonHealth()` | `attendants/` |
| **Courtiers** | Court positions — 22 staffed, one per ABC, the Castle's workforce | `courtiers/` |
| **Chancellor** | State authority — Equipment backbone, infrastructure modules | `chancellor/` |
| **Prerogative** | The Chancellor's authority — DaemonModule, DaemonState, API JSON | `prerogative/` |
| **Client** | IPC protocol types | `client/` |
| **Herald** | Menu bar announcer | `herald/` |
| **Vizier** | Right Hand — collaboration engine (Y.Doc, providers, sessions) | `vizier/` |

### Dependency Flow

```
Apps → Castle (Court + Sceptor + Crownsguard) → Chancellor
                      ↑                             ↑
                   Library                    Courtiers + Prerogative
```

One direction. No exceptions. Programs never import from Throne or Chancellor directly.

### Rust Workspace

Omny is the Cargo workspace root. All Rust crates are workspace members.

```toml
# Cargo.toml
[workspace]
members = ["chancellor", "client", "courtiers", "herald", "prerogative"]
exclude = ["throne/src-tauri"]
```

## Programs

Programs live in the [Apps](https://github.com/neonpixy/apps) repo at `Omnidea/Apps/`. Each program has a `program.json` manifest:

```json
{
  "name": "Hearth",
  "slug": "hearth",
  "entry": "./Hearth.tsx",
  "court": ["crown", "vault", "equipment"],
  "packages": ["ui", "fx"],
  "intents": {
    "sends": ["share-content"],
    "receives": ["share-content"]
  }
}
```

### The One Import Rule

Programs can import from:
- `@omnidea/*` — Library packages (resolved by Castle import maps)
- `@castle/cornerstone` — the `useCastle()` hook
- `./` — internal files

Nothing else.

## Binary Names

| Binary | Purpose |
|--------|---------|
| `chancellor` | State authority daemon |
| `herald` | Menu bar announcer |
| `chancellor-client` | IPC client crate |

## Build & Dev

```bash
# Build Chancellor (from Omny root)
cargo build --workspace

# Dev mode (launches Throne + Chancellor)
cd throne && cargo tauri dev

# Validate a program
npx crownsguard check ../../Apps/hearth/
```

## Key Files

| File | What |
|------|------|
| `court/operations.ts` | All 860+ SDK operations by namespace |
| `court/intents.ts` | Inter-program intent definitions |
| `court/pipelines.ts` | Named multi-step workflows |
| `cornerstone/useCastle.ts` | The one hook programs use |
| `crownsguard/validator.ts` | Manifest validation |
| `attendants/` | Domain hooks (useIdeas, useConfig, useDaemonHealth) |
| `court/invalidation.ts` | Event→operation invalidation rules |
| `sceptor/discovery.ts` | Program scanner |
| `courtiers/courtiers.ts` | Courtier name ↔ daemon namespace mappings (22 positions) |
| `courtiers/src/lib.rs` | Courtier module registry (all_courtiers) |
| `prerogative/src/state.rs` | DaemonState — shared state struct |
| `prerogative/src/daemon_module.rs` | DaemonModule trait |
| `prerogative/src/api_json.rs` | API JSON serialization contract |
| `chancellor/src/server.rs` | IPC server + auth |
| `chancellor/src/modules/` | Infrastructure daemon modules |
| `chancellor/build.rs` | FFI code generator (parses C header, auto-registers ops) |
| `throne/build.ts` | Castle Build Switchboard (esbuild, import maps, CSS, dist) |
| `throne/src/chrome/` | Shell UI components |
| `throne/tsconfig.json` | Full type composition (throne + court + crystal + Library) |
| `tsconfig.castle.json` | Editor DX type context for Castle code |

## Build System Patterns

### Browser Exports Plugin
Vendor deps (solid-js, etc.) are pre-bundled by `throne/build.ts`. esbuild ignores the `exports` field in package.json when resolving entry points, falling back to the legacy `module` field — which for solid-js points to server builds. The `browserExportsPlugin` in `build.ts` reads the `exports` field and resolves using the `browser` condition, ensuring browser code is bundled for browser targets.

**Rule:** Never bypass the exports field with explicit dist paths. Always use the plugin to resolve the correct conditional export.

### Type Composition (Wyndsor Coherence)
Throne is the composition host — like Wyndsor's switchboard, it has full type awareness of everything it compiles. `throne/tsconfig.json` includes court, cornerstone, and crystal type declarations, and maps `solid-js` to its local node_modules. `tsconfig.castle.json` has its own type context for editor DX. Both must type-check to zero errors.

## Reference

- Full architecture: `Omnidea/Plans/Castle Architecture.md`
- Blocked issues: `Omnidea/DOCKET.md`
- Mined from: `Quarry/Omny/` (beryllium, omnidaemon, omnigrams)
