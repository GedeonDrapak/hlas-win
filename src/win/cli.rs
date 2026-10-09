//! Command-line mode, used by CI and for scripting:
//!
//!   hlas.exe --transcribe <audio> [--out <file>] [--engine local|groq|openai]
//!            [--language cs|auto] [--smart]
//!   hlas.exe --download-model
//!   hlas.exe --version
//!
//! The release binary is a GUI-subsystem app, so it attaches to the parent
//! console for output. `--out` writes the result as UTF-8, which is what CI
//! reads.

use super::engine;
use crate::core::config::{Config, Engine};
use crate::core::errors::user_message;
use crate::core::{audio, decode, text};
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};

pub fn wants_cli(args: &[String]) -> bool {
    args.iter().any(|a| {
        matches!(
            a.as_str(),
            "--transcribe" | "--download-model" | "--version" | "--help"
        )
    })
}

fn value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

pub fn run(args: &[String]) -> i32 {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
    if args.iter().any(|a| a == "--version") {
        println!("Hlas for Windows {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    if args.iter().any(|a| a == "--help") {
        println!("hlas.exe --transcribe <audio> [--out <file>] [--engine local|groq|openai] [--language <code>|auto] [--smart]");
        println!("hlas.exe --download-model");
        return 0;
    }
    if args.iter().any(|a| a == "--download-model") {
        return match engine::model::ensure(
            |p| eprintln!("model download {p}%"),
            &AtomicBool::new(false),
        ) {
            Ok(()) => {
                println!("model ready");
                0
            }
            Err(e) => {
                eprintln!("model download failed: {e}");
                1
            }
        };
    }

    let Some(input) = value(args, "--transcribe") else {
        eprintln!("--transcribe needs a file");
        return 2;
    };
    let mut cfg = Config::load();
    if let Some(e) = value(args, "--engine") {
        cfg.engine = match e.as_str() {
            "groq" => Engine::Groq,
            "openai" => Engine::OpenAI,
            _ => Engine::Local,
        };
    }
    if let Some(l) = value(args, "--language") {
        cfg.language = l;
    }
    let smart = args.iter().any(|a| a == "--smart");

    let result = (|| -> anyhow::Result<String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let samples = decode::decode_file(std::path::Path::new(&input), &cancel)?;
        let prepared = audio::padded(&samples)
            .ok_or_else(|| anyhow::anyhow!("audio is too short or silent"))?;
        if cfg.engine == Engine::Local {
            eprintln!("whisper.cpp: {}", engine::local::system_info().trim());
            eprintln!(
                "cpu supported: {}, threads: {}, greedy: {}",
                engine::local::cpu_supported(),
                engine::local::thread_count(),
                std::env::var_os("HLAS_GREEDY").is_some()
            );
        }
        let began = std::time::Instant::now();
        let raw = engine::transcribe(&cfg, &prepared, &cancel)?;
        eprintln!(
            "transcribed {:.1} s of audio in {} ms with {}",
            samples.len() as f32 / 16_000.0,
            began.elapsed().as_millis(),
            cfg.engine.id()
        );
        if cfg.engine == Engine::Local {
            engine::local::print_timings();
        }
        let mut out = text::apply_replacements(&cfg.replacements, &raw);
        if text::filter_hallucination(&raw).is_empty() {
            eprintln!("warning: transcript looks like background audio");
        }
        if smart {
            let (provider, key) = engine::smart_provider(&cfg)
                .ok_or(crate::core::errors::SmartTextError::MissingKey)?;
            out = engine::smart::process(provider, &key, &out)?;
        }
        Ok(out)
    })();

    match result {
        Ok(out) => {
            if let Some(path) = value(args, "--out") {
                if let Err(e) = std::fs::write(&path, out.as_bytes()) {
                    eprintln!("could not write {path}: {e}");
                    return 1;
                }
            } else {
                let mut stdout = std::io::stdout();
                let _ = writeln!(stdout, "{out}");
            }
            0
        }
        Err(e) => {
            eprintln!("{}: {e}", user_message(&e));
            1
        }
    }
}
