//! User-facing errors. Every message says what happened and what to do,
//! matching the wording of Hlas for macOS.

use std::fmt;

/// Failures talking to a cloud provider (transcription or Smart text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    Http { status: u16, provider: &'static str },
    InvalidResponse { provider: &'static str },
    Offline,
    TimedOut { provider: &'static str },
    Unavailable { provider: &'static str },
    MissingKey { provider: &'static str },
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Http { status, provider } => match status {
                401 | 403 => write!(
                    f,
                    "Your {provider} API key was rejected. Check it in Settings."
                ),
                413 => write!(f, "This recording is too large for {provider}."),
                429 => write!(f, "{provider} has reached a limit. Try again in a moment."),
                500..=599 => write!(
                    f,
                    "{provider} is temporarily unavailable. Try again shortly."
                ),
                _ => write!(
                    f,
                    "{provider} could not transcribe this recording. Try again."
                ),
            },
            NetError::InvalidResponse { provider } => {
                write!(f, "{provider} returned an unreadable response. Try again.")
            }
            NetError::Offline => write!(
                f,
                "No internet connection. Choose Local in Settings or reconnect."
            ),
            NetError::TimedOut { provider } => {
                write!(f, "{provider} took too long to respond. Try again.")
            }
            NetError::Unavailable { provider } => {
                write!(
                    f,
                    "Could not reach {provider}. Check your connection and try again."
                )
            }
            NetError::MissingKey { provider } => {
                write!(f, "Add your {provider} API key in Settings.")
            }
        }
    }
}

impl std::error::Error for NetError {}

/// Smart text failures. The plain transcript is always kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartTextError {
    MissingKey,
    InvalidOutput,
    Incomplete,
}

impl fmt::Display for SmartTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SmartTextError::MissingKey => {
                "Smart text needs an OpenAI or Groq key in Settings. Your transcript is safe."
            }
            SmartTextError::InvalidOutput => {
                "Smart text returned an invalid result. Your transcript is safe."
            }
            SmartTextError::Incomplete => "Smart text could not finish. Your transcript is safe.",
        })
    }
}

impl std::error::Error for SmartTextError {}

/// Local engine and audio failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalError {
    ModelMissing,
    ModelInvalid,
    ModelDownload,
    ModelLoad,
    NoMicrophone,
    MicrophoneBlocked,
    TooLong,
    UnsupportedAudio,
    CpuUnsupported,
    Cancelled,
}

impl fmt::Display for LocalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LocalError::ModelMissing => "The local model is not downloaded yet. Open Settings to download it.",
            LocalError::ModelInvalid => {
                "The downloaded model could not be verified. Your existing model was kept safe. Try again."
            }
            LocalError::ModelDownload => "Could not download the model. Check your connection and try again.",
            LocalError::ModelLoad => "The local model could not be loaded. Download it again in Settings.",
            LocalError::NoMicrophone => "No microphone found. Connect one or pick it in Settings.",
            LocalError::MicrophoneBlocked => {
                "Windows is blocking the microphone. Allow desktop apps in Privacy & security > Microphone."
            }
            LocalError::TooLong => "Audio exceeds the 10-minute limit.",
            LocalError::UnsupportedAudio => "Could not decode this audio file.",
            LocalError::CpuUnsupported => {
                "This processor cannot run the local engine (no AVX2). Choose Groq in Settings: fast, about $1 to 3 a month."
            }
            LocalError::Cancelled => "Cancelled.",
        })
    }
}

impl std::error::Error for LocalError {}

/// The best one-line message for any error that reaches the user.
pub fn user_message(err: &anyhow::Error) -> String {
    if let Some(e) = err.downcast_ref::<NetError>() {
        return e.to_string();
    }
    if let Some(e) = err.downcast_ref::<SmartTextError>() {
        return e.to_string();
    }
    if let Some(e) = err.downcast_ref::<LocalError>() {
        return e.to_string();
    }
    "Something went wrong. Details are in the log (tray menu > Open log folder).".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_errors_are_actionable() {
        let e = |status| {
            NetError::Http {
                status,
                provider: "Groq",
            }
            .to_string()
        };
        assert_eq!(
            e(401),
            "Your Groq API key was rejected. Check it in Settings."
        );
        assert!(e(429).contains("limit"));
        assert!(e(503).contains("temporarily unavailable"));
        assert!(e(400).contains("could not transcribe"));
    }

    #[test]
    fn user_message_prefers_known_errors() {
        let err = anyhow::Error::new(NetError::Offline);
        assert!(user_message(&err).starts_with("No internet"));
        let err = anyhow::Error::new(LocalError::TooLong);
        assert_eq!(user_message(&err), "Audio exceeds the 10-minute limit.");
        let err = anyhow::anyhow!("boom");
        assert!(user_message(&err).contains("log"));
    }
}
