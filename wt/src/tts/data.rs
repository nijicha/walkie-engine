//! The files the voice needs, downloaded once and checked against pinned hashes:
//!   - the MakHam ONNX models (pythainlp/MakHamTTS on Hugging Face, Apache-2.0),
//!   - FastThaiG2P's dictionary, IPA table and TLTK fallback data (awslabs/FastThaiG2P),
//!   - PyThaiNLP's default word list (for maiyamok; PyThaiNLP 5.4.0, Apache-2.0).
//! They go to ~/.cache/walkie/tts (WALKIE_TTS_DATA overrides it). A model already in
//! the Hugging Face cache (from the Python version) is used from there.

use crate::util::home;
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

const HF_REPO: &str = "pythainlp/MakHamTTS";
const HF_REV: &str = "cd812941e8848bffe0dcc9182720ba7cc8ddd13e";

pub struct ModelFiles {
    pub onnx: (&'static str, &'static str),
    pub config: (&'static str, &'static str),
}

/// Models in pythainlp/MakHamTTS, same interface: input_ids + speed -> 24 kHz waveform.
/// makham_female (83 MB) sounds natural. paradee (8 MB, 15x faster) is distilled from it and its
/// vocoder adds a metallic buzz up to the top of the band: keep it for quick drafts only.
pub fn model(name: &str) -> Option<ModelFiles> {
    match name {
        "makham_female" => Some(ModelFiles {
            onnx: ("fastthaig2p_single_model/makham_female_thai_int8.onnx", "b9faf2d859e4320e636ca369fb8f764bcc87e7bbec1270e132e405781ecf80b4"),
            config: ("fastthaig2p_single_model/config.json", "481313ad7a5e6d965e1725d5df1da916f2cd4bf91fed79494aac4eaebb413cdc"),
        }),
        "paradee" => Some(ModelFiles {
            onnx: ("paradee_thai/paradee_thai_int8.onnx", "ad179a2c11b54b1185e84d9c0b759a57058588405620f12163084aaad250813d"),
            config: ("paradee_thai/config.json", "1340bbfedc4871b311c8dedcd5cf85022f80773db120b1cd85883ecb1883ca43"),
        }),
        _ => None,
    }
}

pub const MODELS: &str = "makham_female, paradee";

const G2P_COMMIT: &str = "4e82a291b4cf2876326744c6580121e9986ad354";
const G2P_ZIP_SHA: &str = "39bf4ad3be13078cc3b646dd91804c027328d38903f4f7410377e3a1de81b213";
const G2P_FILES: [&str; 7] = [
    "dict.txt", "ipa.json", "fallback/sylrule.lts", "fallback/thaisyl.dict", "fallback/sylseg.3g", "fallback/thdict", "fallback/PhSTrigram.sts",
];
const WORDS_TH_URL: &str = "https://raw.githubusercontent.com/PyThaiNLP/pythainlp/v5.4.0/pythainlp/corpus/words_th.txt";
const WORDS_TH_SHA: &str = "35f65d43ea83b5b981134d5edc3b9afcb64b4ba96146900fc4d312f2940c3265";

