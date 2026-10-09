// Island — a liquid "Dynamic Island" for Windows 10/11.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clips;
mod native;
mod store;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use clips::{ClipEntry, ClipStore, ClipView};
use native::{audio::Audio, clip, img, input, media::Media, shell, util};
use store::{load_json, save_json, Data, Paths, PinItem, Settings, ShelfItem};

/// Logical size of the transparent window the island lives in.
const WIN_W: f64 = 1000.0;
const WIN_H: f64 = 640.0;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Clone, Copy, Default, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        self.w > 0.0 && x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
    fn grow(&self, side: f64, down: f64) -> Rect {
        Rect {
            x: self.x - side,
            y: self.y,
            w: self.w + side * 2.0,
            h: self.h + down,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Geom {
    x: i32,
    y: i32,
    scale: f64,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
struct MediaPayload {
    app: String,
    app_id: String,
    title: String,
    artist: String,
    album: String,
    playing: bool,
    position_ms: i64,
    duration_ms: i64,
    updated_ms: i64,
    can_prev: bool,
    can_next: bool,
    /// Changes whenever the cover image changes (see the "cover" event).
    cover_id: u32,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
struct CoverPayload {
    id: u32,
    url: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
struct VolumePayload {
    level: f32,
    muted: bool,
    device: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CursorPayload {
    x: f64,
    y: f64,
    inside: bool,
    lmb: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct AppItem {
    target: String,
    name: String,
}

enum AudioCmd {
    Volume(f32),
    Mute(bool),
}

struct MediaCmd {
    action: String,
    value: f64,
}

type Job = Box<dyn FnOnce() + Send>;

/// A single STA thread for shell work (icons, app list, ShellExecute).
struct ShellWorker {
    tx: Mutex<Sender<Job>>,
}

impl ShellWorker {
    fn spawn() -> Self {
        let (tx, rx) = channel::<Job>();
        thread::Builder::new()
            .name("shell".into())
            .spawn(move || {
                let _com = util::Com::sta();
                for job in rx {
                    job();
                }
            })
            .expect("failed to start shell thread");
        Self { tx: Mutex::new(tx) }
    }

    fn run<T: Send + 'static>(&self, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (rtx, rrx) = channel();
        let job: Job = Box::new(move || {
            let _ = rtx.send(f());
        });
        lock(&self.tx).send(job).ok()?;
        rrx.recv().ok()
    }
}

struct Shared {
    paths: Paths,
    settings: Mutex<Settings>,
    data: Mutex<Data>,
    clips: Mutex<ClipStore>,
    hit: Mutex<Rect>,
    geom: Mutex<Geom>,
    own_hwnd: AtomicIsize,
    last_fg: AtomicIsize,
    suppressed: AtomicBool,
    launcher_open: AtomicBool,
    media_playing: AtomicBool,
    clip_skip_seq: AtomicU32,
    media: Mutex<Option<MediaPayload>>,
    cover: Mutex<CoverPayload>,
    volume: Mutex<VolumePayload>,
    media_tx: Mutex<Sender<MediaCmd>>,
    audio_tx: Mutex<Sender<AudioCmd>>,
    shell: ShellWorker,
    icons: Mutex<HashMap<String, Option<String>>>,
    apps: Mutex<Option<Vec<AppItem>>>,
    hotkey: Mutex<String>,
    hotkey_error: Mutex<Option<String>>,
}

type St<'a> = State<'a, Arc<Shared>>;

// ---------------------------------------------------------------------------
// Window placement, launcher, hotkey, autostart
// ---------------------------------------------------------------------------

fn place_window(win: &WebviewWindow, shared: &Shared, force: bool) {
    let Ok(Some(mon)) = win.primary_monitor() else { return };
    let scale = mon.scale_factor();
    let w = (WIN_W * scale).round() as i32;
    let h = (WIN_H * scale).round() as i32;
    let pos = *mon.position();
    let size = *mon.size();
    let geom = Geom {
        x: pos.x + (size.width as i32 - w) / 2,
        y: pos.y,
        scale,
    };
    let changed = *lock(&shared.geom) != geom;
    if force || changed {
        let _ = win.set_size(PhysicalSize::new(w as u32, h as u32));
        let _ = win.set_position(PhysicalPosition::new(geom.x, geom.y));
        *lock(&shared.geom) = geom;
    }
}

fn set_launcher(app: &AppHandle, open: bool) {
    let shared = app.state::<Arc<Shared>>();
    shared.launcher_open.store(open, Ordering::Relaxed);
    if open {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_focus();
        }
    }
    let _ = app.emit("launcher", open);
}

fn toggle_launcher(app: &AppHandle) {
    let open = !app.state::<Arc<Shared>>().launcher_open.load(Ordering::Relaxed);
    set_launcher(app, open);
}

fn bind_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(hotkey, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_launcher(app);
            }
        })
        .map_err(|e| e.to_string())
}

/// Must run on the main thread (RegisterHotKey is thread-affine).
fn rebind_hotkey(app: &AppHandle, shared: &Shared, hotkey: &str) -> Result<(), String> {
    let old = lock(&shared.hotkey).clone();
    if !old.is_empty() {
        let _ = app.global_shortcut().unregister(old.as_str());
    }
    match bind_hotkey(app, hotkey) {
        Ok(()) => {
            *lock(&shared.hotkey) = hotkey.to_string();
            Ok(())
        }
        Err(e) => {
            if !old.is_empty() {
                let _ = bind_hotkey(app, &old);
            }
            Err(format!("Не удалось назначить «{hotkey}»: {e}"))
        }
    }
}

fn apply_autostart(app: &AppHandle, enabled: bool) {
    let al = app.autolaunch();
    let _ = if enabled { al.enable() } else { al.disable() };
}

fn media_app_name(id: &str) -> String {
    let l = id.to_lowercase();
    const KNOWN: &[(&str, &str)] = &[
        ("spotify", "Spotify"),
        ("yandex.music", "Яндекс Музыка"),
        ("yandexmusic", "Яндекс Музыка"),
        ("zunemusic", "Медиаплеер"),
        ("applemusic", "Apple Music"),
        ("itunes", "iTunes"),
        ("vkmusic", "VK Музыка"),
        ("deezer", "Deezer"),
        ("tidal", "TIDAL"),
        ("soundcloud", "SoundCloud"),
        ("aimp", "AIMP"),
        ("foobar", "foobar2000"),
        ("vlc", "VLC"),
        ("telegram", "Telegram"),
        ("msedge", "Edge"),
        ("chrome", "Chrome"),
        ("firefox", "Firefox"),
        ("308046b0af4a39cb", "Firefox"),
        ("opera", "Opera"),
        ("brave", "Brave"),
        ("vivaldi", "Vivaldi"),
        ("yandex", "Яндекс Браузер"),
    ];
    for (k, v) in KNOWN {
        if l.contains(k) {
            return (*v).to_string();
        }
    }
    let tail = id.rsplit('!').next().unwrap_or(id);
    let tail = tail.trim_end_matches(".exe").trim_end_matches(".EXE");
    let tail = tail.rsplit('.').next().unwrap_or(tail);
    if tail.is_empty() || tail.chars().all(|c| c.is_ascii_hexdigit()) {
        return "Плеер".into();
    }
    let mut c = tail.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "Плеер".into(),
    }
}

// ---------------------------------------------------------------------------
// Background threads
// ---------------------------------------------------------------------------

/// Cursor tracking, click-through switching, foreground / fullscreen watch.
fn spawn_cursor_thread(app: AppHandle, shared: Arc<Shared>) {
    thread::Builder::new()
        .name("cursor".into())
        .spawn(move || {
            let Some(win) = app.get_webview_window("main") else { return };
            let mut input_on = false;
            let mut last: Option<(i32, i32, bool, bool)> = None;
            let mut was_near = false;
            let mut tick: u64 = 0;
            loop {
                thread::sleep(Duration::from_millis(16));
                tick += 1;
                let geom = *lock(&shared.geom);
                if geom.scale <= 0.0 {
                    continue;
                }
                let (cx, cy) = input::cursor_pos();
                let lx = (cx - geom.x) as f64 / geom.scale;
                let ly = (cy - geom.y) as f64 / geom.scale;
                let rect = *lock(&shared.hit);
                let suppressed = shared.suppressed.load(Ordering::Relaxed);
                let lmb = input::lmb_down();
                let inside = !suppressed && rect.contains(lx, ly);
                // While a button is held (file drag) the catch zone is larger,
                // so the island can grow into drop targets before the cursor arrives.
                let magnet = !suppressed && lmb && rect.grow(90.0, 150.0).contains(lx, ly);
                let want = inside || magnet;
                if want != input_on && win.set_ignore_cursor_events(!want).is_ok() {
                    input_on = want;
                }
                let near = !suppressed && rect.grow(260.0, 320.0).contains(lx, ly);
                if near || was_near {
                    let key = (lx.round() as i32, ly.round() as i32, inside, lmb);
                    if last != Some(key) {
                        last = Some(key);
                        let _ = app.emit("cursor", CursorPayload { x: lx, y: ly, inside, lmb });
                    }
                }
                was_near = near;

                if tick % 15 == 0 {
                    let own = shared.own_hwnd.load(Ordering::Relaxed);
                    let fg = input::foreground();
                    if fg != 0 && fg != own {
                        shared.last_fg.store(fg, Ordering::Relaxed);
                    }
                    let hide = lock(&shared.settings).hide_fullscreen && input::foreground_is_fullscreen(own);
                    if hide != suppressed {
                        shared.suppressed.store(hide, Ordering::Relaxed);
                        let _ = app.emit("suppressed", hide);
                    }
                }
                if tick % 150 == 0 {
                    place_window(&win, &shared, false);
                }
            }
        })
        .expect("failed to start cursor thread");
}

/// Now playing (GSMTC), polled; commands are executed on the same thread.
fn spawn_media_thread(app: AppHandle, shared: Arc<Shared>, rx: Receiver<MediaCmd>) {
    thread::Builder::new()
        .name("media".into())
        .spawn(move || {
            let _com = util::Com::mta();
            let media = loop {
                match Media::new() {
                    Ok(m) => break m,
                    Err(_) => thread::sleep(Duration::from_secs(5)),
                }
            };
            let mut last: Option<MediaPayload> = None;
            let mut cover_key = String::new();
            let mut cover_tries = 0u32;
            let mut cover_id = 0u32;
            let mut has_cover = false;
            loop {
                match rx.recv_timeout(Duration::from_millis(700)) {
                    Ok(cmd) => {
                        let _ = media.control(&cmd.action, cmd.value);
                        thread::sleep(Duration::from_millis(250));
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                let state = media.state();
                if let Some(s) = &state {
                    let key = format!("{}\u{1}{}\u{1}{}\u{1}{}", s.app_id, s.title, s.artist, s.album);
                    if key != cover_key {
                        cover_key = key;
                        cover_tries = 0;
                        has_cover = false;
                        cover_id = cover_id.wrapping_add(1);
                        let cp = CoverPayload { id: cover_id, url: None };
                        *lock(&shared.cover) = cp.clone();
                        let _ = app.emit("cover", cp);
                    }
                    // Players often publish artwork a moment after the metadata.
                    if !has_cover && cover_tries < 6 {
                        cover_tries += 1;
                        if let Some(t) = media.thumbnail() {
                            has_cover = true;
                            let cp = CoverPayload {
                                id: cover_id,
                                url: Some(util::data_url(&t.mime, &t.bytes)),
                            };
                            *lock(&shared.cover) = cp.clone();
                            let _ = app.emit("cover", cp);
                        }
                    }
                }
                let payload = state.map(|s| MediaPayload {
                    app: media_app_name(&s.app_id),
                    app_id: s.app_id,
                    title: s.title,
                    artist: s.artist,
                    album: s.album,
                    playing: s.playing,
                    position_ms: s.position_ms,
                    duration_ms: s.duration_ms,
                    updated_ms: s.updated_ms,
                    can_prev: s.can_prev,
                    can_next: s.can_next,
                    cover_id,
                });
                shared
                    .media_playing
                    .store(payload.as_ref().map(|p| p.playing).unwrap_or(false), Ordering::Relaxed);
                if payload != last {
                    *lock(&shared.media) = payload.clone();
                    let _ = app.emit("media", payload.clone());
                    last = payload;
                }
            }
        })
        .expect("failed to start media thread");
}

fn publish_volume(app: &AppHandle, shared: &Shared, audio: &Audio) {
    let (level, muted) = audio.volume();
    let v = VolumePayload {
        level,
        muted,
        device: audio.device_short_name(),
    };
    let changed = *lock(&shared.volume) != v;
    if changed {
        *lock(&shared.volume) = v.clone();
        let _ = app.emit("volume", v);
    }
}

/// Output peak level (drives the wave) and master volume.
fn spawn_audio_thread(app: AppHandle, shared: Arc<Shared>, rx: Receiver<AudioCmd>) {
    thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let _com = util::Com::mta();
            let mut audio = loop {
                match Audio::new() {
                    Ok(a) => break a,
                    Err(_) => thread::sleep(Duration::from_secs(5)),
                }
            };
            publish_volume(&app, &shared, &audio);
            let mut last_peak = -1.0f32;
            let mut last_volume_poll = Instant::now();
            let mut last_device_check = Instant::now();
            loop {
                let mut dirty = false;
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        AudioCmd::Volume(v) => audio.set_volume(v),
                        AudioCmd::Mute(m) => audio.set_mute(m),
                    }
                    dirty = true;
                }
                if last_device_check.elapsed() >= Duration::from_secs(3) {
                    last_device_check = Instant::now();
                    dirty |= audio.refresh();
                }
                if dirty || last_volume_poll.elapsed() >= Duration::from_millis(500) {
                    last_volume_poll = Instant::now();
                    publish_volume(&app, &shared, &audio);
                }
                let active = shared.media_playing.load(Ordering::Relaxed)
                    && !shared.suppressed.load(Ordering::Relaxed)
                    && lock(&shared.settings).music_reactive;
                if active {
                    let p = audio.peak();
                    if (p - last_peak).abs() > 0.004 {
                        last_peak = p;
                        let _ = app.emit("audio", p);
                    }
                    thread::sleep(Duration::from_millis(33));
                } else {
                    if last_peak != 0.0 {
                        last_peak = 0.0;
                        let _ = app.emit("audio", 0.0f32);
                    }
                    thread::sleep(Duration::from_millis(130));
                }
            }
        })
        .expect("failed to start audio thread");
}

