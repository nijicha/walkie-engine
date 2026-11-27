//! Rule-based G2P for words not in the dictionary: TLTK 1.3.8's g2p (Wirote Aroonmanakun, Dept. of
//! Linguistics, Chulalongkorn University; BSD-3-Clause) as FastThaiG2P vendors it in _th2ipa.py.
//! Ported line by line, quirks included, so it gives the same syllables and tones:
//!   - syllables come from the patterns in sylrule.lts and thaisyl.dict, segmented by trigram
//!     statistics (sylseg.3g), and their readings are picked by PhSTrigram.sts;
//!   - word segmentation (to pick readings in context) updates the collocation counts as it goes,
//!     as the Python does, so a run's later words see the earlier ones' counts.

use super::pickle;
use anyhow::{Context, Result, anyhow, bail};
use indexmap::IndexMap;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

const SEG: &str = "\u{1f}"; // SegSep
const SSEG: &str = "\u{1e}"; // SSegSep
const SYL_SEP: &str = "~";
const WORD_SEP: &str = "|";

/// Python's \s for str patterns, plus \t and \xa0 (the class [\s\t ]).
const WS: &str = r"\t\n\x0B\x0C\r\x1C-\x1F \x{85}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}";
const MARKS: &str = "ุูึัี๊้็่๋ิื์";

fn key2(a: &str, b: &str) -> String {
    format!("{a}\0{b}")
}
fn key3(a: &str, b: &str, c: &str) -> String {
    format!("{a}\0{b}\0{c}")
}
fn key4(a: &str, b: &str, c: &str, d: &str) -> String {
    format!("{a}\0{b}\0{c}\0{d}")
}

struct Pattern {
    re: Regex,
    groups: usize,
    prons: Vec<String>,
}

pub struct Tltk {
    patterns: Vec<Pattern>,
    tdict: HashSet<String>,
    tri: HashMap<String, i64>,
    bi_count: HashMap<String, i64>,
    bi_type: HashMap<String, i64>,
    count: HashMap<String, i64>,
    typ: HashMap<String, i64>,
    total_word: i64,
    total_lex: i64,
    ph_tri: HashMap<String, f64>,
    frm_tri: HashMap<String, f64>,
    ph_bi: HashMap<String, f64>,
    frm_bi: HashMap<String, f64>,
    ph_uni: HashMap<String, f64>,
    frm_uni: HashMap<String, f64>,
    abs_uni: HashMap<String, f64>,
    abs_frm_uni: HashMap<String, f64>,
}

fn cp874(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(encoding_rs::WINDOWS_874.decode(&bytes).0.into_owned())
}

/// Python's `for line in file`: lines with their newline.
fn file_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split_inclusive('\n')
}

static TONE_MARKS: LazyLock<Regex> = LazyLock::new(|| Regex::new("[่้๊๋]").unwrap());
static CONSONANT: LazyLock<Regex> = LazyLock::new(|| Regex::new("[ก-ฮ]").unwrap());
static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new("[0-9]").unwrap());
static NOT_VOWEL: LazyLock<Regex> = LazyLock::new(|| Regex::new("[^aeio@OuxU]").unwrap());

fn abs_form(w: &str) -> String {
    CONSONANT.replace_all(&TONE_MARKS.replace_all(w, ""), "C").into_owned()
}
fn abs_phone(p: &str) -> String {
    NOT_VOWEL.replace_all(&DIGITS.replace_all(p, ""), "C").into_owned()
}

fn stable(code: char, ch: &str) -> Option<&'static str> {
    const X: &[(&str, &str)] = &[
        ("ก", "k"), ("ข", "kh"), ("ฃ", "kh"), ("ค", "kh"), ("ฅ", "kh"), ("ฆ", "kh"), ("ง", "N"), ("จ", "c"), ("ฉ", "ch"), ("ช", "ch"),
        ("ซ", "s"), ("ฌ", "ch"), ("ญ", "j"), ("ฎ", "d"), ("ฏ", "t"), ("ฐ", "th"), ("ฑ", "th"), ("ฒ", "th"), ("ณ", "n"), ("ด", "d"),
        ("ต", "t"), ("ถ", "th"), ("ท", "th"), ("ธ", "th"), ("น", "n"), ("บ", "b"), ("ป", "p"), ("ผ", "ph"), ("ฝ", "f"), ("พ", "ph"),
        ("ฟ", "f"), ("ภ", "ph"), ("ม", "m"), ("ย", "j"), ("ร", "r"), ("ฤ", "r"), ("ล", "l"), ("ฦ", "l"), ("ว", "w"), ("ศ", "s"),
        ("ษ", "s"), ("ส", "s"), ("ห", "h"), ("ฬ", "l"), ("อ", "?"), ("ฮ", "h"),
    ];
    const Y: &[(&str, &str)] = &[
        ("ก", "k"), ("ข", "k"), ("ค", "k"), ("ฆ", "k"), ("ง", "N"), ("จ", "t"), ("ฉ", "-"), ("ช", "t"), ("ซ", "t"), ("ฌ", "-"),
        ("ญ", "n"), ("ฎ", "t"), ("ฏ", "t"), ("ฐ", "t"), ("ฑ", "t"), ("ฒ", "t"), ("ณ", "n"), ("ด", "t"), ("ต", "t"), ("ถ", "t"),
        ("ท", "t"), ("ธ", "t"), ("น", "n"), ("บ", "p"), ("ป", "p"), ("ผ", "-"), ("ฝ", "-"), ("พ", "p"), ("ฟ", "p"), ("ภ", "p"),
        ("ม", "m"), ("ย", "j"), ("ร", "n"), ("ฤ", "-"), ("ล", "n"), ("ฦ", "-"), ("ว", "w"), ("ศ", "t"), ("ษ", "t"), ("ส", "t"),
        ("ห", "-"), ("ฬ", "n"), ("อ", "-"), ("ฮ", "-"),
    ];
    let table = match code {
        'X' | 'A' | 'K' | 'C' | 'R' | 'G' | 'E' => X,
        'D' | 'Z' | 'Y' | 'H' | 'F' => Y,
        _ => return None,
    };
    table.iter().find(|(k, _)| *k == ch).map(|(_, v)| *v)
}

