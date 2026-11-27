//! Pick the profile for this repo and write DEMO_DIR/profile.json, which every other step reads.
//!
//!   wt profile DEMO_DIR            (run from the repo root)
//!   wt profile --list              (every profile found, as JSON name -> folder)
//!   wt profile --resolve NAME      (that profile merged over default, as JSON; for the app)
//!   DEMO_PROFILE=name wt profile DEMO_DIR
//!
//! A profile is a folder with profile.json, theme.json, sequence.json and voice.json; its name is
//! the `name` in profile.json (or the folder name). Each file is merged over the same file in the
//! built-in profiles/default, so a profile only lists what it changes.
//!
//! Profiles are found, first match wins, in: each folder of WALKIE_PROFILES (colon-separated),
//! ~/.config/walkie/profiles, then the engine's profiles/. A team keeps its profile in its own
//! repo and clones (or links) it into ~/.config/walkie/profiles/<name>.
//!
//! Choice: DEMO_PROFILE if set, else the first profile whose `match` globs fit a git remote of the
//! repo, else `default`. A profile with an `allow` list refuses every repo not on that list, even
//! when DEMO_PROFILE names it. Remotes compare as host/owner/name, e.g. github.com/acme/shop.

use crate::util::{engine_dir, home, read_json, to_json_indent1};
use anyhow::{Result, anyhow, bail};
use indexmap::IndexMap;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

const PARTS: [&str; 4] = ["profile", "theme", "sequence", "voice"];

fn remotes() -> Vec<String> {
    let Ok(out) = std::process::Command::new("git").args(["remote", "-v"]).output() else { return vec![] };
    if !out.status.success() {
        return vec![];
    }
    let scheme = regex::Regex::new(r"^(\w+://)?([^@/]+@)?").unwrap();
    let scp = fancy_regex::Regex::new(r"^([^/:]+):(?!\d+/)").unwrap();
    let port = regex::Regex::new(r":\d+/").unwrap();
    let git = regex::Regex::new(r"\.git/?$").unwrap();
    let mut found: Vec<String> = vec![];
    for line in String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.trim().is_empty()) {
        let Some(url) = line.split_whitespace().nth(1) else { continue };
        let url = scheme.replace(url, "").into_owned(); // drop scheme and user
        let url = scp.replace(&url, "$1/").into_owned(); // scp form host:owner/name -> host/owner/name
        let url = port.replace_all(&url, "/").into_owned(); // drop a port
        let url = git.replace(&url, "").into_owned();
        if !found.contains(&url) {
            found.push(url);
        }
    }
    found
}

fn search_path(builtin: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var("WALKIE_PROFILES")
        .unwrap_or_default()
        .split(':')
        .filter(|d| !d.is_empty())
        .map(|d| match d.strip_prefix("~/") {
            Some(rest) => home().join(rest),
            None if d == "~" => home(),
            None => PathBuf::from(d),
        })
        .collect();
    dirs.push(home().join(".config/walkie/profiles"));
    dirs.push(builtin.to_path_buf());
    dirs
}

struct Profiles {
    skill: PathBuf,
    found: IndexMap<String, PathBuf>,
}

