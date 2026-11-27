//! Thai text -> phonemes for the MakHam voice: FastThaiG2P (awslabs, Apache-2.0) ported from its
//! Python: normalize, tokenize (newmm with its dictionary), look each word up in its IPA table, fall
//! back to TLTK's rules for words it does not have, then map the IPA to Kokoro's phoneme set.

use super::newmm::{Trie, py_splitlines, word_tokenize};
use super::normalize::normalize;
use super::tltk::{Tltk, fallback_g2p};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct G2p {
    dict: Trie,
    words_th: Trie,
    ipa: HashMap<String, String>,
    tltk: Tltk,
}

impl G2p {
    /// `data`: FastThaiG2P's data folder (dict.txt, ipa.json, fallback/); `words_th`: PyThaiNLP's
    /// default word list (for maiyamok).
    pub fn load(data: &Path, words_th: &Path) -> Result<G2p> {
        let dict_text = std::fs::read_to_string(data.join("dict.txt")).context("reading dict.txt")?;
        let words: HashSet<&str> = py_splitlines(&dict_text).into_iter().filter(|w| !w.is_empty()).collect();
        let dict = Trie::new(words);
        let th_text = std::fs::read_to_string(words_th).context("reading words_th.txt")?;
        let th_text = th_text.strip_prefix('\u{feff}').unwrap_or(&th_text);
        let words_th = Trie::new(py_splitlines(th_text).into_iter().filter(|w| !w.is_empty()));
        let ipa: HashMap<String, String> = serde_json::from_str(&std::fs::read_to_string(data.join("ipa.json")).context("reading ipa.json")?)?;
        let tltk = Tltk::load(&data.join("fallback"))?;
        Ok(G2p { dict, words_th, ipa, tltk })
    }

    /// Thai text -> IPA: normalize -> tokenize -> dictionary -> fallback -> join.
    pub fn convert(&mut self, text: &str) -> Result<String> {
        let normalized = normalize(text, &self.words_th);
        let mut phonemes = vec![];
        for token in word_tokenize(&normalized, &self.dict) {
            if crate::util::py_strip(&token).is_empty() {
                continue;
            }
            match self.ipa.get(&token).filter(|i| !i.is_empty()) {
                Some(ipa) => phonemes.push(ipa.clone()),
                None => phonemes.push(fallback_g2p(&mut self.tltk, &token)?),
            }
        }
        Ok(phonemes.join(" "))
    }
}

/// FastThaiG2P's IPA -> Kokoro's phoneme vocabulary: slashes dropped, tones as one mark each
/// (high is ↑, an otherwise unused Kokoro token), affricates precomposed, unsupported combining
/// marks removed.
pub fn ipa_to_kokoro(ipa: &str) -> String {
    let words: Vec<&str> = crate::util::py_split(ipa).into_iter().filter(|p| p.starts_with('/')).map(|p| p.trim_matches('/')).collect();
    let mut ph = words.join(" ");
    if ph.is_empty() {
        ph = crate::util::py_strip(&ipa.replace('/', "")).to_string();
    }
    for (old, new) in [("t͡ɕʰ", "ʨʰ"), ("t͡ɕ", "ʨ")] {
        ph = ph.replace(old, new);
    }
    for (old, new) in [("˩˩˦", "↗"), ("˥˩", "↘"), ("˦˥", "↑"), ("˨˩", "↓"), ("˧", "→")] {
        ph = ph.replace(old, new);
    }
    for strip in ['\u{031A}', '\u{032F}', '\u{0361}'] {
        ph = ph.replace(strip, "");
    }
    ph.replace('g', "ɡ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kokoro_marks() {
        assert_eq!(ipa_to_kokoro("/sa˨˩.wat̚˨˩.diː˧/ /kʰrap̚˦˥/"), "sa↓.wat↓.diː→ kʰrap↑");
    }
}
