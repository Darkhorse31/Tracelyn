param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = 'Stop'
# Tauri invokes this for both executables and installers. No credentials means unsigned.
$azureConfigured = $env:TRACELYN_AZURE_METADATA -or $env:TRACELYN_AZURE_DLIB
if (-not $azureConfigured -and -not $env:TRACELYN_SIGN_THUMBPRINT) {
    Write-Output 'No Windows signing credentials configured. Leaving artifact unsigned.'
    exit 0
}
if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw 'The signing target does not exist.'
}
$signTool = if ($env:TRACELYN_SIGNTOOL) { $env:TRACELYN_SIGNTOOL } else { 'signtool.exe' }
$timestampUrl = if ($env:TRACELYN_TIMESTAMP_URL) { $env:TRACELYN_TIMESTAMP_URL } else { 'http://timestamp.acs.microsoft.com' }
$signArgs = @('sign', '/v', '/fd', 'SHA256', '/tr', $timestampUrl, '/td', 'SHA256')
if ($azureConfigured) {
    if (-not $env:TRACELYN_AZURE_METADATA -or -not $env:TRACELYN_AZURE_DLIB) {
        throw 'Azure signing requires TRACELYN_AZURE_METADATA and TRACELYN_AZURE_DLIB.'
    }
    $signArgs += @('/dlib', $env:TRACELYN_AZURE_DLIB, '/dmdf', $env:TRACELYN_AZURE_METADATA)
} else {
    $signArgs += @('/sha1', $env:TRACELYN_SIGN_THUMBPRINT)
    if ($env:TRACELYN_CERT_STORE_MACHINE -eq '1') { $signArgs += '/sm' }
}
$signArgs += $Path
& $signTool @signArgs
if ($LASTEXITCODE -ne 0) { throw "Windows signing failed with exit code $LASTEXITCODE." }
& $signTool verify /pa /v $Path
if ($LASTEXITCODE -ne 0) { throw "Windows signature verification failed with exit code $LASTEXITCODE." }
exit 0
