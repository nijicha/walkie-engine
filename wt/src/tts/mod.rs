//! Synthesize Thai text with pythainlp/MakHamTTS (CPU, ONNX).
//!
//!   wt tts DEMO_DIR < DEMO_DIR/narration.tsv     (each line: name<TAB>Thai text)
//!   wt tts --setup                               download the model and the Thai data, say one line
//!
//! Writes DEMO_DIR/voice/<name>.wav. Settings (model, speed, phrase_gap, max_rate, eq, breath,
//! lexicon) come from `voice` in DEMO_DIR/profile.json. Also writes voice/durations.json, which the
//! Demo kit reads. Prints: name, seconds, syllables per second, phonemes (to check what the G2P
//! made of each line).
//!
//! Pacing: each space in the Thai text is a phrase break. Every phrase is synthesized on its own
//! and joined with phrase_gap seconds of silence, so a long line breathes instead of running on.
//! A line faster than max_rate syllables per second gets a FAST flag: split it with more spaces.
//! A line of 3 or more phrases averaging under CHOPPY syllables each gets a CHOPPY flag: join them.
//!
//! Loanwords: the lexicon swaps a word for exact phonemes when the G2P cannot reach the sound
//! (Thai spelling cannot give "สะ-ป๊าร์ก" with its final k). Inline phonemes work too: write
//! {sa↓.paːk↑} in narration.tsv.

mod data;
mod english;
mod g2p;
mod newmm;
mod normalize;
mod pickle;
mod tltk;

use crate::util::{num, py_split, pystr, read_json, run, to_json_indent1};
use anyhow::{Context, Result, anyhow, bail};
use indexmap::IndexMap;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

const RATE: usize = 24000;
const EDGE: f64 = 0.04; // seconds kept around each phrase after trimming the model's own silence
const TONES: &str = "↑→↓↗↘"; // tone marks: ↓ low, → mid, ↑ high, ↗ rising, ↘ falling
const CHOPPY: usize = 4; // syllables per phrase, on average, below which a line stops and starts too often
const FRAME: usize = 240; // 10 ms

/// Lexicon words, inline {phonemes} and the G2P for the rest.
struct Phonemizer {
    g2p: g2p::G2p,
    lexicon: IndexMap<String, String>,
    split: regex::Regex,
}

impl Phonemizer {
    fn new(lexicon: IndexMap<String, String>) -> Result<Phonemizer> {
        let g2p = g2p::G2p::load(&data::g2p_data()?, &data::words_th()?)?;
        let mut words: Vec<&String> = lexicon.keys().collect();
        words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
        let alts: String = words.iter().map(|w| format!("|{}", regex::escape(w))).collect();
        let split = regex::Regex::new(&format!(r"(\{{[^}}]+\}}{alts})"))?;
        Ok(Phonemizer { g2p, lexicon, split })
    }

    fn phonemize(&mut self, phrase: &str) -> Result<String> {
        let mut parts = vec![];
        let mut last = 0;
        for m in self.split.find_iter(phrase) {
            parts.push(&phrase[last..m.start()]);
            parts.push(m.as_str());
            last = m.end();
        }
        parts.push(&phrase[last..]);
        let mut out = vec![];
        for part in parts.into_iter().filter(|p| !p.is_empty()) {
            if part.starts_with('{') {
                // Python's part[1:-1]
                let chars: Vec<char> = part.chars().collect();
                out.push(chars.get(1..chars.len().saturating_sub(1)).unwrap_or(&[]).iter().collect());
            } else if let Some(ph) = self.lexicon.get(part) {
                out.push(ph.clone());
            } else {
                out.push(g2p::ipa_to_kokoro(&self.g2p.convert(part)?));
            }
        }
        Ok(out.join(" "))
    }
}

struct Voice {
    session: ort::session::Session,
    vocab: HashMap<char, i64>,
    breath: String,
    breath_db: f64,
}

