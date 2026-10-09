# Hlas for Windows - handoff (2026-10-09)

For whoever continues the repo (Tomáš). Branch **`parity-0.2`**, version
**0.2.0**, not merged and not released yet. Everything below was built and
checked on a Mac plus GitHub's Windows runners. **Nobody has run it on a real
Windows PC with a real microphone yet** - that is the next job.

## 1. Where it stands

- 0.1.0 (August) compiled but was never run on Windows. Porting it to parity
  with Hlas for macOS 0.4.2 turned up two bugs that made it unusable:
  1. **Half of every dictation was lost** on 48 kHz microphones (resampler
     delay never flushed). Proven by `core::audio` tests.
  2. **Groq/OpenAI could never work**: `keyring` stored keys in an in-memory
     mock (missing `windows-native` feature).
  Two more surfaced only on GitHub's real Windows machines:
  3. The CI-built exe used **AVX-512** (whisper.cpp compiled for the runner's
     CPU) and would crash on most laptops. Now a portable AVX2 baseline;
     older CPUs get a "use Groq" message instead of a crash.
  4. **Local transcription aborted at random** because of a whisper-rs bug in
     its "safe" abort callback. Replaced with the raw callback.
  Plus a list of smaller ones (see `CHANGELOG.md`).
- 0.2.0 is a rewrite into `src/core` (platform-independent, 40 unit tests) and
  `src/win` (Win32), with every macOS 0.4.2 feature ported.

## 2. What is verified, and how

| Check | Where | Result |
|---|---|---|
| Core unit tests (audio, text rules, history, config, decode, model) | Mac, Linux CI, Windows CI | see CI run |
| Text rules identical to macOS (Smart text prompt, priming, hallucinations) | CI job "Core tests and shared-rule drift" | see CI run |
| Whole Windows layer compiles, clippy `-D warnings` | Mac cross-build + Windows CI | clean |
| Model download through Hlas, SHA-256 verified | Windows CI `--download-model` | see CI run |
| Real Czech transcription on Windows (48 kHz file, local engine) | Windows CI `e2e-transcribe.ps1` | see CI run |
| Screenshots of the tour, settings, history, result window, pill states | Windows CI artifact `qa-screenshots` | see CI run |
| Hold Right Ctrl in Notepad, transcribe, paste | Windows CI `e2e-hotkey.ps1` (test file instead of microphone) | see CI run |

Section 9 is updated with the actual CI outcome at the end of this session.

## 3. What only a real Windows PC can tell (your test plan)

Use a normal Windows 11 x64 laptop, ideally a weaker one: Hlas is aimed at
people who cannot run heavy apps. Install the `hlas-setup` artifact from the
latest CI run of `parity-0.2` (or build it). Keep
`%LOCALAPPDATA%\Hlas\debug.log` open; every step logs there.

**Install and first run**
- [ ] Installer runs without admin; SmartScreen "More info > Run anyway" is the only warning.
- [ ] Welcome tour opens. Step 2: "Test microphone" moves the bar while you speak.
- [ ] Turn off *Settings > Privacy & security > Microphone > Let desktop apps access your microphone*: the tour says so and the button opens that page. Turn it back on.
- [ ] Step 3: "Download" shows progress, then "Model ready" (547 MB).
- [ ] Step 4: click into the box, hold Right Ctrl, say a Czech sentence, release. Text appears in the box.

**Dictation everywhere** (hold Right Ctrl, speak, release)
- [ ] Notepad, Word, Chrome (Gmail compose), Slack desktop, VS Code, Windows Terminal.
- [ ] Diacritics correct (ř, ů, ě), punctuation sensible.
- [ ] Quick-tap Right Ctrl: keeps listening hands-free; tap again stops.
- [ ] Esc during listening or transcribing: "Dictation cancelled", nothing pasted.
- [ ] Right Ctrl+C / Right Ctrl+V still copy and paste, no pill, no dictation.
- [ ] Switch to another app while it transcribes: the "Your text" window appears instead of pasting into the wrong app.
- [ ] Dictate into an app running as administrator (elevated Terminal): "Your text" window explains why.

**Clipboard etiquette**
- [ ] Copy an image in Paint, dictate somewhere, then paste in Paint: the image is still there.
- [ ] Win+V clipboard history does not contain the dictation.

**Cloud and Smart text**
- [ ] Settings: paste a Groq key, Save, quit Hlas, start it again: key still there (Credential Manager).
- [ ] Engine Groq: dictation works. A wrong key shows "Your Groq API key was rejected" in the pill.
- [ ] Output Smart text: a spoken shopping list becomes bullets; a message stays prose. Shift + Right Ctrl gives a plain transcript once.
- [ ] Unplug network with a cloud engine: the pill says "No internet connection...".

