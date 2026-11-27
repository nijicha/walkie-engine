//! Text normalization before the G2P (FastThaiG2P's normalizer.py): numbers, abbreviations and
//! special characters to speakable Thai. In order: expand maiyamok, Thai numerals to Arabic,
//! emails, English abbreviations, units, symbols, times, phone numbers, letter-and-digit codes,
//! comma numbers, numbers, Thai abbreviations, residual Latin letters, hyphens.

use super::newmm::{Trie, word_tokenize};
use crate::util::{py_boundary_before, py_space, sub_where};
use regex::{Captures, Regex};
use std::sync::LazyLock;

const THAI_DIGITS: [&str; 10] = ["ศูนย์", "หนึ่ง", "สอง", "สาม", "สี่", "ห้า", "หก", "เจ็ด", "แปด", "เก้า"];

fn digit(c: char) -> &'static str {
    c.to_digit(10).map(|d| THAI_DIGITS[d as usize]).unwrap_or("")
}

fn letter(c: char) -> Option<&'static str> {
    const LETTERS: [&str; 26] = [
        "เอ", "บี", "ซี", "ดี", "อี", "เอฟ", "จี", "เอช", "ไอ", "เจ", "เค", "แอล", "เอ็ม", "เอ็น", "โอ", "พี", "คิว", "อาร์",
        "เอส", "ที", "ยู", "วี", "ดับเบิลยู", "เอ็กซ์", "วาย", "แซด",
    ];
    let u = c.to_ascii_uppercase();
    u.is_ascii_uppercase().then(|| LETTERS[(u as u8 - b'A') as usize])
}

// Dictionaries in the order Python's dicts keep them: the alternations are sorted by length,
// longest first, and equal lengths keep this order.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("ม.ค.", "มกราคม"), ("ก.พ.", "กุมภาพันธ์"), ("มี.ค.", "มีนาคม"), ("เม.ย.", "เมษายน"), ("พ.ค.", "พฤษภาคม"), ("มิ.ย.", "มิถุนายน"),
    ("ก.ค.", "กรกฎาคม"), ("ส.ค.", "สิงหาคม"), ("ก.ย.", "กันยายน"), ("ต.ค.", "ตุลาคม"), ("พ.ย.", "พฤศจิกายน"), ("ธ.ค.", "ธันวาคม"),
    ("พ.ศ.", "พุทธศักราช"), ("ค.ศ.", "คริสต์ศักราช"),
    ("น.ส.", "นางสาว"), ("นส.", "นางสาว"), ("ดร.", "ด็อกเตอร์"), ("ผศ.", "ผู้ช่วยศาสตราจารย์"), ("รศ.", "รองศาสตราจารย์"),
    ("นพ.", "นายแพทย์"), ("พญ.", "แพทย์หญิง"), ("ทพ.", "ทันตแพทย์"), ("ทพญ.", "ทันตแพทย์หญิง"), ("น.สพ.", "นายสััตวแพทย์"),
    ("สพ.ญ.", "สัตวแพทย์หญิง"), ("ภก.", "เภสัชกร"), ("ภญ.", "เภสัชกรหญิง"), ("ทนพ.", "เทคนิคการแพทย์"), ("กภ.", "กายภาพบำบัด"),
    ("พล.อ.", "พลเอก"), ("พล.ท.", "พลโท"), ("พล.ต.", "พลตรี"), ("พ.อ.", "พันเอก"), ("พ.ท.", "พันโท"), ("พ.ต.", "พันตรี"),
    ("ร.อ.", "ร้อยเอก"), ("ร.ท.", "ร้อยโท"), ("ร.ต.", "ร้อยตรี"),
    ("จ.ส.อ.", "จ่าสิบเอก"), ("จ.ส.ท.", "จ่าสิบโท"), ("จ.ส.ต.", "จ่าสิบตรี"), ("ส.อ.", "สิบเอก"), ("ส.ท.", "สิบโท"), ("ส.ต.", "สิบตรี"),
    ("พล.ต.อ.", "พลตำรวจเอก"), ("พล.ต.ท.", "พลตำรวจโท"), ("พล.ต.ต.", "พลตำรวจตรี"), ("พ.ต.อ.", "พันตำรวจเอก"), ("พ.ต.ท.", "พันตำรวจโท"),
    ("พ.ต.ต.", "พันตำรวจตรี"), ("ร.ต.อ.", "ร้อยตำรวจเอก"), ("ร.ต.ท.", "ร้อยตำรวจโท"), ("ร.ต.ต.", "ร้อยตำรวจตรี"), ("ด.ต.", "ดาบตำรวจ"),
    ("พล.ร.อ.", "พลเรือเอก"), ("พล.ร.ท.", "พลเรือโท"), ("พล.ร.ต.", "พลเรือตรี"), ("น.อ.", "นาวาเอก"), ("น.ท.", "นาวาโท"), ("น.ต.", "นาวาตรี"),
    ("พ.จ.อ.", "พันจ่าเอก"), ("พ.จ.ท.", "พันจ่าโท"), ("พ.จ.ต.", "พันจ่าตรี"), ("จ.อ.", "จ่าเอก"), ("จ.ท.", "จ่าโท"), ("จ.ต.", "จ่าตรี"),
    ("พล.อ.อ.", "พลอากาศเอก"), ("พล.อ.ท.", "พลอากาศโท"), ("พล.อ.ต.", "พลอากาศตรี"), ("พ.อ.อ.", "พันจ่าอากาศเอก"), ("พ.อ.ท.", "พันจ่าอากาศโท"),
    ("พ.อ.ต.", "พันจ่าอากาศตรี"),
    ("ม.จ.", "หม่อมเจ้า"), ("ม.ร.ว.", "หม่อมราชวงศ์"), ("ม.ล.", "หม่อมหลวง"),
    ("กทม.", "กรุงเทพมหานคร"), ("รร.", "โรงเรียน"), ("ร.ร.", "โรงเรียน"), ("รพ.", "โรงพยาบาล"), ("ร.พ.", "โรงพยาบาล"), ("บจก.", "บริษัทจำกัด"),
    ("ฯลฯ", "เป็นต้น"),
];

