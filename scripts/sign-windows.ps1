param(
    [Parameter(Mandatory = $true)]
    [string] $FilePath
)

$ErrorActionPreference = "Stop"

$signer = Join-Path $PSScriptRoot "..\src-tauri\scripts\sign-windows.ps1"

if (-not (Test-Path -LiteralPath $signer -PathType Leaf)) {
    throw "Windows signing helper was not found: $signer"
}

& $signer $FilePath

if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