impl Voice {
    fn load(model: &str, voice: &Value) -> Result<Voice> {
        let files = data::model(model).ok_or_else(|| anyhow!("voice model {model:?} unknown; pick one of {}", data::MODELS))?;
        let config = read_json(&data::model_file(files.config)?)?;
        let vocab = config["vocab"]
            .as_object()
            .context("model config has no vocab")?
            .iter()
            .filter_map(|(k, v)| Some((k.chars().next()?, v.as_i64()?)))
            .collect();
        static ENV: std::sync::OnceLock<ort::environment::Environment> = std::sync::OnceLock::new();
        let env = match ENV.get() {
            Some(env) => env,
            None => {
                let env = ort::init().build()?;
                ENV.get_or_init(|| env)
            }
        };
        let mut builder = ort::session::Session::builder(env)?;
        // WALKIE_TTS_THREADS: how many CPU threads the model may use (default: all cores).
        // The output differs a little from run to run either way: the model adds random noise.
        // Set explicitly: ort's own default uses fewer threads than Python's onnxruntime (on an
        // M3, 28.7 s instead of 23.4 s for 20 lines).
        let n = std::env::var("WALKIE_TTS_THREADS").ok().and_then(|v| v.parse().ok())
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));
        builder = builder.with_intra_threads(n).map_err(|e| anyhow!("{e}"))?;
        let session = builder.commit_from_file(data::model_file(files.onnx)?)?;
        Ok(Voice {
            session,
            vocab,
            breath: voice.get("breath").and_then(Value::as_str).unwrap_or("cut").to_string(),
            breath_db: voice.get("breath_db").map(num).unwrap_or(-18.0),
        })
    }

    fn synth(&mut self, phonemes: &str, speed: f32) -> Result<Vec<f32>> {
        let mut ids = vec![0i64];
        ids.extend(phonemes.chars().filter_map(|c| self.vocab.get(&c).copied()));
        ids.push(0);
        let n = ids.len();
        let input_ids = ort::value::Tensor::from_array(([1usize, n], ids))?;
        let speed = ort::value::Tensor::from_array(([1usize], vec![speed]))?;
        let audio: Vec<f32> = {
            let outputs = self.session.run(ort::inputs!["input_ids" => input_ids, "speed" => speed])?;
            outputs[0].try_extract_tensor::<f32>()?.1.to_vec()
        };
        Ok(soften_breath(trim(audio), &self.breath, self.breath_db))
    }
}

/// The model pads each utterance with near-silence: keep EDGE seconds around the loud part.
fn trim(audio: Vec<f32>) -> Vec<f32> {
    let peak = audio.iter().fold(0f32, |m, &x| m.max(x.abs()));
    let threshold = 0.02f32 * peak;
    let keep = (EDGE * RATE as f64) as usize;
    match (audio.iter().position(|x| x.abs() > threshold), audio.iter().rposition(|x| x.abs() > threshold)) {
        (Some(first), Some(last)) => audio[first.saturating_sub(keep)..(last + keep).min(audio.len())].to_vec(),
        _ => audio,
    }
}