**Hardware and display**
- [ ] Bluetooth headset: change the default input in Windows during a session; the next dictation uses the new device (log: "rebuilding microphone stream").
- [ ] Two monitors, one at 150 %: the pill appears bottom-center of the monitor with the active window, crisp and the right size. Settings fits on screen.
- [ ] Live waveform in the pill moves with your voice.

**Resources and speed** (write the numbers into this file)
- [ ] Idle RAM in Task Manager after start: ____ MB (target: under 30 MB).
- [ ] RAM with the model loaded: ____ MB; it drops back about 3 minutes after the last dictation (log: "local model unloaded after idle").
- [ ] 10 s of speech, local engine: ____ ms (log line `local inference: ... elapsed_ms=`). CPU: ______.
- [ ] The log's `whisper.cpp:` line lists `AVX2 = 1` and `AVX512 = 0`.
- [ ] If you can find a PC without AVX2 (old Celeron/Pentium Silver): Local shows "This processor cannot run the local engine", Groq works, nothing crashes.

**Lifecycle**
- [ ] "Start at sign-in" on, reboot: exactly one tray icon.
- [ ] Run the installer again while Hlas runs: it closes Hlas, installs, "Launch Hlas" works, still one tray icon.
- [ ] Upgrade over 0.1.0 (if you have it): settings carried over, model moved from `%APPDATA%\Hlas\models` to `%LOCALAPPDATA%\Hlas\models`.
- [ ] Uninstall: asks whether to remove settings, history, keys and model.
- [ ] Tray > Transcribe audio file: an mp3 or m4a voice memo opens in "Your text".

Anything that fails: note it under section 8 with the debug.log excerpt.

## 4. How to work on it

- Read `AGENTS.md` (layout, commands, QA hooks, gotchas).
- On Windows: `cargo build --release`, run `target\release\hlas.exe`.
- Screens without clicking through: `set HLAS_STEP=2 && hlas.exe`,
  `set HLAS_SHOW=settings && hlas.exe`.
- Whole pipeline without a microphone: `set HLAS_FAKE_AUDIO=tests\fixtures\czech-48k.wav`.
- From a Mac: the cross-build in `AGENTS.md` type-checks the Windows code in a
  minute; push the branch and run the workflow for real Windows checks:
  `gh workflow run windows.yml --ref <branch>`.

## 5. Releasing

1. Merge `parity-0.2` into `master` once section 3 passes.
2. Tag a beta first: `git tag v0.2.0-beta.1 && git push --tags`. Tags with a
   dash become GitHub pre-releases. CI attaches `hlas.exe`, `Hlas-setup.exe`,
   `hlas-vulkan.exe` (experimental) and `SHA256SUMS.txt`.
3. After a few days of real use: `v0.2.0`.

## 6. Signing

Not set up. The account has to be Eden Makers s.r.o. (EU individuals cannot
get public-trust certificates), about USD 10 a month. Steps and the exact
GitHub secrets: `docs/SIGNING.md`. Needs Gedeon or Ondra (company card and
legal identity). Once the secrets exist, CI signs automatically.

## 7. Known limitations and risks

- **Right Ctrl + mouse click**: a quick Ctrl+click with the right hand counts
  as a quick tap and latches listening (same semantics as macOS Fn). Tap again
  to stop. If testers trip on it, consider ignoring taps followed by a mouse
  click.
- **Low-level hook**: Windows may drop it if the system stalls; Hlas
  re-installs it every 10 minutes. If dictation "stops working" after sleep,
  check the log for `keyboard hook install failed`.
- **Look**: every window uses the dark skin in `src/win/ui/skin.rs`
  (macOS design tokens, bundled Satoshi, owner-drawn buttons, switches,
  segmented controls, sidebar, own dropdown list, borderless edits). Text
  boxes have no scrollbar (wheel and keyboard scroll, as on macOS). The
  dropdown list has only been seen in code review, not on screen: open each
  dropdown once on a real PC (mouse, keyboard, wheel, typing a letter).
- **Narrator**: no screen-reader announcements yet (macOS has VoiceOver ones).
- **Vulkan build** (`hlas-vulkan.exe`, experimental): needs `vulkan-1.dll`
  (present with any current GPU driver) and has not been benchmarked. Compare
  it with the CPU build on an Intel/AMD laptop; whisper.cpp 1.8.3+ can use
  integrated graphics.
- **Local speed on weak CPUs**: beam search 5 is the macOS choice for Czech
  accuracy. If 10 s of speech takes much more than 5 s on a weak laptop,
  consider a "fast" option with greedy decoding.

