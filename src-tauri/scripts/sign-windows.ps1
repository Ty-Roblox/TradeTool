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

$trustedSigningCli = Get-Command trusted-signing-cli -ErrorAction SilentlyContinue
if (-not $trustedSigningCli) {
    throw "trusted-signing-cli was not found on PATH. Install it with: cargo install artifact-signing-cli"
}

& $trustedSigningCli.Source `
    -e $env:AZURE_ARTIFACT_SIGNING_ENDPOINT `
    -a $env:AZURE_ARTIFACT_SIGNING_ACCOUNT `
    -c $env:AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE `
    -d "TradeProject" `
    $FilePath

if ($LASTEXITCODE -ne 0) {
    throw "Azure Artifact Signing failed for $FilePath with exit code $LASTEXITCODE"
}
