//! Clipboard history: deduplication, pinning, limits, image storage.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::native::{clip::ClipData, img, util::data_url};
use crate::store::{file_name, load_json, new_id, now_ms, save_json};

const MAX_TEXT: usize = 200_000;
const PREVIEW: usize = 400;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClipEntry {
    pub id: String,
    /// "text" | "image" | "files"
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub files: Vec<String>,
    /// PNG file name inside the clips directory.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub hash: u64,
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub source: String,
    pub ts: i64,
    #[serde(default)]
    pub pinned: bool,
}

/// What the UI receives: a short preview instead of the full text.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClipView {
    pub id: String,
    pub kind: String,
    pub preview: String,
    pub chars: usize,
    pub files: Vec<String>,
    pub thumb: Option<String>,
    pub width: u32,
    pub height: u32,
    pub source: String,
    pub ts: i64,
    pub pinned: bool,
}

pub struct ClipStore {
    pub entries: Vec<ClipEntry>,
    file: PathBuf,
    dir: PathBuf,
}

/// Hash of every pixel (8 bytes at a time — a 4K image takes a few ms). Sampling
/// only some bytes made two screenshots that differ in one cell look identical,
/// and the newer one never reached the history.
pub fn image_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut words = data.chunks_exact(8);
    for w in &mut words {
        h ^= u64::from_le_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]]);
        h = h.wrapping_mul(0x100000001b3);
        h ^= h >> 29;
    }
    for b in words.remainder() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^ data.len() as u64
}

/// An image ready to enter the history: PNG already written to the clips folder.
/// All the slow work (PNG, thumbnail, disk) happens before the history is locked,
/// so pinning, pasting or deleting never waits for a big screenshot.
pub struct EncodedImage {
    id: String,
    file: String,
    width: u32,
    height: u32,
    hash: u64,
    thumb: Option<String>,
}

pub fn encode_image(dir: &std::path::Path, width: u32, height: u32, rgba: &[u8], hash: u64) -> Option<EncodedImage> {
    let id = new_id();
    let png = img::encode_png(width, height, rgba)?;
    let file = format!("{id}.png");
    fs::write(dir.join(&file), &png).ok()?;
    let (tw, th, small) = img::shrink(width, height, rgba, 160);
    let thumb = img::encode_png(tw, th, &small).map(|b| data_url("image/png", &b));
    Some(EncodedImage { id, file, width, height, hash, thumb })
}

impl ClipStore {
    pub fn load(file: PathBuf, dir: PathBuf) -> Self {
        let entries: Vec<ClipEntry> = load_json(&file);
        Self { entries, file, dir }
    }

    pub fn save(&self) {
        save_json(&self.file, &self.entries);
    }