impl Tltk {
    pub fn load(dir: &Path) -> Result<Tltk> {
        let mut pron: IndexMap<String, Vec<String>> = IndexMap::new();
        // read_sylpattern
        let subs: [(&str, &str); 10] = [
            ("X", "([ก-ฮ])"),
            ("C", "([กขคจดตทบปผพฟสศซ])"),
            ("Y", "([ก-ฬฮ])"),
            ("R", "([รลว])"),
            ("K", "([ก-ฮ])"),
            ("A", "([กจฆดตบปอขฉฐถผฝศษสหคชพภทธมยรลวนณซญฑฏฌ])"),
            ("Z", "([กงดนมบรลฎฏจตณถพศทสชคภปญ])"),
            ("D", "([กงดนมบวยต])"),
            ("W", "[ก-ฮ]"),
            (".", r"[\.]"),
        ];
        for line in file_lines(&cp874(&dir.join("sylrule.lts"))?) {
            if line.starts_with('#') {
                continue;
            }
            let line = line.trim_end_matches(crate::util::py_space);
            let tmp: Vec<&str> = line.split(',').collect();
            if tmp.len() < 3 {
                bail!("sylrule.lts: bad line {line:?}");
            }
            let mut p = tmp[0].to_string();
            for (from, to) in subs {
                p = p.replace(from, to);
            }
            p = p.replace('T', if tmp[2] == "T" { "[่้๊๋]" } else { "[่้๊๋]*" });
            pron.entry(p).or_default().push(tmp[1].to_string());
        }
        // read_syldict
        for line in file_lines(&cp874(&dir.join("thaisyl.dict"))?) {
            if line.starts_with('#') {
                continue;
            }
            let line = line.trim_end_matches(crate::util::py_space);
            let tmp: Vec<&str> = line.split('\t').collect();
            if tmp.len() < 2 {
                bail!("thaisyl.dict: bad line {line:?}");
            }
            pron.entry(tmp[0].to_string()).or_default().push(tmp[1].to_string());
        }
        let mut patterns = vec![];
        for (p, prons) in pron {
            let re = Regex::new(&format!("^(?:{p})")).with_context(|| format!("syllable pattern {p:?}"))?;
            let groups = re.captures_len() - 1;
            patterns.push(Pattern { re, groups, prons });
        }

        // read_stat: trigram counts of syllables
        let mut t = Tltk {
            patterns,
            tdict: HashSet::new(),
            tri: HashMap::new(),
            bi_count: HashMap::new(),
            bi_type: HashMap::new(),
            count: HashMap::new(),
            typ: HashMap::new(),
            total_word: 0,
            total_lex: 0,
            ph_tri: HashMap::new(),
            frm_tri: HashMap::new(),
            ph_bi: HashMap::new(),
            frm_bi: HashMap::new(),
            ph_uni: HashMap::new(),
            frm_uni: HashMap::new(),
            abs_uni: HashMap::new(),
            abs_frm_uni: HashMap::new(),
        };
        for (k, v) in pickle::load(&std::fs::read(dir.join("sylseg.3g"))?)? {
            let parts = pickle::as_tuple(&k).ok_or_else(|| anyhow!("sylseg.3g: key is not a tuple"))?;
            let s: Vec<&str> = parts.iter().filter_map(pickle::as_str).collect();
            let n = pickle::as_int(&v).unwrap_or(0);
            if s.len() != 3 {
                bail!("sylseg.3g: key is not 3 strings");
            }
            *t.tri.entry(key3(s[0], s[1], s[2])).or_default() += n;
            *t.bi_type.entry(key2(s[0], s[1])).or_default() += 1;
            *t.bi_count.entry(key2(s[0], s[1])).or_default() += n;
            *t.count.entry(s[1].to_string()).or_default() += n;
        }
        for k in t.bi_count.keys() {
            let x = k.split('\0').next().unwrap();
            *t.typ.entry(x.to_string()).or_default() += 1;
        }
        for c in t.count.values() {
            t.total_lex += 1;
            t.total_word += c;
        }
        // read_thdict
        for (k, _) in pickle::load(&std::fs::read(dir.join("thdict"))?)? {
            if let Some(s) = pickle::as_str(&k) {
                t.tdict.insert(s.to_string());
            }
        }
        // read_PhSTrigram
        for line in cp874(&dir.join("PhSTrigram.sts"))?.lines() {
            let line = line.trim_end_matches(crate::util::py_space).replace("<w>", "|");
            let Some((x, ct)) = line.split_once('\t') else { bail!("PhSTrigram.sts: bad line {line:?}") };
            let ct: f64 = ct.trim().parse().with_context(|| format!("PhSTrigram.sts: count {ct:?}"))?;
            let Some((fs, p)) = x.split_once('/') else { bail!("PhSTrigram.sts: bad line {line:?}") };
            let f: Vec<&str> = fs.split('-').collect();
            let [x1, x2, x3] = f[..] else { bail!("PhSTrigram.sts: bad forms {fs:?}") };
            *t.ph_tri.entry(key4(x1, x2, x3, p)).or_default() += ct;
            *t.frm_tri.entry(key3(x1, x2, x3)).or_default() += ct;
            *t.ph_bi.entry(key3(x1, x2, p)).or_default() += ct;
            *t.frm_bi.entry(key2(x1, x2)).or_default() += ct;
            *t.ph_bi.entry(key3(x2, x3, p)).or_default() += ct;
            *t.frm_bi.entry(key2(x2, x3)).or_default() += ct;
            *t.ph_uni.entry(key2(x2, p)).or_default() += ct;
            *t.frm_uni.entry(x2.to_string()).or_default() += ct;
            *t.abs_uni.entry(key2(&abs_form(x2), &abs_phone(p))).or_default() += ct;
            *t.abs_frm_uni.entry(abs_form(x2)).or_default() += ct;
        }
        Ok(t)
    }

