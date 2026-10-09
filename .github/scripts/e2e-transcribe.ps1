# End-to-end check of the local engine on real Windows: decode a 48 kHz
# Czech recording, resample, run whisper.cpp, and look for the spoken words.
param([string]$Exe = "target\release\hlas.exe")
$ErrorActionPreference = "Stop"

$out = Join-Path $env:RUNNER_TEMP "transcript.txt"
$err = Join-Path $env:RUNNER_TEMP "transcript.err"
$p = Start-Process -FilePath $Exe -Wait -PassThru -NoNewWindow `
    -ArgumentList @("--transcribe", "tests\fixtures\czech-48k.wav", "--out", $out, "--engine", "local", "--language", "cs") `
    -RedirectStandardError $err
Get-Content $err -ErrorAction SilentlyContinue | Write-Host
if ($p.ExitCode -ne 0) { throw "hlas.exe --transcribe exited with $($p.ExitCode)" }

$text = Get-Content $out -Raw -Encoding UTF8
Write-Host "Transcript: $text"
$lower = $text.ToLowerInvariant()
foreach ($word in @("zkouška", "nabídku", "petrovi")) {
    if (-not $lower.Contains($word)) { throw "Transcript is missing '$word'" }
}
# The tail of the recording must survive resampling (the 0.1.0 bug lost it).
if (-not $lower.Contains("zítra")) { throw "The second half of the recording is missing" }
Write-Host "OK: local transcription on Windows"
