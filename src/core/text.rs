//! Text rules applied after transcription: hallucination filter, exact
//! replacements, Whisper priming prompts and Smart text request/response.
//! All of them mirror Hlas for macOS so both apps produce the same text.

use super::errors::SmartTextError;
use super::shared::SHARED;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Whisper invents subtitle credits ("Titulky vytvořil...") on silence or
/// background audio. Returns an empty string when the transcript looks like one.
pub fn filter_hallucination(text: &str) -> String {
    let lower = text.to_lowercase();
    let total = lower.chars().count().max(1) as f64;
    for pattern in &SHARED.hallucination_patterns {
        if lower.contains(pattern.as_str())
            && pattern.chars().count() as f64 / total > SHARED.hallucination_ratio
        {
            return String::new();
        }
    }
    text.to_string()
}

/// A user's exact spelling rule, e.g. "eden makers" -> "Edenmakers".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Applies replacements case-insensitively, on whole words only, in order.
pub fn apply_replacements(rules: &[Replacement], text: &str) -> String {
    rules
        .iter()
        .fold(text.to_string(), |acc, rule| apply_one(rule, &acc))
}

fn apply_one(rule: &Replacement, text: &str) -> String {
    if rule.from.trim().is_empty() {
        return text.to_string();
    }
    let Ok(re) = regex::Regex::new(&format!("(?i){}", regex::escape(&rule.from))) else {
        return text.to_string();
    };
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut pos = 0;
    while pos <= text.len() {
        let Some(m) = re.find_at(text, pos) else {
            break;
        };
        let before_ok = text[..m.start()]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after_ok = text[m.end()..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(c));
        if before_ok && after_ok && m.end() > m.start() {
            out.push_str(&text[last..m.start()]);
            out.push_str(&rule.to);
            last = m.end();
            pos = m.end();
        } else {
            // Step one character forward and keep looking.
            let step = text[m.start()..].chars().next().map_or(1, |c| c.len_utf8());
            pos = m.start() + step;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Parses the Settings text box: one `from => to` rule per line.
pub fn parse_replacements(text: &str) -> Vec<Replacement> {
    text.lines()
        .filter_map(|line| {
            let (from, to) = line
                .split_once("=>")
                .or_else(|| line.split_once('\u{2192}'))?;
            let (from, to) = (from.trim(), to.trim());
            (!from.is_empty()).then(|| Replacement {
                from: from.into(),
                to: to.into(),
            })
        })
        .collect()
}

pub fn format_replacements(rules: &[Replacement]) -> String {
    rules
        .iter()
        .map(|r| format!("{} => {}", r.from, r.to))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Initial prompt for local Whisper: a well-formed sentence in the target
/// language steers diacritics and punctuation, and the user's vocabulary
/// teaches it names and jargon.
pub fn whisper_prompt(language: Option<&str>, vocabulary: &[String]) -> Option<String> {
    let priming = language
        .and_then(|l| SHARED.priming_prompts.get(l))
        .cloned()
        .unwrap_or_default();
    let vocab = vocabulary_prompt(vocabulary).unwrap_or_default();
    let joined = [priming, vocab]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!joined.is_empty()).then_some(joined)
}

/// Vocabulary as a comma-separated prompt for cloud engines.
pub fn vocabulary_prompt(vocabulary: &[String]) -> Option<String> {
    let words: Vec<&str> = vocabulary
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .collect();
    (!words.is_empty()).then(|| words.join(", "))
}

/// Which provider formats Smart text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartProvider {
    Groq,
    OpenAI,
}

impl SmartProvider {
    pub fn name(self) -> &'static str {
        match self {
            SmartProvider::Groq => "Groq",
            SmartProvider::OpenAI => "OpenAI",
        }
    }
    pub fn endpoint(self) -> &'static str {
        match self {
            SmartProvider::Groq => "https://api.groq.com/openai/v1/chat/completions",
            SmartProvider::OpenAI => "https://api.openai.com/v1/chat/completions",
        }
    }
    pub fn model(self) -> &'static str {
        match self {
            SmartProvider::Groq => &SHARED.smart_text_models.groq,
            SmartProvider::OpenAI => &SHARED.smart_text_models.openai,
        }
    }
}

