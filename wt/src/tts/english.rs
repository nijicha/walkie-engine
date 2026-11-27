//! Spike: English voice with Kokoro-82M and misaki-rs (no espeak).
//!
//!   wt tts-en [--voice af_heart] [--model kokoro|kokoro_fp32] [--speed 1.0] OUT.wav "English text"
//!
//! Prints the phonemes, words the G2P could not read (❓, a lexicon entry would fix them), the
//! length, the synthesis time and the real-time factor.

use super::{RATE, data, trim, write_wav};
use anyhow::{Context, Result, anyhow, bail};
use misaki_rs::{G2P, Language};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

const STYLE: usize = 256;
const MAX_TOKENS: usize = 510;

pub fn main(args: &[String]) -> Result<()> {
    let (mut voice, mut model, mut speed) = ("af_heart".to_string(), "kokoro".to_string(), 1.0f32);
    let mut rest = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--voice" => voice = it.next().context("--voice NAME")?.clone(),
            "--model" => model = it.next().context("--model NAME")?.clone(),
            "--speed" => speed = it.next().context("--speed N")?.parse()?,
            _ => rest.push(a.clone()),
        }
    }
    let [out, text] = rest.as_slice() else {
        bail!("usage: wt tts-en [--voice {}] [--model kokoro|kokoro_fp32] [--speed 1.0] OUT.wav \"text\"", data::KOKORO_VOICES);
    };

    let t0 = Instant::now();
    let g2p = G2P::new(Language::EnglishUS);
    let (phonemes, tokens) = g2p.g2p(text).map_err(|e| anyhow!("g2p: {e:?}"))?;
    let unknown: Vec<String> = tokens.iter().filter(|t| t.phonemes.is_none() && t.text.chars().any(char::is_alphabetic)).map(|t| t.text.clone()).collect();
    let g2p_ms = t0.elapsed().as_millis();

    let tok = super::read_json(&data::kokoro_file(data::KOKORO_TOKENIZER.0, data::KOKORO_TOKENIZER.1)?)?;
    let vocab: HashMap<char, i64> = tok["model"]["vocab"]
        .as_object()
        .context("tokenizer has no vocab")?
        .iter()
        .filter_map(|(k, v)| Some((k.chars().next()?, v.as_i64()?)))
        .collect();
    let (vfile, vsha) = data::kokoro_voice(&voice).ok_or_else(|| anyhow!("voice {voice:?} unknown; pick one of {}", data::KOKORO_VOICES))?;
    let table: Vec<f32> = std::fs::read(data::kokoro_file(&vfile, vsha)?)?.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let (mfile, msha) = data::kokoro_model(&model).ok_or_else(|| anyhow!("model {model:?} unknown; pick kokoro or kokoro_fp32"))?;

    let mut ids: Vec<i64> = phonemes.chars().filter_map(|c| vocab.get(&c).copied()).collect();
    let dropped: String = phonemes.chars().filter(|c| !vocab.contains_key(c)).collect();
    ids.truncate(MAX_TOKENS);
    let row = ids.len().min(table.len() / STYLE - 1);
    let style = table[row * STYLE..(row + 1) * STYLE].to_vec();
    let mut padded = vec![0i64];
    padded.extend(&ids);
    padded.push(0);
    let n = padded.len();

    let t1 = Instant::now();
    let env = ort::init().build()?;
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut session = ort::session::Session::builder(&env)?
        .with_intra_threads(threads)
        .map_err(|e| anyhow!("{e}"))?
        .commit_from_file(data::kokoro_file(mfile, msha)?)?;
    let load_ms = t1.elapsed().as_millis();

    let t2 = Instant::now();
    let audio: Vec<f32> = {
        let outputs = session.run(ort::inputs![
            "input_ids" => ort::value::Tensor::from_array(([1usize, n], padded))?,
            "style" => ort::value::Tensor::from_array(([1usize, STYLE], style))?,
            "speed" => ort::value::Tensor::from_array(([1usize], vec![speed]))?,
        ])?;
        outputs[0].try_extract_tensor::<f32>()?.1.to_vec()
    };
    let synth = t2.elapsed().as_secs_f64();
    let audio = trim(audio);
    write_wav(Path::new(out), &audio)?;
    let secs = audio.len() as f64 / RATE as f64;

    println!("phonemes  {phonemes}");
    if !dropped.is_empty() {
        println!("dropped   {dropped:?} (not in Kokoro's vocab)");
    }
    if !unknown.is_empty() {
        println!("UNKNOWN   {} (add them to the lexicon)", unknown.join(", "));
    }
    println!("audio     {secs:.2}s, {} tokens, voice {voice}, model {model}", ids.len());
    println!("time      g2p {g2p_ms} ms, load {load_ms} ms, synth {:.0} ms, RTF {:.3}", synth * 1000.0, synth / secs);
    println!("wrote     {out}");
    Ok(())
}
