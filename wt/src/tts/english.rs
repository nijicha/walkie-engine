//! English with Kokoro-82M v1.0 (CPU, ONNX) and misaki-rs as the G2P.
//!
//! MakHam is a Thai Kokoro distilled to one voice; this is the full multi-voice model, so it also
//! takes a speaker: a style table of 510 rows of 256 floats, one row per token count.
//!
//! The G2P is misaki's English pipeline without espeak-ng (GPL): a word it does not know comes back
//! without phonemes and is reported as UNKNOWN, so a lexicon entry can fix it. Pronunciations, in
//! the lexicon or inline as {phonemes}, use misaki's US set: ˈ primary and ˌ secondary stress before
//! the vowel, A (eɪ), I (aɪ), O (oʊ), W (aʊ), Y (ɔɪ), ɹ, ʤ, ʧ, T (the flap in "water").
//!
//! Before the G2P, a few things misaki reads wrongly are rewritten: versions (0.1.1, v2.3), times
//! (10:30) and dates (28 Nov, November 28) — see `normalize`.

use super::data;
use anyhow::{Context, Result, anyhow};
use indexmap::IndexMap;
use misaki_rs::{G2P, Language};
use std::collections::HashMap;

const STYLE: usize = 256;
const MAX_TOKENS: usize = 510;
/// Vowel symbols in misaki's set: one per syllable (A I O W Y are diphthongs, ᵊ a syllabic consonant).
const VOWELS: &str = "AIOWYaeiouæɐɑɒɔəɛɜɪʊʌᵻᵊɚ";

pub struct Kokoro {
    session: ort::session::Session,
    pub vocab: HashMap<char, i64>,
    table: Vec<f32>,
    g2p: G2P,
    lexicon: IndexMap<String, String>,
    marks: regex::Regex,
}

/// One line, read: its phonemes and the words the G2P could not read.
pub struct Read {
    pub phonemes: String,
    pub unknown: Vec<String>,
}

impl Kokoro {
    pub fn load(model: &str, speaker: &str, lexicon: IndexMap<String, String>, threads: usize) -> Result<Kokoro> {
        let (mfile, msha) = data::kokoro_model(model).ok_or_else(|| anyhow!("voice model {model:?} unknown for English; pick one of {}", data::KOKORO_MODELS))?;
        let (vfile, vsha) = data::kokoro_speaker(speaker).ok_or_else(|| anyhow!("speaker {speaker:?} unknown; pick one of {}", data::KOKORO_SPEAKERS))?;
        let tok = super::read_json(&data::kokoro_file(data::KOKORO_TOKENIZER.0, data::KOKORO_TOKENIZER.1)?)?;
        let vocab = tok["model"]["vocab"]
            .as_object()
            .context("Kokoro tokenizer has no vocab")?
            .iter()
            .filter_map(|(k, v)| Some((k.chars().next()?, v.as_i64()?)))
            .collect();
        let table: Vec<f32> =
            std::fs::read(data::kokoro_file(&vfile, vsha)?)?.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let session = ort::session::Session::builder(super::ort_env()?)?
            .with_intra_threads(threads)
            .map_err(|e| anyhow!("{e}"))?
            .commit_from_file(data::kokoro_file(mfile, msha)?)?;
        // Inline [word](/phonemes/) and {phonemes} are left alone; lexicon words become links.
        let mut words: Vec<&String> = lexicon.keys().collect();
        words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
        let alts: String = words.iter().map(|w| format!(r"|\b{}\b", regex::escape(w))).collect();
        let marks = regex::Regex::new(&format!(r"\[[^\]]*\]\(/[^)]*/\)|\{{[^}}]+\}}{alts}"))?;
        Ok(Kokoro { session, vocab, table, g2p: G2P::new(Language::EnglishUS), lexicon, marks })
    }

    /// The text with lexicon words and {phonemes} as misaki's [word](/phonemes/) links.
    fn mark(&self, text: &str) -> String {
        self.marks
            .replace_all(text, |c: &regex::Captures| {
                let m = &c[0];
                if m.starts_with('[') {
                    m.to_string()
                } else if let Some(ph) = m.strip_prefix('{').and_then(|m| m.strip_suffix('}')) {
                    format!("[x](/{ph}/)")
                } else {
                    format!("[{m}](/{}/)", self.lexicon[m])
                }
            })
            .into_owned()
    }

    pub fn read(&self, text: &str) -> Result<Read> {
        let (phonemes, tokens) = self.g2p.g2p(&self.mark(&normalize(text))).map_err(|e| anyhow!("English G2P: {e:?}"))?;
        let mut unknown: Vec<String> = vec![];
        for t in tokens.iter().filter(|t| t.phonemes.as_deref().is_none_or(|p| p.contains('❓')) && t.text.chars().any(char::is_alphanumeric)) {
            if !unknown.contains(&t.text) {
                unknown.push(t.text.clone());
            }
        }
        Ok(Read { phonemes: phonemes.replace('❓', "").trim().to_string(), unknown })
    }