/// Same preference order as macOS: Groq when it is the transcription engine
/// and has a key, otherwise OpenAI, otherwise Groq.
pub fn choose_smart_provider(
    engine_is_groq: bool,
    has_groq: bool,
    has_openai: bool,
) -> Option<SmartProvider> {
    if engine_is_groq && has_groq {
        Some(SmartProvider::Groq)
    } else if has_openai {
        Some(SmartProvider::OpenAI)
    } else if has_groq {
        Some(SmartProvider::Groq)
    } else {
        None
    }
}

/// The chat-completions request body for Smart text.
pub fn smart_request(provider: SmartProvider, text: &str) -> Value {
    let max_tokens = text.len().clamp(1024, 16_000);
    let mut body = json!({
        "model": provider.model(),
        "temperature": 0,
        "max_completion_tokens": max_tokens,
        "messages": [
            {"role": "system", "content": SHARED.smart_text_prompt},
            {"role": "user", "content": text},
        ],
    });
    if provider == SmartProvider::Groq {
        body["reasoning_effort"] = json!("low");
    }
    body
}

#[derive(Deserialize)]
struct Reply {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}
#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

/// Parses and validates a Smart text reply. Only a complete answer
/// (`finish_reason == "stop"`) is accepted.
pub fn parse_smart_reply(body: &str, original: &str) -> Result<String, SmartTextError> {
    let reply: Reply = serde_json::from_str(body).map_err(|_| SmartTextError::Incomplete)?;
    let choice = reply
        .choices
        .into_iter()
        .next()
        .ok_or(SmartTextError::Incomplete)?;
    if choice.finish_reason.as_deref() != Some("stop") {
        return Err(SmartTextError::Incomplete);
    }
    let content = choice.message.content.ok_or(SmartTextError::Incomplete)?;
    validate_smart(&content, original)
}

