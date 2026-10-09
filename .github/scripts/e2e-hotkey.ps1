# Full pipeline on real Windows: hold Right Ctrl in Notepad, let Hlas
# "record" (a test file stands in for the microphone), transcribe locally and
# paste. Passes when the Czech words appear in Notepad.
param([string]$Exe = "target\release\hlas.exe")
$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class K {
    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit, Size = 40)]
    public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; }
    [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr after, string cls, string title);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr w, StringBuilder l);
    public static void Key(ushort vk, bool up) {
        var i = new INPUT();
        i.type = 1;
        i.ki.wVk = vk;
        i.ki.dwFlags = (up ? 2u : 0u) | 1u; // KEYEVENTF_EXTENDEDKEY: right-hand Ctrl
        SendInput(1, new[] { i }, Marshal.SizeOf(typeof(INPUT)));
    }
    public static string Text(IntPtr h) {
        var sb = new StringBuilder(8192);
        SendMessage(h, 0x000D, (IntPtr)8192, sb);
        return sb.ToString();
    }
}
"@

$data = Join-Path $env:LOCALAPPDATA "Hlas"
New-Item -ItemType Directory -Force -Path $data | Out-Null
'{"engine":"Local","language":"cs","has_onboarded":true,"output_mode":"transcript"}' |
    Set-Content -Encoding UTF8 (Join-Path $data "config.json")

$env:HLAS_FAKE_AUDIO = (Resolve-Path "tests\fixtures\czech-48k.wav").Path
$hlas = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 3

$np = Start-Process notepad -PassThru
Start-Sleep -Seconds 3
$np.Refresh()
$main = $np.MainWindowHandle
[K]::SetForegroundWindow($main) | Out-Null
Start-Sleep -Milliseconds 500
$edit = [K]::FindWindowEx($main, [IntPtr]::Zero, "Edit", $null)
if ($edit -eq [IntPtr]::Zero) { $edit = [K]::FindWindowEx($main, [IntPtr]::Zero, "RichEditD2DPT", $null) }
Write-Host "Notepad window $main, edit $edit, foreground $([K]::GetForegroundWindow())"

[K]::Key(0xA3, $false)
Start-Sleep -Milliseconds 1500
[K]::Key(0xA3, $true)

$text = ""
for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep -Seconds 1
    $text = [K]::Text($edit)
    if ($text.Length -gt 10) { break }
}
Write-Host "Notepad text: $text"
Get-Content (Join-Path $data "debug.log") -Tail 40 -ErrorAction SilentlyContinue | Write-Host
Stop-Process -Id $hlas.Id -Force -ErrorAction SilentlyContinue
Stop-Process -Id $np.Id -Force -ErrorAction SilentlyContinue
if (-not $text.ToLowerInvariant().Contains("zkouška")) { throw "The dictation did not land in Notepad" }
Write-Host "OK: hotkey, transcription and paste on Windows"
