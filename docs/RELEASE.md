# Release Tracelyn

## Configure licensing

Set `store_id`, `product_id`, and `variant_id` to the Lemon Squeezy IDs for your paid license variant in `src-tauri/license-config.json`. Set `buy_url` and `manage_url` to your checkout and customer portal links. Zero IDs deliberately prevent activation. Enable license keys on the variant and choose your activation limit and subscription expiry rules in Lemon Squeezy. The desktop client uses only the public License API. It contains no API secret.

The license key is the account. The optional email is stored locally and is not used for authentication. Trial time starts on first launch, lasts 14 days, and is stored in an HMAC-signed app-data file with an authoritative signed copy and secret in Windows Credential Manager. File deletion cannot restart the trial. Corruption or credential access failure leaves monitoring available in read-only mode. Clock rollback beyond five minutes is rejected. An administrator who deletes both the credential records and all app data can bypass a local-only trial. Stronger enforcement would require a server-backed trial identity.

Successful validation allows seven days of offline use. Startup validates immediately, and running sessions validate at least every 24 hours with retries after failures. Expired or disabled responses invalidate the key immediately. Device deactivation requires connectivity and does not restart the trial. Monitoring history stays visible after expiry. Server actions, terminal writes, and alert delivery are enforced in Rust.

## Azure Trusted Signing (now Azure Artifact Signing)

1. Create an Azure Trusted Signing account in a supported region, complete identity validation, and create a public trust certificate profile.
2. Grant the build identity the Trusted Signing Certificate Profile Signer role for that profile. Prefer a CI workload identity and avoid long-lived client secrets.
3. Install Windows SDK SignTool version 10.0.2261.755 or newer, .NET 8, the Visual C++ Redistributable, and the Microsoft signing client tools, including `Azure.CodeSigning.Dlib.dll` and its runtime dependencies matching the SignTool architecture.
4. Create a metadata JSON file outside this repository:

```json
{
  "Endpoint": "https://eus.codesigning.azure.net/",
  "CodeSigningAccountName": "YOUR_ACCOUNT",
  "CertificateProfileName": "YOUR_PROFILE"
}
```

Use your account region's endpoint. Authenticate with an identity supported by the client library's DefaultAzureCredential. For example, use Azure CLI login locally or the configured workload identity in CI. Set these environment variables in the release job:

```powershell
$env:TRACELYN_SIGNTOOL = 'C:\Program Files (x86)\Windows Kits\10\bin\YOUR_SDK\x64\signtool.exe'
$env:TRACELYN_AZURE_DLIB = 'C:\signing\Azure.CodeSigning.Dlib.dll'
$env:TRACELYN_AZURE_METADATA = 'C:\signing\metadata.json'
```

The hook `scripts/sign-windows.ps1` uses SHA-256, an RFC 3161 timestamp, and verifies each signature. Signing errors fail the build. Partial Azure configuration also fails. With no signing environment variables it exits successfully without signing. `bundle.windows.certificateThumbprint` remains null by default.

## OV certificate alternative

Install a code-signing certificate and its hardware-backed provider in the Windows certificate store accessible to the build identity. Set `TRACELYN_SIGN_THUMBPRINT` to the certificate thumbprint. Set `TRACELYN_CERT_STORE_MACHINE=1` when using the machine store. Set `TRACELYN_SIGNTOOL` if SignTool is not on PATH. Optionally set `TRACELYN_TIMESTAMP_URL` to your certificate authority's RFC 3161 timestamp server. Do not set the Azure variables for this path. Provision hardware tokens or remote certificate providers according to your CA's instructions.

## Release steps

1. Replace licensing IDs, purchase links, updater owner, and updater public key placeholders. Exercise activation, device limits, renewal, revocation, offline grace, and deactivation with a real test license.
2. Bump `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json` to the same version.
3. Run `npm test`, `npm run build`, `cargo check --manifest-path src-tauri/Cargo.toml`, and `cargo test --manifest-path src-tauri/Cargo.toml`.
4. Generate an updater key with `npx tauri signer generate` if you do not already have one. Paste the public key into `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. Set `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Configure Azure or OV signing environment variables. The updater private key and Authenticode credentials serve different purposes. Details are in [UPDATER.md](UPDATER.md).
5. Run `npm run tauri:release`. That command merges `src-tauri/tauri.release.conf.json`, which turns updater artifacts on. The Windows hook signs executables and installers during bundling. The updater signing step produces matching `.sig` artifacts. Never modify these artifacts afterward.
6. Verify the installer with `signtool verify /pa /v <installer>`. Install it in a Windows test environment and confirm license and updater behavior.
7. Create `latest.json` following [UPDATER.md](UPDATER.md), referencing the final artifact URL and exact `.sig` contents. Create a GitHub Release for the version and upload the installer, signature, and manifest. Publish once the whole set is available.
8. Test the published upgrade from the previous installed version and confirm it preserves data and license state. Retain the previous release artifacts and key backups.

For a local unsigned installer build with no credentials, run `npm run tauri build`. Updater artifact generation stays off unless you run `npm run tauri:release`. The Windows signing hook also succeeds without credentials.

References: [Tauri Windows signing](https://v2.tauri.app/distribute/sign/windows/), [Azure Trusted Signing](https://learn.microsoft.com/azure/trusted-signing/), and [Lemon Squeezy License API](https://docs.lemonsqueezy.com/api/license-api).
