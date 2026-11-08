$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
  $compilerCandidates = @(
    (Get-Command ISCC.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
    (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
    (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
  ) | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) }
  if (-not $compilerCandidates) {
    throw 'Inno Setup 6 is required. Install it from https://jrsoftware.org/isinfo.php and rerun this script.'
  }
  $compiler = $compilerCandidates | Select-Object -First 1

  npm run build:extension
  if ($LASTEXITCODE -ne 0) { throw 'Building the ERLCX extension failed.' }

  $manifest = Get-Content -Raw (Join-Path $root 'apps/erlcx-extension/manifest.json') | ConvertFrom-Json
  $versionedExtension = Join-Path $root "dist/apex-erlcx-extension-$($manifest.version).zip"
  $bundledExtension = Join-Path $root 'dist/apex-erlcx-extension.zip'
  if (-not (Test-Path -LiteralPath $versionedExtension -PathType Leaf)) {
    throw "The versioned ERLCX extension package was not created: $versionedExtension"
  }
  Copy-Item -LiteralPath $versionedExtension -Destination $bundledExtension -Force

  npm run build
  if ($LASTEXITCODE -ne 0) { throw 'Building the Apex Studio frontend failed.' }

  cargo build --release --locked -p apex-desktop --bin apex-chrome-bridge --features custom-protocol
  if ($LASTEXITCODE -ne 0) { throw 'Building the Chrome bridge failed.' }

  cargo build --release --locked -p apex-cli
  if ($LASTEXITCODE -ne 0) { throw 'Building the Apex Studio CLI failed.' }

  npm run tauri --workspace @apex/desktop -- build --no-bundle --config src-tauri/tauri.bundle.conf.json --ci
  if ($LASTEXITCODE -ne 0) { throw 'Building Apex Studio failed.' }

  $version = (Get-Content -Raw (Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json') | ConvertFrom-Json).version
  $stage = Join-Path $root 'target/release/inno-stage'
  New-Item -ItemType Directory -Path $stage -Force | Out-Null
  foreach ($file in @('apex-desktop.exe', 'apex-chrome-bridge.exe', 'apex-studio-cli.exe')) {
    $source = Join-Path $root "target/release/$file"
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Required build output is missing: $source" }
    Copy-Item -LiteralPath $source -Destination $stage -Force
  }
  if (-not (Test-Path -LiteralPath $bundledExtension -PathType Leaf)) { throw "ERLCX extension package is missing: $bundledExtension" }
  Copy-Item -LiteralPath $bundledExtension -Destination $stage -Force

  $installerScript = Join-Path $root 'apps/desktop/src-tauri/windows/installer.iss'
  & $compiler "/DAppVersion=$version" "/DStageDir=$stage" $installerScript
  if ($LASTEXITCODE -ne 0) { throw 'Compiling the Inno Setup installer failed.' }

  Write-Host "Created Apex Studio Inno installer in target/release/bundle/inno (version $version)."
}
finally {
  Pop-Location
}