    fn get(map: &HashMap<String, i64>, k: &str) -> i64 {
        map.get(k).copied().unwrap_or(0)
    }

    /// p(w | pw2 pw1), Witten-Bell smoothed trigram, as a log.
    fn prob_wb(&self, w: &str, pw1: &str, pw2: &str) -> f64 {
        let (mut p3, mut p2, mut p1) = (0.0, 0.0, 0.0);
        let tri = Self::get(&self.tri, &key3(pw2, pw1, w));
        if tri > 0 {
            let b = key2(pw2, pw1);
            p3 = tri as f64 / (Self::get(&self.bi_count, &b) + Self::get(&self.bi_type, &b)) as f64;
        }
        let bi = Self::get(&self.bi_count, &key2(pw1, w));
        if bi > 0 {
            p2 = bi as f64 / (Self::get(&self.count, pw1) + Self::get(&self.typ, pw1)) as f64;
        }
        let c = Self::get(&self.count, w);
        let total = self.total_word + self.total_lex;
        if c > 0 {
            p1 = c as f64 / total as f64;
        }
        let p = 0.8 * p3 + 0.15 * p2 + 0.04 * p1 + 1.0 / ((total as i128 * total as i128) as f64);
        p.ln()
    }

    fn prob_trisyl(&self, syls: &[String]) -> f64 {
        let (mut pw2, mut pw1) = (SEG, SEG);
        let mut probx = 1.0;
        for w in syls {
            probx += self.prob_wb(w, pw1, pw2);
            pw2 = pw1;
            pw1 = w;
        }
        probx
    }

    /// Collocation strength (mutual information) of two syllables. Counts a pair it has not seen,
    /// as the Python does; that changes later trigram probabilities too.
    fn compute_colloc(&mut self, w1: &str, w2: &str) -> f64 {
        let k = key2(w1, w2);
        if Self::get(&self.bi_count, &k) < 1 || Self::get(&self.count, w1) < 1 || Self::get(&self.count, w2) < 1 {
            *self.bi_count.entry(k.clone()).or_default() += 1;
            *self.count.entry(w1.to_string()).or_default() += 1;
            *self.count.entry(w2.to_string()).or_default() += 1;
            self.total_word += 2;
        }
        let num = (Self::get(&self.bi_count, &k) as i128 * self.total_word as i128) as f64;
        let den = (Self::get(&self.count, w1) as i128 * Self::get(&self.count, w2) as i128) as f64;
        ((num / den).ln() / 2f64.ln()).abs()
    }

