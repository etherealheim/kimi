use color_eyre::Result;
use reqwest::blocking::Client;
use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

/// Text-to-speech service using ElevenLabs API
#[derive(Clone)]
pub struct TTSService {
    api_key: String,
    voice_id: String,
    model: String,
    client: Client,
    current_sink: Arc<Mutex<Option<Arc<Sink>>>>,
}

impl TTSService {
    /// Creates a new TTS service with ElevenLabs credentials
    pub fn new(api_key: String, voice_id: String, model: String) -> Self {
        Self {
            api_key,
            voice_id,
            model,
            client: Client::new(),
            current_sink: Arc::new(Mutex::new(None)),
        }
    }

    /// Converts text to speech and plays it
    pub fn speak_text(&self, text: &str) -> Result<()> {
        let body = serde_json::json!({
            "text": text,
            "model_id": self.model,
            "voice_settings": {
                "stability": 0.5,
                "similarity_boost": 0.5
            }
        });

        let response = self
            .client
            .post(format!(
                "https://api.elevenlabs.io/v1/text-to-speech/{}",
                self.voice_id
            ))
            .header("xi-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()?
            .error_for_status()?;

        let audio_data = response.bytes()?.to_vec();
        self.play_audio(audio_data)?;
        Ok(())
    }

    /// Checks if TTS is configured with valid credentials
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty() && self.api_key != "your_api_key_here"
    }

    /// Checks if audio is currently playing
    #[must_use]
    pub fn is_playing(&self) -> bool {
        if let Ok(sink_guard) = self.current_sink.lock()
            && let Some(sink) = sink_guard.as_ref()
        {
            return !sink.empty();
        }
        false
    }

    /// Stops currently playing audio
    pub fn stop(&self) {
        if let Ok(mut sink_guard) = self.current_sink.lock()
            && let Some(sink) = sink_guard.take()
        {
            sink.stop();
        }
    }

    fn play_audio(&self, audio_data: Vec<u8>) -> Result<()> {
        self.stop();

        let current_sink = Arc::clone(&self.current_sink);

        std::thread::spawn(move || {
            let (_stream, stream_handle) = OutputStream::try_default().ok()?;
            let sink = Arc::new(Sink::try_new(&stream_handle).ok()?);

            if let Ok(mut sink_guard) = current_sink.lock() {
                *sink_guard = Some(Arc::clone(&sink));
            }

            if let Ok(source) = Decoder::new(Cursor::new(audio_data)) {
                sink.append(source);
                sink.sleep_until_end();
            }

            if let Ok(mut sink_guard) = current_sink.lock() {
                *sink_guard = None;
            }
            Some(())
        });

        Ok(())
    }
}