const SYMBOLS: &[(&str, &str)] = &[("%", "เปอร์เซ็นต์"), ("°C", "องศาเซลเซียส"), ("°F", "องศาฟาเรนไฮต์"), ("°", "องศา"), ("@", " แอท "), ("/", " ทับ ")];

const ENGLISH_ABBREVS: &[(&str, &str)] = &[
    ("thb", "บาท"), ("usd", "ดอลลาร์"), ("eur", "ยูโร"), ("vat", "แวต"), ("pin", "พิน"),
    ("ram", "แรม"), ("wifi", "ไวไฟ"), ("otp", "โอทีพี"), ("covid", "โควิด"), ("fifa", "ฟีฟ่า"),
    ("line", "ไลน์"), ("facebook", "เฟซบุ๊ก"), ("instagram", "อินสตาแกรม"), ("amazon", "อมาซอน"), ("twitter", "ทวิตเตอร์"),
    ("google", "กูเกิล"), ("youtube", "ยูทูบ"), ("tiktok", "ติ๊กต็อก"), ("gmail", "จีเมล"), ("hotmail", "ฮอตเมล"),
    ("email", "อีเมล"), ("com", "คอม"), ("net", "เน็ต"), ("app", "แอป"), ("lazada", "ลาซาด้า"), ("shopee", "ช้อปปี้"),
    ("grab", "แกร็บ"), ("uber", "อูเบอร์"), ("whatsapp", "วอทส์แอป"), ("paypal", "เพย์พาล"), ("promptpay", "พร้อมเพย์"),
    ("truemoney", "ทรูมันนี่"),
];