    /// Thai text -> "<forms><tr/><phones>|<s/>" as TLTK's g2p() returns it.
    pub fn g2p(&mut self, input: &str) -> Result<String> {
        let input = preprocess(input);
        let mut output = String::new();
        for s in input.split(SEG) {
            for inp in s.split(SSEG) {
                if inp.is_empty() {
                    continue;
                }
                let out = if inp.chars().next().is_some_and(|c| !('\u{0E01}'..='\u{0E4C}').contains(&c)) {
                    format!("{inp}<tr/>{inp}")
                } else {
                    let mut pronun: HashMap<String, Vec<String>> = HashMap::new();
                    let y = self.sylparse(inp, &mut pronun)?;
                    self.wordparse(&y, &pronun)
                };
                output.push_str(&out);
                output.push_str(WORD_SEP);
            }
            output.push_str("<s/>");
        }
        Ok(output)
    }

    /// Segment into syllables by the trigram statistics; only strings matching a syllable pattern
    /// become syllables. Fills `pronun` with every reading of each syllable.
    fn sylparse(&self, input: &str, pronun: &mut HashMap<String, Vec<String>>) -> Result<String> {
        let chars: Vec<char> = input.chars().collect();
        let n = chars.len();
        let offsets: Vec<usize> = input.char_indices().map(|(b, _)| b).chain([input.len()]).collect();
        let mut schart: HashMap<usize, IndexMap<usize, Vec<String>>> = HashMap::new();
        let mut prob_end: HashMap<(usize, usize), f64> = HashMap::new();
        let mut charmatch: Option<String> = None; // a Python local: kept from the last match that set it
        for pat in &self.patterns {
            for i in 0..n {
                let Some(caps) = pat.re.captures(&input[offsets[i]..]) else { continue };
                let keymatch = caps[0].to_string();
                let group = |g: usize| caps.get(g).map(|m| m.as_str().to_string()).ok_or_else(|| anyhow!("unmatched group in {:?}", pat.re.as_str()));
                match pat.groups {
                    0 => pronun.entry(keymatch.clone()).or_default().extend(pat.prons.iter().cloned()),
                    1 => charmatch = Some(group(1)?),
                    2 => charmatch = Some(format!("{} {}", group(1)?, group(2)?)),
                    _ => charmatch = Some(format!("{} {} {}", group(1)?, group(2)?, group(3)?)),
                }
                let k = i + keymatch.chars().count();
                schart.entry(i).or_default().insert(k, vec![keymatch.clone()]);
                for pron_f in &pat.prons {
                    let codematch: String = pron_f.chars().filter(|c| "AKYDZCRX".contains(*c)).collect();
                    if codematch.is_empty() {
                        continue;
                    }
                    let cm = charmatch.clone().ok_or_else(|| anyhow!("no characters for {pron_f:?}"))?;
                    let mut phone = replace_snd(pron_f, &codematch, &cm)?;
                    // NotExceptionSyl returns 1 or -1, and both are true in Python: always taken.
                    let (p, tone) = tone_assign(&keymatch, &phone, &codematch, &cm).ok_or_else(|| anyhow!("no tone for {keymatch:?}"))?;
                    phone = p;
                    if tone.as_str() < "5" {
                        phone = phone.replace('8', &tone);
                    }
                    phone = transform_syl(&keymatch, &phone);
                    let form = schart[&i][&k].concat();
                    pronun.entry(form.clone()).or_default().push(phone.clone());
                    if keymatch.starts_with("ทร") && phone.starts_with("thr") {
                        // gen more syllable ทร thr => s
                        pronun.entry(form).or_default().push(phone.replace("thr", "s"));
                    }
                    prob_end.insert((i, k), self.prob_trisyl(&schart[&i][&k]));
                }
            }
        }
        for j in 0..n {
            let old0 = schart.get(&0).cloned().unwrap_or_default();
            let Some(s1) = old0.get(&j).cloned() else { continue };
            let row: Vec<(usize, Vec<String>)> = if j == 0 { old0.clone() } else { schart.get(&j).cloned().unwrap_or_default() }.into_iter().collect();
            let mut new0 = old0.clone();
            for (k, s2) in row {
                let tmp = mergekaran1([s1.clone(), s2].concat(), pronun);
                if !old0.contains_key(&k) {
                    prob_end.insert((0, k), self.prob_trisyl(&tmp));
                    new0.insert(k, tmp);
                } else {
                    let p = self.prob_trisyl(&tmp);
                    if p > prob_end.get(&(0, k)).copied().unwrap_or(0.0) {
                        new0.insert(k, tmp);
                        prob_end.insert((0, k), p);
                    }
                }
            }
            schart.insert(0, new0);
        }
        Ok(match schart.get(&0).and_then(|r| r.get(&n)) {
            Some(syls) => syls.join(SYL_SEP),
            None => format!("<Fail>{input}</Fail>"),
        })
    }