/// Clipboard history watcher.
fn spawn_clip_thread(app: AppHandle, shared: Arc<Shared>) {
    thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || {
            let mut last = clip::sequence();
            loop {
                thread::sleep(Duration::from_millis(350));
                let seq = clip::sequence();
                if seq == last {
                    continue;
                }
                last = seq;
                if seq <= shared.clip_skip_seq.load(Ordering::Relaxed) || clip::should_ignore() {
                    continue;
                }
                // let the source app finish writing all of its formats
                thread::sleep(Duration::from_millis(60));
                let source = clip::owner_app()
                    .or_else(|| util::window_exe_path(input::foreground()).map(|p| util::exe_stem(&p)))
                    .unwrap_or_default();
                let Some(data) = clip::read() else { continue };
                let limit = lock(&shared.settings).clip_limit;
                let views = {
                    let mut store = lock(&shared.clips);
                    if !store.add(data, source, limit) {
                        continue;
                    }
                    store.views()
                };
                let _ = app.emit("clips", views);
            }
        })
        .expect("failed to start clipboard thread");
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitPayload {
    settings: Settings,
    data: Data,
    clips: Vec<ClipView>,
    media: Option<MediaPayload>,
    cover: CoverPayload,
    volume: VolumePayload,
    version: String,
    hotkey_error: Option<String>,
}

