param([string[]]$Targets = @('x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl'))
$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot -Parent
Push-Location $workspace
try {
  if (-not (Get-Command zig -ErrorAction SilentlyContinue)) { throw 'Install Zig and add it to PATH, then run cargo install cargo-zigbuild --locked' }
  foreach ($target in $Targets) {
    rustup target add $target
    if ($LASTEXITCODE -ne 0) { throw "Could not install Rust target $target" }
    cargo zigbuild --release --package tracelyn-companion --target $target
    if ($LASTEXITCODE -ne 0) { throw "Companion build failed for $target" }
    $destination = Join-Path $workspace "src-tauri/resources/companion/tracelyn-companion-$target"
    Copy-Item -LiteralPath (Join-Path $workspace "target/$target/release/tracelyn-companion") -Destination $destination -Force
    $checksum = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$destination.sha256", "$checksum  tracelyn-companion-$target`n", [Text.UTF8Encoding]::new($false))
  }
} finally { Pop-Location }
