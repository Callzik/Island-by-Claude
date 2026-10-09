// Synced lyrics from LRCLIB (https://lrclib.net) — fetched at runtime, never bundled.
// LRCLIB gives a timestamp per line; word timing inside a line is estimated
// from word lengths so the highlight can sweep through the line.

export interface LyricWord {
  text: string;
  /** ms from the start of the line */
  from: number;
  to: number;
}

export interface LyricLine {
  /** ms from the start of the track */
  t: number;
  text: string;
  words: LyricWord[];
}

export interface LyricsData {
  lines: LyricLine[] | null;
  plain: string | null;
  instrumental: boolean;
}

export interface LyricsQuery {
  artist: string;
  title: string;
  album: string;
  durationMs: number;
}

interface Item {
  duration?: number | null;
  instrumental?: boolean;
  plainLyrics?: string | null;
  syncedLyrics?: string | null;
}

const BASE = "https://lrclib.net/api";
const cache = new Map<string, LyricsData | null>();

export const lyricsKey = (q: LyricsQuery) => `${q.artist}\u0001${q.title}\u0001${q.album}\u0001${Math.round(q.durationMs / 1000)}`;

/** "Song (feat. X) - 2011 Remaster" → "Song" */
function cleanTitle(s: string): string {
  return s
    .replace(/\s*[(\[][^)\]]*(feat|ft\.|remaster|live|version|edit|mono|stereo|deluxe|bonus)[^)\]]*[)\]]/gi, "")
    .replace(/\s+-\s+(\d{4}\s+)?(remaster|live|single|radio|mono|stereo).*$/i, "")
    .trim();
}

function firstArtist(s: string): string {
  return s.split(/\s*(?:,|;|&|\/|\bfeat\.?|\bft\.?)\s*/i)[0].trim();
}

async function getJson(url: string, signal: AbortSignal): Promise<unknown> {
  const r = await fetch(url, { signal });
  if (!r.ok) return null;
  return r.json();
}

function pick(items: Item[], durationMs: number): Item | null {
  const d = durationMs / 1000;
  let best: Item | null = null;
  let bestScore = Infinity;
  for (const it of items) {
    if (!it.syncedLyrics && !it.plainLyrics && !it.instrumental) continue;
    const diff = d > 0 && it.duration ? Math.abs(it.duration - d) : 0;
    if (d > 0 && it.duration && diff > 6) continue;
    const score = diff + (it.syncedLyrics ? 0 : 100);
    if (score < bestScore) {
      best = it;
      bestScore = score;
    }
  }
  return best;
}

function makeWords(text: string, span: number): LyricWord[] {
  const parts = text.split(/\s+/).filter(Boolean);
  const weights = parts.map((p) => p.length + 1.5);
  const sum = weights.reduce((a, b) => a + b, 0) || 1;
  let acc = 0;
  return parts.map((p, i) => {
    const from = (acc / sum) * span;
    acc += weights[i];
    return { text: p, from, to: (acc / sum) * span };
  });
}

export function parseLrc(src: string): LyricLine[] {
  const raw: { t: number; text: string }[] = [];
  for (const row of src.split(/\r?\n/)) {
    const m = row.match(/^((?:\[\d+:\d+(?:[.:]\d+)?\])+)(.*)$/);
    if (!m) continue;
    const text = m[2].replace(/<\d+:\d+(?:[.:]\d+)?>/g, "").trim();
    for (const s of m[1].matchAll(/\[(\d+):(\d+)(?:[.:](\d+))?\]/g)) {
      const frac = s[3] ? Number(`0.${s[3]}`) : 0;
      raw.push({ t: Math.round((Number(s[1]) * 60 + Number(s[2]) + frac) * 1000), text });
    }
  }
  raw.sort((a, b) => a.t - b.t);
  const out: LyricLine[] = [];
  raw.forEach((r, i) => {
    if (!r.text) return;
    const end = i + 1 < raw.length ? raw[i + 1].t : r.t + 5000;
    const span = Math.max(400, Math.min(end - r.t - 120, r.text.length * 105 + 500));
    out.push({ t: r.t, text: r.text, words: makeWords(r.text, span) });
  });
  return out;
}

function toData(it: Item): LyricsData {
  const lines = it.syncedLyrics ? parseLrc(it.syncedLyrics) : null;
  return {
    lines: lines && lines.length ? lines : null,
    plain: it.plainLyrics?.trim() || null,
    instrumental: !!it.instrumental,
  };
}

/** null = nothing found. Throws on network errors (not cached). */
export async function fetchLyrics(q: LyricsQuery, signal: AbortSignal): Promise<LyricsData | null> {
  const key = lyricsKey(q);
  if (cache.has(key)) return cache.get(key)!;

  const title = cleanTitle(q.title) || q.title;
  const artist = firstArtist(q.artist) || q.artist;
  let found: Item | null = null;

  if (q.album && q.durationMs > 0) {
    const p = new URLSearchParams({
      artist_name: q.artist,
      track_name: q.title,
      album_name: q.album,
      duration: String(Math.round(q.durationMs / 1000)),
    });
    const it = (await getJson(`${BASE}/get?${p}`, signal)) as Item | null;
    if (it && (it.syncedLyrics || it.plainLyrics || it.instrumental)) found = it;
  }

  if (!found) {
    const p = new URLSearchParams({ track_name: title, artist_name: artist });
    const list = (await getJson(`${BASE}/search?${p}`, signal)) as Item[] | null;
    if (Array.isArray(list)) found = pick(list, q.durationMs);
  }

  if (!found) {
    const p = new URLSearchParams({ q: `${artist} ${title}` });
    const list = (await getJson(`${BASE}/search?${p}`, signal)) as Item[] | null;
    if (Array.isArray(list)) found = pick(list, q.durationMs);
  }

  const data = found ? toData(found) : null;
  cache.set(key, data);
  return data;
}