#[tauri::command]
fn init(app: AppHandle, state: St<'_>) -> InitPayload {
    InitPayload {
        settings: lock(&state.settings).clone(),
        data: lock(&state.data).clone(),
        clips: lock(&state.clips).views(),
        media: lock(&state.media).clone(),
        cover: lock(&state.cover).clone(),
        volume: lock(&state.volume).clone(),
        version: app.package_info().version.to_string(),
        hotkey_error: lock(&state.hotkey_error).clone(),
    }
}

#[tauri::command]
fn set_hit(state: St<'_>, rect: Rect) {
    *lock(&state.hit) = rect;
}

#[tauri::command]
fn save_settings(app: AppHandle, state: St<'_>, settings: Settings) -> Result<Settings, String> {
    let old = lock(&state.settings).clone();
    let mut next = settings;
    next.clip_limit = next.clip_limit.clamp(10, 2000);
    if next.hotkey.trim().is_empty() {
        next.hotkey = old.hotkey.clone();
    }
    let failed_before = lock(&state.hotkey_error).is_some();
    if next.hotkey != old.hotkey || failed_before {
        rebind_hotkey(&app, &state, &next.hotkey)?;
        *lock(&state.hotkey_error) = None;
    }
    if next.autostart != old.autostart {
        apply_autostart(&app, next.autostart);
    }
    if next.clip_limit != old.clip_limit {
        let views = {
            let mut c = lock(&state.clips);
            c.apply_limit(next.clip_limit);
            c.views()
        };
        let _ = app.emit("clips", views);
    }
    save_json(&state.paths.settings(), &next);
    *lock(&state.settings) = next.clone();
    Ok(next)
}