fn cache() -> PathBuf {
    std::env::var("WALKIE_TTS_DATA").map(PathBuf::from).unwrap_or_else(|_| home().join(".cache/walkie/tts"))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn download(url: &str) -> Result<Vec<u8>> {
    eprintln!("downloading {url}");
    let mut body = ureq::get(url).call().with_context(|| format!("downloading {url}"))?.into_body();
    let mut bytes = vec![];
    body.as_reader().read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Downloads `url` to `path` unless it is there, checking its SHA-256.
fn fetch(url: &str, sha: &str, path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.to_path_buf());
    }
    let bytes = download(url)?;
    let got = sha256(&bytes);
    if got != sha {
        bail!("{url}: SHA-256 {got}, expected {sha}");
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(path.to_path_buf())
}

/// A file of the voice model: from the Hugging Face cache when it has this revision, else
/// downloaded into ours.
pub fn model_file((file, sha): (&str, &str)) -> Result<PathBuf> {
    let hf_home = std::env::var("HF_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".cache/huggingface"));
    let hf = hf_home.join("hub/models--pythainlp--MakHamTTS/snapshots").join(HF_REV).join(file);
    if hf.exists() {
        return Ok(hf);
    }
    let url = format!("https://huggingface.co/{HF_REPO}/resolve/{HF_REV}/{file}");
    fetch(&url, sha, &cache().join(format!("makham-{}", &HF_REV[..8])).join(file))
}

/// FastThaiG2P's data folder (dict.txt, ipa.json, fallback/).
pub fn g2p_data() -> Result<PathBuf> {
    let dir = cache().join(format!("fastthaig2p-{}", &G2P_COMMIT[..8]));
    if dir.join("ipa.json").exists() && dir.join("fallback/sylseg.3g").exists() {
        return Ok(dir);
    }
    let url = format!("https://github.com/awslabs/FastThaiG2P/archive/{G2P_COMMIT}.zip");
    let bytes = download(&url)?;
    let got = sha256(&bytes);
    if got != G2P_ZIP_SHA {
        bail!("{url}: SHA-256 {got}, expected {G2P_ZIP_SHA}");
    }
    let tmp = dir.with_extension("part");
    let _ = std::fs::remove_dir_all(&tmp);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    for rel in G2P_FILES {
        let mut f = zip.by_name(&format!("FastThaiG2P-{G2P_COMMIT}/data/{rel}"))?;
        let out = tmp.join(rel);
        std::fs::create_dir_all(out.parent().unwrap())?;
        let mut buf = vec![];
        f.read_to_end(&mut buf)?;
        std::fs::write(out, buf)?;
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir)?;
    Ok(dir)
}

/// PyThaiNLP's default word list.
pub fn words_th() -> Result<PathBuf> {
    fetch(WORDS_TH_URL, WORDS_TH_SHA, &cache().join("pythainlp-5.4.0/words_th.txt"))
}

// Kokoro-82M v1.0 (English), onnx-community/Kokoro-82M-v1.0-ONNX on Hugging Face, Apache-2.0.
// MakHam is a Thai Kokoro distilled to one voice; this is the full multi-voice model, so it
// also takes a voice: a style table, 510 rows of 256 floats, one row per token count.
const KOKORO_REPO: &str = "onnx-community/Kokoro-82M-v1.0-ONNX";
const KOKORO_REV: &str = "1939ad2a8e416c0acfeecc08a694d14ef25f2231";

/// Kokoro models: int8 (92 MB) and fp32 (326 MB).
pub fn kokoro_model(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "kokoro" => Some(("onnx/model_quantized.onnx", "fbae9257e1e05ffc727e951ef9b9c98418e6d79f1c9b6b13bd59f5c9028a1478")),
        "kokoro_fp32" => Some(("onnx/model.onnx", "8fbea51ea711f2af382e88c833d9e288c6dc82ce5e98421ea61c058ce21a34cb")),
        _ => None,
    }
}

/// Kokoro voices (US: af_ female, am_ male; British: bf_, bm_).
pub fn kokoro_voice(name: &str) -> Option<(String, &'static str)> {
    let sha = match name {
        "af_heart" => "d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b",
        "af_bella" => "f69d836209b78eb8c66e75e3cda491e26ea838a3674257e9d4e5703cbaf55c8b",
        "am_michael" => "1d1f21dd8da39c30705cd4c75d039d265e9bc4a2a93ed09bc9e1b1225eb95ba1",
        "bf_emma" => "669fe0647f9dd04fcab92f1439a40eeb4c8b4ab1f82e4996fe3d918ce4a63b73",
        _ => return None,
    };
    Some((format!("voices/{name}.bin"), sha))
}

pub const KOKORO_VOICES: &str = "af_heart, af_bella, am_michael, bf_emma";
pub const KOKORO_TOKENIZER: (&str, &str) = ("tokenizer.json", "77a02c8e164413299b4b4c403b14f8e0e1c1b727db4d46a09d6327b861060a34");

/// A file of the Kokoro model, from the Hugging Face cache when it has this revision.
pub fn kokoro_file(file: &str, sha: &str) -> Result<PathBuf> {
    let hf_home = std::env::var("HF_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".cache/huggingface"));
    let hf = hf_home.join("hub/models--onnx-community--Kokoro-82M-v1.0-ONNX/snapshots").join(KOKORO_REV).join(file);
    if hf.exists() {
        return Ok(hf);
    }
    let url = format!("https://huggingface.co/{KOKORO_REPO}/resolve/{KOKORO_REV}/{file}");
    fetch(&url, sha, &cache().join(format!("kokoro-{}", &KOKORO_REV[..8])).join(file))
}
