// Island — a liquid "Dynamic Island" for Windows 10/11.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod clips;
mod native;
mod store;
mod weather;

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
use tauri::{AppHandle, Emitter, EventTarget, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use clips::{ClipEntry, ClipStore, ClipView};
use native::{audio::Audio, capture, clip, img, input, media::Media, shell, util};
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
    /// action -> registered combo
    hotkeys: Mutex<HashMap<String, String>>,
    hotkey_error: Mutex<Option<String>>,
    /// Frozen screen shown by the capture overlay.
    shot: Mutex<Option<Shot>>,
    shot_seq: AtomicU32,
    capturing: AtomicBool,
    /// Bumped to cancel the reply being streamed.
    ai_gen: AtomicU32,
    weather: Mutex<Option<weather::WeatherPayload>>,
    weather_wake: Mutex<Sender<()>>,
}

/// A monitor snapshot (physical pixels, top-down BGRA).
struct Shot {
    id: u32,
    mode: String,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    bgra: Arc<Vec<u8>>,
    bmp: Arc<Vec<u8>>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct ToastAction {
    label: String,
    cmd: String,
    args: serde_json::Value,
}

/// Toast shown by the island (sent to the main window).
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct ToastPayload {
    icon: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    swatch: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actions: Vec<ToastAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ms: Option<u32>,
}

fn toast(app: &AppHandle, t: ToastPayload) {
    let _ = app.emit_to(EventTarget::webview_window("main"), "toast", t);
}

fn toast_error(app: &AppHandle, title: &str, subtitle: &str) {
    toast(
        app,
        ToastPayload {
            icon: "error".into(),
            tone: Some("error".into()),
            title: title.into(),
            subtitle: Some(subtitle.into()),
            ..Default::default()
        },
    );
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

/// (action, combo) pairs wanted by the settings.
fn hotkey_bindings(s: &Settings) -> Vec<(&'static str, String)> {
    let mut v = vec![("launcher", s.hotkey.clone())];
    if s.capture_enabled {
        v.push(("region", s.hotkey_region.clone()));
        v.push(("ocr", s.hotkey_ocr.clone()));
    }
    v.retain(|(_, c)| !c.trim().is_empty());
    v
}

fn run_hotkey(app: &AppHandle, action: &str) {
    match action {
        "launcher" => toggle_launcher(app),
        "region" | "ocr" => begin_capture(app.clone(), action.to_string()),
        _ => {}
    }
}

/// Re-registers every global shortcut. Must run on the main thread (RegisterHotKey
/// is thread-affine). Returns the shortcuts that could not be registered.
fn apply_hotkeys(app: &AppHandle, shared: &Shared, settings: &Settings) -> Vec<String> {
    let gs = app.global_shortcut();
    let mut cur = lock(&shared.hotkeys);
    for (_, combo) in cur.drain() {
        let _ = gs.unregister(combo.as_str());
    }
    let mut errors = Vec::new();
    for (action, combo) in hotkey_bindings(settings) {
        if cur.values().any(|c| c.eq_ignore_ascii_case(&combo)) {
            errors.push(format!("«{combo}» назначено дважды"));
            continue;
        }
        let act = action.to_string();
        let res = gs.on_shortcut(combo.as_str(), move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                run_hotkey(app, &act);
            }
        });
        match res {
            Ok(()) => {
                cur.insert(action.to_string(), combo);
            }
            Err(e) => errors.push(format!("«{combo}»: {e}")),
        }
    }
    errors
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

/// Forecast every 15 minutes (sooner when the city changes or on request).
fn spawn_weather_thread(app: AppHandle, shared: Arc<Shared>, rx: Receiver<()>) {
    thread::Builder::new()
        .name("weather".into())
        .spawn(move || {
            let mut ip_place: Option<weather::Place> = None;
            let mut wait = Duration::from_secs(3);
            loop {
                match rx.recv_timeout(wait) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                while rx.try_recv().is_ok() {}
                let s = lock(&shared.settings).clone();
                if !s.weather_enabled {
                    wait = Duration::from_secs(3600);
                    continue;
                }
                let result = tauri::async_runtime::block_on(async {
                    let (city, lat, lon) = if !s.weather_city.is_empty() {
                        (s.weather_city.clone(), s.weather_lat, s.weather_lon)
                    } else {
                        if ip_place.is_none() {
                            ip_place = Some(weather::locate().await?);
                        }
                        let p = ip_place.clone().unwrap();
                        (p.name, p.lat, p.lon)
                    };
                    let data = weather::forecast(lat, lon).await?;
                    Ok::<_, String>(weather::WeatherPayload {
                        city,
                        fetched_ms: store::now_ms(),
                        data,
                    })
                });
                match result {
                    Ok(w) => {
                        *lock(&shared.weather) = Some(w.clone());
                        let _ = app.emit("weather", w);
                        wait = Duration::from_secs(15 * 60);
                    }
                    Err(_) => wait = Duration::from_secs(120),
                }
            }
        })
        .expect("failed to start weather thread");
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
    weather: Option<weather::WeatherPayload>,
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
        weather: lock(&state.weather).clone(),
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
    if next.hotkey_region.trim().is_empty() {
        next.hotkey_region = old.hotkey_region.clone();
    }
    if next.hotkey_ocr.trim().is_empty() {
        next.hotkey_ocr = old.hotkey_ocr.clone();
    }
    let failed_before = lock(&state.hotkey_error).is_some();
    if hotkey_bindings(&next) != hotkey_bindings(&old) || failed_before {
        let errors = apply_hotkeys(&app, &state, &next);
        if !errors.is_empty() {
            let _ = apply_hotkeys(&app, &state, &old);
            return Err(format!("Не удалось назначить {}", errors.join(", ")));
        }
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
    let weather_changed = next.weather_enabled != old.weather_enabled
        || next.weather_city != old.weather_city
        || next.weather_lat != old.weather_lat
        || next.weather_lon != old.weather_lon;
    save_json(&state.paths.settings(), &next);
    *lock(&state.settings) = next.clone();
    if weather_changed {
        let _ = lock(&state.weather_wake).send(());
    }
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

/// "Запись экрана": Xbox Game Bar records the active window / screen.
#[tauri::command]
async fn record_screen(state: St<'_>) -> Result<bool, String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Game Bar records the foreground app, so give focus back first.
        input::activate(shared.last_fg.load(Ordering::Relaxed));
        thread::sleep(Duration::from_millis(150));
        input::toggle_game_bar_recording();
        true
    })
    .await
    .map_err(|e| e.to_string())
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
// Screen capture overlay
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OverlayStart {
    id: u32,
    mode: String,
    width: u32,
    height: u32,
}

#[derive(Deserialize, Clone, Copy)]
struct PxRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn build_overlay(app: &AppHandle) -> tauri::Result<()> {
    let win = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html#overlay".into()))
        .title("Island — захват")
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build()?;
    if let Ok(h) = win.hwnd() {
        input::make_tool_window(h.0 as isize);
    }
    Ok(())
}

/// Takes a snapshot of the monitor under the cursor (without our own windows).
fn snapshot(shared: &Shared, mode: &str) -> Option<Shot> {
    let own = shared.own_hwnd.load(Ordering::Relaxed);
    let (cx, cy) = input::cursor_pos();
    let (x, y, w, h) = capture::monitor_at(cx, cy);
    capture::exclude_from_capture(own, true);
    thread::sleep(Duration::from_millis(60));
    let px = capture::grab(x, y, w, h);
    capture::exclude_from_capture(own, false);
    let bgra = Arc::new(px?);
    let bmp = Arc::new(capture::bmp(&bgra, w, h));
    Some(Shot {
        id: shared.shot_seq.fetch_add(1, Ordering::Relaxed) + 1,
        mode: mode.to_string(),
        x,
        y,
        w,
        h,
        bgra,
        bmp,
    })
}

fn end_overlay(app: &AppHandle, shared: &Shared) {
    if let Some(ov) = app.get_webview_window("overlay") {
        let _ = ov.emit_to(EventTarget::webview_window("overlay"), "overlay-end", ());
        let _ = ov.hide();
    }
    *lock(&shared.shot) = None;
    shared.capturing.store(false, Ordering::Relaxed);
}

/// Region / full screen / text / QR / colour picker. Runs off the main thread.
fn begin_capture(app: AppHandle, mode: String) {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    if !lock(&shared.settings).capture_enabled || shared.capturing.swap(true, Ordering::Relaxed) {
        return;
    }
    thread::spawn(move || {
        let Some(shot) = snapshot(&shared, &mode) else {
            shared.capturing.store(false, Ordering::Relaxed);
            toast_error(&app, "Не удалось снять экран", "");
            return;
        };
        if mode == "full" {
            let (w, h) = (shot.w, shot.h);
            let bgra = shot.bgra.clone();
            shared.capturing.store(false, Ordering::Relaxed);
            finish_image(&app, &shared, &bgra, w, h);
            return;
        }
        let start = OverlayStart {
            id: shot.id,
            mode: mode.clone(),
            width: shot.w,
            height: shot.h,
        };
        let (x, y, w, h) = (shot.x, shot.y, shot.w, shot.h);
        *lock(&shared.shot) = Some(shot);
        let Some(ov) = app.get_webview_window("overlay") else {
            shared.capturing.store(false, Ordering::Relaxed);
            return;
        };
        let _ = ov.set_position(PhysicalPosition::new(x, y));
        let _ = ov.set_size(PhysicalSize::new(w, h));
        let id = start.id;
        let _ = app.emit_to(EventTarget::webview_window("overlay"), "overlay-start", start);
        // the overlay page never answered: don't stay stuck in "capturing"
        thread::sleep(Duration::from_secs(4));
        let stuck = lock(&shared.shot).as_ref().map(|s| s.id) == Some(id) && !ov.is_visible().unwrap_or(false);
        if stuck {
            end_overlay(&app, &shared);
            toast_error(&app, "Оверлей не открылся", "Попробуйте ещё раз");
        }
    });
}

/// Screenshot → clipboard + Pictures\Screenshots\Island-*.png + toast with a thumbnail.
fn finish_image(app: &AppHandle, shared: &Shared, bgra: &[u8], w: u32, h: u32) {
    let rgba = capture::bgra_to_rgba(bgra);
    let Some(png) = img::encode_png(w, h, &rgba) else {
        toast_error(app, "Не удалось сохранить скриншот", "");
        return;
    };
    let copied = clip::write_image(w, h, &rgba, Some(&png));
    if copied {
        shared.clip_skip_seq.store(clip::sequence(), Ordering::Relaxed);
    }
    let name = format!("Island-{}.png", capture::local_stamp());
    let path = capture::screenshots_dir().map(|d| d.join(&name));
    let saved = path.as_ref().map(|p| fs::write(p, &png).is_ok()).unwrap_or(false);
    let (tw, th, thumb) = img::shrink(w, h, &rgba, 120);
    let image = img::encode_png(tw, th, &thumb).map(|b| util::data_url("image/png", &b));
    let mut actions = Vec::new();
    if let (true, Some(p)) = (saved, &path) {
        actions.push(ToastAction {
            label: "Показать".into(),
            cmd: "reveal".into(),
            args: serde_json::json!({ "path": p.to_string_lossy() }),
        });
    }
    toast(
        app,
        ToastPayload {
            icon: "image".into(),
            title: "Скриншот готов".into(),
            subtitle: Some(match (copied, saved) {
                (true, true) => format!("В буфере · {w} × {h}"),
                (true, false) => "В буфере · файл не сохранился".into(),
                (false, true) => "Сохранён в «Снимки экрана»".into(),
                (false, false) => "Не удалось сохранить".into(),
            }),
            image,
            actions,
            ms: Some(4200),
            ..Default::default()
        },
    );
}

fn finish_ocr(app: &AppHandle, shared: &Shared, bgra: Vec<u8>, w: u32, h: u32) {
    // WinRT OCR wants an MTA thread
    let text = thread::spawn(move || {
        let _com = util::Com::mta();
        capture::ocr(&bgra, w, h)
    })
    .join()
    .ok()
    .and_then(|r| r.ok())
    .unwrap_or_default();
    let text = text.trim().to_string();
    if text.is_empty() {
        toast_error(app, "Текст не найден", "Попробуйте выделить крупнее");
        return;
    }
    let n = text.chars().count();
    if clip::write_text(&text) {
        let _ = shared;
        toast(
            app,
            ToastPayload {
                icon: "text".into(),
                title: format!("Текст распознан · {n} симв."),
                subtitle: Some(text.lines().next().unwrap_or("").chars().take(60).collect()),
                ms: Some(3200),
                ..Default::default()
            },
        );
    } else {
        toast_error(app, "Не удалось скопировать", "");
    }
}

fn finish_qr(app: &AppHandle, bgra: &[u8], w: u32, h: u32) {
    let Some(content) = capture::qr(bgra, w, h) else {
        toast_error(app, "QR-код не найден", "Выделите код целиком");
        return;
    };
    let content = content.trim().to_string();
    let is_url = content.starts_with("http://") || content.starts_with("https://");
    let _ = clip::write_text(&content);
    let short: String = content.chars().take(60).collect();
    toast(
        app,
        ToastPayload {
            icon: "qr".into(),
            title: if is_url { "Ссылка из QR-кода".into() } else { "QR-код скопирован".into() },
            subtitle: Some(short),
            actions: if is_url {
                vec![ToastAction {
                    label: "Открыть".into(),
                    cmd: "open_target".into(),
                    args: serde_json::json!({ "target": content, "count": false }),
                }]
            } else {
                Vec::new()
            },
            ms: Some(if is_url { 6000 } else { 3200 }),
            ..Default::default()
        },
    );
}

#[tauri::command]
fn capture(app: AppHandle, mode: String) {
    begin_capture(app, mode);
}

/// The overlay has the snapshot on screen: show it and take focus (for Esc).
#[tauri::command]
fn overlay_show(app: AppHandle, state: St<'_>, id: u32) {
    if lock(&state.shot).as_ref().map(|s| s.id) != Some(id) {
        return;
    }
    if let Some(ov) = app.get_webview_window("overlay") {
        let _ = ov.show();
        let _ = ov.set_focus();
    }
}

#[tauri::command]
fn capture_cancel(app: AppHandle, state: St<'_>) {
    end_overlay(&app, &state);
    input::activate(state.last_fg.load(Ordering::Relaxed));
}

/// `rect` is in snapshot pixels; `color` comes from the picker.
#[tauri::command]
fn capture_finish(app: AppHandle, state: St<'_>, id: u32, rect: Option<PxRect>, color: Option<String>) {
    let shot = lock(&state.shot).take();
    end_overlay(&app, &state);
    input::activate(state.last_fg.load(Ordering::Relaxed));
    let Some(shot) = shot.filter(|s| s.id == id) else { return };
    let shared = state.inner().clone();
    thread::spawn(move || {
        if shot.mode == "picker" {
            let Some(hex) = color else { return };
            let _ = clip::write_text(&hex);
            toast(
                &app,
                ToastPayload {
                    icon: "color".into(),
                    title: hex.clone(),
                    subtitle: Some("Цвет скопирован".into()),
                    swatch: Some(hex),
                    ms: Some(3000),
                    ..Default::default()
                },
            );
            return;
        }
        // whole screen when nothing was selected (QR: a click scans everything)
        let r = rect.unwrap_or(PxRect { x: 0.0, y: 0.0, w: shot.w as f64, h: shot.h as f64 });
        let x = r.x.max(0.0).min(shot.w as f64 - 1.0) as u32;
        let y = r.y.max(0.0).min(shot.h as f64 - 1.0) as u32;
        let w = (r.w.round() as u32).clamp(1, shot.w - x);
        let h = (r.h.round() as u32).clamp(1, shot.h - y);
        let px = capture::crop(&shot.bgra, shot.w, x, y, w, h);
        match shot.mode.as_str() {
            "ocr" => finish_ocr(&app, &shared, px, w, h),
            "qr" => finish_qr(&app, &px, w, h),
            _ => finish_image(&app, &shared, &px, w, h),
        }
    });
}

// ---------------------------------------------------------------------------
// AI chat
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AiChunk {
    id: String,
    delta: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AiDone {
    id: String,
    error: Option<String>,
}

#[tauri::command]
async fn ai_models(state: St<'_>, url: Option<String>, key: Option<String>) -> Result<Vec<String>, String> {
    let (u, k) = {
        let s = lock(&state.settings);
        (url.unwrap_or_else(|| s.ai_url.clone()), key.unwrap_or_else(|| s.ai_key.clone()))
    };
    ai::models(&u, &k).await
}

#[tauri::command]
async fn ai_attach(path: String) -> Result<ai::Attachment, String> {
    tauri::async_runtime::spawn_blocking(move || ai::attach(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Streams a chat completion: "ai-chunk" for every piece, then "ai-done".
#[tauri::command]
fn ai_send(app: AppHandle, state: St<'_>, id: String, messages: serde_json::Value) {
    let shared = state.inner().clone();
    let gen = shared.ai_gen.fetch_add(1, Ordering::Relaxed) + 1;
    let (base, key, model) = {
        let s = lock(&shared.settings);
        (s.ai_url.clone(), s.ai_key.clone(), s.ai_model.clone())
    };
    tauri::async_runtime::spawn(async move {
        let done = |error: Option<String>| {
            let _ = app.emit("ai-done", AiDone { id: id.clone(), error });
        };
        let mut body = serde_json::json!({ "messages": messages, "stream": true, "temperature": 0.7 });
        if !model.is_empty() {
            body["model"] = serde_json::Value::String(model);
        }
        let mut req = ai::client().post(ai::endpoint(&base, "chat/completions")).json(&body);
        if !key.is_empty() {
            req = req.bearer_auth(&key);
        }
        let mut resp = match req.send().await {
            Ok(r) => r,
            Err(e) => return done(Some(ai::explain(&e, &base))),
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().or(v["error"].as_str()).map(String::from))
                .unwrap_or_else(|| text.chars().take(200).collect());
            return done(Some(format!("Сервер ответил {status}: {msg}")));
        }
        let mut buf = String::new();
        loop {
            if shared.ai_gen.load(Ordering::Relaxed) != gen {
                return done(None);
            }
            let chunk = match resp.chunk().await {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(e) => return done(Some(ai::explain(&e, &base))),
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(nl) = buf.find('\n') {
                let line: String = buf.drain(..=nl).collect();
                match ai::parse_sse_line(&line) {
                    ai::Sse::Delta(d) => {
                        let _ = app.emit("ai-chunk", AiChunk { id: id.clone(), delta: d });
                    }
                    ai::Sse::Done => return done(None),
                    ai::Sse::Skip => {}
                }
            }
        }
        if let ai::Sse::Delta(d) = ai::parse_sse_line(&buf) {
            let _ = app.emit("ai-chunk", AiChunk { id: id.clone(), delta: d });
        }
        done(None)
    });
}

#[tauri::command]
fn ai_stop(state: St<'_>) {
    state.ai_gen.fetch_add(1, Ordering::Relaxed);
}

#[tauri::command]
async fn weather_search(q: String) -> Result<Vec<weather::Place>, String> {
    weather::search(q.trim()).await
}

#[tauri::command]
async fn weather_locate() -> Result<weather::Place, String> {
    weather::locate().await
}

#[tauri::command]
fn weather_refresh(state: St<'_>) {
    let _ = lock(&state.weather_wake).send(());
}

#[tauri::command]
fn chat_load(state: St<'_>) -> serde_json::Value {
    let v: serde_json::Value = load_json(&state.paths.chat());
    if v.is_array() {
        v
    } else {
        serde_json::Value::Array(Vec::new())
    }
}

#[tauri::command]
fn chat_save(state: St<'_>, messages: serde_json::Value) {
    save_json(&state.paths.chat(), &messages);
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
        // http://shot.localhost/<id> — the frozen screen for the capture overlay
        .register_uri_scheme_protocol("shot", |ctx, req| {
            let shared = ctx.app_handle().state::<Arc<Shared>>();
            let want: u32 = req.uri().path().trim_matches('/').parse().unwrap_or(0);
            let body = lock(&shared.shot)
                .as_ref()
                .filter(|s| s.id == want)
                .map(|s| s.bmp.as_ref().clone());
            let builder = tauri::http::Response::builder()
                .header("Access-Control-Allow-Origin", "*")
                .header("Cache-Control", "no-store");
            match body {
                Some(b) => builder.header("Content-Type", "image/bmp").body(b),
                None => builder.status(404).body(Vec::new()),
            }
            .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
        })
        .setup(|app| {
            let paths = Paths::new(app.path().app_data_dir()?);
            let first_run = !paths.settings().exists();
            let settings: Settings = load_json(&paths.settings());
            let data: Data = load_json(&paths.data());
            let clips = ClipStore::load(paths.clips(), paths.clip_dir());
            let (media_tx, media_rx) = channel();
            let (audio_tx, audio_rx) = channel();
            let (weather_tx, weather_rx) = channel();

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
                hotkeys: Mutex::new(HashMap::new()),
                hotkey_error: Mutex::new(None),
                shot: Mutex::new(None),
                shot_seq: AtomicU32::new(0),
                capturing: AtomicBool::new(false),
                ai_gen: AtomicU32::new(0),
                weather: Mutex::new(None),
                weather_wake: Mutex::new(weather_tx),
            });
            app.manage(shared.clone());

            if first_run {
                save_json(&shared.paths.settings(), &settings);
                apply_autostart(app.handle(), settings.autostart);
            }
            let errors = apply_hotkeys(app.handle(), &shared, &settings);
            if !errors.is_empty() {
                *lock(&shared.hotkey_error) = Some(errors.join(", "));
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
            build_overlay(app.handle())?;

            spawn_cursor_thread(app.handle().clone(), shared.clone());
            spawn_media_thread(app.handle().clone(), shared.clone(), media_rx);
            spawn_audio_thread(app.handle().clone(), shared.clone(), audio_rx);
            spawn_clip_thread(app.handle().clone(), shared.clone());
            spawn_weather_thread(app.handle().clone(), shared.clone(), weather_rx);
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
            record_screen,
            capture,
            overlay_show,
            capture_cancel,
            capture_finish,
            ai_models,
            ai_attach,
            ai_send,
            ai_stop,
            chat_load,
            chat_save,
            weather_search,
            weather_locate,
            weather_refresh,
            quit
        ])
        .run(tauri::generate_context!())
        .expect("error while running Island");
}
