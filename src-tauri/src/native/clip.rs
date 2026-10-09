//! Clipboard access: reading text / images / file lists and writing them back.

use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};

use super::util::{pcwstr, wide, window_exe_path};

pub enum ClipData {
    Text(String),
    Image { width: u32, height: u32, rgba: Vec<u8> },
    Files(Vec<String>),
}

pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

fn register(name: &str) -> u32 {
    let w = wide(name);
    unsafe { RegisterClipboardFormatW(pcwstr(&w)) }
}

fn available(fmt: u32) -> bool {
    fmt != 0 && unsafe { IsClipboardFormatAvailable(fmt) }.is_ok()
}

struct Opened;

impl Opened {
    fn open() -> Option<Self> {
        for _ in 0..12 {
            if unsafe { OpenClipboard(None::<HWND>) }.is_ok() {
                return Some(Opened);
            }
            sleep(Duration::from_millis(15));
        }
        None
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Password managers and some apps ask monitors to ignore their entries.
pub fn should_ignore() -> bool {
    available(register("ExcludeClipboardContentFromMonitorProcessing"))
        || available(register("Clipboard Viewer Ignore"))
        || {
            let fmt = register("CanIncludeInClipboardHistory");
            available(fmt) && read_u32_format(fmt) == Some(0)
        }
}

fn read_u32_format(fmt: u32) -> Option<u32> {
    let _guard = Opened::open()?;
    unsafe {
        let h = GetClipboardData(fmt).ok()?;
        let g = HGLOBAL(h.0);
        let p = GlobalLock(g) as *const u32;
        if p.is_null() {
            return None;
        }
        let v = if GlobalSize(g) >= 4 { Some(*p) } else { None };
        let _ = GlobalUnlock(g);
        v
    }
}

/// Executable stem of the app that put data on the clipboard ("chrome", "Telegram").
pub fn owner_app() -> Option<String> {
    let owner = unsafe { GetClipboardOwner() }.ok()?;
    window_exe_path(owner.0 as isize).map(|p| super::util::exe_stem(&p))
}

pub fn read() -> Option<ClipData> {
    let has_files = available(CF_HDROP.0 as u32);
    let has_text = available(CF_UNICODETEXT.0 as u32);
    let has_dib = available(CF_DIB.0 as u32);
    if !(has_files || has_text || has_dib) {
        return None;
    }
    let _guard = Opened::open()?;
    unsafe {
        if has_files {
            if let Some(files) = read_files() {
                return Some(ClipData::Files(files));
            }
        }
        if has_text {
            if let Some(t) = read_text() {
                if !t.trim().is_empty() {
                    return Some(ClipData::Text(t));
                }
            }
        }
        if has_dib {
            if let Some((width, height, rgba)) = read_dib() {
                return Some(ClipData::Image { width, height, rgba });
            }
        }
    }
    None
}

unsafe fn read_text() -> Option<String> {
    let h = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
    let g = HGLOBAL(h.0);
    let p = GlobalLock(g) as *const u16;
    if p.is_null() {
        return None;
    }
    let max = GlobalSize(g) / 2;
    let slice = std::slice::from_raw_parts(p, max);
    let len = slice.iter().position(|&c| c == 0).unwrap_or(max);
    let s = String::from_utf16_lossy(&slice[..len]);
    let _ = GlobalUnlock(g);
    Some(s)
}

unsafe fn read_files() -> Option<Vec<String>> {
    let h = GetClipboardData(CF_HDROP.0 as u32).ok()?;
    let drop = HDROP(h.0);
    let count = DragQueryFileW(drop, u32::MAX, None);
    let mut out = Vec::new();
    for i in 0..count {
        let len = DragQueryFileW(drop, i, None) as usize;
        let mut buf = vec![0u16; len + 1];
        let got = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
        out.push(String::from_utf16_lossy(&buf[..got]));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

unsafe fn read_dib() -> Option<(u32, u32, Vec<u8>)> {
    let h = GetClipboardData(CF_DIB.0 as u32).ok()?;
    let g = HGLOBAL(h.0);
    let p = GlobalLock(g) as *const u8;
    if p.is_null() {
        return None;
    }
    let total = GlobalSize(g);
    let bytes = std::slice::from_raw_parts(p, total);
    let r = parse_dib(bytes);
    let _ = GlobalUnlock(g);
    r
}

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn rd_i32(b: &[u8], o: usize) -> i32 {
    rd_u32(b, o) as i32
}
fn rd_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

/// Packed DIB (BITMAPINFOHEADER / V4 / V5 + pixels) -> RGBA. 24/32 bpp only.
fn parse_dib(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if b.len() < 40 {
        return None;
    }
    let hdr = rd_u32(b, 0) as usize;
    let width = rd_i32(b, 4);
    let height = rd_i32(b, 8);
    let bpp = rd_u16(b, 14);
    let compression = rd_u32(b, 16);
    let clr_used = rd_u32(b, 32) as usize;
    if width <= 0 || height == 0 || !(bpp == 24 || bpp == 32) {
        return None;
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    if w * h > 40_000_000 {
        return None;
    }
    let mut offset = hdr;
    if compression == 3 && hdr == 40 {
        offset += 12; // BI_BITFIELDS masks follow a plain BITMAPINFOHEADER
    }
    offset += clr_used * 4;
    let stride = (w * bpp as usize).div_ceil(32) * 4;
    if b.len() < offset + stride * h {
        return None;
    }
    let bottom_up = height > 0;
    let mut rgba = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for y in 0..h {
        let src_row = if bottom_up { h - 1 - y } else { y };
        let row = &b[offset + src_row * stride..];
        for x in 0..w {
            let (bl, gr, rd, a) = if bpp == 32 {
                let px = &row[x * 4..x * 4 + 4];
                (px[0], px[1], px[2], px[3])
            } else {
                let px = &row[x * 3..x * 3 + 3];
                (px[0], px[1], px[2], 255)
            };
            if a != 0 {
                any_alpha = true;
            }
            let o = (y * w + x) * 4;
            rgba[o] = rd;
            rgba[o + 1] = gr;
            rgba[o + 2] = bl;
            rgba[o + 3] = a;
        }
    }
    if !any_alpha {
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }
    Some((w as u32, h as u32, rgba))
}

unsafe fn set_global(format: u32, data: &[u8]) -> bool {
    let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) else { return false };
    let p = GlobalLock(g) as *mut u8;
    if p.is_null() {
        let _ = GlobalFree(Some(g));
        return false;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
    let _ = GlobalUnlock(g);
    if SetClipboardData(format, Some(HANDLE(g.0))).is_err() {
        let _ = GlobalFree(Some(g));
        return false;
    }
    true
}

pub fn write_text(text: &str) -> bool {
    let Some(_guard) = Opened::open() else { return false };
    unsafe {
        let _ = EmptyClipboard();
        let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 2);
        set_global(CF_UNICODETEXT.0 as u32, bytes)
    }
}

/// Puts an RGBA image on the clipboard as CF_DIB (+ "PNG" when bytes are given).
pub fn write_image(width: u32, height: u32, rgba: &[u8], png: Option<&[u8]>) -> bool {
    let (w, h) = (width as usize, height as usize);
    if rgba.len() < w * h * 4 {
        return false;
    }
    let mut dib = Vec::with_capacity(40 + w * h * 4);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(width as i32).to_le_bytes());
    dib.extend_from_slice(&(height as i32).to_le_bytes()); // bottom-up
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    dib.extend_from_slice(&((w * h * 4) as u32).to_le_bytes());
    dib.extend_from_slice(&[0u8; 16]); // ppm x/y, clr used/important
    for y in (0..h).rev() {
        for x in 0..w {
            let o = (y * w + x) * 4;
            dib.extend_from_slice(&[rgba[o + 2], rgba[o + 1], rgba[o], rgba[o + 3]]);
        }
    }
    let Some(_guard) = Opened::open() else { return false };
    unsafe {
        let _ = EmptyClipboard();
        let ok = set_global(CF_DIB.0 as u32, &dib);
        if let Some(png) = png {
            set_global(register("PNG"), png);
        }
        ok
    }
}

/// Puts a list of files on the clipboard (Ctrl+V in Explorer / messengers).
pub fn write_files(paths: &[String]) -> bool {
    if paths.is_empty() {
        return false;
    }
    let header = size_of::<DROPFILES>();
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.encode_utf16());
        list.push(0);
    }
    list.push(0);
    let mut data = vec![0u8; header + list.len() * 2];
    let df = DROPFILES { pFiles: header as u32, fWide: true.into(), ..Default::default() };
    unsafe {
        std::ptr::copy_nonoverlapping(&df as *const DROPFILES as *const u8, data.as_mut_ptr(), header);
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, data.as_mut_ptr().add(header), list.len() * 2);
    }
    let Some(_guard) = Opened::open() else { return false };
    unsafe {
        let _ = EmptyClipboard();
        set_global(CF_HDROP.0 as u32, &data)
    }
}
