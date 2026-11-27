//! Phone recordings: a Flutter integration test on an iOS Simulator or Android emulator, with the
//! screen recorded on the host (see flutter/walkie_kit.dart, the Dart twin of the Demo kit).
//!
//!   wt device record DIR REPO [DEVICE]
//!                    runs DIR/demo_test.dart on the device (profile record.device, or DEVICE)
//!                    while recording its screen -> DIR/raw.mp4, then collects its events (below).
//!                    The test and the kit are copied into the app's integration_test/walkie/
//!                    for the run and removed after. The test's output is in DIR/test.log.
//!   wt device config DIR          JSON for --dart-define=WALKIE_CONFIG: clip lengths
//!                                 (DIR/voice/durations.json) and the profile's zoom
//!   wt device collect LOG DIR     the test's output -> DIR/events.json (source "device") and
//!                                 DIR/voice/timeline.json

use crate::sv;
use crate::util::{engine_dir, num, output, output_lax, pystr, read_json, which, write_json};
use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::sleep;
use std::time::{Duration, Instant};

const RUN_DIR: &str = "integration_test/walkie"; // in the app, for the run only

fn config(d: &Path) -> Result<String> {
    let profile = read_json(&d.join("profile.json"))?;
    let layout = &profile["theme"]["layout"];
    let scale = layout.get("device").and_then(|dv| dv.get("zoom_scale")).cloned().unwrap_or(json!(2.2));
    Ok(json!({
        "sequence": profile["sequence"],
        "durations": read_json(&d.join("voice/durations.json"))?,
        "zoom": {"scale": scale, "ms": layout["focus_zoom"]["ms"]},
        "fade_ms": layout["fade_ms"],
    })
    .to_string())
}