    pub fn synth(&mut self, phonemes: &str, speed: f32) -> Result<Vec<f32>> {
        let mut ids: Vec<i64> = phonemes.chars().filter_map(|c| self.vocab.get(&c).copied()).collect();
        ids.truncate(MAX_TOKENS);
        let row = ids.len().min(self.table.len() / STYLE - 1);
        let style = self.table[row * STYLE..(row + 1) * STYLE].to_vec();
        let mut padded = Vec::with_capacity(ids.len() + 2);
        padded.push(0i64);
        padded.extend(&ids);
        padded.push(0);
        let n = padded.len();
        let outputs = self.session.run(ort::inputs![
            "input_ids" => ort::value::Tensor::from_array(([1usize, n], padded))?,
            "style" => ort::value::Tensor::from_array(([1usize, STYLE], style))?,
            "speed" => ort::value::Tensor::from_array(([1usize], vec![speed]))?,
        ])?;
        Ok(outputs[0].try_extract_tensor::<f32>()?.1.to_vec())
    }
}

/// Syllables in a phoneme string, for the pace check.
pub fn syllables(phonemes: &str) -> usize {
    phonemes.chars().filter(|c| VOWELS.contains(*c)).count()
}

/// The longest line, in tokens, that the model takes in one go.
pub fn fits(phonemes: &str) -> bool {
    phonemes.chars().count() <= MAX_TOKENS
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn month(word: &str) -> Option<&'static str> {
    let w = word.trim_end_matches('.').to_lowercase();
    if w.len() < 3 {
        return None;
    }
    MONTHS.iter().copied().find(|m| {
        let full = m.to_lowercase();
        full == w || (w.len() <= 4 && full.starts_with(&w) && (w.len() == 3 || w == "sept"))
    })
}

/// Rewrites what misaki reads wrongly: "0.1.1" (zero one one), "v2.3" (vee), "10:30" (unread),
/// "28 November" (twenty-eight), "Nov 28" (unread).
pub fn normalize(text: &str) -> String {
    static RULES: std::sync::OnceLock<[regex::Regex; 5]> = std::sync::OnceLock::new();
    let [version, dotted, time, day_month, month_day] = RULES.get_or_init(|| {
        let m = r"(Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|June?|July?|Aug(?:ust)?|Sep(?:t(?:ember)?)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\.?";
        [
            regex::Regex::new(r"\bv(\d+(?:\.\d+)+)\b").unwrap(),
            regex::Regex::new(r"\b\d+(?:\.\d+){2,}\b").unwrap(),
            regex::Regex::new(r"\b(\d{1,2}):(\d{2})\b").unwrap(),
            regex::Regex::new(&format!(r"\b(\d{{1,2}})(?:st|nd|rd|th)? {m}(?:\b|$)")).unwrap(),
            regex::Regex::new(&format!(r"\b{m} (\d{{1,2}})(?:st|nd|rd|th)?\b")).unwrap(),
        ]
    });
    let points = |s: &str| s.split('.').collect::<Vec<_>>().join(" point ");
    let text = version.replace_all(text, |c: &regex::Captures| format!("version {}", points(&c[1])));
    let text = dotted.replace_all(&text, |c: &regex::Captures| points(&c[0]));
    let text = time.replace_all(&text, |c: &regex::Captures| {
        let h: u32 = c[1].parse().unwrap_or(0);
        match &c[2] {
            "00" => format!("{h} o'clock"),
            m if m.starts_with('0') => format!("{h} oh {}", &m[1..]),
            m => format!("{h} {m}"),
        }
    });
    let text = day_month.replace_all(&text, |c: &regex::Captures| match (c[1].parse::<u32>(), month(&c[2])) {
        (Ok(d @ 1..=31), Some(m)) => format!("the {} of {m}", ordinal(d)),
        _ => c[0].to_string(),
    });
    let text = month_day.replace_all(&text, |c: &regex::Captures| match (month(&c[1]), c[2].parse::<u32>()) {
        (Some(m), Ok(d @ 1..=31)) => format!("{m} {}", ordinal(d)),
        _ => c[0].to_string(),
    });
    text.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_what_misaki_misreads() {
        assert_eq!(normalize("Version 0.1.1 is out"), "Version 0 point 1 point 1 is out");
        assert_eq!(normalize("since v2.3"), "since version 2 point 3");
        assert_eq!(normalize("it costs 3.5 baht"), "it costs 3.5 baht");
        assert_eq!(normalize("at 10:30, 9:05 and 09:00"), "at 10 30, 9 oh 5 and 9 o'clock");
        assert_eq!(normalize("on 28 November 2026"), "on the 28th of November 2026");
        assert_eq!(normalize("on 1 Nov."), "on the 1st of November");
        assert_eq!(normalize("on Nov 22, then March 3rd"), "on November 22nd, then March 3rd");
        assert_eq!(normalize("May I? On 12 May"), "May I? On the 12th of May");
        assert_eq!(normalize("Mark 3 items"), "Mark 3 items");
    }

    #[test]
    fn counts_syllables() {
        // "guests can now edit the name": ɡˈɛsts kæn nˈW ˈɛdət ðə nˈAm
        assert_eq!(syllables("ɡˈɛsts kæn nˈW ˈɛdət ðə nˈAm"), 7);
    }
}