#[tauri::command]
fn launcher_closed(state: St<'_>, restore: bool) {
    state.launcher_open.store(false, Ordering::Relaxed);
    if restore {
        input::activate(state.last_fg.load(Ordering::Relaxed));
    }
}

#[tauri::command]
fn open_launcher(app: AppHandle) {
    set_launcher(&app, true);
}

#[tauri::command]
fn media_control(state: St<'_>, action: String, value: Option<f64>) {
    let _ = lock(&state.media_tx).send(MediaCmd {
        action,
        value: value.unwrap_or(0.0),
    });
}

#[tauri::command]
fn volume_set(state: St<'_>, level: f32) {
    let _ = lock(&state.audio_tx).send(AudioCmd::Volume(level));
}

#[tauri::command]
fn volume_mute(state: St<'_>, muted: bool) {
    let _ = lock(&state.audio_tx).send(AudioCmd::Mute(muted));
}

#[tauri::command]
async fn list_apps(state: St<'_>, refresh: bool) -> Result<Vec<AppItem>, String> {
    let shared = state.inner().clone();
    let cached = lock(&shared.apps).clone();
    if let (false, Some(apps)) = (refresh, cached) {
        return Ok(apps);
    }
    let worker = shared.clone();
    let apps = tauri::async_runtime::spawn_blocking(move || {
        worker
            .shell
            .run(|| shell::list_apps().unwrap_or_default())
            .unwrap_or_default()
    })
    .await
    .map_err(|e| e.to_string())?;
    let items: Vec<AppItem> = apps
        .into_iter()
        .map(|a| AppItem {
            target: format!("app:{}", a.id),
            name: a.name,
        })
        .collect();
    *lock(&shared.apps) = Some(items.clone());
    Ok(items)
}