## 8. Findings from hardware testing

### 2026-10-09, Tomáš's laptop (i7-13620H, Windows 11 Home, Smart App Control on)

- **Local engine speed, root cause found (fix in `vendor/whisper-rs-sys/build.rs`).**
  With the default Visual Studio generator, cmake-rs 0.1.58 overwrites
  `CMAKE_<LANG>_FLAGS_RELEASE` with cc's flags and strips every `/O` switch
  (`cmake` crate `src/lib.rs`, the `generator.is_none() && msvc` branch), so
  MSVC compiled ggml **unoptimized**. The clang job escapes it because
  `CMAKE_GENERATOR=Ninja` skips that branch. CI run 37914441177: the clang
  benchmark ran all three configs in 3.6 min and "Transcribe Czech audio
  locally" took 53 s for the whole step; the MSVC step hit its 15 min limit.
  The fix restores CMake's own Release flags (`/MD /O2 /Ob2 /DNDEBUG`, plus
  `/EHsc` for C++) for MSVC targets. Confirmed by CI run 37922714743, the
  first fully green run (6.1 s of Czech audio, Windows runner, 4 vCPU):

  | Build | 1 thread greedy | 4 threads greedy | 4 threads beam | default (2 threads, beam) |
  |---|---|---|---|---|
  | MSVC before fix | > 300 s | > 300 s | > 300 s | 870 s |
  | MSVC after fix | 94.0 s | 50.8 s | 44.7 s | 52.3 s |
  | clang + Ninja | 91.1 s | 48.9 s | 47.1 s | 62.4 s |

  MSVC and clang are now equal, so the release build stays on MSVC. The
  hotkey-to-Notepad end-to-end step passes for the first time. Still about
  6x slower than the Mac (7 s): the Mac build uses Accelerate, and the runner
  is a weak shared VM.
- **Real laptop numbers** (i7-13620H 6P+4E, on AC, "High performance" plan,
  RTX 5070 Laptop + Intel UHD; CI-built exes run under Smart App Control,
  only the local `cargo build` is blocked). Wall time per dictation,
  default settings (beam 5, 8 threads), model already cached:

  | Build / device | 6.1 s fixture | 10 s Czech VO | 25 s Czech VO |
  |---|---|---|---|
  | `hlas.exe` CPU (MSVC, fixed) | 14.1 s | 35.7 s | 27.4 s |
  | `hlas-vulkan.exe` on RTX 5070 | 10.4 s (first run, shader warm-up) | **1.6 s** | **1.5 s** |
  | `hlas-vulkan.exe` on Intel UHD (`GGML_VK_VISIBLE_DEVICES=1`) | 25.0 s | 18.3 s | 20.2 s |
  | CPU + `HLAS_AUDIO_CTX=auto` | 4.8 s, **repeats the last sentence** | 12.1 s | 26.1 s |

  Transcripts are identical across CPU and both GPUs. Conclusions:
  1. On CPU even a fast laptop misses the "10 s of speech in under 5 s"
     target by 3-7x. Time does not grow with clip length (10 s slower than
     25 s), which points at the decoder/temperature fallback rather than the
     encoder.
  2. The Vulkan build on a dedicated GPU matches the Mac (Metal, 1-1.5 s).
     It is the real fix for PCs with an NVIDIA/AMD GPU. Intel iGPU gains
     nothing over the CPU.
  3. `HLAS_AUDIO_CTX` (shrinking the 30 s encoder window) helps only very
     short clips and makes whisper repeat text. Keep it as a QA switch only,
     do not make it the default.
  4. Product decision for Gedeon: ship the Vulkan build as the main exe (it
     falls back to the CPU when no GPU is usable - to verify), and on
     CPU-only PCs make Groq the default in the welcome tour with Local as
     the private, slower option.
- `HLAS_AUDIO_CTX=auto|N`, `HLAS_BEAM=N` and `HLAS_NO_FALLBACK=1` added to
  `engine/local.rs` as benchmark overrides next to `HLAS_THREADS` and
  `HLAS_GREEDY`.
- **Where the CPU time goes.** Without temperature fallback the CPU numbers
  do not move (6.1 s: 14.3 s beam / 14.5 s greedy; 25 s: 25.3 s / 29.6 s), and
  greedy is no faster than beam. Only shrinking the encoder window helped
  (6.1 s clip: 14 s -> 4.8 s). So the floor is the encoder: turbo keeps
  large-v3's full 32-layer encoder and only trims the decoder, and the
  encoder always runs a 30 s window - about 12-14 s on this i7. No decoder
  setting fixes that; a GPU, a smaller model or a shorter window would.
