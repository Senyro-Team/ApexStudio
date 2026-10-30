$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
  cargo build --release --locked -p apex-desktop --bin apex-chrome-bridge --features custom-protocol
  if ($LASTEXITCODE -ne 0) { throw 'Building the Chrome bridge failed.' }

  npm run tauri --workspace @apex/desktop -- build --bundles nsis --config src-tauri/tauri.bundle.conf.json --no-sign --ci
  if ($LASTEXITCODE -ne 0) { throw 'Building the Windows installer failed.' }
}
finally {
  Pop-Location
}
