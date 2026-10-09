//! Persistent settings and user data (shelf, launcher pins, launch counts).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Unique-enough id for list items.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    format!("{:x}{:04x}", now_ms(), COUNTER.fetch_add(1, Ordering::Relaxed) & 0xffff)
}

pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Writes atomically (tmp file + rename) so a crash never leaves half a file.
pub fn save_json<T: Serialize>(path: &Path, value: &T) {
    if let Ok(bytes) = serde_json::to_vec_pretty(value) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, bytes).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Global shortcut for the launcher.
    pub hotkey: String,
    pub autostart: bool,
    /// Wave along the island edge follows the system audio level.
    pub music_reactive: bool,
    /// Hide the island while a fullscreen app (game, video) is in front.
    pub hide_fullscreen: bool,
    /// Expand the island when the cursor rests on it.
    pub hover_expand: bool,
    /// Maximum number of unpinned clipboard entries.
    pub clip_limit: usize,
    /// Liquid pull towards the cursor.
    pub cursor_pull: bool,
    /// Screen capture overlay (region, text, QR, colour picker) and its hotkeys.
    pub capture_enabled: bool,
    pub hotkey_region: String,
    pub hotkey_ocr: String,
    /// AI chat: OpenAI-compatible server, optional key (kept only in this file), model id.
    pub ai_enabled: bool,
    pub ai_url: String,
    pub ai_key: String,
    pub ai_model: String,
    /// Weather (Open-Meteo). Empty city = detect by IP.
    pub weather_enabled: bool,
    pub weather_city: String,
    pub weather_lat: f64,
    pub weather_lon: f64,
    /// Voice → text: hotkey, optional Whisper-compatible endpoint (else Windows speech).
    pub voice_enabled: bool,
    pub hotkey_voice: String,
    pub voice_whisper_url: String,
    pub voice_whisper_key: String,
    pub voice_whisper_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".into(),
            autostart: true,
            music_reactive: true,
            hide_fullscreen: true,
            hover_expand: true,
            clip_limit: 200,
            cursor_pull: true,
            capture_enabled: true,
            hotkey_region: "Ctrl+Shift+S".into(),
            hotkey_ocr: "Ctrl+Shift+T".into(),
            ai_enabled: true,
            ai_url: "http://localhost:1234/v1".into(),
            ai_key: String::new(),
            ai_model: String::new(),
            weather_enabled: true,
            weather_city: String::new(),
            weather_lat: 0.0,
            weather_lon: 0.0,
            voice_enabled: true,
            hotkey_voice: "Ctrl+Alt+Space".into(),
            voice_whisper_url: String::new(),
            voice_whisper_key: String::new(),
            voice_whisper_model: "whisper-1".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ShelfItem {
    pub id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub added: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PinItem {
    pub id: String,
    /// `app:<AppsFolder id>` or a filesystem path.
    pub target: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Data {
    pub shelf: Vec<ShelfItem>,
    pub pins: Vec<PinItem>,
    /// Launch counter per target, used to rank launcher results.
    pub usage: HashMap<String, u32>,
}

pub struct Paths {
    pub dir: PathBuf,
}

impl Paths {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::create_dir_all(dir.join("clips"));
        Self { dir }
    }
    pub fn settings(&self) -> PathBuf {
        self.dir.join("settings.json")
    }
    pub fn data(&self) -> PathBuf {
        self.dir.join("data.json")
    }
    pub fn clips(&self) -> PathBuf {
        self.dir.join("clipboard.json")
    }
    pub fn chat(&self) -> PathBuf {
        self.dir.join("chat.json")
    }
    pub fn clip_dir(&self) -> PathBuf {
        self.dir.join("clips")
    }
}

/// "C:\\Users\\me\\Отчёт Q3.pdf" -> "Отчёт Q3.pdf"
pub fn file_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed).to_string()
}

pub fn shelf_item(path: &str) -> Option<ShelfItem> {
    let meta = fs::metadata(path).ok()?;
    Some(ShelfItem {
        id: new_id(),
        path: path.to_string(),
        name: file_name(path),
        size: if meta.is_dir() { 0 } else { meta.len() },
        is_dir: meta.is_dir(),
        added: now_ms(),
    })
}

/// Name for a launcher pin: shortcut / exe name without extension.
pub fn pin_item(path: &str) -> PinItem {
    let name = file_name(path);
    let lower = name.to_lowercase();
    let name = [".lnk", ".exe", ".url", ".bat", ".cmd", ".appref-ms"]
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map(|ext| name[..name.len() - ext.len()].to_string())
        .unwrap_or(name);
    PinItem {
        id: new_id(),
        target: path.to_string(),
        name,
    }
}