impl Profiles {
    /// Every profile on the search path, name -> folder; earlier folders win.
    fn scan() -> Result<Self> {
        let skill = engine_dir()?;
        let mut found = IndexMap::new();
        for base in search_path(&skill.join("profiles")) {
            let Ok(entries) = std::fs::read_dir(&base) else { continue };
            let mut folders: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
            folders.sort();
            for folder in folders {
                let meta = folder.join("profile.json");
                if !meta.exists() {
                    continue;
                }
                let name = read_json(&meta)?
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| crate::util::file_name(&folder));
                if !found.contains_key(&name) {
                    found.insert(name, folder.canonicalize()?);
                }
            }
        }
        Ok(Profiles { skill, found })
    }

    fn folder(&self, name: &str) -> Result<&PathBuf> {
        self.found.get(name).ok_or_else(|| {
            let dirs: Vec<String> = search_path(&self.skill.join("profiles")).iter().map(|d| d.display().to_string()).collect();
            anyhow!("no profile named {name:?} in {}", dirs.join(", "))
        })
    }

    fn load(&self, name: &str) -> Result<Map<String, Value>> {
        let folder = self.folder(name)?;
        let mut out = Map::new();
        for part in PARTS {
            let file = folder.join(format!("{part}.json"));
            out.insert(part.into(), if file.exists() { read_json(&file)? } else { json!({}) });
        }
        Ok(out)
    }

    /// The profile merged over default, with font, image and template paths made absolute.
    fn resolve(&self, name: &str) -> Result<Map<String, Value>> {
        let picked = Value::Object(self.load(name)?);
        let resolved = if name != "default" { merge(&Value::Object(self.load("default")?), &picked) } else { picked };
        let Value::Object(mut resolved) = resolved else { unreachable!() };
        let caskaydia = self.skill.join("fonts/caskaydia");
        let theme = resolved.entry("theme").or_insert_with(|| json!({}));
        let fonts = theme.as_object_mut().unwrap().entry("fonts").or_insert_with(|| json!({}));
        fonts.as_object_mut().unwrap().insert(
            "mono".into(),
            json!({
                "regular": caskaydia.join("CaskaydiaCoveNerdFontMono-Regular.woff2").display().to_string(),
                "bold": caskaydia.join("CaskaydiaCoveNerdFontMono-Bold.woff2").display().to_string(),
            }),
        );
        let folder = self.folder(name)?.clone();
        let default = self.folder("default")?.clone();
        let folders = [folder.clone(), default.clone()];
        let find = |rel: &str| folders.iter().map(|f| f.join(rel)).find(|p| p.exists());
        if let Some(Value::Object(sans)) = theme.pointer_mut("/fonts/sans") {
            for (_, rel) in sans.iter_mut() {
                let r = crate::util::pystr(rel);
                let hit = find(&r).ok_or_else(|| anyhow!("font not found: {r}"))?;
                *rel = Value::String(hit.display().to_string());
            }
        } else {
            bail!("theme has no fonts.sans");
        }
        if let Some(Value::Object(cards)) = theme.get_mut("cards") {
            for (_, st) in cards.iter_mut() {
                for key in ["logo", "background_image"] {
                    let Some(rel) = st.get(key).and_then(Value::as_str).filter(|r| !r.is_empty()).map(str::to_string) else { continue };
                    let hit = find(&rel).ok_or_else(|| anyhow!("card image not found: {rel}"))?;
                    st[key] = Value::String(hit.display().to_string());
                }
            }
        }
        let prof = resolved.get_mut("profile").and_then(Value::as_object_mut).ok_or_else(|| anyhow!("no profile.json"))?;
        let rel = prof.get("template").and_then(Value::as_str).unwrap_or_default().to_string();
        let template = [folder.join(&rel), default.join(&rel)]
            .into_iter()
            .find(|p| p.exists())
            .ok_or_else(|| anyhow!("template not found: {rel}"))?;
        prof.insert("name".into(), name.into());
        prof.insert("dir".into(), folder.display().to_string().into());
        prof.insert("template".into(), template.display().to_string().into());
        Ok(resolved)
    }
}

fn merge(base: &Value, over: &Value) -> Value {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            let mut out = Map::new();
            for (k, v) in b {
                out.insert(k.clone(), match o.get(k) {
                    Some(ov) => merge(v, ov),
                    None => v.clone(),
                });
            }
            for (k, v) in o {
                if !b.contains_key(k) {
                    out.insert(k.clone(), merge(&Value::Null, v));
                }
            }
            Value::Object(out)
        }
        (b, Value::Null) => b.clone(),
        (_, o) => o.clone(),
    }
}