    /// Word segmentation over syllables (maximum collocation), then the most likely reading of
    /// each syllable in its word.
    fn wordparse(&mut self, input: &str, pronun: &HashMap<String, Vec<String>>) -> String {
        static MAIYAMOK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(<tr/>|\|)([?a-zENOU0-9~'@^]+?)[|~]ๆ").unwrap());
        let inx = input.split(SEG).next().unwrap_or("");
        let syl: Vec<&str> = inx.split(SYL_SEP).collect();
        let n = syl.len();
        let mut chart: HashMap<usize, IndexMap<usize, Vec<String>>> = HashMap::new();
        let mut colloc: HashMap<(usize, usize), f64> = HashMap::new();
        for (i, s) in syl.iter().enumerate() {
            chart.entry(i).or_default().insert(i + 1, vec![s.to_string()]);
        }
        for i in 0..n {
            for j in i..=n {
                if !self.tdict.contains(&syl[i..j].concat()) {
                    continue;
                }
                chart.entry(i).or_default().insert(j, vec![syl[i..j].join(SYL_SEP)]);
                let mut st = 0.0;
                if j > i + 1 {
                    for ii in i..j - 1 {
                        st += self.compute_colloc(syl[ii], syl[ii + 1]);
                    }
                }
                colloc.insert((i, j), st);
            }
        }
        // chart_parse
        let cs = |c: &HashMap<(usize, usize), f64>, a: usize, b: usize| c.get(&(a, b)).copied().unwrap_or(0.0);
        for j in 0..n {
            let old0 = chart.get(&0).cloned().unwrap_or_default();
            let Some(s1) = old0.get(&j).cloned() else { continue };
            let row: Vec<(usize, Vec<String>)> = if j == 0 { old0.clone() } else { chart.get(&j).cloned().unwrap_or_default() }.into_iter().collect();
            let mut new0 = old0.clone();
            for (k, s2) in row {
                let both = cs(&colloc, 0, j) + cs(&colloc, j, k);
                if !old0.contains_key(&k) {
                    new0.insert(k, [s1.clone(), s2].concat());
                    colloc.insert((0, k), both);
                } else if both > cs(&colloc, 0, k) {
                    colloc.insert((0, k), both);
                    new0.insert(k, [s1.clone(), s2].concat());
                }
            }
            chart.insert(0, new0);
        }
        let Some(words) = chart.get(&0).and_then(|r| r.get(&n)) else {
            return format!("<Fail>{input}</Fail>");
        };
        let mut outx = words.join(WORD_SEP).replace("~ๆ", "|ๆ");
        outx.push_str("<tr/>");
        let phones: Vec<String> = words.iter().map(|wx| self.select_phones(&wx.split(SYL_SEP).collect::<Vec<_>>(), pronun)).collect();
        outx.push_str(&phones.join(WORD_SEP));
        MAIYAMOK.replace_all(&outx, "${1}${2}|${2}").into_owned()
    }

    fn select_phones(&self, slst: &[&str], pronun: &HashMap<String, Vec<String>>) -> String {
        let padded: Vec<&str> = [&["|"], slst, &["|"]].concat();
        let mut out = vec![];
        for i in 1..padded.len() - 1 {
            let prons = pronun.get(padded[i]).map(Vec::as_slice).unwrap_or(&[]);
            if prons.len() == 1 {
                out.push(prons[0].clone());
                continue;
            }
            let (mut outp, mut prmax) = (String::new(), 0.0);
            for p in prons {
                let pr = self.prob_phone(p, padded[i - 1], padded[i], padded[i + 1]);
                if pr > prmax || (pr == prmax && p.contains('\'') && p.chars().count() > outp.chars().count()) {
                    prmax = pr;
                    outp = p.clone();
                }
            }
            out.push(outp);
        }
        out.join(SYL_SEP)
    }

    fn prob_phone(&self, p: &str, pw: &str, w: &str, nw: &str) -> f64 {
        let f = |m: &HashMap<String, f64>, k: &str| m.get(k).copied().unwrap_or(0.0);
        let (mut p3, mut p2, mut p1, mut p0) = (0.0, 0.0, 0.0, 0.0);
        let t = f(&self.ph_tri, &key4(pw, w, nw, p));
        if t > 0.0 {
            p3 = (1.0 + t.ln()) / (1.0 + f(&self.frm_tri, &key3(pw, w, nw)).ln());
        }
        let b1 = f(&self.ph_bi, &key3(pw, w, p));
        if b1 > 0.0 {
            p2 = (1.0 + b1.ln()) / (1.0 + f(&self.frm_bi, &key2(pw, w)).ln()) * 0.25;
        }
        // the following syllable decides the linking sound: it weighs 3/4
        let b2 = f(&self.ph_bi, &key3(w, nw, p));
        if b2 > 0.0 {
            p2 += (1.0 + b2.ln()) / (1.0 + f(&self.frm_bi, &key2(w, nw)).ln()) * 0.75;
        }
        let u = f(&self.ph_uni, &key2(w, p));
        if u > 0.0 {
            p1 = (1.0 + u.ln()) / (1.0 + f(&self.frm_uni, w).ln());
        }
        let (aw, ap) = (abs_form(w), abs_phone(p));
        let a = f(&self.abs_uni, &key2(&aw, &ap));
        if a > 0.0 {
            p0 = (1.0 + a.ln()) / (1.0 + f(&self.abs_frm_uni, &aw).ln());
        }
        0.8 * p3 + 0.16 * p2 + 0.03 * p1 + 0.00001 * p0 + 0.00000000001
    }
}

/// Reconnect a karan (์) part to the syllable before it, with its readings.
fn mergekaran1(mut lst: Vec<String>, pronun: &mut HashMap<String, Vec<String>>) -> Vec<String> {
    static IU_KARAN: LazyLock<Regex> = LazyLock::new(|| Regex::new("(.+)[ิุ]์").unwrap());
    static KARAN: LazyLock<Regex> = LazyLock::new(|| Regex::new("(.+)์").unwrap());
    let mut mkaran: HashSet<(String, String)> = HashSet::new();
    let mut rs = vec![];
    let mut found = false;
    let mut x = String::new();
    lst.reverse();
    for s in lst {
        let short = s.chars().count() < 4;
        if IU_KARAN.is_match(&s) {
            if short {
                found = true;
                x = s;
                continue;
            }
        } else if KARAN.is_match(&s) && short {
            found = true;
            x = s;
            continue;
        }
        if found {
            let merged = format!("{s}{x}");
            for ph in pronun.get(&s).cloned().unwrap_or_default() {
                if mkaran.insert((merged.clone(), ph.clone())) {
                    pronun.entry(merged.clone()).or_default().push(ph);
                }
            }
            rs.push(merged);
            found = false;
        } else {
            rs.push(s);
        }
    }
    rs.reverse();
    rs
}

fn replace_snd(phone: &str, codematch: &str, charmatch: &str) -> Result<String> {
    let chars: Vec<&str> = charmatch.split(' ').collect();
    let mut snd = phone.to_string();
    for (i, x) in codematch.chars().enumerate() {
        let ch = chars.get(i).ok_or_else(|| anyhow!("no character {i} in {charmatch:?}"))?;
        let s = stable(x, ch).ok_or_else(|| anyhow!("no sound for {ch:?} as {x}"))?;
        snd = snd.replace(x, s);
    }
    snd.push('8');
    Ok(snd)
}

fn dead_syl(phone: &str) -> char {
    static NUM: LazyLock<Regex> = LazyLock::new(|| Regex::new("[0-4]").unwrap());
    static LIVE_END: LazyLock<Regex> = LazyLock::new(|| Regex::new("[mnwjlN]8?$").unwrap());
    static DEAD_END: LazyLock<Regex> = LazyLock::new(|| Regex::new("[pktfscC]8?$").unwrap());
    let inx = NUM.replace_all(&phone.replace("ch", "C"), "").into_owned();
    if LIVE_END.is_match(&inx) {
        'L'
    } else if DEAD_END.is_match(&inx) {
        'D'
    } else if long_vowel(&inx, true) {
        'L'
    } else {
        'D'
    }
}

/// `([aeiouxOU\@])\1` (same vowel twice) when `same`, else `[aeiouxOU\@][aeiouxOU\@]+`.
fn long_vowel(s: &str, same: bool) -> bool {
    let v: Vec<char> = s.chars().collect();
    let is_v = |c: char| "aeiouxOU@".contains(c);
    v.windows(2).any(|w| is_v(w[0]) && is_v(w[1]) && (!same || w[0] == w[1]))
}

/// Tone of a syllable from its consonant class, live or dead, vowel length and tone mark.
/// `None` where the Python returns None (and would then fail).
fn tone_assign(keymatch: &str, phone: &str, codematch: &str, charmatch: &str) -> Option<(String, String)> {
    static ASSIGNED: LazyLock<Regex> = LazyLock::new(|| Regex::new("[0-4]8").unwrap());
    static ASSIGNED_SUB: LazyLock<Regex> = LazyLock::new(|| Regex::new("([0-4])8").unwrap());
    if phone.is_empty() {
        return Some((String::new(), "9".into()));
    }
    if ASSIGNED.is_match(phone) {
        // tone is already assigned
        return Some((ASSIGNED_SUB.replace_all(phone, "$1").into_owned(), String::new()));
    }
    let lx: Vec<&str> = charmatch.split(' ').collect();
    let (mut lead, mut init) = ("", "");
    if codematch.contains('X') || codematch == "GH" || codematch == "EF" {
        init = lx[0];
    } else if codematch.contains("AK") || codematch.contains("CR") {
        lead = lx[0];
        if lx.len() > 1 {
            init = lx[1];
        }
    }
    let dead = dead_syl(phone);
    let mut phone = phone.to_string();
    // change + for leading syllable
    if phone.contains("+'") {
        let t = if "ผฝถขสหฉศษ".contains(lead) || "กจดตบปอ".contains(lead) { "1" } else { "3" };
        phone = phone.replace('+', t);
    }
    let mark = |c: char| keymatch.contains(c);
    // tone by mark: [none, ่, ้, ๊, ๋]
    let pick = |t: [&str; 5]| -> String {
        let s = if mark('\u{0E48}') {
            t[1]
        } else if mark('\u{0E49}') {
            t[2]
        } else if mark('\u{0E4A}') {
            t[3]
        } else if mark('\u{0E4B}') {
            t[4]
        } else {
            t[0]
        };
        s.to_string()
    };
    let live = dead == 'L';
    let high = "ขฃฉฐถผฝสศษห";
    let middle = "กจดตฎฏบปอ";
    let sonorant = "งญณนมยรลวฬ";
    let tone = if "กจดตฎฏบปอ".contains(init) {
        // middle consonant
        if live { pick(["0", "1", "2", "3", "4"]) } else { pick(["1", "9", "2", "3", "4"]) }
    } else if high.contains(init) {
        if live { pick(["4", "1", "2", "9", "9"]) } else { pick(["1", "9", "2", "9", "9"]) }
    } else if sonorant.contains(init) && !lead.is_empty() && high.contains(lead) {
        // low consonant led by a high one
        if live { pick(["4", "1", "2", "9", "9"]) } else { pick(["1", "9", "2", "9", "9"]) }
    } else if sonorant.contains(init) && !lead.is_empty() && middle.contains(lead) {
        // low consonant led by a middle one
        if live { pick(["0", "1", "2", "3", "4"]) } else { pick(["1", "9", "2", "3", "4"]) }
    } else if "คฅฆชฌซฑฒทธพภฟฮงญณนมยรลวฬฤฦ".contains(init) {
        // low consonant
        if live {
            pick(["0", "2", "3", "9", "9"])
        } else if long_vowel(&phone, false) {
            pick(["2", "9", "3", "9", "4"])
        } else {
            pick(["3", "2", "9", "9", "4"])
        }
    } else {
        return None;
    };
    Some((phone, tone))
}

fn transform_syl(form: &str, phone: &str) -> String {
    static XX: LazyLock<Regex> = LazyLock::new(|| Regex::new("xx[nmN][12]").unwrap());
    static EE: LazyLock<Regex> = LazyLock::new(|| Regex::new("ee[nmN][12]").unwrap());
    static OO: LazyLock<Regex> = LazyLock::new(|| Regex::new("@@[nmN][12]").unwrap());
    static AYA: LazyLock<Regex> = LazyLock::new(|| Regex::new("^อย่า$|^อยู่$|^อย่าง$|^อยาก$").unwrap());
    static HO_NOT: LazyLock<Regex> = LazyLock::new(|| Regex::new("หนุ$|หก|หท|หพ|หฤ|หโ").unwrap());
    static CLUSTER_FORM: LazyLock<Regex> = LazyLock::new(|| Regex::new("[จซศส]ร").unwrap());
    static CLUSTER_PHONE: LazyLock<Regex> = LazyLock::new(|| Regex::new("[cs]r").unwrap());
    let mut phone = phone.to_string();
    // xxY[12] eeY[12] @@Y[12] => short vowel, Y = [nmN]
    if XX.is_match(&phone) {
        phone = phone.replace("xx", "x");
    } else if EE.is_match(&phone) {
        phone = phone.replace("ee", "e");
    } else if OO.is_match(&phone) {
        // Python's re.sub(r'\@\@', '\@', ...) keeps the backslash: '\@' is not an escape.
        phone = phone.replace("@@", "\\@");
    }
    if AYA.is_match(form) && phone.contains('\'') {
        // อยxxx: "?a1'jxxx" -> "jxxx"
        phone = phone.rsplit('\'').next().unwrap_or("").to_string();
    } else if form.contains('ห') && phone.contains("ha1") && !HO_NOT.is_match(form) {
        // หxxx: "ha1'xxx" -> "xxx"
        phone = phone.rsplit('\'').next().unwrap_or("").to_string();
    } else if CLUSTER_FORM.is_match(form) && CLUSTER_PHONE.is_match(&phone) && phone.chars().any(|c| c != '\'') {
        // cluster: the "r" is not said
        phone = phone.replace('r', "");
    }
    phone
}

fn preprocess(input: &str) -> String {
    static RULES: LazyLock<Vec<(Regex, String)>> = LazyLock::new(|| {
        let ws = WS;
        let thai = format!("[ก-ฮเแาำะไใโฯๆ][{MARKS}]*");
        let r = |p: &str, rep: &str| (Regex::new(p).unwrap(), rep.to_string());
        vec![
            r(" +ๆ", "ๆ"),
            r("ๆ([^ ])", &format!("ๆ{SEG}${{1}}")),
            // handle Thai written one character, one space: delete the spaces
            r(&format!("({thai}) +({thai}) +|({thai}) +({thai})$"), "${1}${2}"),
            // spaces between longer chunks, before numbers, before longer chunks -> SegSep
            r(&format!("([^{ws}]{{3,}})[{ws}]+([^{ws}]+?)"), &format!("${{1}}{SEG}${{2}}")),
            r(&format!("([^{ws}]+)[{ws}]+([0-9]+)"), &format!("${{1}}{SEG}${{2}}")),
            r(&format!("([^{ws}]+?)[{ws}]+([^{ws}]{{3,}})"), &format!("${{1}}{SEG}${{2}}")),
            // English and Thai mixed without a space: SSegSep between them
            r("([ก-์][ฯๆ])", &format!("${{1}}{SSEG}")),
            r(r"([\x{0E01}-\x{0E5B}]+\.?)([^\.\x{0E01}-\x{0E5B}\x{1F}]+)", &format!("${{1}}{SSEG}${{2}}")),
            r(r"([^\.\x{0E01}-\x{0E5B}\x{1F}]+)([\x{0E01}-\x{0E5B}]+)", &format!("${{1}}{SSEG}${{2}}")),
            r("(<.+?>)", &format!("{SSEG}${{1}}")),
            r(r"([0-9a-zA-Z\.\-]{2,})([\x{0E01}-\x{0E5B}]+)", &format!("${{1}}{SSEG}${{2}}")),
            r(r"(\.\.\.+)", &format!("{SSEG}${{1}}{SSEG}")),
        ]
    });
    let mut s = input.to_string();
    for (i, (re, rep)) in RULES.iter().enumerate() {
        s = re.replace_all(&s, rep.as_str()).into_owned();
        if i == 1 {
            for (k, v) in [("\u{0E40}\u{0E40}", "\u{0E41}"), ("\u{0E4D}\u{0E32}", "\u{0E33}"), ("\u{0E24}\u{0E32}", "\u{0E24}\u{0E45}"), ("\u{0E26}\u{0E32}", "\u{0E26}\u{0E45}")] {
                s = s.replace(k, v);
            }
        }
    }
    s
}

/// FastThaiG2P's fallback_g2p: TLTK's phones for a word, in Wiktionary-style IPA (/syl.la.ble˧/).
pub fn fallback_g2p(tltk: &mut Tltk, word: &str) -> Result<String> {
    static PHON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<tr/>(.+?)\|<s/>").unwrap());
    static SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[|~^]").unwrap());
    let result = tltk.g2p(word)?;
    let Some(c) = PHON.captures(&result) else { return Ok(word.to_string()) };
    let mut syllables = vec![];
    for part in SPLIT.split(&c[1]) {
        for sp in part.split('\'') {
            let sp = crate::util::py_strip(sp);
            if !sp.is_empty() {
                syllables.push(syllable_to_ipa(sp));
            }
        }
    }
    Ok(if syllables.is_empty() { word.to_string() } else { format!("/{}/", syllables.join(".")) })
}

fn syllable_to_ipa(syl: &str) -> String {
    let (syl, digit) = match syl.chars().last() {
        Some(d @ '0'..='4') => (&syl[..syl.len() - 1], d),
        _ => (syl, '0'),
    };
    let tone = match digit {
        '1' => "˨˩",
        '2' => "˥˩",
        '3' => "˦˥",
        '4' => "˩˩˦",
        _ => "˧",
    };
    let mut result = syl.to_string();
    for (old, new) in [("kh", "kʰ"), ("ph", "pʰ"), ("th", "tʰ"), ("ch", "t͡ɕʰ"), ("c", "t͡ɕ"), ("N", "ŋ"), ("?", "ʔ")] {
        result = result.replace(old, new);
    }
    for (old, new) in [
        ("UUa", "ɯa̯"), ("Ua", "ɯa̯"), ("iia", "ia̯"), ("ia", "ia̯"), ("uua", "ua̯"), ("ua", "ua̯"),
        ("aa", "aː"), ("ii", "iː"), ("uu", "uː"), ("xx", "ɛː"), ("ee", "eː"), ("oo", "oː"), ("OO", "ɔː"), ("@@", "ɤː"), ("UU", "ɯː"),
        ("a", "a"), ("i", "i"), ("u", "u"), ("x", "ɛ"), ("e", "e"), ("o", "o"), ("O", "ɔ"), ("@", "ɤ"), ("U", "ɯ"),
    ] {
        result = result.replace(old, new);
    }
    // unreleased mark on final stops
    if let Some(last) = result.chars().last().filter(|c| "ktp".contains(*c)) {
        let head = &result[..result.len() - 1];
        if head.chars().any(|c| "aeiouɔɛɯɤː̯".contains(c)) {
            result = format!("{head}{last}\u{031A}");
        }
    }
    result + tone
}
