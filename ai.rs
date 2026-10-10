//! AI chat against any OpenAI-compatible server (LM Studio by default):
//! model list, streamed completions (SSE) and file attachments.

use std::fs;
use std::io::Read;
use std::path::Path;

use serde::Serialize;

/// Long documents are cut to keep the request within a local model's context.
const TEXT_LIMIT: usize = 60_000;
const IMAGE_LIMIT: u64 = 12 * 1024 * 1024;
/// PDF / DOCX are parsed whole; bigger ones are refused instead of eating RAM.
const DOC_LIMIT: u64 = 64 * 1024 * 1024;
/// Plain text: only the beginning is read — more than TEXT_LIMIT characters in
/// any encoding (a 3 GB log or video is never loaded whole).
const TEXT_READ: usize = TEXT_LIMIT * 4;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    /// "image" | "text"
    pub kind: String,
    /// chip label: "12 стр", "3,4 тыс. симв.", "PNG"
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// One shared client (it is a cheap handle): keeps connections and TLS sessions
/// alive between requests instead of setting everything up every time. All
/// callers run on Tauri's single async runtime, so sharing it is safe.
pub fn client() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(8))
                .user_agent("Island/1 (+https://github.com/Callzik/Island-by-Claude)")
                .build()
                .unwrap_or_default()
        })
        .clone()
}

/// "http://localhost:1234/v1/" + "models" → "http://localhost:1234/v1/models"
pub fn endpoint(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim().trim_end_matches('/'), path)
}

pub fn explain(e: &reqwest::Error, base: &str) -> String {
    if e.is_connect() {
        format!("Нет связи с {base}. Сервер запущен?")
    } else if e.is_timeout() {
        "Сервер не ответил вовремя".into()
    } else {
        format!("Ошибка запроса: {e}")
    }
}

