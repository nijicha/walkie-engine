//! Video layers: cut the page recording, lay the overlay and the voice over it.
//!
//!   wt mix content RAW DIR        page layer: cut RAW at the sync marker, scale it to the video
//!                                 size -> DIR/content.mp4 (no sound)
//!   wt mix compose DIR OUT.mp4    DIR/content.mp4 + the overlay frames in DIR/overlay (render.ts,
//!                                 30 fps PNG) + the voice -> OUT.mp4
//!   wt mix remix VIDEO.mp4 OUT.mp4 DIR
//!                                 a finished video: keep its picture, rebuild the sound from
//!                                 DIR/voice (revoice)
//!   wt mix backdrops DIR          clean stills of the page layer, one per step, for the
//!                                 configurator -> DIR/backdrops/NN.png
//!
//! The voice: DIR/voice/timeline.json (seconds from time zero) and DIR/voice/<name>.wav. The size:
//! DIR/profile.json. Time zero is the end of the magenta sync square Demo.start() shows in the
//! top-left corner, so the cut works with any profile and with or without an intro card.

use crate::sv;
use crate::util::{num, output, pystr, read_json, run, write_json};
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

const FPS_SCAN: f64 = 25.0;
const SIDE: i64 = 8;
const MAX_BACKDROPS: usize = 12;
const FPS: i64 = 30;

fn video_size(d: &Path) -> Result<(i64, i64)> {
    let p = read_json(&d.join("profile.json"))?;
    let v = &p["theme"]["layout"]["video"];
    Ok((num(&v[0]) as i64, num(&v[1]) as i64))
}

/// A device recording keeps the screen's own orientation: an app turned to landscape is recorded
/// on its side. The sync square marks the app's top-left corner, so the corner it shows up in says
/// how to turn the recording upright.
const TURN: [(&str, &str); 4] = [
    ("top-left", ""),
    ("bottom-left", "transpose=clock"),
    ("top-right", "transpose=cclock"),
    ("bottom-right", "hflip,vflip"),
];

