//! Watches the Downloads folder for browser temp files (*.crdownload, *.part, …),
//! reports progress and the finished file.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

const TEMP_EXT: &[&str] = &["crdownload", "part", "partial", "download", "opdownload"];

pub fn is_temp(p: &Path) -> bool {
    p.extension()
        .map(|e| TEMP_EXT.contains(&e.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(false)
}

/// "Отчёт.pdf.crdownload" → "Отчёт.pdf"; "Unconfirmed 123.crdownload" → None (name not known yet).
pub fn final_name(p: &Path) -> Option<String> {
    let stem = p.file_stem()?.to_string_lossy().to_string();
    if stem.starts_with("Unconfirmed ") || stem.starts_with("Не подтверждено ") || !stem.contains('.') {
        return None;
    }
    Some(stem)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub name: String,
    pub bytes: u64,
    /// bytes per second (smoothed)
    pub speed: f64,
    /// how many downloads are running
    pub count: usize,
}

struct Track {
    bytes: u64,
    at: Instant,
    speed: f64,
    first_seen: Instant,
}

pub struct Watcher {
    dir: PathBuf,
    tracks: HashMap<PathBuf, Track>,
}

pub struct Finished {
    pub path: PathBuf,
    pub size: u64,
}

impl Watcher {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, tracks: HashMap::new() }
    }

    pub fn active(&self) -> bool {
        !self.tracks.is_empty()
    }

    /// Rescans the folder. Returns current progress (if anything is downloading)
    /// and downloads that finished since the last scan.
    pub fn scan(&mut self) -> (Option<Progress>, Vec<Finished>) {
        let now = Instant::now();
        let mut seen: HashMap<PathBuf, u64> = HashMap::new();
        if let Ok(rd) = fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let p = e.path();
                if is_temp(&p) {
                    if let Ok(m) = e.metadata() {
                        if m.is_file() {
                            seen.insert(p, m.len());
                        }
                    }
                }
            }
        }

        // gone temp files: the browser renamed them to the final name (or the user cancelled)
        let gone: Vec<(PathBuf, u64)> = self
            .tracks
            .iter()
            .filter(|(p, _)| !seen.contains_key(*p))
            .map(|(p, t)| (p.clone(), t.bytes))
            .collect();
        let mut finished = Vec::new();
        for (p, bytes) in gone {
            self.tracks.remove(&p);
            if let Some(f) = self.find_final(&p, bytes) {
                finished.push(f);
            }
        }

        for (p, bytes) in &seen {
            let t = self.tracks.entry(p.clone()).or_insert(Track {
                bytes: *bytes,
                at: now,
                speed: 0.0,
                first_seen: now,
            });
            let dt = now.duration_since(t.at).as_secs_f64();
            if dt >= 0.4 {
                let inst = (bytes.saturating_sub(t.bytes)) as f64 / dt;
                t.speed = if t.speed == 0.0 { inst } else { t.speed * 0.6 + inst * 0.4 };
                t.bytes = *bytes;
                t.at = now;
            }
        }

        // the most recent download is shown; a just-created empty one waits a moment
        let shown = self
            .tracks
            .iter()
            .filter(|(_, t)| t.bytes > 0 || now.duration_since(t.first_seen) > Duration::from_millis(800))
            .max_by_key(|(_, t)| t.first_seen);
        let progress = shown.map(|(p, t)| Progress {
            name: final_name(p).unwrap_or_else(|| "Загрузка…".into()),
            bytes: t.bytes,
            speed: t.speed,
            count: self.tracks.len(),
        });
        (progress, finished)
    }

    /// The finished file: "<name without temp ext>" if it exists, otherwise the
    /// newest regular file written in the last few seconds with a similar size.
    fn find_final(&self, temp: &Path, last_bytes: u64) -> Option<Finished> {
        if let Some(name) = final_name(temp) {
            let p = self.dir.join(&name);
            if let Ok(m) = fs::metadata(&p) {
                if m.is_file() {
                    return Some(Finished { path: p, size: m.len() });
                }
            }
        }
        let recent = SystemTime::now() - Duration::from_secs(8);
        fs::read_dir(&self.dir)
            .ok()?
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let m = e.metadata().ok()?;
                let modified = m.modified().ok()?;
                (m.is_file() && !is_temp(&p) && modified >= recent && m.len() >= last_bytes / 2).then_some((p, m.len(), modified))
            })
            .max_by_key(|(_, _, t)| *t)
            .map(|(path, size, _)| Finished { path, size })
    }
}