const UNITS: &[(&str, &str)] = &[
    ("km", "กิโลเมตร"), ("cm", "เซนติเมตร"), ("mm", "มิลลิเมตร"), ("ml", "มิลลิลิตร"), ("kwh", "กิโลวัตต์ชั่วโมง"), ("kw", "กิโลวัตต์"),
    ("mb", "เมกะไบต์"), ("gb", "กิกะไบต์"), ("tb", "เทราไบต์"), ("kb", "กิโลไบต์"), ("mbps", "เมกะบิตต่อวินาที"),
    ("กม.", "กิโลเมตร"), ("ซม.", "เซนติเมตร"), ("มม.", "มิลลิเมตร"),
    ("ตร.กม.", "ตารางกิโลเมตร"), ("ตร.ม.", "ตารางเมตร"), ("ตร.ซม.", "ตารางเซนติเมตร"), ("ตร.มม.", "ตารางมิลลิเมตร"), ("ตร.ว.", "ตารางวา"),
    ("ลบ.ม.", "ลูกบาศก์เมตร"), ("ลบ.ซม.", "ลูกบาศก์เซนติเมตร"), ("มล.", "มิลลิลิตร"), ("กล.", "กิโลลิตร"),
    ("กก.", "กิโลกรัม"), ("มก.", "มิลลิกรัม"), ("kg", "กิโลกรัม"), ("mg", "มิลลิกรัม"),
];

const EMAIL_WORDS: &[(&str, &str)] = &[
    ("gmail", "จีเมล"), ("hotmail", "ฮอตเมล"), ("yahoo", "ยาฮู"), ("outlook", "เอาต์ลุก"), ("com", "คอม"), ("net", "เน็ต"),
    ("org", "ออร์ก"), ("mail", "เมล"), ("email", "อีเมล"),
];

fn lookup(table: &[(&str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// An alternation of the keys, longest (in characters) first.
fn alternation(table: &[(&str, &str)]) -> String {
    let mut keys: Vec<&str> = table.iter().map(|(k, _)| *k).collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.chars().count()));
    keys.iter().map(|k| regex::escape(k)).collect::<Vec<_>>().join("|")
}

static SYMBOL: LazyLock<Regex> = LazyLock::new(|| Regex::new(&alternation(SYMBOLS)).unwrap());
// Python: \b(...) with IGNORECASE; the \b is checked by hand (Python's \w is not Rust's).
static ENGLISH: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("(?i)({})", alternation(ENGLISH_ABBREVS))).unwrap());
// Python: (?<=\d)\s*(...); the lookbehind is checked by hand.
static UNIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\s*({})", alternation(UNITS))).unwrap());
static TIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{1,2}):(\d{2})(?:\s*(?:นาฬิกา|น\.))?").unwrap());
static EMAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9._-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap());
static PHONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{2,4}[-\.]\d{3,4}[-\.]\d{3,4}").unwrap());
static ALPHANUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z]+[\-]?\d+[\-\dA-Za-z]*|\d+[\-]?[A-Za-z]+[\-\dA-Za-z]*").unwrap());
static COMMA_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-?\d{1,3}(?:,\d{3})+(?:\.\d+)?").unwrap());
static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-?\d+(?:\.\d+)?").unwrap());
static ABBREV: LazyLock<Regex> = LazyLock::new(|| Regex::new(&alternation(ABBREVIATIONS)).unwrap());
static LATIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z]+").unwrap());
static DIGIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d$").unwrap());

fn number_group_to_thai(n: u128) -> String {
    let mut parts = String::new();
    let mut remaining = n;
    for place in [100000u128, 10000, 1000, 100, 10, 1] {
        let d = (remaining / place) as usize;
        remaining %= place;
        if d == 0 {
            continue;
        }
        match place {
            1 => parts.push_str(if n > 1 && d == 1 { "เอ็ด" } else { THAI_DIGITS[d] }),
            10 => match d {
                1 => parts.push_str("สิบ"),
                2 => parts.push_str("ยี่สิบ"),
                _ => parts.push_str(&format!("{}สิบ", THAI_DIGITS[d])),
            },
            100 => parts.push_str(&format!("{}ร้อย", THAI_DIGITS[d])),
            1000 => parts.push_str(&format!("{}พัน", THAI_DIGITS[d])),
            10000 => parts.push_str(&format!("{}หมื่น", THAI_DIGITS[d])),
            _ => parts.push_str(&format!("{}แสน", THAI_DIGITS[d])),
        }
    }
    parts
}