/// Rejects empty or runaway output and normalizes long dashes to hyphens.
pub fn validate_smart(text: &str, original: &str) -> Result<String, SmartTextError> {
    let cleaned = text.trim();
    let limit = (original.chars().count() * 4).max(1000);
    if cleaned.is_empty() || cleaned.chars().count() > limit {
        return Err(SmartTextError::InvalidOutput);
    }
    Ok(cleaned
        .replace('\u{2014}', "-")
        .replace(" \u{2013} ", " - ")
        .replace('\u{2013}', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(from: &str, to: &str) -> Replacement {
        Replacement {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn hallucination_filter_drops_subtitle_credits() {
        assert_eq!(filter_hallucination("Titulky vytvořil JohnyX"), "");
        assert_eq!(filter_hallucination("Thanks for watching!"), "");
        let real = "Zítra ráno pošlu nabídku a ještě poděkuju za spolupráci, thanks for watching the demo with us today";
        assert_eq!(filter_hallucination(real), real);
        assert_eq!(
            filter_hallucination("Ahoj, jak se máš?"),
            "Ahoj, jak se máš?"
        );
    }

    #[test]
    fn replacements_are_whole_word_and_case_insensitive() {
        let rules = vec![rule("eden makers", "Edenmakers"), rule("ai", "AI")];
        assert_eq!(
            apply_replacements(
                &rules,
                "Eden Makers dělá ai videa, ale ne v Thailandu ani v Haiti."
            ),
            "Edenmakers dělá AI videa, ale ne v Thailandu ani v Haiti."
        );
    }

    #[test]
    fn replacements_respect_czech_letters_as_word_characters() {
        let rules = vec![rule("pes", "kočka")];
        assert_eq!(
            apply_replacements(&rules, "pes, pesík, Pes."),
            "kočka, pesík, kočka."
        );
        assert_eq!(apply_replacements(&rules, "špes pes"), "špes kočka");
    }

    #[test]
    fn replacements_ignore_empty_rules_and_treat_target_literally() {
        let rules = vec![rule("  ", "x"), rule("cena", "$1 Kč")];
        assert_eq!(
            apply_replacements(&rules, "cena je dobrá"),
            "$1 Kč je dobrá"
        );
    }

    #[test]
    fn replacement_lines_round_trip() {
        let r = parse_replacements(
            "eden makers => Edenmakers\n  ai=>AI \nbad line\n => x\nhlas \u{2192} Hlas",
        );
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], rule("eden makers", "Edenmakers"));
        assert_eq!(r[2].to, "Hlas");
        assert_eq!(parse_replacements(&format_replacements(&r)), r);
    }

    #[test]
    fn whisper_prompt_combines_priming_and_vocabulary() {
        let vocab = vec![
            "Edenmakers".to_string(),
            " Hlas ".to_string(),
            String::new(),
        ];
        let p = whisper_prompt(Some("cs"), &vocab).unwrap();
        assert!(p.starts_with("Toto je přesný přepis"));
        assert!(p.ends_with("Edenmakers, Hlas"));
        assert_eq!(whisper_prompt(Some("de"), &[]), None);
        assert_eq!(whisper_prompt(None, &["X".into()]), Some("X".into()));
    }

    #[test]
    fn smart_provider_preference_matches_mac() {
        use SmartProvider::*;
        assert_eq!(choose_smart_provider(true, true, true), Some(Groq));
        assert_eq!(choose_smart_provider(false, true, true), Some(OpenAI));
        assert_eq!(choose_smart_provider(false, true, false), Some(Groq));
        assert_eq!(choose_smart_provider(true, false, true), Some(OpenAI));
        assert_eq!(choose_smart_provider(false, false, false), None);
    }

    #[test]
    fn smart_request_shape() {
        let groq = smart_request(SmartProvider::Groq, "ahoj");
        assert_eq!(groq["reasoning_effort"], "low");
        assert_eq!(groq["max_completion_tokens"], 1024);
        assert_eq!(groq["messages"][1]["content"], "ahoj");
        let openai = smart_request(SmartProvider::OpenAI, &"a".repeat(50_000));
        assert!(openai.get("reasoning_effort").is_none());
        assert_eq!(openai["max_completion_tokens"], 16_000);
        assert_eq!(openai["model"], "gpt-4.1-mini");
    }

    #[test]
    fn smart_reply_needs_a_finished_answer() {
        let ok =
            r#"{"choices":[{"message":{"content":" Ahoj Evo — díky. "},"finish_reason":"stop"}]}"#;
        assert_eq!(
            parse_smart_reply(ok, "ahoj evo diky").unwrap(),
            "Ahoj Evo - díky."
        );
        let cut = r#"{"choices":[{"message":{"content":"Ahoj"},"finish_reason":"length"}]}"#;
        assert_eq!(parse_smart_reply(cut, "x"), Err(SmartTextError::Incomplete));
        assert_eq!(
            parse_smart_reply("not json", "x"),
            Err(SmartTextError::Incomplete)
        );
        let empty = r#"{"choices":[{"message":{"content":"  "},"finish_reason":"stop"}]}"#;
        assert_eq!(
            parse_smart_reply(empty, "x"),
            Err(SmartTextError::InvalidOutput)
        );
    }

    #[test]
    fn smart_output_length_is_bounded() {
        assert!(validate_smart(&"a".repeat(1000), "b").is_ok());
        assert!(validate_smart(&"a".repeat(1001), "b").is_err());
        assert!(validate_smart(&"a".repeat(4000), &"b".repeat(1000)).is_ok());
    }
}