#[tauri::command]
async fn get_icons(
    state: St<'_>,
    keys: Vec<String>,
    size: Option<i32>,
    thumb: Option<bool>,
) -> Result<HashMap<String, Option<String>>, String> {
    let shared = state.inner().clone();
    let size = size.unwrap_or(64).clamp(16, 256);
    let thumb = thumb.unwrap_or(false);
    let ck = |k: &str| format!("{size}|{thumb}|{k}");
    let mut out = HashMap::new();
    let mut missing = Vec::new();
    {
        let cache = lock(&shared.icons);
        for k in keys {
            match cache.get(&ck(&k)) {
                Some(v) => {
                    out.insert(k, v.clone());
                }
                None => missing.push(k),
            }
        }
    }
    if missing.is_empty() {
        return Ok(out);
    }
    let worker = shared.clone();
    let fetched: Vec<(String, Option<String>)> = tauri::async_runtime::spawn_blocking(move || {
        worker
            .shell
            .run(move || {
                missing
                    .into_iter()
                    .map(|k| {
                        let url = shell::icon_rgba(&k, size, thumb)
                            .and_then(|(w, h, px)| img::encode_png(w, h, &px))
                            .map(|b| util::data_url("image/png", &b));
                        (k, url)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
    .await
    .map_err(|e| e.to_string())?;
    let mut cache = lock(&shared.icons);
    for (k, v) in fetched {
        cache.insert(ck(&k), v.clone());
        out.insert(k, v);
    }
    Ok(out)
}

/// Opens an app (`app:<id>`), file, folder or URL.
#[tauri::command]
async fn open_target(state: St<'_>, target: String, count: Option<bool>) -> Result<bool, String> {
    let shared = state.inner().clone();
    if count.unwrap_or(true) {
        let mut d = lock(&shared.data);
        *d.usage.entry(target.clone()).or_insert(0) += 1;
        save_json(&shared.paths.data(), &*d);
    }
    let worker = shared.clone();
    tauri::async_runtime::spawn_blocking(move || worker.shell.run(move || shell::open(&target)).unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn reveal(state: St<'_>, path: String) -> Result<bool, String> {
    let worker = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || worker.shell.run(move || shell::reveal(&path)).unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn snip(state: St<'_>) -> Result<bool, String> {
    let worker = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || worker.shell.run(|| shell::open("ms-screenclip:")).unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn pick_files(state: St<'_>, title: String) -> Result<Vec<String>, String> {
    let owner = state.own_hwnd.load(Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        thread::spawn(move || {
            let _com = util::Com::sta();
            shell::pick_files(owner, &title)
        })
        .join()
        .unwrap_or_default()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn shelf_add(state: St<'_>, paths: Vec<String>) -> Vec<ShelfItem> {
    let mut d = lock(&state.data);
    for p in paths {
        if d.shelf.iter().any(|s| s.path == p) {
            continue;
        }
        if let Some(item) = store::shelf_item(&p) {
            d.shelf.insert(0, item);
        }
    }
    save_json(&state.paths.data(), &*d);
    d.shelf.clone()
}

#[tauri::command]
fn shelf_remove(state: St<'_>, id: String) -> Vec<ShelfItem> {
    let mut d = lock(&state.data);
    d.shelf.retain(|s| s.id != id);
    save_json(&state.paths.data(), &*d);
    d.shelf.clone()
}

#[tauri::command]
fn shelf_clear(state: St<'_>) -> Vec<ShelfItem> {
    let mut d = lock(&state.data);
    d.shelf.clear();
    save_json(&state.paths.data(), &*d);
    d.shelf.clone()
}

/// Puts shelf files on the clipboard as a file list.
#[tauri::command]
async fn shelf_copy(state: St<'_>, ids: Vec<String>) -> Result<bool, String> {
    let shared = state.inner().clone();
    let paths: Vec<String> = lock(&shared.data)
        .shelf
        .iter()
        .filter(|s| ids.contains(&s.id))
        .map(|s| s.path.clone())
        .collect();
    tauri::async_runtime::spawn_blocking(move || clip::write_files(&paths))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn pins_add(state: St<'_>, targets: Vec<String>) -> Vec<PinItem> {
    let mut d = lock(&state.data);
    for t in targets {
        if d.pins.iter().any(|p| p.target == t) {
            continue;
        }
        d.pins.push(store::pin_item(&t));
    }
    save_json(&state.paths.data(), &*d);
    d.pins.clone()
}

#[tauri::command]
fn pins_add_app(state: St<'_>, target: String, name: String) -> Vec<PinItem> {
    let mut d = lock(&state.data);
    if !d.pins.iter().any(|p| p.target == target) {
        d.pins.push(PinItem {
            id: store::new_id(),
            target,
            name,
        });
    }
    save_json(&state.paths.data(), &*d);
    d.pins.clone()
}

#[tauri::command]
fn pins_remove(state: St<'_>, id: String) -> Vec<PinItem> {
    let mut d = lock(&state.data);
    d.pins.retain(|p| p.id != id);
    save_json(&state.paths.data(), &*d);
    d.pins.clone()
}

fn write_entry(e: &ClipEntry, image: Option<PathBuf>) -> bool {
    match e.kind.as_str() {
        "image" => image
            .and_then(|p| fs::read(p).ok())
            .and_then(|png| img::decode_png(&png).map(|(w, h, px)| clip::write_image(w, h, &px, Some(&png))))
            .unwrap_or(false),
        "files" => clip::write_files(&e.files),
        _ => clip::write_text(&e.text),
    }
}

/// Copies a history entry back to the clipboard and (optionally) pastes it
/// into the window that was active before the island.
#[tauri::command]
async fn clip_paste(app: AppHandle, state: St<'_>, id: String, paste: bool) -> Result<bool, String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (entry, image) = {
            let store = lock(&shared.clips);
            match store.get(&id) {
                Some(e) => {
                    let img = store.image_path(&e);
                    (e, img)
                }
                None => return false,
            }
        };
        if !write_entry(&entry, image) {
            return false;
        }
        shared.clip_skip_seq.store(clip::sequence(), Ordering::Relaxed);
        let views = {
            let mut store = lock(&shared.clips);
            store.touch(&entry.id);
            store.views()
        };
        let _ = app.emit("clips", views);
        if paste {
            thread::sleep(Duration::from_millis(40));
            input::activate(shared.last_fg.load(Ordering::Relaxed));
            thread::sleep(Duration::from_millis(110));
            input::send_paste();
        }
        true
    })
    .await
    .map_err(|e| e.to_string())
}

/// "Чистый текст": strips formatting from the clipboard text and pastes it.
#[tauri::command]
async fn paste_plain(state: St<'_>) -> Result<bool, String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(clip::ClipData::Text(text)) = clip::read() else { return false };
        if !clip::write_text(&text) {
            return false;
        }
        shared.clip_skip_seq.store(clip::sequence(), Ordering::Relaxed);
        thread::sleep(Duration::from_millis(40));
        input::activate(shared.last_fg.load(Ordering::Relaxed));
        thread::sleep(Duration::from_millis(110));
        input::send_paste();
        true
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn copy_text(text: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || clip::write_text(&text))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn clip_delete(app: AppHandle, state: St<'_>, id: String) {
    let views = {
        let mut c = lock(&state.clips);
        c.remove(&id);
        c.views()
    };
    let _ = app.emit("clips", views);
}

#[tauri::command]
fn clip_pin(app: AppHandle, state: St<'_>, id: String, pinned: bool) {
    let views = {
        let mut c = lock(&state.clips);
        c.set_pinned(&id, pinned);
        c.views()
    };
    let _ = app.emit("clips", views);
}

#[tauri::command]
fn clip_clear(app: AppHandle, state: St<'_>) {
    let views = {
        let mut c = lock(&state.clips);
        c.clear();
        c.views()
    };
    let _ = app.emit("clips", views);
}

#[tauri::command]
fn lock_screen() -> bool {
    input::lock_workstation()
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

// ---------------------------------------------------------------------------
// Tray & entry point
// ---------------------------------------------------------------------------

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Открыть остров", true, None::<&str>)?;
    let launcher = MenuItem::with_id(app, "launcher", "Лаунчер", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Настройки", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let restart = MenuItem::with_id(app, "restart", "Перезапустить", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &launcher, &settings, &sep, &restart, &quit])?;
    let mut builder = TrayIconBuilder::with_id("island")
        .tooltip("Island")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                let _ = app.emit("open-panel", "home");
            }
            "launcher" => set_launcher(app, true),
            "settings" => {
                let _ = app.emit("open-panel", "settings");
            }
            "restart" => app.restart(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = tray.app_handle().emit("open-panel", "home");
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app.emit("open-panel", "home");
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_drag::init())
        .setup(|app| {
            let paths = Paths::new(app.path().app_data_dir()?);
            let first_run = !paths.settings().exists();
            let settings: Settings = load_json(&paths.settings());
            let data: Data = load_json(&paths.data());
            let clips = ClipStore::load(paths.clips(), paths.clip_dir());
            let (media_tx, media_rx) = channel();
            let (audio_tx, audio_rx) = channel();

            let shared = Arc::new(Shared {
                paths,
                settings: Mutex::new(settings.clone()),
                data: Mutex::new(data),
                clips: Mutex::new(clips),
                hit: Mutex::new(Rect::default()),
                geom: Mutex::new(Geom::default()),
                own_hwnd: AtomicIsize::new(0),
                last_fg: AtomicIsize::new(0),
                suppressed: AtomicBool::new(false),
                launcher_open: AtomicBool::new(false),
                media_playing: AtomicBool::new(false),
                clip_skip_seq: AtomicU32::new(0),
                media: Mutex::new(None),
                cover: Mutex::new(CoverPayload::default()),
                volume: Mutex::new(VolumePayload::default()),
                media_tx: Mutex::new(media_tx),
                audio_tx: Mutex::new(audio_tx),
                shell: ShellWorker::spawn(),
                icons: Mutex::new(HashMap::new()),
                apps: Mutex::new(None),
                hotkey: Mutex::new(String::new()),
                hotkey_error: Mutex::new(None),
            });
            app.manage(shared.clone());

            if first_run {
                save_json(&shared.paths.settings(), &settings);
                apply_autostart(app.handle(), settings.autostart);
            }
            if let Err(e) = rebind_hotkey(app.handle(), &shared, &settings.hotkey) {
                *lock(&shared.hotkey_error) = Some(e);
            }

            let win = app.get_webview_window("main").ok_or("main window is missing")?;
            place_window(&win, &shared, true);
            if let Ok(h) = win.hwnd() {
                let h = h.0 as isize;
                shared.own_hwnd.store(h, Ordering::Relaxed);
                input::make_tool_window(h);
            }
            // The window is created visible but unfocused (see tauri.conf.json);
            // it never takes clicks until the cursor is over the island.
            let _ = win.set_ignore_cursor_events(true);

            build_tray(app.handle())?;

            spawn_cursor_thread(app.handle().clone(), shared.clone());
            spawn_media_thread(app.handle().clone(), shared.clone(), media_rx);
            spawn_audio_thread(app.handle().clone(), shared.clone(), audio_rx);
            spawn_clip_thread(app.handle().clone(), shared.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            init,
            set_hit,
            save_settings,
            launcher_closed,
            open_launcher,
            media_control,
            volume_set,
            volume_mute,
            list_apps,
            get_icons,
            open_target,
            reveal,
            snip,
            pick_files,
            shelf_add,
            shelf_remove,
            shelf_clear,
            shelf_copy,
            pins_add,
            pins_add_app,
            pins_remove,
            clip_paste,
            paste_plain,
            copy_text,
            clip_delete,
            clip_pin,
            clip_clear,
            lock_screen,
            quit
        ])
        .run(tauri::generate_context!())
        .expect("error while running Island");
}