/// The model opens every utterance with an inhale (about 250 ms, low-frequency noise), and each
/// phrase is its own utterance. voice `breath`: "cut" (default) drops it, so the pause at a
/// space is phrase_gap and not phrase_gap plus an inhale; "soften" turns it down by breath_db
/// and keeps its length; "keep" leaves it. Speech starts at the first loud frame (within 10 dB
/// of the peak) or the first hiss of a fricative (energy above 4 kHz); 60 ms before it stay,
/// for a soft initial consonant.
fn soften_breath(audio: Vec<f32>, breath: &str, breath_db: f64) -> Vec<f32> {
    let n = audio.len() / FRAME;
    if breath == "keep" || n < 10 {
        return audio;
    }
    let frames: Vec<&[f32]> = audio[..n * FRAME].chunks(FRAME).collect();
    // numpy: (frames ** 2).mean(1) in float32, summed pairwise
    let rms: Vec<f32> = frames.iter().map(|f| (pairwise_f32(&f.iter().map(|x| x * x).collect::<Vec<_>>()) / FRAME as f32).sqrt()).collect();
    let top = rms.iter().copied().fold(f32::MIN, f32::max);
    let db: Vec<f32> = rms.iter().map(|r| 20f32 * (r / top + 1e-9f32).log10()).collect();
    let window = hanning(FRAME);
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(FRAME);
    let mut spectrum = fft.make_output_vec();
    let hiss: Vec<f64> = frames
        .iter()
        .map(|f| {
            let mut input: Vec<f64> = f.iter().zip(&window).map(|(&x, &w)| x as f64 * w).collect();
            fft.process(&mut input, &mut spectrum).expect("FFT of one frame");
            let power: Vec<f64> = spectrum.iter().map(|c| c.norm_sqr()).collect();
            // bins above 4 kHz: k * 100 Hz > 4000
            pairwise_f64(&power[41..]) / (pairwise_f64(&power) + 1e-12)
        })
        .collect();
    let onset = (0..n).find(|&i| db[i] > -10.0 || (hiss[i] > 0.25 && db[i] > -40.0)).unwrap_or(0);
    let end = (onset * FRAME).saturating_sub((0.06 * RATE as f64) as usize);
    if end < (0.08 * RATE as f64) as usize {
        return audio; // no room for an inhale
    }
    let ramp = (0.02 * RATE as f64) as usize;
    if breath == "cut" {
        let mut out = audio[end - ramp..].to_vec();
        for (i, v) in linspace(0.0, 1.0, ramp).into_iter().enumerate() {
            out[i] = (out[i] as f64 * v) as f32;
        }
        return out;
    }
    let g = 10f64.powf(breath_db / 20.0);
    let mut gain = vec![1f32; audio.len()];
    gain[..end].fill(g as f32);
    for (i, v) in linspace(g, 1.0, ramp).into_iter().enumerate() {
        gain[end - ramp + i] = v as f32;
    }
    audio.iter().zip(gain).map(|(a, g)| a * g).collect()
}

