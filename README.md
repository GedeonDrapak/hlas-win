# Hlas for Windows

**Dictation. Not typing.**

Hold **Right Ctrl**, speak Czech or English, release: the text lands at your
cursor, in any app. No subscription, no account, no telemetry, no cloud unless
you ask for it.

The Windows twin of [Hlas for macOS](https://github.com/GedeonDrapak/hlas).
Native Rust, no Electron, no runtime: one small `.exe` that idles at a few MB
of RAM.

## Install

Download `Hlas-setup.exe` from the
[latest release](https://github.com/GedeonDrapak/hlas-win/releases). It
installs per user, needs no administrator rights and registers a normal
uninstaller.

Until the installer is code-signed, Windows shows "Windows protected your PC".
Click **More info > Run anyway**. The source of every line is in this repo.

On first launch a short welcome tour checks the microphone, lets you pick the
engine and download the local model, and gives you a box to try it in.

## How it works

- **Hold Right Ctrl**, speak, release: the text is pasted where your cursor is.
- **Quick-tap Right Ctrl** to keep listening hands-free; tap again to stop.
- **Esc** cancels at any time. **Shift** held at the start gives a plain
  transcript even when Smart text is on.
- Right Ctrl shortcuts (Right Ctrl+C and friends) keep working: pressing
  another key with it cancels the dictation silently.
- Switched to another app while it was transcribing? Hlas does not paste into
  the wrong place; it shows the text in a small window with a Copy button.
- Windows has no Fn key for apps to listen to, so the macOS "hold Fn" gesture
  maps to Right Ctrl. Settings offers Right Alt, Right Shift, Scroll Lock,
  Pause and F13 to F16 instead.

## Engines

| Engine | What runs | Cost |
|---|---|---|
| **Local** (default) | whisper.cpp `large-v3-turbo-q5_0` on this PC. Audio never leaves it. One-time 547 MB download, verified by SHA-256. | free |
| **Groq** | `whisper-large-v3-turbo`, your own API key | about $1 to 3 a month |
| **OpenAI** | `gpt-transcribe`, your own API key | about $8 a month at heavy use |

**Smart text** (optional) reformats the transcript by intent: messages stay
prose, real lists become bullets, procedures become numbered steps. It uses
Groq or OpenAI with the same prompt and safeguards as the macOS app.

## Privacy

- Local engine: audio is processed on this PC and discarded.
- Cloud engines and Smart text: audio or text goes straight to the provider
  you chose. API keys are stored in Windows Credential Manager.
- History (last 50 results) stays in `%LOCALAPPDATA%\Hlas` and can be turned
  off or cleared in Settings. Dictations are kept out of Windows clipboard
  history and cloud clipboard.
- The log never contains transcripts or keys. Hlas never connects anywhere on
  its own; "Check for updates" asks GitHub only when you click it.

## Build

On Windows with the Rust toolchain:

```powershell
winget install LLVM.LLVM Kitware.CMake
cargo build --release
```

`target\release\hlas.exe` is the whole app. `installer\hlas.nsi` packages it
with NSIS. CI builds both on every push and attaches them to tagged releases.

The platform-independent core (audio preparation, text rules, history,
config) is tested on any OS:

```bash
cargo test
```

More for contributors and coding agents: [AGENTS.md](AGENTS.md). Current state
and the hardware test plan: [docs/HANDOFF.md](docs/HANDOFF.md).

## License

MIT. whisper.cpp and the Whisper model are MIT licensed by their authors.
