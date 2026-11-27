//! Shared helpers: the engine folder, running commands, JSON files, and the few places where
//! Python's string rules differ from Rust's (the Thai pipeline was written against Python's).

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The engine folder (the one with profiles/ and scripts/). WALKIE_ENGINE overrides it;
/// otherwise it is found above the binary (engine/bin/wt, or engine/wt/target/*/wt in a build).
pub fn engine_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("WALKIE_ENGINE") {
        return Ok(PathBuf::from(dir));
    }
    let exe = std::env::current_exe()?.canonicalize()?;
    exe.ancestors()
        .find(|d| d.join("profiles/default/profile.json").exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("cannot find the engine folder above {}: set WALKIE_ENGINE", exe.display()))
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
}

pub fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// JSON as Python's json.dumps(indent=1) writes it.
pub fn to_json_indent1(v: &Value) -> String {
    use serde::Serialize;
    let mut out = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
    v.serialize(&mut ser).expect("JSON serializes");
    String::from_utf8(out).expect("JSON is UTF-8")
}

pub fn write_json(path: &Path, v: &Value) -> Result<()> {
    std::fs::write(path, to_json_indent1(v)).with_context(|| format!("writing {}", path.display()))
}

/// Python's str() of a JSON number or string, for building ffmpeg expressions.
pub fn pystr(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// Runs a command and returns its stdout; fails with its stderr when it exits non-zero.
pub fn output(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd).args(args).stdin(Stdio::null()).output().with_context(|| format!("running {cmd}"))?;
    if !out.status.success() {
        bail!("{cmd} {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Like output(), but a failure is fine (returns what it printed).
pub fn output_lax(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Runs a command with its output passed through; fails when it exits non-zero.
pub fn run(cmd: &str, args: &[String]) -> Result<()> {
    let status = Command::new(cmd).args(args).stdin(Stdio::null()).status().with_context(|| format!("running {cmd}"))?;
    if !status.success() {
        bail!("{cmd} failed ({status})");
    }
    Ok(())
}

pub fn which(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(cmd).is_file()))
        .unwrap_or(false)
}

/// Python's str.isspace() for one character (it also counts the separators \x1c-\x1f).
pub fn py_space(c: char) -> bool {
    matches!(c, '\t'..='\r' | '\x1c'..='\x1f' | ' ' | '\u{85}' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
        | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

/// Python's str.split() with no arguments.
pub fn py_split(s: &str) -> Vec<&str> {
    s.split(py_space).filter(|p| !p.is_empty()).collect()
}

pub fn py_strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

/// A character Python's \w matches (str patterns): letters, numbers, underscore. Unlike Rust's
/// \w, Thai vowel and tone marks (Mn) are not word characters.
pub fn py_word(c: char) -> bool {
    use std::sync::LazyLock;
    static W: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"^[\p{L}\p{N}_]$").unwrap());
    let mut buf = [0u8; 4];
    W.is_match(c.encode_utf8(&mut buf))
}

/// re.sub with a condition Rust's regex cannot express (a Python \b or lookbehind before the
/// match): a match whose start fails `ok` is skipped and the search goes on one character later.
pub fn sub_where(re: &regex::Regex, text: &str, ok: impl Fn(&str, usize) -> bool, rep: impl Fn(&regex::Captures) -> String) -> String {
    let mut out = String::new();
    let mut last = 0;
    let mut at = 0;
    while at <= text.len() {
        let Some(c) = re.captures_at(text, at) else { break };
        let m = c.get(0).unwrap();
        if !ok(text, m.start()) {
            at = m.start() + text[m.start()..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        out.push_str(&text[last..m.start()]);
        out.push_str(&rep(&c));
        last = m.end();
        at = if m.end() > m.start() { m.end() } else { m.end() + 1 };
    }
    out.push_str(&text[last..]);
    out
}

/// Python \b before a match that starts with a word character: no word character before it.
pub fn py_boundary_before(text: &str, start: usize) -> bool {
    text[..start].chars().next_back().is_none_or(|c| !py_word(c))
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A Vec<String> from anything printable: `sv!["-i", path.display(), format!("{t:.2}")]`.
#[macro_export]
macro_rules! sv {
    ($($x:expr),* $(,)?) => { vec![$($x.to_string()),*] };
}
