//! Microphone recording (cpal, 16 kHz mono) and Whisper-compatible upload.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};

pub const RATE: u32 = 16_000;

pub struct Recorder {
    _stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    src_rate: u32,
}

fn push_mono(buf: &Mutex<Vec<f32>>, level: &AtomicU32, data: &[f32], channels: usize) {
    let mut sum = 0.0f32;
    let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
    for frame in data.chunks(channels.max(1)) {
        let v = frame.iter().sum::<f32>() / frame.len() as f32;
        sum += v * v;
        b.push(v);
    }
    let n = (data.len() / channels.max(1)).max(1);
    let rms = (sum / n as f32).sqrt();
    // speech sits around 0.02–0.2 RMS: stretch it to a 0..1 level
    let lvl = (rms * 6.0).min(1.0);
    level.store(lvl.to_bits(), Ordering::Relaxed);
}

impl Recorder {
    /// Opens the default microphone and starts capturing.
    pub fn start() -> Result<Self, String> {
        let host = cpal::default_host();
        let dev = host.default_input_device().ok_or("Микрофон не найден")?;
        let cfg = dev.default_input_config().map_err(|e| format!("Микрофон недоступен: {e}"))?;
        let src_rate = cfg.sample_rate().0;
        let channels = cfg.channels() as usize;
        let samples = Arc::new(Mutex::new(Vec::with_capacity(src_rate as usize * 30)));
        let level = Arc::new(AtomicU32::new(0));
        let (s, l) = (samples.clone(), level.clone());
        let err = |_e| {};
        let stream_cfg = cfg.config();
        let stream = match cfg.sample_format() {
            SampleFormat::F32 => dev.build_input_stream(&stream_cfg, move |d: &[f32], _| push_mono(&s, &l, d, channels), err, None),
            SampleFormat::I16 => dev.build_input_stream(
                &stream_cfg,
                move |d: &[i16], _| {
                    let f: Vec<f32> = d.iter().map(|&x| x as f32 / 32768.0).collect();
                    push_mono(&s, &l, &f, channels)
                },
                err,
                None,
            ),
            SampleFormat::U16 => dev.build_input_stream(
                &stream_cfg,
                move |d: &[u16], _| {
                    let f: Vec<f32> = d.iter().map(|&x| (x as f32 - 32768.0) / 32768.0).collect();
                    push_mono(&s, &l, &f, channels)
                },
                err,
                None,
            ),
            other => return Err(format!("Формат микрофона {other:?} не поддерживается")),
        }
        .map_err(|e| format!("Не удалось открыть микрофон: {e}"))?;
        stream.play().map_err(|e| format!("Не удалось начать запись: {e}"))?;
        Ok(Self {
            _stream: stream,
            samples,
            level,
            src_rate,
        })
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Stops and returns 16 kHz mono samples.
    pub fn finish(self) -> Vec<f32> {
        let src = std::mem::take(&mut *self.samples.lock().unwrap_or_else(|e| e.into_inner()));
        drop(self._stream);
        resample(&src, self.src_rate, RATE)
    }
}

/// Linear-interpolation resampler (good enough for speech).
pub fn resample(src: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || src.is_empty() {
        return src.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (src.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * ratio;
            let j = p as usize;
            let f = (p - j as f64) as f32;
            let a = src[j];
            let b = *src.get(j + 1).unwrap_or(&a);
            a + (b - a) * f
        })
        .collect()
}

/// 16-bit PCM WAV.
pub fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data as u32).to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// POST to a Whisper-compatible `/audio/transcriptions` endpoint.
pub async fn whisper(url: &str, key: &str, model: &str, wav: Vec<u8>) -> Result<String, String> {
    let part = reqwest::multipart::Part::bytes(wav)
        .file_name("speech.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", if model.is_empty() { "whisper-1".to_string() } else { model.to_string() })
        .text("language", "ru")
        .text("response_format", "json");
    let mut req = crate::ai::client()
        .post(url.trim())
        .multipart(form)
        .timeout(std::time::Duration::from_secs(120));
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| crate::ai::explain(&e, url))?;
    if !resp.status().is_success() {
        return Err(format!("Whisper ответил {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(v["text"].as_str().unwrap_or("").trim().to_string())
}