/// Per scanned frame: is the 8x8 block inside the sync square at this corner magenta?
fn marker_frames(raw: &str, corner: &str) -> Result<Vec<bool>> {
    let size = output("ffprobe", &["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0", raw])?;
    let wh: Vec<i64> = size.trim().split(',').take(2).map(|v| v.trim().parse().unwrap_or(0)).collect();
    let (w, h) = (wh[0], wh[1]);
    let x = if corner.ends_with("left") { 4 } else { w - 4 - SIDE };
    let y = if corner.starts_with("top") { 4 } else { h - 4 - SIDE };
    let out = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-i", raw, "-vf", &format!("fps={FPS_SCAN},crop={SIDE}:{SIDE}:{x}:{y},scale=1:1"), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()?;
    if !out.status.success() {
        bail!("ffmpeg could not scan {raw}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout.chunks(3).map(|p| p.len() == 3 && p[0] > 180 && p[1] < 90 && p[2] > 180).collect())
}

/// Seconds into the raw recording where the video starts (right after the sync marker), and the
/// ffmpeg filter that turns it upright (empty when it is). A browser recording has the marker
/// top-left; a device recording looks in every corner.
fn find_cut(raw: &str, corners: &[&str]) -> Result<(f64, String)> {
    for corner in corners {
        let magenta = marker_frames(raw, corner)?;
        let Some(first) = magenta.iter().position(|&m| m) else { continue };
        let last = (first..magenta.len()).find(|&i| !magenta[i]).unwrap_or(magenta.len()) - 1;
        let cut = (last + 1) as f64 / FPS_SCAN;
        let turn = TURN.iter().find(|(c, _)| c == corner).map(|(_, t)| *t).unwrap_or("");
        let turned = if turn.is_empty() { String::new() } else { format!(", recorded on its side ({corner}): turned upright") };
        println!("sync marker {:.2}-{cut:.2}s in the raw video, cut at {cut:.2}s{turned}", first as f64 / FPS_SCAN);
        return Ok((cut, turn.to_string()));
    }
    bail!("no sync marker in the recording: the spec must call demo.intro() or demo.start()")
}

fn device_layout(d: &Path) -> Result<Map<String, Value>> {
    let mut dev: Map<String, Value> = json!({"side": "left", "margin": 40, "radius": 36, "bezel": 6, "caption_size": 40, "zoom_scale": 2.2})
        .as_object()
        .unwrap()
        .clone();
    let p = read_json(&d.join("profile.json"))?;
    if let Some(over) = p["theme"]["layout"].get("device").and_then(Value::as_object) {
        for (k, v) in over {
            dev.insert(k.clone(), v.clone());
        }
    }
    Ok(dev)
}

/// Python's round(): to the nearest integer, ties to even.
fn pyround(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// Where a device recording sits in the video. Written to DIR/stage.json; render.ts places the
/// overlay by it.
///   side: a tall screen (a phone) in a column at one side, the step caption in a text panel at
///         the other.
///   full: a wide screen (a tablet, a phone in landscape) as large as the frame allows, the
///         caption at the bottom as on the web. Picked when the screen would take more than 42 %
///         of the width in a column.
fn stage(d: &Path, viewport: &Value) -> Result<Value> {
    let (width, height) = video_size(d)?;
    let dev = device_layout(d)?;
    let m = num(&dev["margin"]) as i64;
    let aspect = num(&viewport["width"]) / num(&viewport["height"]);
    let mut out = Map::new();
    let mut ph = (height - 2 * m).div_euclid(2) * 2;
    let mut pw = pyround(ph as f64 * aspect / 2.0) * 2;
    if pw as f64 > width as f64 * 0.42 {
        pw = pw.min((width - 2 * m).div_euclid(2) * 2);
        ph = pyround(pw as f64 / aspect / 2.0) * 2;
        out.insert("layout".into(), "full".into());
        out.insert("phone".into(), json!({"x": (width - pw).div_euclid(2), "y": (height - ph).div_euclid(2), "w": pw, "h": ph}));
        out.insert("panel".into(), Value::Null);
    } else {
        let col = (pw + 2 * m).max(pyround(width as f64 * 0.4)); // the phone's column; the panel takes the rest
        let left = dev["side"].as_str() == Some("left");
        let x = if left { (col - pw).div_euclid(2) } else { width - col + (col - pw).div_euclid(2) };
        out.insert("layout".into(), "side".into());
        out.insert("phone".into(), json!({"x": x, "y": m, "w": pw, "h": ph}));
        out.insert("panel".into(), json!({"x": if left { col } else { m }, "y": m, "w": width - col - m, "h": height - 2 * m}));
    }
    for k in ["radius", "bezel", "caption_size"] {
        out.insert(k.into(), dev[k].clone());
    }
    Ok(Value::Object(out))
}

/// zoompan expressions (zoom, x, y) for the camera events: the shown part of the screen moves from
/// one window to the next over each event's ms, eased. Windows are in screen points.
fn camera_expr(cams: &[&Value], viewport: &Value) -> (String, String, String) {
    let (vw, vh) = (&viewport["width"], &viewport["height"]);
    let full = json!({"x": 0, "y": 0, "w": vw});
    let mut wins = vec![full.clone()];
    for c in cams {
        wins.push(json!({"x": c["rect"]["x"], "y": c["rect"]["y"], "w": c["rect"]["w"]}));
    }
    fn piece(cams: &[&Value], wins: &[Value], key: &str, i: usize) -> String {
        if i == cams.len() {
            return format!("{:.3}", num(&wins[i][key]));
        }
        let (c, a, b) = (cams[i], num(&wins[i][key]), num(&wins[i + 1][key]));
        let t0 = num(&c["t"]);
        let dt = c.get("ms").map(num).unwrap_or(0.0).max(1.0) / 1000.0;
        let p = format!("clip((it-{t0:.3})/{dt:.3},0,1)");
        let ease = format!("({p}*{p}*(3-2*{p}))");
        format!("if(lt(it,{:.3}),{a:.3}+({:.3})*{ease},{})", t0 + dt, b - a, piece(cams, wins, key, i + 1))
    }
    let value = |key: &str| {
        if cams.is_empty() {
            pystr(&full[key])
        } else {
            format!("if(lt(it,{:.3}),{:.3},{})", num(&cams[0]["t"]), num(&wins[0][key]), piece(cams, &wins, key, 0))
        }
    };
    let (w, x, y) = (value("w"), value("x"), value("y"));
    (format!("{}/({w})", pystr(vw)), format!("({x})*iw/{}", pystr(vw)), format!("({y})*ih/{}", pystr(vh)))
}

fn content(raw: &str, d: &Path) -> Result<()> {
    let (width, height) = video_size(d)?;
    let events = d.join("events.json");
    let rec = if events.exists() { read_json(&events)? } else { json!({}) };
    if rec.get("source").and_then(Value::as_str) == Some("device") {
        let corners: Vec<&str> = TURN.iter().map(|(c, _)| *c).collect();
        let (cut, turn) = find_cut(raw, &corners)?;
        return device_content(raw, d, cut, &turn, &rec);
    }
    let (cut, _) = find_cut(raw, &["top-left"])?;
    // Near-lossless: this is the master the overlay is laid on, maybe many times.
    let out = d.join("content.mp4");
    run("ffmpeg", &sv!["-y", "-loglevel", "error", "-ss", format!("{cut:.2}"), "-i", raw,
        "-vf", format!("scale={width}:{height}:flags=lanczos"), "-an",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "14", "-preset", "medium", out.display()])?;
    println!("wrote {}", out.display());
    Ok(())
}

/// Page layer of a phone recording: the screen, zoomed by the camera events, in its column of the
/// stage. The recording has 3-4x the pixels it shows at, so a zoom stays sharp. The overlay draws
/// the rounded mask, the frame line and the panel background over it.
fn device_content(raw: &str, d: &Path, cut: f64, turn: &str, rec: &Value) -> Result<()> {
    let (width, height) = video_size(d)?;
    let st = stage(d, &rec["viewport"])?;
    write_json(&d.join("stage.json"), &st)?;
    let p = &st["phone"];
    let (px, py, pw, ph) = (num(&p["x"]) as i64, num(&p["y"]) as i64, num(&p["w"]) as i64, num(&p["h"]) as i64);
    let cams: Vec<&Value> = rec["events"].as_array().into_iter().flatten().filter(|e| e["type"] == "camera").collect();
    let (z, x, y) = camera_expr(&cams, &rec["viewport"]);
    let turn = if turn.is_empty() { String::new() } else { format!("{turn},") };
    let screen = format!("{turn}fps={FPS},zoompan=z='{z}':x='{x}':y='{y}':d=1:fps={FPS}:s={}x{},scale={pw}:{ph}:flags=lanczos", pw * 2, ph * 2);
    let graph = format!("[0:v]{screen}[s];color=c=black:s={width}x{height}:r={FPS}[bg];[bg][s]overlay={px}:{py}:shortest=1");
    let out = d.join("content.mp4");
    run("ffmpeg", &sv!["-y", "-loglevel", "error", "-ss", format!("{cut:.2}"), "-i", raw,
        "-filter_complex", graph, "-an",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "14", "-preset", "medium", out.display()])?;
    println!("wrote {} (device, phone at {px},{py} {pw}x{ph})", out.display());
    Ok(())
}

/// ffmpeg inputs and filters that place each clip at its time, at a gain that brings the loudest
/// clip's peak to -1.5 dB whatever TTS model made them, mixed without normalisation.
fn voice_inputs(d: &Path, first_index: usize) -> Result<(Vec<String>, Vec<String>)> {
    let voice_dir = d.join("voice");
    let timeline = read_json(&voice_dir.join("timeline.json"))?;
    let timeline = timeline.as_array().context("timeline.json is not a list")?.clone();
    let clips: Vec<PathBuf> = timeline.iter().map(|c| voice_dir.join(format!("{}.wav", pystr(&c["name"])))).collect();
    let mut probe = Command::new("ffmpeg");
    probe.args(["-loglevel", "info"]);
    for c in &clips {
        probe.arg("-i").arg(c);
    }
    probe.args(["-filter_complex", &format!("concat=n={}:v=0:a=1,volumedetect", clips.len()), "-f", "null", "-"]);
    let out = probe.output()?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        bail!("ffmpeg could not read the voice clips: {}", stderr.trim());
    }
    let peak: f64 = stderr
        .split("max_volume:")
        .nth(1)
        .and_then(|s| s.split("dB").next())
        .and_then(|s| s.trim().parse().ok())
        .context("no max_volume in ffmpeg's volumedetect output")?;
    let gain_db = -1.5 - peak;
    println!("clip peak {peak:.1} dB, gain {gain_db:+.1} dB");
    let (mut inputs, mut filters, mut labels) = (vec![], vec![], String::new());
    for (n, (clip, path)) in timeline.iter().zip(&clips).enumerate() {
        let i = n + first_index;
        inputs.extend(sv!["-i", path.display()]);
        let ms = pyround(num(&clip["start"]) * 1000.0).max(0);
        filters.push(format!("[{i}:a]aresample=48000,adelay={ms}:all=1[a{i}]"));
        labels.push_str(&format!("[a{i}]"));
    }
    filters.push(format!("{labels}amix=inputs={}:normalize=0,volume={gain_db:.2}dB,apad[voice]", timeline.len()));
    Ok((inputs, filters))
}

const AUDIO_OUT: [&str; 7] = ["-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart", "-shortest"];

fn duration(video: &Path) -> Result<String> {
    Ok(output("ffprobe", &["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", &video.display().to_string()])?.trim().to_string())
}

fn compose(d: &Path, out: &str) -> Result<()> {
    let (inputs, filters) = voice_inputs(d, 2)?;
    // -t, not -shortest: with the picture coming out of a filter graph, -shortest does not stop the
    // endless voice padding.
    // Chrome saves an all-opaque frame (a full card) as RGB, the others as RGBA. Without
    // -reinit_filter 0 that change rebuilds the filter graph and the video stops there.
    let picture = "[1:v]format=rgba[ov];[0:v][ov]overlay=format=auto:eof_action=repeat,format=yuv420p[v]".to_string();
    let mut args = sv!["-y", "-loglevel", "error", "-i", d.join("content.mp4").display(),
        "-framerate", "30", "-reinit_filter", "0", "-i", d.join("overlay").join("%06d.png").display()];
    args.extend(inputs);
    args.extend(sv!["-filter_complex", [vec![picture], filters].concat().join(";"),
        "-map", "[v]", "-map", "[voice]", "-r", "30", "-t", duration(&d.join("content.mp4"))?,
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "20"]);
    args.extend(AUDIO_OUT.map(String::from));
    args.push(out.to_string());
    run("ffmpeg", &args)?;
    println!("wrote {out}");
    Ok(())
}

fn remix(video: &str, out: &str, d: &Path) -> Result<()> {
    let (inputs, filters) = voice_inputs(d, 1)?;
    let mut args = sv!["-y", "-loglevel", "error", "-i", video];
    args.extend(inputs);
    args.extend(sv!["-filter_complex", filters.join(";"), "-map", "0:v", "-map", "[voice]", "-c:v", "copy"]); // picture copied, no quality lost
    args.extend(AUDIO_OUT.map(String::from));
    args.push(out.to_string());
    run("ffmpeg", &args)?;
    println!("wrote {out}");
    Ok(())
}

/// A still of the page layer just after each step's box lands (or each caption, without steps).
fn backdrops(d: &Path) -> Result<()> {
    let events = read_json(&d.join("events.json"))?;
    let boxes: Vec<f64> = events["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["type"] == "box" && e.get("rect").is_some_and(|r| !r.is_null() && r != &json!(false)))
        .map(|e| num(&e["t"]))
        .collect();
    let (mut times, mut last) = (vec![], -10.0);
    for t in boxes {
        if t - last >= 1.0 {
            // skip boxes moments apart (same scene)
            times.push(t + 0.5);
            last = t;
        }
    }
    let out = d.join("backdrops");
    if out.exists() {
        for f in std::fs::read_dir(&out)? {
            std::fs::remove_file(f?.path())?;
        }
    }
    std::fs::create_dir_all(&out)?;
    for (n, t) in times.iter().take(MAX_BACKDROPS).enumerate() {
        run("ffmpeg", &sv!["-y", "-loglevel", "error", "-ss", format!("{t:.2}"), "-i", d.join("content.mp4").display(),
            "-frames:v", "1", out.join(format!("{:02}.png", n + 1)).display()])?;
    }
    println!("{} backdrops -> {}", times.len().min(MAX_BACKDROPS), out.display());
    Ok(())
}

pub fn main(args: &[String]) -> Result<()> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["content", raw, dir] => content(raw, Path::new(dir)),
        ["compose", dir, out] => compose(Path::new(dir), out),
        ["remix", video, out, dir] => remix(video, out, Path::new(dir)),
        ["backdrops", dir] => backdrops(Path::new(dir)),
        _ => bail!("usage: wt mix content RAW DIR | compose DIR OUT.mp4 | remix VIDEO.mp4 OUT.mp4 DIR | backdrops DIR"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_without_events_is_the_full_screen() {
        let (z, x, y) = camera_expr(&[], &json!({"width": 440.0, "height": 956.0}));
        assert_eq!(z, "440.0/(440.0)");
        assert_eq!(x, "(0)*iw/440.0");
        assert_eq!(y, "(0)*ih/956.0");
    }

    #[test]
    fn camera_eases_to_each_window() {
        let cam = json!({"t": 1.0, "ms": 500, "rect": {"x": 10, "y": 20, "w": 200}});
        let (z, _, _) = camera_expr(&[&cam], &json!({"width": 400, "height": 800}));
        assert_eq!(z, "400/(if(lt(it,1.000),400.000,if(lt(it,1.500),400.000+(-200.000)*(clip((it-1.000)/0.500,0,1)*clip((it-1.000)/0.500,0,1)*(3-2*clip((it-1.000)/0.500,0,1))),200.000)))");
    }
}
