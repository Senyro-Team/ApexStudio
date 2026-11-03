param([string[]]$TestArgs = @())
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
  # Tauri's app resource does not reach Rust's library test executable. Activate
  # common-controls v6 on the test artifact only; never change the app manifest.
  $buildOutput = & cargo test --locked -p apex-desktop --lib --no-run --message-format=json
  if ($LASTEXITCODE -ne 0) { throw 'Compiling desktop tests failed.' }
  $artifact = $buildOutput | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'apex_desktop_lib' -and $_.profile.test -and $_.executable } |
    Select-Object -Last 1
  if (-not $artifact) { throw 'Desktop test executable was not reported by Cargo.' }
  $manifestTool = Get-Command mt.exe -ErrorAction SilentlyContinue
  $manifestToolPath = if ($manifestTool) { $manifestTool.Source } else {
    Get-ChildItem -LiteralPath "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory |
      Sort-Object Name -Descending | ForEach-Object { Join-Path $_.FullName 'x64\mt.exe' } |
      Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
  }
  if (-not $manifestToolPath) { throw 'Windows SDK Manifest Tool (mt.exe) is required to run desktop tests.' }
  $manifestPath = Join-Path (Split-Path -Parent $artifact.executable) 'apex-library-tests.manifest'
  '<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"><dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" /></dependentAssembly></dependency></assembly>' |
    Set-Content -LiteralPath $manifestPath -Encoding utf8
  & $manifestToolPath -nologo -manifest $manifestPath "-outputresource:$($artifact.executable);1"
  if ($LASTEXITCODE -ne 0) { throw 'Activating common controls for desktop tests failed.' }
  & $artifact.executable @TestArgs
  if ($LASTEXITCODE -ne 0) { throw 'Desktop tests failed.' }
}
finally { Pop-Location }
