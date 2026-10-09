//! AI chat against any OpenAI-compatible server (LM Studio by default):
//! model list, streamed completions (SSE) and file attachments.

use std::fs;
use std::io::Read;
use std::path::Path;

use serde::Serialize;

/// Long documents are cut to keep the request within a local model's context.
const TEXT_LIMIT: usize = 60_000;
const IMAGE_LIMIT: u64 = 12 * 1024 * 1024;

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

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .user_agent("Island/1 (+https://github.com/Callzik/Island-by-Claude)")
        .build()
        .unwrap_or_default()
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
        format!("{:.1} тыс. симв.", n as f64 / 1000.0).replace('.', ",")
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
    while i < b.len() {
        if b[i] == b'<' {
            let end = xml[i..].find('>').map(|e| i + e).unwrap_or(b.len() - 1);
            let tag = &xml[i + 1..end];
            if tag.starts_with("/w:p") {
                out.push('\n');
            } else if tag.starts_with("w:tab") {
                out.push('\t');
            } else if tag.starts_with("w:br") {
                out.push('\n');
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

    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let (text, label) = match ext.as_str() {
        "pdf" => {
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
            let t = docx_text(&bytes).ok_or_else(|| format!("Не удалось прочитать «{name}»"))?;
            let l = thousands(t.chars().count());
            (t, l)
        }
        _ if TEXT_EXT.contains(&ext.as_str()) || std::str::from_utf8(&bytes).is_ok() => {
            let t = String::from_utf8_lossy(&bytes).into_owned();
            let l = thousands(t.chars().count());
            (t, l)
        }
        _ => return Err(format!("«{name}»: такой формат пока не поддерживается")),
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
