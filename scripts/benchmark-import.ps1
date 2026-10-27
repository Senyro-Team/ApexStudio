param(
 [Parameter(Mandatory=$true)][string]$Source,
 [ValidateSet('debug','release')][string]$Profile='release',
 [int]$Runs=3,
 [string]$Output='.tools/import-performance.json'
)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$binary=Join-Path $repo "target/$Profile/examples/import_bench.exe"
if(!(Test-Path -LiteralPath $binary)){throw 'Build the import_bench example first.'}
$inputRoot=(Resolve-Path -LiteralPath $Source).Path
$testRoot=Join-Path ([System.IO.Path]::GetTempPath()) ('apex-import-review-'+[guid]::NewGuid())
New-Item -ItemType Directory -Path $testRoot | Out-Null
$inputCopy=Join-Path $testRoot 'input'
Copy-Item -LiteralPath $inputRoot -Destination $inputCopy -Recurse
$results=@()
for($run=1;$run -le $Runs;$run++){
 $stdout=Join-Path $testRoot "run-$run.json"
 $stderr=Join-Path $testRoot "run-$run.log"
 $process=Start-Process -FilePath $binary -ArgumentList ('"'+$inputCopy+'"') -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -WindowStyle Hidden
 $peakWorking=0L;$peakPrivate=0L
 while(!$process.HasExited){
  $process.Refresh()
  $peakWorking=[Math]::Max($peakWorking,$process.PeakWorkingSet64)
  $peakPrivate=[Math]::Max($peakPrivate,$process.PrivateMemorySize64)
  Start-Sleep -Milliseconds 100
 }
 $process.WaitForExit()
 if($process.ExitCode -ne 0){throw (Get-Content -LiteralPath $stderr -Raw)}
 $row=Get-Content -LiteralPath $stdout -Raw | ConvertFrom-Json
 $row | Add-Member -NotePropertyName run -NotePropertyValue $run
 $row | Add-Member -NotePropertyName peakWorkingSetBytes -NotePropertyValue $peakWorking
 $row | Add-Member -NotePropertyName peakPrivateBytes -NotePropertyValue $peakPrivate
 $results+=$row
 $results | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Output -Encoding utf8
 $row | ConvertTo-Json -Compress
}
Write-Output "Isolated input copy retained at $inputCopy"
