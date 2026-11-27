//! wt: the engine's tools, one binary. The MCP server (mcp/server.ts) and scripts call it.
//!
//!   wt profile DEMO_DIR | --list | --resolve NAME     pick and resolve the profile
//!   wt mix content RAW DIR | compose DIR OUT | remix VIDEO OUT DIR | backdrops DIR
//!   wt frames VIDEO DIR OUT.png                       contact sheet and flash check
//!   wt device record DIR REPO [DEVICE] | config DIR | collect LOG DIR
//!   wt tts DEMO_DIR < narration.tsv                   Thai voice clips (MakHam)
//!   wt tts --setup                                    download the voice model and Thai data
//!   wt g2p [LEXICON_JSON] < lines                     phonemes per line (to check the G2P)
//!   wt tts-en [--voice V] OUT.wav "text"              spike: English voice (Kokoro)

mod device;
mod frames;
mod mix;
mod profile;
mod tts;
mod util;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = args.split_first().map(|(c, r)| (c.as_str(), r)).unwrap_or(("", &[]));
    let result = match cmd {
        "profile" => profile::main(rest),
        "mix" => mix::main(rest),
        "frames" => frames::main(rest),
        "device" => device::main(rest),
        "tts" => tts::main(rest),
        "g2p" => tts::g2p_main(rest),
        "tts-en" => tts::english_main(rest),
        _ => Err(anyhow::anyhow!(include_str!("usage.txt").trim().to_string())),
    };
    if let Err(e) = result {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