/// numpy's pairwise summation (float32 reductions sum this way, and round at each step).
fn pairwise_f32(a: &[f32]) -> f32 {
    let n = a.len();
    if n < 8 {
        a.iter().fold(0f32, |r, &x| r + x)
    } else if n <= 128 {
        let mut r: [f32; 8] = a[..8].try_into().unwrap();
        let mut i = 8;
        while i < n - n % 8 {
            for j in 0..8 {
                r[j] += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        for &x in &a[i..] {
            res += x;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_f32(&a[..n2]) + pairwise_f32(&a[n2..])
    }
}

fn pairwise_f64(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        a.iter().fold(0.0, |r, &x| r + x)
    } else if n <= 128 {
        let mut r: [f64; 8] = a[..8].try_into().unwrap();
        let mut i = 8;
        while i < n - n % 8 {
            for j in 0..8 {
                r[j] += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        for &x in &a[i..] {
            res += x;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_f64(&a[..n2]) + pairwise_f64(&a[n2..])
    }
}

/// np.hanning(m): 0.5 + 0.5 * cos(pi * k / (m - 1)), k = 1-m, 3-m, ..., m-1.
fn hanning(m: usize) -> Vec<f64> {
    (0..m).map(|i| 0.5 + 0.5 * (std::f64::consts::PI * (1 - m as i64 + 2 * i as i64) as f64 / (m - 1) as f64).cos()).collect()
}

/// np.linspace(start, stop, num): start + i * step, the last one exactly stop.
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    let step = (stop - start) / (num - 1) as f64;
    let mut v: Vec<f64> = (0..num).map(|i| i as f64 * step + start).collect();
    if let Some(last) = v.last_mut() {
        *last = stop;
    }
    v
}

/// 16-bit PCM as libsndfile 1.2 writes float samples (soundfile's default for .wav): rounded to
/// 32 bits, then the top 16 kept. Clipped at full scale.
fn write_wav(path: &Path, audio: &[f32]) -> Result<()> {
    let spec = hound::WavSpec { channels: 1, sample_rate: RATE as u32, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &x in audio {
        w.write_sample(((x * 2147483648f32).round_ties_even() as i32 >> 16) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn lexicon(voice: &Value) -> IndexMap<String, String> {
    voice.get("lexicon").and_then(Value::as_object).into_iter().flatten().map(|(k, v)| (k.clone(), pystr(v))).collect()
}

pub fn main(args: &[String]) -> Result<()> {
    match args {
        [flag] if flag == "--setup" => return setup(),
        [dir] => synthesize(Path::new(dir)),
        _ => bail!("usage: wt tts DEMO_DIR < DEMO_DIR/narration.tsv | wt tts --setup"),
    }
}

fn synthesize(demo_dir: &Path) -> Result<()> {
    let profile = read_json(&demo_dir.join("profile.json"))?;
    let voice = &profile["voice"];
    let engine = pystr(&voice["engine"]);
    if engine != "makham" {
        bail!("voice engine {engine:?} is not supported yet; wt tts knows makham (Thai)");
    }
    let model = voice.get("model").and_then(Value::as_str).unwrap_or("makham_female");
    let mut tts = Voice::load(model, voice)?;
    let mut ph = Phonemizer::new(lexicon(voice))?;
    let out_dir = demo_dir.join("voice");
    std::fs::create_dir_all(&out_dir)?;
    let speed = num(&voice["speed"]) as f32;
    let silence = vec![0f32; (num(&voice["phrase_gap"]) * RATE as f64) as usize];
    let max_rate = num(&voice["max_rate"]);
    let eq = voice.get("eq").and_then(Value::as_str).filter(|e| !e.is_empty()).map(str::to_string);
    // Lines not in this run keep their earlier length (revoice makes only the clips that changed).
    let durations_file = out_dir.join("durations.json");
    let mut durations: Map<String, Value> =
        if durations_file.exists() { read_json(&durations_file)?.as_object().cloned().unwrap_or_default() } else { Map::new() };

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if crate::util::py_strip(&line).is_empty() {
            continue;
        }
        let (name, text) = line.split_once('\t').ok_or_else(|| anyhow!("narration line without a tab: {line:?}"))?;
        let phrases: Vec<String> = py_split(text).into_iter().map(|p| ph.phonemize(p)).collect::<Result<_>>()?;
        if phrases.is_empty() {
            bail!("narration line {name} has no text");
        }
        let mut audio = vec![];
        for (i, p) in phrases.iter().enumerate() {
            if i > 0 {
                audio.extend_from_slice(&silence);
            }
            audio.extend(tts.synth(p, speed)?);
        }
        let wav = out_dir.join(format!("{name}.wav"));
        write_wav(&wav, &audio)?;
        if let Some(eq) = &eq {
            // ffmpeg audio filter, e.g. soften the top end
            let tmp = out_dir.join(format!("{name}.raw.wav"));
            std::fs::rename(&wav, &tmp)?;
            run("ffmpeg", &crate::sv!["-loglevel", "error", "-y", "-i", tmp.display(), "-af", eq, "-ar", RATE, wav.display()])?;
            std::fs::remove_file(&tmp)?;
        }
        let seconds = audio.len() as f64 / RATE as f64;
        let rounded: f64 = format!("{seconds:.3}").parse()?;
        durations.insert(name.to_string(), Value::from(rounded));
        let phonemes = phrases.join(" | ");
        let syllables = phonemes.chars().filter(|c| TONES.contains(*c)).count();
        let rate = syllables as f64 / seconds;
        let choppy = phrases.len() >= 3 && (syllables as f64 / phrases.len() as f64) < CHOPPY as f64;
        let mut dropped: Vec<char> = phonemes.chars().filter(|c| !tts.vocab.contains_key(c) && !" |".contains(*c)).collect();
        dropped.sort();
        dropped.dedup();
        let mut flags = String::new();
        if rate > max_rate {
            flags.push_str("FAST ");
        }
        if choppy {
            flags.push_str("CHOPPY ");
        }
        if !dropped.is_empty() {
            flags.push_str(&format!("DROPPED {}", dropped.iter().collect::<String>()));
        }
        println!("{name}\t{seconds:.2}s\t{rate:.1}/s\t{phonemes}\t{flags}");
    }
    std::fs::write(&durations_file, to_json_indent1(&Value::Object(durations)))?;
    Ok(())
}

/// One-time download of the default model and the Thai data, then one line said as a check.
fn setup() -> Result<()> {
    let files = data::model("makham_female").unwrap();
    data::model_file(files.config)?;
    data::model_file(files.onnx)?;
    data::g2p_data()?;
    data::words_th()?;
    let dir = tempfile::tempdir()?;
    let profile = serde_json::json!({"voice": {"engine": "makham", "speed": 1.0, "phrase_gap": 0.3, "max_rate": 10}});
    std::fs::write(dir.path().join("profile.json"), profile.to_string())?;
    let voice = &profile["voice"];
    let mut tts = Voice::load("makham_female", voice)?;
    let mut ph = Phonemizer::new(IndexMap::new())?;
    let phonemes = ph.phonemize("ทดสอบเสียง")?;
    let audio = tts.synth(&phonemes, 1.0)?;
    println!("ok\t{:.2}s\t{phonemes}", audio.len() as f64 / RATE as f64);
    println!("MakHam is ready");
    Ok(())
}

/// Phonemes for each line on stdin, as wt tts would say it (phrases joined by " | "); for checking
/// the G2P against the Python one.
/// Spike: English with Kokoro.
pub fn english_main(args: &[String]) -> Result<()> {
    english::main(args)
}

pub fn g2p_main(args: &[String]) -> Result<()> {
    let lex = match args {
        [] => IndexMap::new(),
        [file] => read_json(Path::new(file))?.as_object().into_iter().flatten().map(|(k, v)| (k.clone(), pystr(v))).collect(),
        _ => bail!("usage: wt g2p [LEXICON.json] < lines"),
    };
    let mut ph = Phonemizer::new(lex)?;
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let phrases: Vec<String> = py_split(&line).into_iter().map(|p| ph.phonemize(p)).collect::<Result<_>>()?;
        println!("{}", phrases.join(" | "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numpy_helpers() {
        let l = linspace(0.0, 1.0, 5);
        assert_eq!(l, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        let h = hanning(5);
        assert!((h[2] - 1.0).abs() < 1e-12 && h[0].abs() < 1e-12);
        let v: Vec<f32> = (0..240).map(|i| i as f32 * 0.1).collect();
        assert!((pairwise_f32(&v) - 2868.0).abs() < 0.01);
    }

    /// Trim and breath handling against tts.py's, on the same raw model output. The fixtures
    /// (raw .in, Python's .out, cases.json) come from a script run with the Python venv; set
    /// WT_POST_FIXTURES to their folder and run `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn post_processing_matches_python() {
        let dir = std::path::PathBuf::from(std::env::var("WT_POST_FIXTURES").expect("WT_POST_FIXTURES"));
        let read = |p: std::path::PathBuf| -> Vec<f32> {
            std::fs::read(p).unwrap().chunks(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
        };
        let cases = read_json(&dir.join("cases.json")).unwrap();
        for c in cases.as_array().unwrap() {
            let name = c["name"].as_str().unwrap();
            let got = soften_breath(trim(read(dir.join(format!("{name}.in")))), c["breath"].as_str().unwrap(), num(&c["breath_db"]));
            let want = read(dir.join(format!("{name}.out")));
            assert_eq!(got.len(), want.len(), "{name}: length");
            let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            assert!(bits(&got) == bits(&want), "{name}: samples differ");
            // and the 16-bit file soundfile wrote from them
            let wav = std::env::temp_dir().join(format!("wt-{name}.wav"));
            write_wav(&wav, &got).unwrap();
            let ours: Vec<i16> = hound::WavReader::open(&wav).unwrap().samples().map(Result::unwrap).collect();
            let theirs: Vec<i16> = hound::WavReader::open(dir.join(format!("{name}.wav"))).unwrap().samples().map(Result::unwrap).collect();
            assert!(ours == theirs, "{name}: 16-bit samples differ");
            let _ = std::fs::remove_file(wav);
        }
    }
}