fn integer_to_thai(n: i128) -> String {
    if n == 0 {
        return "ศูนย์".into();
    }
    if n < 0 {
        return format!("ลบ{}", integer_to_thai(-n));
    }
    let mut n = n as u128;
    let mut parts = vec![];
    let mut millions = 0;
    while n > 0 {
        let group = n % 1_000_000;
        n /= 1_000_000;
        if group > 0 {
            parts.push(number_group_to_thai(group) + &"ล้าน".repeat(millions));
        }
        millions += 1;
    }
    parts.reverse();
    parts.concat()
}

/// Python's int() of a string of digits with an optional minus sign.
fn py_int(s: &str) -> i128 {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let v = digits.chars().filter_map(|c| c.to_digit(10)).fold(0i128, |a, d| a.saturating_mul(10).saturating_add(d as i128));
    if neg { -v } else { v }
}

fn digits_to_thai(digits: &str) -> String {
    digits.chars().map(digit).collect()
}

fn decimal_to_thai(text: &str) -> String {
    match text.split_once('.') {
        Some((int_part, dec)) => {
            let int_part = if int_part.is_empty() { "0" } else { int_part };
            format!("{}จุด{}", integer_to_thai(py_int(int_part)), digits_to_thai(dec))
        }
        None => integer_to_thai(py_int(text)),
    }
}

fn is_digit(c: char) -> bool {
    let mut b = [0u8; 4];
    DIGIT.is_match(c.encode_utf8(&mut b))
}

/// somchai.w@gmail.com -> Thai readout with แอท/ดอท.
fn email_to_thai(c: &Captures) -> String {
    let mut out = String::new();
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if !token.is_empty() {
            match lookup(EMAIL_WORDS, &token.to_lowercase()) {
                Some(w) => out.push_str(w),
                None => {
                    for ch in token.chars() {
                        if is_digit(ch) {
                            out.push_str(digit(ch));
                        } else {
                            match ch.to_uppercase().next().and_then(letter).filter(|_| ch.is_ascii_alphabetic()) {
                                Some(l) => out.push_str(l),
                                None => out.push(ch),
                            }
                        }
                    }
                }
            }
            token.clear();
        }
    };
    for ch in c[0].chars() {
        let sep = match ch {
            '@' => Some(" แอท "),
            '.' => Some(" ดอท "),
            '-' => Some(" ขีด "),
            '_' => Some(" ขีดล่าง "),
            _ => None,
        };
        match sep {
            Some(s) => {
                flush(&mut token, &mut out);
                out.push_str(s);
            }
            None => token.push(ch),
        }
    }
    flush(&mut token, &mut out);
    out
}

fn time_to_thai(c: &Captures) -> String {
    let (h, m) = (py_int(&c[1]), py_int(&c[2]));
    let mut out = integer_to_thai(h) + "นาฬิกา";
    if m != 0 {
        out += &(integer_to_thai(m) + "นาที");
    }
    out
}

fn alphanum_to_thai(c: &Captures) -> String {
    c[0].chars()
        .map(|ch| {
            if is_digit(ch) {
                digit(ch).to_string()
            } else if let Some(l) = letter(ch).filter(|_| ch.is_ascii_alphabetic()) {
                l.to_string()
            } else if ch == '-' {
                " ".to_string()
            } else {
                ch.to_string()
            }
        })
        .collect()
}

