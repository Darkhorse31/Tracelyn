# Tracelyn

Windows desktop app for monitoring Linux VPS servers over SSH — system health, web traffic, API endpoint performance, Redis, and more. No agent required on the servers.

## Stack

- **Tauri 2** + **React** + **TypeScript** (Vite)
- **Rust** core (SSH via russh, local config and secrets)

## Development

```bash
npm install
npm run tauri dev
```

Build the frontend:

```bash
npm run build
```

Check the Rust crate:

```bash
cd src-tauri && cargo check
```

## Docs

- Design spec: `docs/specs/2026-10-23-serverlens-design.md`
- Phase 1 plan: `docs/plans/2026-10-23-phase1-plan.md`
- Progress: `docs/progress.md`

## Licensing and releases

See [release setup](docs/RELEASE.md) and [signed updater setup](docs/UPDATER.md). Use `npm run tauri build` for a local installer with no signing secrets. Use `npm run tauri:release` when the updater private key is set.
