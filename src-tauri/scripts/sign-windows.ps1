param(
    [Parameter(Mandatory = $true)]
    [string] $FilePath
)

$ErrorActionPreference = "Stop"

$requiredEnvironment = @(
    "AZURE_CLIENT_ID",
    "AZURE_CLIENT_SECRET",
    "AZURE_TENANT_ID",
    "AZURE_ARTIFACT_SIGNING_ENDPOINT",
    "AZURE_ARTIFACT_SIGNING_ACCOUNT",
    "AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE"
)

foreach ($name in $requiredEnvironment) {
    if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
        throw "Missing required environment variable: $name"
    }
}

if (-not (Test-Path -LiteralPath $FilePath)) {
    throw "File to sign does not exist: $FilePath"
}

$resolvedFile = (Resolve-Path -LiteralPath $FilePath).Path
Write-Host "Signing Windows artifact: $resolvedFile"

$artifactSigningCli = Get-Command artifact-signing-cli -ErrorAction SilentlyContinue
if (-not $artifactSigningCli) {
    throw "artifact-signing-cli was not found on PATH. Install it with: cargo install artifact-signing-cli"
}

Write-Host "Using artifact-signing-cli: $($artifactSigningCli.Source)"

& $artifactSigningCli.Source `
    -e $env:AZURE_ARTIFACT_SIGNING_ENDPOINT `
    -a $env:AZURE_ARTIFACT_SIGNING_ACCOUNT `
    -c $env:AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE `
    -d "TradeProject" `
    $resolvedFile

if ($LASTEXITCODE -ne 0) {
    throw "Azure Artifact Signing failed for $resolvedFile with exit code $LASTEXITCODE"
}
