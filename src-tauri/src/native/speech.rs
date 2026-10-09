//! Offline dictation with Windows.Media.SpeechRecognition.
//! The recognizer listens to the microphone itself while we record.

use std::sync::{Arc, Mutex};

use windows::core::{Result, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Globalization::Language;
use windows::Media::SpeechRecognition::{
    SpeechContinuousRecognitionResultGeneratedEventArgs, SpeechContinuousRecognitionSession,
    SpeechRecognitionResultStatus, SpeechRecognizer,
};

pub struct Dictation {
    _recognizer: SpeechRecognizer,
    session: SpeechContinuousRecognitionSession,
    parts: Arc<Mutex<Vec<String>>>,
}

impl Dictation {
    /// Starts listening (ru-RU when installed, otherwise the system language).
    /// Must be called on an MTA thread.
    pub fn start() -> Result<Self> {
        let recognizer = Language::CreateLanguage(&HSTRING::from("ru-RU"))
            .and_then(|l| SpeechRecognizer::Create(&l))
            .or_else(|_| SpeechRecognizer::new())?;
        let compiled = recognizer.CompileConstraintsAsync()?.join()?;
        if compiled.Status()? != SpeechRecognitionResultStatus::Success {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                "распознавание речи недоступно",
            ));
        }
        let session = recognizer.ContinuousRecognitionSession()?;
        let parts = Arc::new(Mutex::new(Vec::new()));
        let sink = parts.clone();
        session.ResultGenerated(&TypedEventHandler::<
            SpeechContinuousRecognitionSession,
            SpeechContinuousRecognitionResultGeneratedEventArgs,
        >::new(move |_, args| {
            if let Some(a) = args.as_ref() {
                if let Ok(t) = a.Result().and_then(|r| r.Text()) {
                    let t = t.to_string();
                    if !t.trim().is_empty() {
                        sink.lock().unwrap_or_else(|e| e.into_inner()).push(t);
                    }
                }
            }
            Ok(())
        }))?;
        session.StartAsync()?.join()?;
        Ok(Self {
            _recognizer: recognizer,
            session,
            parts,
        })
    }

    /// Stops and returns everything recognised (final results arrive before
    /// StopAsync completes).
    pub fn finish(self) -> String {
        let _ = self.session.StopAsync().and_then(|op| op.join());
        let parts = self.parts.lock().unwrap_or_else(|e| e.into_inner());
        parts.join(" ")
    }

    pub fn cancel(self) {
        let _ = self.session.CancelAsync().and_then(|op| op.join());
    }
}