pub async fn models(base: &str, key: &str) -> Result<Vec<String>, String> {
    let mut req = client().get(endpoint(base, "models")).timeout(std::time::Duration::from_secs(10));
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| explain(&e, base))?;
    if !resp.status().is_success() {
        return Err(format!("Сервер ответил {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut out: Vec<String> = v["data"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    out.sort();
    Ok(out)
}

/// One parsed SSE line of a streamed chat completion.
pub enum Sse {
    Delta(String),
    Done,
    Skip,
}

pub fn parse_sse_line(line: &str) -> Sse {
    let line = line.trim();
    let Some(data) = line.strip_prefix("data:") else { return Sse::Skip };
    let data = data.trim();
    if data == "[DONE]" {
        return Sse::Done;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else { return Sse::Skip };
    match v["choices"][0]["delta"]["content"].as_str() {
        Some(s) if !s.is_empty() => Sse::Delta(s.to_string()),
        _ => Sse::Skip,
    }
}

fn ext_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn thousands(n: usize) -> String {
    if n < 1000 {
        format!("{n} симв.")
    } else {
        // only the decimal point becomes a comma, not the dots of "тыс." / "симв."
        format!("{} тыс. симв.", format!("{:.1}", n as f64 / 1000.0).replace('.', ","))
    }
}

fn cut(text: String) -> (String, bool) {
    let total = text.chars().count();
    if total <= TEXT_LIMIT {
        return (text, false);
    }
    let mut s: String = text.chars().take(TEXT_LIMIT).collect();
    s.push_str(&format!("\n\n[…обрезано: показано {TEXT_LIMIT} из {total} символов]"));
    (s, true)
}

/// Text inside word/document.xml: paragraphs become lines, tabs stay tabs.
fn docx_text(bytes: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut xml = String::new();
    zip.by_name("word/document.xml").ok()?.read_to_string(&mut xml).ok()?;
    let mut out = String::new();
    let mut i = 0;
    let b = xml.as_bytes();
    // <w:tabs> holds tab-stop definitions (also called <w:tab>), not tabs
    let mut in_tabs = false;
    while i < b.len() {
        if b[i] == b'<' {
            let end = xml[i..].find('>').map(|e| i + e).unwrap_or(b.len() - 1);
            let tag = &xml[i + 1..end];
            let closing = tag.starts_with('/');
            let self_closing = tag.ends_with('/');
            // exact element name: "w:p" must not match "w:pPr", "w:tab" not "w:tabs"
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("");
            match (closing, name) {
                (true, "w:p") => out.push('\n'),
                (false, "w:tabs") if !self_closing => in_tabs = true,
                (true, "w:tabs") => in_tabs = false,
                (false, "w:tab") if !in_tabs => out.push('\t'),
                (false, "w:br") | (false, "w:cr") => out.push('\n'),
                _ => {}
            }
            i = end + 1;
        } else {
            let end = xml[i..].find('<').map(|e| i + e).unwrap_or(b.len());
            out.push_str(&xml[i..end]);
            i = end;
        }
    }
    let out = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    Some(out.trim().to_string())
}

fn read_prefix(path: &str, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    fs::File::open(path)?.take(limit as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

fn utf16(b: &[u8], big_endian: bool) -> String {
    let units: Vec<u16> = b
        .chunks_exact(2)
        .map(|p| if big_endian { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Windows-1251, the usual encoding of old Russian .txt files.
fn cp1251(b: &[u8]) -> String {
    const HIGH: [u16; 64] = [
        0x0402, 0x0403, 0x201A, 0x0453, 0x201E, 0x2026, 0x2020, 0x2021, 0x20AC, 0x2030, 0x0409, 0x2039, 0x040A, 0x040C,
        0x040B, 0x040F, 0x0452, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0xFFFD, 0x2122, 0x0459, 0x203A,
        0x045A, 0x045C, 0x045B, 0x045F, 0x00A0, 0x040E, 0x045E, 0x0408, 0x00A4, 0x0490, 0x00A6, 0x00A7, 0x0401, 0x00A9,
        0x0404, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x0407, 0x00B0, 0x00B1, 0x0406, 0x0456, 0x0491, 0x00B5, 0x00B6, 0x00B7,
        0x0451, 0x2116, 0x0454, 0x00BB, 0x0458, 0x0405, 0x0455, 0x0457,
    ];
    b.iter()
        .map(|&c| match c {
            0..=0x7F => c as char,
            0x80..=0xBF => char::from_u32(HIGH[(c - 0x80) as usize] as u32).unwrap_or('\u{FFFD}'),
            _ => char::from_u32(0x0410 + (c - 0xC0) as u32).unwrap_or('\u{FFFD}'),
        })
        .collect()
}

/// Most common byte at even or odd positions and its share. UTF-16 text has
/// its high bytes there: almost all 0x00 (Latin) or 0x04 (Cyrillic).
fn dominant(b: &[u8], odd: bool) -> (u8, f32) {
    let mut count = [0u32; 256];
    let mut n = 0u32;
    for &c in b.iter().skip(odd as usize).step_by(2).take(4096) {
        count[c as usize] += 1;
        n += 1;
    }
    let (v, c) = count.iter().enumerate().max_by_key(|(_, c)| **c).map(|(v, c)| (v as u8, *c)).unwrap_or((0, 0));
    (v, if n == 0 { 0.0 } else { c as f32 / n as f32 })
}

/// Text in UTF-8, UTF-16 (Notepad "Unicode", PowerShell 5 `>`) or, for known
/// text extensions, Windows-1251. None for binary data.
fn decode_text(b: &[u8], known_ext: bool, partial: bool) -> Option<String> {
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Some(String::from_utf8_lossy(rest).into_owned());
    }
    if let Some(rest) = b.strip_prefix(&[0xFF, 0xFE]) {
        return Some(utf16(rest, false));
    }
    if let Some(rest) = b.strip_prefix(&[0xFE, 0xFF]) {
        return Some(utf16(rest, true));
    }
    if b.len() >= 8 {
        // checked before UTF-8: UTF-16 of ASCII / Cyrillic is technically valid UTF-8
        let (ov, os) = dominant(b, true);
        let (ev, es) = dominant(b, false);
        if ov < 0x09 && os > 0.5 && es < os {
            return Some(utf16(b, false));
        }
        if ev < 0x09 && es > 0.5 && os < es {
            return Some(utf16(b, true));
        }
    }
    // count broken UTF-8 bytes: a stray ANSI byte in a UTF-8 log must not turn
    // the whole file into «РџСЂРёРІРµС‚», while real 1251 text is broken everywhere
    let mut bad = 0usize;
    let mut end = b.len();
    let mut rest = b;
    while let Err(e) = std::str::from_utf8(rest) {
        match e.error_len() {
            Some(n) => {
                bad += n;
                rest = &rest[e.valid_up_to() + n..];
            }
            None => {
                if partial {
                    // the read stopped in the middle of a character: drop it
                    end = b.len() - rest.len() + e.valid_up_to();
                } else {
                    bad += rest.len() - e.valid_up_to();
                }
                break;
            }
        }
    }
    if bad == 0 {
        return Some(String::from_utf8_lossy(&b[..end]).into_owned());
    }
    if !known_ext {
        return None;
    }
    Some(if bad * 100 > b.len() { cp1251(b) } else { String::from_utf8_lossy(b).into_owned() })
}

const TEXT_EXT: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "log", "html", "htm",
    "css", "scss", "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "rs", "go", "java", "kt", "c", "h", "cpp", "hpp",
    "cs", "php", "rb", "swift", "sh", "bat", "ps1", "sql", "lua", "dart", "vue", "svelte", "r", "m", "tex",
];

/// Reads a file into something a chat model can take.
pub fn attach(path: &str) -> Result<Attachment, String> {
    let name = crate::store::file_name(path);
    let ext = ext_of(path);
    let meta = fs::metadata(path).map_err(|_| format!("Не удалось открыть «{name}»"))?;
    if meta.is_dir() {
        return Err(format!("«{name}» — это папка"));
    }

    let image_mime = match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        _ => None,
    };
    if let Some(mime) = image_mime {
        if meta.len() > IMAGE_LIMIT {
            return Err(format!("«{name}» слишком большая картинка"));
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        return Ok(Attachment {
            name,
            kind: "image".into(),
            label: ext.to_uppercase(),
            data_url: Some(crate::native::util::data_url(mime, &bytes)),
            text: None,
        });
    }

    if (ext == "pdf" || ext == "docx") && meta.len() > DOC_LIMIT {
        return Err(format!("«{name}» слишком большой файл"));
    }
    let (text, label) = match ext.as_str() {
        "pdf" => {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            let pages = pdf_extract::extract_text_from_mem_by_pages(&bytes)
                .map_err(|_| format!("Не удалось прочитать PDF «{name}»"))?;
            let n = pages.len();
            let text = pages
                .iter()
                .enumerate()
                .map(|(i, p)| format!("— стр. {} —\n{}", i + 1, p.trim()))
                .collect::<Vec<_>>()
                .join("\n\n");
            (text, format!("{n} стр"))
        }
        "docx" => {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            let t = docx_text(&bytes).ok_or_else(|| format!("Не удалось прочитать «{name}»"))?;
            let l = thousands(t.chars().count());
            (t, l)
        }
        _ => {
            let bytes = read_prefix(path, TEXT_READ).map_err(|e| e.to_string())?;
            let partial = meta.len() > TEXT_READ as u64;
            let known = TEXT_EXT.contains(&ext.as_str());
            let Some(t) = decode_text(&bytes, known, partial) else {
                return Err(format!("«{name}»: такой формат пока не поддерживается"));
            };
            let l = thousands(t.chars().count());
            (t, l)
        }
    };
    if text.trim().is_empty() {
        return Err(format!("В «{name}» нет текста"));
    }
    let (text, cut_off) = cut(text);
    Ok(Attachment {
        name,
        kind: "text".into(),
        label: if cut_off { format!("{label} · обрезано") } else { label },
        data_url: None,
        text: Some(text),
    })
}
