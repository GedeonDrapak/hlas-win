# Times the local engine with a few thread/decoder settings and prints
# whisper.cpp's own timing breakdown. Each run is capped so a slow build
# cannot eat the job.
param([string]$Exe = "target\release\hlas.exe", [int]$CapSeconds = 300)
$err = Join-Path $env:RUNNER_TEMP "bench.err"
$cores = [Environment]::ProcessorCount
foreach ($cfg in @(@{ t = 1; g = $true }, @{ t = $cores; g = $true }, @{ t = $cores; g = $false })) {
    $env:HLAS_THREADS = "$($cfg.t)"
    if ($cfg.g) { $env:HLAS_GREEDY = "1" } else { Remove-Item Env:HLAS_GREEDY -ErrorAction SilentlyContinue }
    Write-Host "=== threads=$($cfg.t) greedy=$($cfg.g)"
    $p = Start-Process -FilePath $Exe -PassThru -NoNewWindow -RedirectStandardError $err `
        -ArgumentList @("--transcribe", "tests\fixtures\czech-48k.wav", "--out", (Join-Path $env:RUNNER_TEMP "bench.txt"), "--engine", "local", "--language", "cs")
    if (-not $p.WaitForExit($CapSeconds * 1000)) {
        Stop-Process -Id $p.Id -Force
        Write-Host "TIMEOUT after $CapSeconds s"
    }
    Get-Content $err -ErrorAction SilentlyContinue |
        Select-String -Pattern "transcribed|threads:|load time|mel time|sample time|encode time|decode time|batchd time|prompt time|total time" |
        ForEach-Object { Write-Host $_.Line }
}
Remove-Item Env:HLAS_THREADS, Env:HLAS_GREEDY -ErrorAction SilentlyContinue