fn collect(log: &Path, d: &Path) -> Result<()> {
    let bytes = std::fs::read(log).with_context(|| format!("reading {}", log.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let tagged = |tag: &str| -> Vec<&str> {
        let prefix = format!("{tag} ");
        lines.iter().filter_map(|l| l.strip_prefix(prefix.as_str())).collect()
    };
    let Some(vp) = tagged("WTVIEWPORT").last().copied() else {
        bail!("no WTVIEWPORT line in {}: the test must call demo.finish()", log.display());
    };
    let mut viewport: Map<String, Value> = serde_json::from_str(vp)?;
    let t0 = viewport.shift_remove("t0").map(|v| num(&v)).unwrap_or(0.0);
    let mut events: Vec<Map<String, Value>> = tagged("WTEV").into_iter().map(serde_json::from_str).collect::<Result<_, _>>()?;
    events.sort_by(|a, b| num(&a["ts"]).total_cmp(&num(&b["ts"]))); // stable: equal times keep their order
    let out: Vec<Value> = events
        .into_iter()
        .map(|mut e| {
            let ts = e.shift_remove("ts").map(|v| num(&v)).unwrap_or(0.0);
            let mut ev = Map::new();
            ev.insert("t".into(), json!((ts - t0).round_ties_even().max(0.0) / 1000.0));
            ev.extend(e);
            Value::Object(ev)
        })
        .collect();
    let n = out.len();
    let (w, h) = (num(&viewport["width"]), num(&viewport["height"]));
    write_json(&d.join("events.json"), &json!({"version": 1, "source": "device", "viewport": viewport, "events": out}))?;
    let timeline = tagged("WTTIMELINE").last().copied().ok_or_else(|| anyhow!("no WTTIMELINE line in {}", log.display()))?;
    std::fs::write(d.join("voice/timeline.json"), timeline)?;
    println!("{n} events, screen {w:.0}x{h:.0} pt -> {}", d.join("events.json").display());
    Ok(())
}

// ---- devices ------------------------------------------------------------------------------------

fn xcrun(args: &[&str]) -> Result<String> {
    output("xcrun", args)
}

fn xcrun_lax(args: &[&str]) -> String {
    output_lax("xcrun", args)
}

/// Runs the app's flutter command (e.g. `puro flutter build apk --debug`) in the app folder, quiet.
fn flutter_run(flutter: &[String], args: &[&str], app: &Path) -> Result<()> {
    let (cmd, pre) = flutter.split_first().context("record.flutter is empty")?;
    let out = Command::new(cmd).args(pre).args(args).current_dir(app).stdin(Stdio::null()).output()?;
    if !out.status.success() {
        let log = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
        bail!("{} {} failed:\n{}", flutter.join(" "), args.join(" "), log.trim());
    }
    Ok(())
}

const IOS_PRIVACY: [&str; 11] = [
    "calendar", "contacts", "location", "location-always", "photos", "photos-add", "media-library",
    "microphone", "motion", "reminders", "siri",
];

/// The app's bundle id (the Runner target's PRODUCT_BUNDLE_IDENTIFIER), or None.
fn ios_bundle_id(app: &Path) -> Option<String> {
    let pbx = std::fs::read_to_string(app.join("ios/Runner.xcodeproj/project.pbxproj")).ok()?;
    let re = Regex::new(r"PRODUCT_BUNDLE_IDENTIFIER = ([\w.-]+);").unwrap();
    re.captures_iter(&pbx).map(|c| c[1].to_string()).find(|i| !i.ends_with("Tests"))
}

/// Runtime ("dangerous") permissions: an app that declares one of these can show a prompt.
fn android_asks_permissions(app: &Path) -> bool {
    let dangerous = Regex::new(
        r"CAMERA|LOCATION|CONTACTS|RECORD_AUDIO|POST_NOTIFICATIONS|READ_MEDIA|EXTERNAL_STORAGE|CALENDAR|BLUETOOTH_(SCAN|CONNECT|ADVERTISE)|BODY_SENSORS|ACTIVITY_RECOGNITION|READ_PHONE|CALL_PHONE|SMS|NEARBY_WIFI|ACCOUNTS",
    )
    .unwrap();
    let perm = Regex::new(r#"uses-permission[^>]*android:name="([^"]+)""#).unwrap();
    let Ok(dirs) = std::fs::read_dir(app.join("android/app/src")) else { return false };
    dirs.filter_map(|e| e.ok())
        .filter_map(|e| std::fs::read_to_string(e.path().join("AndroidManifest.xml")).ok())
        .any(|m| perm.captures_iter(&m).any(|c| dangerous.is_match(&c[1])))
}

enum Device {
    Ios(Ios),
    Android(Android),
}

struct Ios {
    id: String,
    name: String,
    rec: Option<Child>,
}

struct Android {
    id: String,
    name: String,
    zen: Option<String>,
    demo_allowed: Option<String>,
    out: PathBuf,
    tmp: PathBuf,
}

impl Ios {
    /// An iOS Simulator, recorded with simctl io recordVideo (full device pixels, no frame).
    fn find(want: Option<&str>) -> Result<Option<Ios>> {
        if !which("xcrun") {
            return Ok(None);
        }
        let list: Value = serde_json::from_str(&xcrun(&["simctl", "list", "devices", "available", "-j"])?)?;
        let devices: Vec<&Value> = list["devices"].as_object().into_iter().flat_map(|m| m.values()).filter_map(Value::as_array).flatten().collect();
        let booted = |d: &&Value| d["state"] == "Booted";
        let hit: Vec<&Value> = match want {
            Some(w) => {
                let mut hit: Vec<&Value> = devices.iter().copied().filter(|d| d["udid"] == w || d["name"] == w).collect();
                hit.sort_by_key(|d| !booted(d)); // same name on two runtimes: the running one
                hit
            }
            None => {
                let b: Vec<&Value> = devices.iter().copied().filter(booted).collect();
                if b.is_empty() { devices.iter().copied().filter(|d| pystr(&d["name"]).starts_with("iPhone")).collect() } else { b }
            }
        };
        Ok(hit.first().map(|d| Ios { id: pystr(&d["udid"]), name: pystr(&d["name"]), rec: None }))
    }

    fn spawn(&self, args: &[&str]) -> String {
        let mut all = vec!["simctl", "spawn", self.id.as_str()];
        all.extend_from_slice(args);
        xcrun_lax(&all)
    }

    fn boot(&self) -> Result<()> {
        xcrun_lax(&["simctl", "boot", &self.id]); // fails harmlessly when booted
        xcrun(&["simctl", "bootstatus", &self.id, "-b"])?;
        self.close_apps();
        Ok(())
    }

    /// No system pop-up over the app while it records: an app that may ask for permissions is
    /// installed first with every privacy permission simctl can grant (photos, contacts, location,
    /// microphone, ...), so it never asks; no keyboard tips; and no alert left over from another app
    /// (a respring clears it). iOS cannot grant the notifications permission this way: an app that
    /// asks for it shows the prompt.
    fn quiet(&self, app: &Path, flutter: &[String], on: bool) -> Result<()> {
        if !on {
            return Ok(());
        }
        let bundle = ios_bundle_id(app);
        let plist = std::fs::read_to_string(app.join("ios/Runner/Info.plist")).unwrap_or_default();
        if let Some(bundle) = bundle.filter(|_| plist.contains("UsageDescription")) {
            // The app may ask (it has usage descriptions). A grant only holds for an installed app
            // (iOS resets it when the app is installed fresh), so install it first; flutter test
            // installs over it and the grant stays.
            println!("the app may ask for permissions: installing it with them granted");
            flutter_run(flutter, &["build", "ios", "--simulator", "--debug"], app)?;
            xcrun(&["simctl", "install", &self.id, &app.join("build/ios/iphonesimulator/Runner.app").display().to_string()])?;
            // Each service by name: "all" writes one wildcard entry that iOS does not honor for
            // every service (it still asked for contacts).
            for service in IOS_PRIVACY {
                xcrun_lax(&["simctl", "privacy", &self.id, "grant", service, &bundle]);
            }
        }
        for key in ["DidShowContinuousPathIntroduction", "DidShowGestureKeyboardIntroduction"] {
            self.spawn(&["defaults", "write", "com.apple.keyboard.preferences", key, "-bool", "true"]);
        }
        let other_app = fancy_regex::Regex::new(r"UIKitApplication:(?!com\.apple\.(Spotlight|chrono))").unwrap();
        if other_app.is_match(&self.spawn(&["launchctl", "list"])).unwrap_or(false) {
            self.close_apps();
        }
        // A permission alert another app left stays on screen after that app quits; SpringBoard
        // restarting clears it. It posts its new pid as com.apple.springboard.finishedstartup once
        // it is up (about 6 s); then the home screen draws.
        let started = || self.spawn(&["notifyutil", "-g", "com.apple.springboard.finishedstartup"]).split_whitespace().last().map(str::to_string);
        let before = started();
        self.spawn(&["launchctl", "kickstart", "-k", "user/foreground/com.apple.SpringBoard"]);
        for _ in 0..60 {
            sleep(Duration::from_millis(500));
            if started() != before {
                break;
            }
        }
        sleep(Duration::from_secs(2));
        Ok(())
    }

    /// Quit the apps that are open, so the app under test starts from the home screen. Started
    /// while another app is in front, iOS shows a "◀ <that app>" link in its status bar.
    fn close_apps(&self) {
        let running = self.spawn(&["launchctl", "list"]);
        let app = Regex::new(r"UIKitApplication:([\w.-]+)\[").unwrap();
        let system = Regex::new(r"(?i)Spotlight|WidgetRenderer|springboard").unwrap();
        let mut bundles: Vec<String> = app.captures_iter(&running).map(|c| c[1].to_string()).collect();
        bundles.sort();
        bundles.dedup();
        for bundle in bundles.iter().filter(|b| !system.is_match(b)) {
            xcrun_lax(&["simctl", "terminate", &self.id, bundle]);
        }
    }

    fn clean_status_bar(&self, on: bool) -> Result<()> {
        if on {
            xcrun(&["simctl", "status_bar", &self.id, "override", "--time", "9:41", "--batteryState", "charged", "--batteryLevel", "100",
                "--wifiBars", "3", "--cellularBars", "4", "--dataNetwork", "wifi"])?;
        } else {
            xcrun_lax(&["simctl", "status_bar", &self.id, "clear"]);
        }
        Ok(())
    }

    fn start_recording(&mut self, out: &Path, display: Option<&str>) -> Result<()> {
        let mut cmd = Command::new("xcrun");
        cmd.args(["simctl", "io", &self.id, "recordVideo", "--codec=h264", "--force"]);
        if let Some(d) = display {
            cmd.arg(format!("--display={d}"));
        }
        cmd.arg(out);
        // simctl prints to stderr; both go to one pipe, as Python's stderr=STDOUT did.
        let (reader, writer) = std::io::pipe()?;
        let mut child = cmd.stdin(Stdio::null()).stdout(writer.try_clone()?).stderr(writer).spawn()?;
        drop(cmd); // closes our copies of the write end, so the reader ends with the child
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(reader).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut said = vec![];
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            // it says so once frames are being written
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(line) => {
                    let started = line.contains("Recording started");
                    said.push(line);
                    if started {
                        self.rec = Some(child);
                        return Ok(());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if child.try_wait()?.is_some() {
                        break;
                    }
                    sleep(Duration::from_millis(50));
                }
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        let text = said.join("\n");
        // A foldable has two displays and simctl will not pick: take the primary one, as in
        //   --display=1  LCD (primary) 1398x2034
        let primary = Regex::new(r"--display=(\S+)\s+.*\(primary\)").unwrap();
        if display.is_none() {
            if let Some(c) = primary.captures(&text) {
                return self.start_recording(out, Some(&c[1].to_string()));
            }
        }
        bail!("simctl could not record {}:\n{}", self.name, text.trim())
    }

    fn stop_recording(&mut self) -> Result<()> {
        let Some(mut child) = self.rec.take() else { return Ok(()) };
        // SIGINT finishes the file; a kill leaves it unplayable
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) };
        let deadline = Instant::now() + Duration::from_secs(60);
        while child.try_wait()?.is_none() {
            if Instant::now() > deadline {
                let _ = child.kill();
                bail!("simctl did not finish the recording within 60 s");
            }
            sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

impl Android {
    /// An Android emulator, recorded by the emulator itself on the host (no 3-minute limit of adb
    /// shell screenrecord).
    fn find(want: Option<&str>) -> Result<Option<Android>> {
        if !which("adb") {
            return Ok(None);
        }
        let serial = Regex::new(r"(?m)^(emulator-\d+)\s+device$").unwrap();
        let devices = output("adb", &["devices"])?;
        for c in serial.captures_iter(&devices) {
            let s = c[1].to_string();
            let avd = output_lax("adb", &["-s", &s, "emu", "avd", "name"]);
            let name = avd.lines().next().map(|l| l.trim().to_string()).unwrap_or_else(|| s.clone());
            if want.is_none_or(|w| w == s || w == name) {
                return Ok(Some(Android { id: s, name, zen: None, demo_allowed: None, out: PathBuf::new(), tmp: PathBuf::new() }));
            }
        }
        Ok(None)
    }

    fn adb(&self, args: &[&str]) -> Result<String> {
        let mut all = vec!["-s", self.id.as_str()];
        all.extend_from_slice(args);
        output("adb", &all)
    }

    fn adb_lax(&self, args: &[&str]) -> String {
        let mut all = vec!["-s", self.id.as_str()];
        all.extend_from_slice(args);
        output_lax("adb", &all)
    }

    fn boot(&self) -> Result<()> {
        self.adb(&["wait-for-device"])?;
        Ok(())
    }

    /// No system pop-up over the app while it records: an app that asks for runtime permissions is
    /// installed first with all of them granted (flutter test installs over it and the grants
    /// stay); Do Not Disturb keeps other apps' notifications away; system dialogs left open are
    /// closed. Do Not Disturb is restored after.
    fn quiet(&mut self, app: &Path, flutter: &[String], on: bool) -> Result<()> {
        if !on {
            if matches!(self.zen.as_deref(), Some("0" | "")) {
                // it was off: turn it off again
                self.adb_lax(&["shell", "cmd", "notification", "set_dnd", "off"]);
            }
            return Ok(());
        }
        if android_asks_permissions(app) {
            println!("the app asks for permissions: installing it with them granted");
            flutter_run(flutter, &["build", "apk", "--debug"], app)?;
            let apk = app.join("build/app/outputs/flutter-apk/app-debug.apk");
            self.adb(&["install", "-r", "-t", "-g", &apk.display().to_string()])?;
        }
        self.zen = Some(self.adb_lax(&["shell", "settings", "get", "global", "zen_mode"]).trim().to_string());
        self.adb_lax(&["shell", "cmd", "notification", "set_dnd", "on"]);
        self.adb_lax(&["shell", "am", "broadcast", "-a", "android.intent.action.CLOSE_SYSTEM_DIALOGS"]);
        Ok(())
    }

    fn demo(&self, cmd: &str, extra: &[(&str, &str)]) {
        let mut args = vec!["shell", "am", "broadcast", "-a", "com.android.systemui.demo", "-e", "command", cmd];
        for (k, v) in extra {
            args.extend(["-e", k, v]);
        }
        self.adb_lax(&args);
    }

    fn clean_status_bar(&mut self, on: bool) -> Result<()> {
        if on {
            self.demo_allowed = Some(self.adb_lax(&["shell", "settings", "get", "global", "sysui_demo_allowed"]).trim().to_string());
            self.adb_lax(&["shell", "settings", "put", "global", "sysui_demo_allowed", "1"]);
            sleep(Duration::from_secs(1)); // SystemUI ignores demo commands until it has seen the setting
            self.demo("enter", &[]);
            self.demo("clock", &[("hhmm", "0941")]);
            self.demo("battery", &[("level", "100"), ("plugged", "false")]);
            self.demo("network", &[("wifi", "show"), ("level", "4")]);
            self.demo("notifications", &[("visible", "false")]);
        } else {
            self.demo("exit", &[]);
            let was = self.demo_allowed.clone().unwrap_or_else(|| "1".into());
            if was != "1" {
                let v = if was.is_empty() { "0" } else { was.as_str() };
                self.adb_lax(&["shell", "settings", "put", "global", "sysui_demo_allowed", v]);
            }
        }
        Ok(())
    }

    fn start_recording(&mut self, out: &Path) -> Result<()> {
        self.out = out.to_path_buf();
        self.tmp = out.with_extension("webm");
        let _ = std::fs::remove_file(&self.tmp);
        let r = self.adb_lax(&["emu", "screenrecord", "start", "--time-limit", "1800", &self.tmp.display().to_string()]);
        if !r.contains("OK") {
            bail!("the emulator could not record: {}", r.trim());
        }
        sleep(Duration::from_secs(1));
        Ok(())
    }

    fn stop_recording(&mut self) -> Result<()> {
        self.adb_lax(&["emu", "screenrecord", "stop"]);
        for _ in 0..60 {
            // the emulator finishes the file after it answers
            if std::fs::metadata(&self.tmp).is_ok_and(|m| m.len() > 0) {
                break;
            }
            sleep(Duration::from_millis(500));
        }
        sleep(Duration::from_secs(2));
        crate::util::run("ffmpeg", &sv!["-y", "-loglevel", "error", "-i", self.tmp.display(), "-c:v", "libx264", "-crf", "12",
            "-preset", "fast", "-pix_fmt", "yuv420p", self.out.display()])
    }
}

impl Device {
    fn platform(&self) -> &str {
        match self {
            Device::Ios(_) => "ios",
            Device::Android(_) => "android",
        }
    }
    fn id(&self) -> &str {
        match self {
            Device::Ios(d) => &d.id,
            Device::Android(d) => &d.id,
        }
    }
    fn name(&self) -> &str {
        match self {
            Device::Ios(d) => &d.name,
            Device::Android(d) => &d.name,
        }
    }
    fn boot(&self) -> Result<()> {
        match self {
            Device::Ios(d) => d.boot(),
            Device::Android(d) => d.boot(),
        }
    }
    fn quiet(&mut self, app: &Path, flutter: &[String], on: bool) -> Result<()> {
        match self {
            Device::Ios(d) => d.quiet(app, flutter, on),
            Device::Android(d) => d.quiet(app, flutter, on),
        }
    }
    fn clean_status_bar(&mut self, on: bool) -> Result<()> {
        match self {
            Device::Ios(d) => d.clean_status_bar(on),
            Device::Android(d) => d.clean_status_bar(on),
        }
    }
    fn start_recording(&mut self, out: &Path) -> Result<()> {
        match self {
            Device::Ios(d) => d.start_recording(out, None),
            Device::Android(d) => d.start_recording(out),
        }
    }
    fn stop_recording(&mut self) -> Result<()> {
        match self {
            Device::Ios(d) => d.stop_recording(),
            Device::Android(d) => d.stop_recording(),
        }
    }
}

fn pick_device(want: Option<&str>) -> Result<Device> {
    if let Some(d) = Ios::find(want)? {
        return Ok(Device::Ios(d));
    }
    if let Some(d) = Android::find(want)? {
        return Ok(Device::Android(d));
    }
    match want {
        Some(w) => bail!("no simulator or emulator {w:?}"),
        None => bail!("no simulator or emulator found: boot one (iOS: Simulator.app; Android: flutter emulators --launch <id>)"),
    }
}

// ---- record -------------------------------------------------------------------------------------

fn remove_run_dir(run_dir: &Path) {
    let _ = std::fs::remove_dir_all(run_dir);
    let _ = std::fs::remove_dir(run_dir.parent().unwrap()); // integration_test/ when the run made it
}

fn record(d: &Path, repo: &Path, want: Option<&str>) -> Result<()> {
    let profile = read_json(&d.join("profile.json"))?;
    let rec = profile["profile"].get("record").cloned().unwrap_or(json!({}));
    let app_dir = rec.get("app_dir").and_then(Value::as_str).unwrap_or(".");
    let app = repo.join(app_dir);
    let app = app.canonicalize().unwrap_or(app);
    let flutter_cmd = rec.get("flutter").and_then(Value::as_str).unwrap_or("flutter").to_string();
    let pubspec = app.join("pubspec.yaml");
    let Ok(pub_text) = std::fs::read_to_string(&pubspec) else {
        bail!("no pubspec.yaml in {}: set record.app_dir in the profile", app.display());
    };
    if !Regex::new(r"(?m)^\s+integration_test:").unwrap().is_match(&pub_text) {
        bail!("{} has no integration_test dev dependency. Add it in {}:\n  {flutter_cmd} pub add 'dev:integration_test:{{\"sdk\":\"flutter\"}}'",
            pubspec.display(), app.display());
    }
    let spec = d.join("demo_test.dart");
    if !spec.exists() {
        bail!("no {}: call start, then write the test", spec.display());
    }

    let want = want.map(str::to_string).or_else(|| rec.get("device").and_then(Value::as_str).map(str::to_string));
    let mut dev = pick_device(want.as_deref())?;
    println!("device: {} ({}, {})", dev.name(), dev.platform(), dev.id());
    dev.boot()?;

    let run_dir = app.join(RUN_DIR);
    if run_dir.exists() {
        std::fs::remove_dir_all(&run_dir)?; // a run that was cut off
    }
    std::fs::create_dir_all(&run_dir)?;
    std::fs::copy(&spec, run_dir.join("demo_test.dart"))?;
    std::fs::copy(engine_dir()?.join("flutter/walkie_kit.dart"), run_dir.join("walkie_kit.dart"))?;
    let (raw, log) = (d.join("raw.mp4"), d.join("test.log"));
    let flutter = shlex::split(&flutter_cmd).ok_or_else(|| anyhow!("cannot parse record.flutter: {flutter_cmd}"))?;
    let mut args: Vec<String> = flutter[1..].to_vec();
    args.extend(sv!["test", format!("{RUN_DIR}/demo_test.dart"), "-d", dev.id(), format!("--dart-define=WALKIE_CONFIG={}", config(d)?)]);

    let result = (|| -> Result<i32> {
        dev.quiet(&app, &flutter, true)?;
        dev.clean_status_bar(true)?;
        // The recording runs through the build too (flutter test has no build-only switch for a
        // device); the build cuts it at the sync square, so only the file is longer.
        dev.start_recording(&raw)?;
        let code = (|| -> Result<i32> {
            let out = std::fs::File::create(&log)?;
            let status = Command::new(&flutter[0]).args(&args).current_dir(&app).stdin(Stdio::null())
                .stdout(out.try_clone()?).stderr(out).status()?;
            Ok(status.code().unwrap_or(-1))
        })();
        let stopped = dev.stop_recording();
        let code = code?;
        stopped?;
        Ok(code)
    })();
    let _ = dev.clean_status_bar(false);
    let _ = dev.quiet(&app, &flutter, false);
    remove_run_dir(&run_dir);
    let code = result?;

    if code != 0 {
        let text = String::from_utf8_lossy(&std::fs::read(&log)?).into_owned();
        let report: Vec<&str> = text.lines().filter(|l| !l.starts_with("WTEV ")).collect();
        let first = report
            .iter()
            .position(|l| l.contains("EXCEPTION") || l.contains("Error") || l.contains("error:"))
            .unwrap_or(report.len().saturating_sub(50));
        let end = (first + 50).min(report.len());
        bail!("the test failed (exit {code}):\n{}", report[first..end].join("\n"));
    }
    collect(&log, d)?;
    println!("recording: {}", raw.display());
    Ok(())
}

pub fn main(args: &[String]) -> Result<()> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let abs = |p: &str| -> Result<PathBuf> { Ok(std::path::absolute(p)?) };
    match a.as_slice() {
        ["record", dir, repo] => record(&abs(dir)?, &abs(repo)?, None),
        ["record", dir, repo, device] => record(&abs(dir)?, &abs(repo)?, Some(device)),
        ["config", dir] => {
            println!("{}", config(Path::new(dir))?);
            Ok(())
        }
        ["collect", log, dir] => collect(Path::new(log), Path::new(dir)),
        _ => bail!("usage: wt device record DIR REPO [DEVICE] | config DIR | collect LOG DIR"),
    }
}
