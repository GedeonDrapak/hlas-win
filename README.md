# Hlas for Windows

Ultra-minimal dictation for Windows. Hold a key, speak Czech or English, release - text lands at your cursor. No subscription, no account, no telemetry.

The Windows counterpart to [Hlas for macOS](https://github.com/GedeonDrapak/hlas). Native Rust, no Electron, no runtime - a single small `.exe` that idles at a few MB of RAM.

## Push-to-talk key

Windows has no `Fn` scancode (the keyboard firmware eats it), so the macOS "hold Fn" gesture maps to **Right Ctrl** by default. Hold it, speak, release.

## Engines

- **Local (default):** whisper.cpp `large-v3-turbo-q5_0`, downloaded automatically on the first local dictation (~547 MB) to `%APPDATA%\Hlas\models\`. Runs on CPU; audio never leaves the PC.
- **Groq API:** `whisper-large-v3-turbo`. BYOK.
- **OpenAI API:** `gpt-4o-transcribe`. BYOK.

API keys are stored in the Windows Credential Manager, never in a config file.

## Install

Grab `Hlas-setup.exe` from the [latest release](https://github.com/GedeonDrapak/hlas-win/releases/latest), or build it yourself. The installer is per-user, needs no administrator rights, and registers a normal Windows uninstaller.

## Build

Requires the Rust toolchain and, for the local engine, an LLVM install (bindgen) plus CMake (whisper.cpp). On Windows:

```powershell
winget install LLVM.LLVM
cargo build --release
```

The executable lands at `target\release\hlas.exe`. CI builds it and the NSIS installer on every push. Pushing a version tag such as `v0.1.0` publishes both files to a GitHub Release.

## Architecture

One module, one concern - mirrors the macOS layout:

| File | Concern |
|---|---|
| `hotkey.rs` | `WH_KEYBOARD_LL` low-level hook, hold-to-talk edges |
| `audio.rs` | cpal (WASAPI) capture → 16 kHz mono f32 |
| `transcribe/` | engine dispatch: `local` (whisper-rs), `cloud` (Groq/OpenAI), `model` downloader |
| `inject.rs` | clipboard swap + synthesized Ctrl+V |
| `overlay.rs` | layered, click-through status pill (bottom-center) |
| `settings.rs` | native Win32 settings window (native-windows-gui) |
| `tray.rs` | tray icon, menu, main message loop |
| `autostart.rs` | launch-at-login via HKCU Run key |
| `keystore.rs` | API keys in Credential Manager |

## Status

First cut. Builds green in CI; on-device behavior (hook timing, WASAPI device quirks, overlay compositing) needs testing on real Windows hardware.
