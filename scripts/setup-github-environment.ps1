param(
    [string]$Repo = "Ty-Roblox/TradeTool",
    [string]$TauriPrivateKeyPath = "$env:USERPROFILE\.tauri\tradeproject.key"
)

$ErrorActionPreference = "Stop"

function Require-GhAuth {
    gh auth status | Out-Null
}

function Require-EnvValue {
    param([string]$Name)

    $value = [Environment]::GetEnvironmentVariable($Name, "Process")
    if (-not $value) {
        $value = [Environment]::GetEnvironmentVariable($Name, "User")
    }
    if (-not $value) {
        $value = [Environment]::GetEnvironmentVariable($Name, "Machine")
    }
    if (-not $value) {
        throw "Missing environment variable: $Name"
    }

    return $value
}

function Set-GitHubSecret {
    param(
        [string]$Name,
        [string]$Value
    )

    $Value | gh secret set $Name --repo $Repo
}

function Set-GitHubVariable {
    param(
        [string]$Name,
        [string]$Value
    )

    gh variable set $Name --repo $Repo --body $Value
}

Require-GhAuth

if (-not (Test-Path -LiteralPath $TauriPrivateKeyPath)) {
    throw "Tauri signing private key was not found: $TauriPrivateKeyPath"
}

Get-Content -Raw -LiteralPath $TauriPrivateKeyPath | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo $Repo

$tauriPrivateKeyPassword = [Environment]::GetEnvironmentVariable("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", "Process")
if (-not $tauriPrivateKeyPassword) {
    $tauriPrivateKeyPassword = [Environment]::GetEnvironmentVariable("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", "User")
}
if (-not $tauriPrivateKeyPassword) {
    $tauriPrivateKeyPassword = [Environment]::GetEnvironmentVariable("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", "Machine")
}
if ($tauriPrivateKeyPassword) {
    Set-GitHubSecret -Name "TAURI_SIGNING_PRIVATE_KEY_PASSWORD" -Value $tauriPrivateKeyPassword
}

Set-GitHubSecret -Name "AZURE_CLIENT_ID" -Value (Require-EnvValue "AZURE_CLIENT_ID")
Set-GitHubSecret -Name "AZURE_CLIENT_SECRET" -Value (Require-EnvValue "AZURE_CLIENT_SECRET")
Set-GitHubSecret -Name "AZURE_TENANT_ID" -Value (Require-EnvValue "AZURE_TENANT_ID")

Set-GitHubVariable -Name "AZURE_ARTIFACT_SIGNING_ENDPOINT" -Value (Require-EnvValue "AZURE_ARTIFACT_SIGNING_ENDPOINT")
Set-GitHubVariable -Name "AZURE_ARTIFACT_SIGNING_ACCOUNT" -Value (Require-EnvValue "AZURE_ARTIFACT_SIGNING_ACCOUNT")
Set-GitHubVariable -Name "AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE" -Value (Require-EnvValue "AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE")

Write-Host "GitHub Actions secrets and variables are configured for $Repo."
