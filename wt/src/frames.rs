//! Contact sheet to check that captions and highlights match the narration.
//!
//!   wt frames VIDEO.mp4 DEMO_DIR OUT.png
//!
//! Takes two frames per narration clip (1.5 s and 3.5 s after it starts) and tiles them, two per
//! row. Then checks for flashes: a frame-to-frame brightness jump of more than JUMP (0-255) is
//! printed as FLASH. A fade spreads the change over several frames and passes.

use crate::sv;
use crate::util::{num, read_json, run};
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

const JUMP: f64 = 40.0;
const W: usize = 64;
const H: usize = 36;

pub fn main(args: &[String]) -> Result<()> {
    let [video, dir, out] = args else { bail!("usage: wt frames VIDEO.mp4 DEMO_DIR OUT.png") };
    let timeline = read_json(&Path::new(dir).join("voice/timeline.json"))?;
    let tmp = tempfile::tempdir()?;
    let shots: Vec<f64> = timeline
        .as_array()
        .context("timeline.json is not a list")?
        .iter()
        .flat_map(|clip| [1.5, 3.5].map(|offset| num(&clip["start"]) + offset))
        .collect();
    for (i, t) in shots.iter().enumerate() {
        let png = tmp.path().join(format!("{i:03}.png"));
        run("ffmpeg", &sv!["-loglevel", "error", "-y", "-ss", format!("{t:.2}"), "-i", video,
            "-frames:v", "1", "-vf", "scale=560:-1", png.display()])?;
    }
    let rows = shots.len().div_ceil(2);
    run("ffmpeg", &sv!["-loglevel", "error", "-y", "-i", tmp.path().join("%03d.png").display(),
        "-vf", format!("tile=2x{rows}"), out])?;
    println!("wrote {out}");

    let raw = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-i", video, "-vf", &format!("fps=30,scale={W}:{H},format=gray"), "-f", "rawvideo", "-"])
        .output()?;
    if !raw.status.success() {
        bail!("ffmpeg could not read {video}: {}", String::from_utf8_lossy(&raw.stderr).trim());
    }
    let means: Vec<f64> = raw.stdout.chunks(W * H).map(|f| f.iter().map(|&b| b as f64).sum::<f64>() / (W * H) as f64).collect();
    let flashes: Vec<(f64, f64, f64)> = means
        .windows(2)
        .enumerate()
        .filter(|(_, ab)| (ab[1] - ab[0]).abs() > JUMP)
        .map(|(i, ab)| ((i + 1) as f64 / 30.0, ab[0], ab[1]))
        .collect();
    for (t, a, b) in &flashes {
        println!("FLASH at {t:.2}s: brightness {a:.0} -> {b:.0} in one frame");
    }
    if flashes.is_empty() {
        println!("no flashes");
    } else {
        println!("{} flash(es): fade the change instead of cutting", flashes.len());
    }
    Ok(())
}