/// Python's fnmatch.fnmatch on POSIX: * ? [seq] [!seq], the whole name.
fn fnmatch(name: &str, pat: &str) -> bool {
    let chars: Vec<char> = pat.chars().collect();
    let mut re = String::from("(?s)^");
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            '[' => {
                let mut j = i;
                if j < chars.len() && chars[j] == '!' {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ']' {
                    j += 1;
                }
                while j < chars.len() && chars[j] != ']' {
                    j += 1;
                }
                if j >= chars.len() {
                    re.push_str(r"\[");
                } else {
                    let mut set: String = chars[i..j].iter().collect::<String>().replace('\\', r"\\");
                    i = j + 1;
                    if let Some(rest) = set.strip_prefix('!') {
                        set = format!("^{rest}");
                    } else if set.starts_with('^') {
                        set = format!("\\{set}");
                    }
                    re.push('[');
                    re.push_str(&set.replace('[', r"\["));
                    re.push(']');
                }
            }
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    re.push('$');
    regex::Regex::new(&re).map(|r| r.is_match(name)).unwrap_or(false)
}

fn choose(p: &Profiles, repo: &[String]) -> Result<(String, String)> {
    if let Ok(name) = std::env::var("DEMO_PROFILE") {
        if !name.is_empty() {
            return Ok((name, "DEMO_PROFILE".into()));
        }
    }
    for (name, folder) in &p.found {
        if name == "default" {
            continue;
        }
        let meta = read_json(&folder.join("profile.json"))?;
        let globs: Vec<String> = meta.get("match").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        for remote in repo {
            if globs.iter().any(|g| fnmatch(remote, g)) {
                return Ok((name.clone(), format!("git remote {remote} matches")));
            }
        }
    }
    Ok(("default".into(), "no profile matched this repo".into()))
}

/// kit.ts next to the spec: the engine's Playwright and demo kit, plus the profile's helpers.ts
/// (as `profile`) when it has one. Specs import from ./kit only.
fn write_kit(skill: &Path, demo_dir: &Path, profile_dir: &Path) -> Result<()> {
    let mut lines = vec![
        "// Written by walkie. Import from here, never from \"@playwright/test\".".to_string(),
        format!("export * from \"{}\"", skill.join("scripts").join("kit").display()),
    ];
    if profile_dir.join("helpers.ts").exists() {
        lines.push(format!("export * as profile from \"{}\"", profile_dir.join("helpers").display()));
    }
    std::fs::write(demo_dir.join("kit.ts"), lines.join("\n") + "\n")?;
    Ok(())
}

fn is_flutter_app(folder: &Path) -> bool {
    std::fs::read_to_string(folder.join("pubspec.yaml")).is_ok_and(|t| t.contains("sdk: flutter"))
}

/// record.runner auto -> browser or flutter for this repo, and the spec file and template that go
/// with it: demo.spec.ts (Playwright) or demo_test.dart (a Flutter integration test).
fn pick_runner(skill: &Path, resolved: &mut Map<String, Value>, repo: &Path) {
    let prof = resolved.get_mut("profile").and_then(Value::as_object_mut).unwrap();
    let rec = prof.entry("record").or_insert_with(|| json!({})).as_object_mut().unwrap();
    let mut runner = rec.get("runner").and_then(Value::as_str).unwrap_or("auto").to_string();
    if runner == "auto" {
        let app_dir = rec.get("app_dir").and_then(Value::as_str).unwrap_or(".");
        runner = if is_flutter_app(&repo.join(app_dir)) { "flutter" } else { "browser" }.into();
    }
    rec.insert("runner".into(), runner.clone().into());
    let flutter = runner == "flutter";
    prof.insert("spec".into(), if flutter { "demo_test.dart" } else { "demo.spec.ts" }.into());
    let ext = if flutter { ".dart" } else { ".ts" };
    if !prof.get("template").and_then(Value::as_str).unwrap_or_default().ends_with(ext) {
        let t = skill.join(if flutter { "flutter/template_test.dart" } else { "profiles/default/template.spec.ts" });
        prof.insert("template".into(), t.display().to_string().into());
    }
}

pub fn main(args: &[String]) -> Result<()> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let p = Profiles::scan()?;
    match args.as_slice() {
        ["--list"] => {
            let list: Map<String, Value> = p.found.iter().map(|(n, f)| (n.clone(), f.display().to_string().into())).collect();
            println!("{}", Value::Object(list));
        }
        ["--resolve", name] => {
            if !p.found.contains_key(*name) {
                bail!("no profile named {name:?}");
            }
            println!("{}", to_json_indent1(&Value::Object(p.resolve(name)?)));
        }
        [dir] if !dir.starts_with("--") => {
            let demo_dir = PathBuf::from(dir);
            let repo = remotes();
            let (name, why) = choose(&p, &repo)?;
            let picked = p.load(&name)?;
            let allow: Vec<String> = picked["profile"].get("allow").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
            if !allow.is_empty() && !repo.iter().any(|r| allow.contains(r)) {
                let here = if repo.is_empty() { "not a git repo".to_string() } else { repo.join(", ") };
                bail!("profile {name:?} is only allowed in {}; this repo is {here}.\nUse DEMO_PROFILE=default, or make a profile for this repo.", allow.join(", "));
            }
            let mut resolved = p.resolve(&name)?;
            let prof = resolved.get_mut("profile").and_then(Value::as_object_mut).unwrap();
            prof.insert("chosen_because".into(), why.clone().into());
            prof.insert("repo".into(), json!(repo));
            pick_runner(&p.skill, &mut resolved, &std::env::current_dir()?);
            std::fs::create_dir_all(&demo_dir)?;
            std::fs::write(demo_dir.join("profile.json"), to_json_indent1(&Value::Object(resolved.clone())))?;
            let profile_dir = PathBuf::from(resolved["profile"]["dir"].as_str().unwrap_or_default());
            write_kit(&p.skill, &demo_dir, &profile_dir)?;
            println!("profile {name} ({why}) -> {}", demo_dir.join("profile.json").display());
            println!("template {}", resolved["profile"]["template"].as_str().unwrap_or_default());
        }
        _ => bail!("usage: wt profile DEMO_DIR | --list | --resolve NAME"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnmatch_like_python() {
        assert!(fnmatch("github.com/acme/shop", "github.com/acme/*"));
        assert!(fnmatch("github.com/acme/shop", "*/shop"));
        assert!(!fnmatch("github.com/acme/shop", "github.com/other/*"));
        assert!(fnmatch("a1", "a[0-9]"));
        assert!(!fnmatch("ab", "a[!b]"));
        assert!(fnmatch("a.b", "a?b"));
    }

    #[test]
    fn merge_over_default() {
        let m = merge(&json!({"a": 1, "b": {"c": 2, "d": 3}}), &json!({"b": {"c": 5}, "e": null}));
        assert_eq!(m, json!({"a": 1, "b": {"c": 5, "d": 3}, "e": null}));
    }
}
