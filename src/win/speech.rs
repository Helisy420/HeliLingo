//! Text-to-speech for the speaker icon, using the Windows OneCore voices.

use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};

use windows::Media::Core::MediaSource;
use windows::Media::Playback::MediaPlayer;
use windows::Media::SpeechSynthesis::SpeechSynthesizer;
use windows::Storage::Streams::IRandomAccessStream;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::{HSTRING, Interface};

static TX: OnceLock<Sender<(String, String)>> = OnceLock::new();

/// Speaks `text` with a voice for `lang` (e.g. "ru"), if one is installed.
pub fn speak(text: &str, lang: &str) {
    let tx = TX.get_or_init(|| {
        let (tx, rx) = channel::<(String, String)>();
        std::thread::Builder::new()
            .name("speech".into())
            .spawn(move || {
                unsafe {
                    let _ = RoInitialize(RO_INIT_MULTITHREADED);
                }
                let (Ok(synth), Ok(player)) = (SpeechSynthesizer::new(), MediaPlayer::new()) else {
                    return;
                };
                for (text, lang) in rx {
                    let _ = say(&synth, &player, &text, &lang);
                }
            })
            .expect("spawn speech thread");
        tx
    });
    let _ = tx.send((text.to_owned(), lang.to_owned()));
}

fn say(synth: &SpeechSynthesizer, player: &MediaPlayer, text: &str, lang: &str) -> windows::core::Result<()> {
    let want = lang.split('-').next().unwrap_or(lang).to_lowercase();
    let voice = SpeechSynthesizer::AllVoices()?
        .into_iter()
        .find(|v| {
            v.Language()
                .map(|l| l.to_string().to_lowercase().starts_with(&want))
                .unwrap_or(false)
        });
    match voice {
        Some(v) => synth.SetVoice(&v)?,
        // Reading Russian with an English voice is worse than silence.
        None => return Ok(()),
    }
    let stream = synth.SynthesizeTextToStreamAsync(&HSTRING::from(text))?.join()?;
    let source = MediaSource::CreateFromStream(&stream.cast::<IRandomAccessStream>()?, &stream.ContentType()?)?;
    player.SetSource(&source)?;
    player.Play()
}
