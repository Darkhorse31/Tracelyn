import { spawnSync } from "node:child_process"

// `--release` is the signed updater build. Local `tauri build` leaves artifacts off.
const release = process.argv.includes("--release")
const passthrough = process.argv.slice(2).filter((arg) => arg !== "--release")
const hasKey = Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY)

if (release && !hasKey) {
  console.error(
    "Set TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD before npm run tauri:release. Generate a key with npx tauri signer generate."
  )
  process.exit(1)
}

const args = ["node_modules/@tauri-apps/cli/tauri.js", "build", ...passthrough]
if (release || hasKey) {
  args.push("--config", "src-tauri/tauri.release.conf.json")
}

const result = spawnSync(process.execPath, args, { stdio: "inherit" })
process.exit(result.status ?? 1)
