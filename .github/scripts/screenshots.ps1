# Opens every surface of Hlas through the QA hooks and saves a screenshot of
# each, so UI changes can be reviewed without a Windows machine.
param([string]$Exe = "target\release\hlas.exe")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms, System.Drawing

$shots = Join-Path (Get-Location) "screenshots"
New-Item -ItemType Directory -Force -Path $shots | Out-Null
$data = Join-Path $env:LOCALAPPDATA "Hlas"
New-Item -ItemType Directory -Force -Path $data | Out-Null

@'
{"engine":"Local","language":"cs","favorite_languages":["cs","en"],"has_onboarded":true,"output_mode":"smart",
 "vocabulary":["Edenmakers","Hlas"],"replacements":[{"from":"eden makers","to":"Edenmakers"}]}
'@ | Set-Content -Encoding UTF8 (Join-Path $data "config.json")

$now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
@"
[{"id":3,"text":"Ahoj Evo, dneska proberu návrh, doplním podklady a večer ti to pošlu. Souhlasíš?","raw_text":"ahoj evo dneska proberu návrh doplním podklady a večer ti to pošlu souhlasíš","mode":"smart","engine":"groq","duration":6.2,"date":$now},
 {"id":2,"text":"Zítra ráno pošlu nabídku Petrovi.","mode":"transcript","engine":"local","duration":3.1,"date":$($now-3600)},
 {"id":1,"text":"- Pláštěnku\n- Láhev s vodou\n- Mapu","raw_text":"zabalit na výlet pláštěnku láhev s vodou mapu","mode":"smart","engine":"openai","duration":4.0,"date":$($now-90000)}]
"@ | Set-Content -Encoding UTF8 (Join-Path $data "history.json")

function Shot([string]$name) {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $bmp.Save((Join-Path $shots "$name.png"))
    $g.Dispose(); $bmp.Dispose()
}

function Capture([string]$name, [string]$var, [string]$value, [int]$wait = 5) {
    Set-Item -Path "Env:$var" -Value $value
    $p = Start-Process -FilePath $Exe -PassThru
    Start-Sleep -Seconds $wait
    Shot $name
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "Env:$var"
    Start-Sleep -Milliseconds 800
}

0..3 | ForEach-Object { Capture "onboarding-step-$_" "HLAS_STEP" "$_" }
foreach ($surface in @("settings", "history", "result", "pill-recording", "pill-transcribing", "pill-error")) {
    Capture $surface "HLAS_SHOW" $surface
}
Get-ChildItem $shots | ForEach-Object { Write-Host $_.Name }
