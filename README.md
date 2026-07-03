# TradeProject

SvelteKit, Vite, and Tauri v2 desktop app with updater-ready GitHub releases.

## Local Development

PowerShell blocks `npm.ps1` on this machine, so use npm through `cmd.exe`:

```powershell
cmd.exe /c npm ci
cmd.exe /c npm run dev
cmd.exe /c npm run check
cmd.exe /c npm run build
cmd.exe /c npm run tauri:dev
```

The local Tauri updater private key was generated at `%USERPROFILE%\.tauri\tradeproject.key`. Keep it out of git.

## Release Setup

GitHub Actions expects these repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY`: contents of `%USERPROFILE%\.tauri\tradeproject.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: optional, currently blank because the generated key has no password
- `AZURE_CLIENT_ID`
- `AZURE_CLIENT_SECRET`
- `AZURE_TENANT_ID`

GitHub Actions expects these repository variables:

- `AZURE_ARTIFACT_SIGNING_ENDPOINT`
- `AZURE_ARTIFACT_SIGNING_ACCOUNT`
- `AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE`

The updater endpoint is `https://github.com/Ty-Roblox/TradeProject/releases/latest/download/latest.json`. Update `src-tauri/tauri.conf.json` before the first release if the final GitHub repository changes.

Release builds are triggered manually or by pushing an `app-v*` tag. Releases are created as drafts.