- **Two start-up dependencies found with `dumpbin /dependents`:**
  1. `hlas.exe` imported `MSVCP140.dll` / `VCRUNTIME140.dll`, which the
     installer does not ship: no start on a PC without the Visual C++
     Redistributable. Fixed with a static CRT (`.cargo/config.toml`
     crt-static, `/MT` for whisper.cpp).
  2. `hlas-vulkan.exe` imported `vulkan-1.dll`: no start without a Vulkan
     driver, although ggml loads Vulkan dynamically and falls back to the
     CPU. Fixed with `/DELAYLOAD:vulkan-1.dll` (`build.rs`).
  CI run 37928597093 is green with both fixes (tests, local transcription,
  hotkey to Notepad; speed unchanged, 6.1 s in 45 s on the runner), and
  `dumpbin` confirms no CRT import and vulkan-1.dll only as a delay-load
  import. Not yet run on a PC without the redistributable or without Vulkan.
- **Smart App Control is inconsistent.** It let the CI builds from runs
  37922714743 and 37924930740 run on this laptop, then blocked both exes
  from run 37928597093 (CodeIntegrity 3077). An unsigned Hlas may start for
  one user and not for the next.
- **Smart App Control blocks Hlas entirely, not just a SmartScreen warning.**
  On a PC with Smart App Control on, every unsigned binary without cloud
  reputation is blocked with no "Run anyway" (CodeIntegrity events 3077/3118).
  Here it even blocked `cargo build` (build scripts and proc-macro DLLs), so
  Hlas cannot be built or run on this laptop. SAC is on by default on clean
  Windows 11 installs, so unsigned releases will simply not start for a share
  of users: signing (section 6) is a release blocker, and the test in
  section 3 needs a PC with SAC off.

## 9. CI outcome at handoff (2026-10-09, Windows Server runner, 4 vCPU)

| Check | Result |
|---|---|
| Core tests (Linux, Windows), drift vs macOS rules | pass |
| Release build, clippy, installer (`hlas.exe` 3.9 MB, `Hlas-setup.exe` 3.0 MB) | pass |
| Vulkan build (`hlas-vulkan.exe`, 20 MB) | builds, not run on a GPU |
| Model download through Hlas + SHA-256 | pass |
| Local Czech transcription | **correct text, but 870 s for 6 s of audio** |
| Hotkey to Notepad | pipeline runs (hook, fake mic, model load), text not pasted within the 2 min limit because of the slow engine |
| Screenshots | first upload failed (fixed); see `qa-screenshots` of the latest run |

**The open problem (your first task): local engine speed on Windows.**
Same model, same settings on a Mac CPU with 2 threads: 7 s and the identical
transcript. On the Windows runner: 870 s. Already ruled out or fixed: random
encoder aborts (whisper-rs callback bug), AVX-512 build, flash attention
(off on CPU now). Not yet known: MSVC-compiled ggml vs clang, thread
scheduling, or the runner itself.

What is in place to find it:
- `.github/scripts/bench.ps1` runs 1 thread / all threads / beam with
  whisper.cpp's own timing breakdown (encode vs decode). Results are in the
  "Benchmark the local engine" step of run 37910141122 and later.
- CI job "Local engine built with clang" builds ggml with clang-cl + Ninja
  for comparison (fixed `/EHsc` in the last commit, not yet run).
- Process power throttling (EcoQoS) is now disabled, in case Windows parks
  the work on slow cores.
- On a real PC: `hlas.exe --transcribe tests\fixtures\czech-48k.wav --out out.txt`
  with `HLAS_THREADS=4`, `HLAS_GREEDY=1` prints the same breakdown.

**Benchmark result (run 37910141122, MSVC build):** 1 thread greedy, 4 threads
greedy and 4 threads beam all ran past the 300 s cap. Thread count and the
decoder are therefore not the cause; suspect the MSVC-compiled ggml or the
runner itself. Next: the clang job, and the same command on a real PC.

If encode time dominates and clang is much faster, switch the release build to
clang. If even a fast PC needs more than about 5 s for 10 s of speech, make
Groq the default in the welcome tour and keep Local as the private option.

## 10. Backlog after 0.2.0

1. Local engine speed on Windows (section 9), then the hardware test (section 3), then release 0.2.0.
2. Signing (section 6).
3. winget manifest (`winget-pkgs`), once releases are signed.
4. Benchmark the Vulkan build; if it is reliably faster, offer it in the installer.
5. ~~Dark theme for Settings/History~~ done in `ui-dark`; Narrator labels for the painted text.
6. Parakeet / ElevenLabs Scribe engines, mirroring the macOS backlog.