fn number_or_id_to_thai(c: &Captures) -> String {
    let text = &c[0];
    let raw = text.trim_start_matches('-');
    let negative = text.starts_with('-');
    let integer_part = raw.split('.').next().unwrap_or(raw);
    if integer_part.chars().count() >= 7 {
        // Read digit by digit. (Python's first attempt here raises on a decimal point; what it
        // means to say is this.)
        let mut result = match raw.split_once('.') {
            Some((i, d)) => format!("{}จุด{}", digits_to_thai(i), digits_to_thai(d)),
            None => digits_to_thai(integer_part),
        };
        if negative {
            result = format!("ลบ{result}");
        }
        return result;
    }
    decimal_to_thai(text)
}

/// pythainlp.util.expand_maiyamok on a string: tokenized with PyThaiNLP's own dictionary, each ๆ
/// repeats the word before it.
fn expand_maiyamok(text: &str, words_th: &Trie) -> String {
    let mut toks: Vec<String> = vec![];
    for token in word_tokenize(text, words_th) {
        // re.split("(ๆ)") without empty parts
        let mut cur = String::new();
        for ch in token.chars() {
            if ch == 'ๆ' {
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
                toks.push("ๆ".into());
            } else {
                cur.push(ch);
            }
        }
        if !cur.is_empty() {
            toks.push(cur);
        }
    }
    let mut out: Vec<String> = vec![];
    let mut yamok = 0;
    for tok in toks.into_iter().rev() {
        if tok == "ๆ" {
            yamok += 1;
        } else if yamok == 0 {
            out.push(tok);
        } else if !tok.chars().all(py_space) {
            // drop space before Maiyamok
            for _ in 0..=yamok {
                out.push(tok.clone());
            }
            yamok = 0;
        }
    }
    out.reverse();
    out.concat()
}

fn sub(re: &Regex, text: &str, rep: impl Fn(&Captures) -> String) -> String {
    re.replace_all(text, |c: &Captures| rep(c)).into_owned()
}

pub fn normalize(text: &str, words_th: &Trie) -> String {
    let mut t = if text.contains('ๆ') { expand_maiyamok(text, words_th) } else { text.to_string() };
    t = t.chars().map(|c| if ('๐'..='๙').contains(&c) { char::from(b'0' + (c as u32 - '๐' as u32) as u8) } else { c }).collect();
    t = sub(&EMAIL, &t, email_to_thai);
    t = sub_where(&ENGLISH, &t, py_boundary_before, |c| lookup(ENGLISH_ABBREVS, &c[0].to_lowercase()).unwrap_or_default().to_string());
    t = sub_where(&UNIT, &t, |s, at| s[..at].chars().next_back().is_some_and(is_digit), |c| lookup(UNITS, &c[1]).unwrap_or_default().to_string());
    t = sub(&SYMBOL, &t, |c| lookup(SYMBOLS, &c[0]).unwrap_or_default().to_string());
    t = sub_where(&TIME, &t, py_boundary_before, time_to_thai);
    t = sub(&PHONE, &t, |c| c[0].split(['-', '.']).map(digits_to_thai).collect::<Vec<_>>().join(" "));
    t = sub(&ALPHANUM, &t, alphanum_to_thai);
    t = sub(&COMMA_NUMBER, &t, |c| decimal_to_thai(&c[0].replace(',', "")));
    t = sub(&NUMBER, &t, number_or_id_to_thai);
    t = sub(&ABBREV, &t, |c| lookup(ABBREVIATIONS, &c[0]).unwrap_or_default().to_string());
    t = sub(&LATIN, &t, |c| c[0].chars().filter_map(letter).collect());
    // leftover hyphens (ชื่อ-นามสกุล) — connectors, not speakable tokens
    t.replace('-', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(integer_to_thai(21), "ยี่สิบเอ็ด");
        assert_eq!(integer_to_thai(101), "หนึ่งร้อยเอ็ด");
        assert_eq!(integer_to_thai(1), "หนึ่ง");
        assert_eq!(integer_to_thai(2_000_001), "สองล้านหนึ่ง");
        assert_eq!(decimal_to_thai("3.14"), "สามจุดหนึ่งสี่");
    }
}
