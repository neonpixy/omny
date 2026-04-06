# Omny

> **Public mirror.** Active development happens elsewhere.

The browser for Omnidea. Omny IS the Castle.

## Structure

```
Omny/                    The Castle — Cargo workspace root
├── court/               The vocabulary — operations, intents, pipelines
├── cornerstone/         The foundation — useCastle() hook for programs
├── crownsguard/         The enforcer — validates program contracts
├── sceptor/             The switchboard — discovery, scoping, injection, mounting
├── attendants/          Domain hooks — useIdeas(), useConfig(), useDaemonHealth()
├── courtiers/           22 court positions — one Rust module per ABC, all staffed
├── prerogative/         Shared authority — DaemonModule, DaemonState, API JSON
├── chancellor/          The state authority — infrastructure modules, IPC server
├── client/              IPC protocol types
├── herald/              Menu bar announcer
├── throne/              The shell — Tauri 2.x desktop app, renders everything
└── vizier/              The Right Hand — collaboration engine
```

## How It Works

Programs live in [Apps/](https://github.com/neonpixy/apps). Each program has a `program.json` manifest declaring what Court operations it uses, what Library packages it needs, and what intents it sends/receives.

Castle reads those manifests, validates them, builds scoped bridges, and mounts programs into Throne. Programs never touch the system directly — they speak Court, and Castle handles the rest.

## Dependencies

- [Omninet](https://github.com/neonpixy/omninet) — the protocol (29 Rust crates)
- [Library](https://github.com/neonpixy/library) — shared packages (@omnidea/ui, /net, /crystal, /editor, /fx)
- [Apps](https://github.com/neonpixy/apps) — programs that run inside Omny

## Build

```bash
# Full Rust workspace (chancellor + courtiers + prerogative + client + herald)
cargo build --workspace

# Throne (desktop shell)
cd throne && npm install && cargo tauri dev
```

## License

AGPL-3.0 + Covenant