    pub fn views(&self) -> Vec<ClipView> {
        let mut v: Vec<ClipView> = self
            .entries
            .iter()
            .map(|e| ClipView {
                id: e.id.clone(),
                kind: e.kind.clone(),
                preview: e.text.chars().take(PREVIEW).collect(),
                chars: e.text.chars().count(),
                files: e.files.clone(),
                thumb: e.thumb.clone(),
                width: e.width,
                height: e.height,
                source: e.source.clone(),
                ts: e.ts,
                pinned: e.pinned,
            })
            .collect();
        // pinned first, then newest first
        v.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.ts.cmp(&a.ts)));
        v
    }

    pub fn get(&self, id: &str) -> Option<ClipEntry> {
        self.entries.iter().find(|e| e.id == id).cloned()
    }

    pub fn image_path(&self, e: &ClipEntry) -> Option<PathBuf> {
        e.image.as_ref().map(|f| self.dir.join(f))
    }

    pub fn touch(&mut self, id: &str) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.ts = now_ms();
        }
        self.save();
    }

    fn bump(&mut self, idx: usize, source: String) {
        let e = &mut self.entries[idx];
        e.ts = now_ms();
        if !source.is_empty() {
            e.source = source;
        }
    }

    /// Adds a new clipboard item. Returns false when nothing changed.
    pub fn add(&mut self, data: ClipData, source: String, limit: usize) -> bool {
        match data {
            ClipData::Text(mut text) => {
                if text.len() > MAX_TEXT {
                    let mut cut = MAX_TEXT;
                    while !text.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    text.truncate(cut);
                }
                if let Some(i) = self.entries.iter().position(|e| e.kind == "text" && e.text == text) {
                    self.bump(i, source);
                } else {
                    self.entries.push(ClipEntry {
                        id: new_id(),
                        kind: "text".into(),
                        text,
                        files: vec![],
                        image: None,
                        width: 0,
                        height: 0,
                        hash: 0,
                        thumb: None,
                        source,
                        ts: now_ms(),
                        pinned: false,
                    });
                }
            }
            ClipData::Files(files) => {
                if let Some(i) = self.entries.iter().position(|e| e.kind == "files" && e.files == files) {
                    self.bump(i, source);
                } else {
                    let text = files.iter().map(|f| file_name(f)).collect::<Vec<_>>().join(", ");
                    self.entries.push(ClipEntry {
                        id: new_id(),
                        kind: "files".into(),
                        text,
                        files,
                        image: None,
                        width: 0,
                        height: 0,
                        hash: 0,
                        thumb: None,
                        source,
                        ts: now_ms(),
                        pinned: false,
                    });
                }
            }
            ClipData::Image { width, height, rgba } => {
                let hash = image_hash(&rgba);
                if self.bump_image(hash, width, height, &source) {
                    return true;
                }
                let Some(img) = encode_image(&self.dir, width, height, &rgba, hash) else { return false };
                return self.add_image(img, source, limit);
            }
        }
        self.trim(limit);
        self.save();
        true
    }

    fn image_index(&self, hash: u64, width: u32, height: u32) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.kind == "image" && e.hash == hash && e.width == width && e.height == height)
    }

    /// The same picture copied again: move it to the top. False when it's new.
    pub fn bump_image(&mut self, hash: u64, width: u32, height: u32, source: &str) -> bool {
        let Some(i) = self.image_index(hash, width, height) else { return false };
        self.bump(i, source.to_string());
        self.save();
        true
    }

    pub fn add_image(&mut self, img: EncodedImage, source: String, limit: usize) -> bool {
        if let Some(i) = self.image_index(img.hash, img.width, img.height) {
            // added meanwhile: keep the existing entry
            let _ = fs::remove_file(self.dir.join(&img.file));
            self.bump(i, source);
        } else {
            let (width, height) = (img.width, img.height);
            self.entries.push(ClipEntry {
                id: img.id,
                kind: "image".into(),
                text: format!("Изображение {width}×{height}"),
                files: vec![],
                image: Some(img.file),
                width,
                height,
                hash: img.hash,
                thumb: img.thumb,
                source,
                ts: now_ms(),
                pinned: false,
            });
            self.trim(limit);
        }
        self.save();
        true
    }

    fn trim(&mut self, limit: usize) {
        self.entries.sort_by_key(|e| std::cmp::Reverse(e.ts));
        let mut kept = 0usize;
        let mut removed = Vec::new();
        self.entries.retain(|e| {
            if e.pinned {
                return true;
            }
            kept += 1;
            if kept > limit {
                removed.push(e.image.clone());
                false
            } else {
                true
            }
        });
        for f in removed.into_iter().flatten() {
            let _ = fs::remove_file(self.dir.join(f));
        }
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(i) = self.entries.iter().position(|e| e.id == id) {
            let e = self.entries.remove(i);
            if let Some(f) = e.image {
                let _ = fs::remove_file(self.dir.join(f));
            }
            self.save();
        }
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.pinned = pinned;
        }
        self.save();
    }

    /// Removes everything except pinned entries.
    pub fn clear(&mut self) {
        let dir = self.dir.clone();
        self.entries.retain(|e| {
            if !e.pinned {
                if let Some(f) = &e.image {
                    let _ = fs::remove_file(dir.join(f));
                }
            }
            e.pinned
        });
        self.save();
    }

    pub fn apply_limit(&mut self, limit: usize) {
        self.trim(limit);
        self.save();
    }
}
