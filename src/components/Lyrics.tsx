import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { call } from "../api";
import { css } from "../lib/color";
import { fetchLyrics, lyricsKey, type LyricsData } from "../lib/lyrics";
import type { PanelProps } from "./Panel";

/** how far ahead of the reported position a line lights up (ms) */
const LEAD = 90;

type Loaded = { key: string; status: "loading" | "ok" | "none"; data: LyricsData | null };

export function Lyrics({ media, mediaAt, accent }: Pick<PanelProps, "media" | "mediaAt" | "accent">) {
  const mediaRef = useRef(media);
  mediaRef.current = media;
  const atRef = useRef(mediaAt);
  atRef.current = mediaAt;

  const key = media && media.title ? lyricsKey({ artist: media.artist, title: media.title, album: media.album, durationMs: media.durationMs }) : "";
  const [loaded, setLoaded] = useState<Loaded>({ key: "", status: "loading", data: null });
  const [idx, setIdx] = useState(-1);

  const wrapRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const lineEls = useRef<(HTMLDivElement | null)[]>([]);
  const placed = useRef(false);

  // ---- load ---------------------------------------------------------------
  useEffect(() => {
    const m = mediaRef.current;
    if (!key || !m) return;
    let dead = false;
    const ac = new AbortController();
    const timer = window.setTimeout(() => ac.abort(), 9000);
    setLoaded({ key, status: "loading", data: null });
    setIdx(-1);
    placed.current = false;
    fetchLyrics({ artist: m.artist, title: m.title, album: m.album, durationMs: m.durationMs }, ac.signal)
      .then((data) => !dead && setLoaded({ key, status: data ? "ok" : "none", data }))
      .catch(() => !dead && setLoaded({ key, status: "none", data: null }))
      .finally(() => window.clearTimeout(timer));
    return () => {
      dead = true;
      ac.abort();
      window.clearTimeout(timer);
    };
  }, [key]);

  const ready = loaded.key === key && loaded.status === "ok";
  const data = ready ? loaded.data : null;
  const lines = data?.lines ?? null;

  // ---- per-frame highlight ------------------------------------------------
  useEffect(() => {
    if (!lines) return;
    let raf = 0;
    let cur = -2;
    let words: HTMLElement[] = [];
    let wordsOf: HTMLElement | null = null;
    let lastP: number[] = [];

    const release = () => {
      for (const w of words) {
        w.style.removeProperty("--p");
        w.classList.remove("cur");
      }
      words = [];
      wordsOf = null;
      lastP = [];
    };

    const tick = () => {
      raf = requestAnimationFrame(tick);
      const m = mediaRef.current;
      if (!m) return;
      const now = Date.now();
      const base = m.updatedMs > 0 && Math.abs(now - m.updatedMs) < 6 * 3600e3 ? m.updatedMs : atRef.current;
      const pos = m.positionMs + (m.playing ? Math.max(0, now - base) : 0) + LEAD;

      let lo = 0;
      let hi = lines.length - 1;
      let i = -1;
      while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        if (lines[mid].t <= pos) {
          i = mid;
          lo = mid + 1;
        } else hi = mid - 1;
      }
      if (i !== cur) {
        release();
        cur = i;
        setIdx(i);
      }
      if (i < 0) return;

      const el = lineEls.current[i];
      if (!el) return;
      if (wordsOf !== el) {
        words = Array.from(el.querySelectorAll<HTMLElement>(".w"));
        wordsOf = el;
        lastP = [];
      }
      const line = lines[i];
      const rel = pos - line.t;
      for (let k = 0; k < words.length; k++) {
        const wd = line.words[k];
        if (!wd) break;
        const p = Math.max(0, Math.min(1, (rel - wd.from) / Math.max(1, wd.to - wd.from)));
        const q = Math.round(p * 200) / 200;
        if (lastP[k] === q) continue;
        lastP[k] = q;
        words[k].style.setProperty("--p", String(q));
        words[k].classList.toggle("cur", q > 0 && q < 1);
      }
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      release();
    };
  }, [lines]);

  // ---- keep the active line centred ----------------------------------------
  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    const sc = scrollRef.current;
    if (!wrap || !sc || !lines) return;
    const el = lineEls.current[Math.max(0, idx)];
    if (!el) return;
    const y = Math.round(wrap.clientHeight * 0.4 - (el.offsetTop + el.offsetHeight / 2));
    if (!placed.current) {
      sc.style.transition = "none";
      sc.style.transform = `translateY(${y}px)`;
      sc.getBoundingClientRect();
      sc.style.transition = "";
      placed.current = true;
    } else {
      sc.style.transform = `translateY(${y}px)`;
    }
  }, [idx, lines]);

  // ---- render ------------------------------------------------------------------
  if (!media || !media.title) {
    return (
      <div className="ly-state" key="idle">
        <div className="ly-state-title">Здесь будет текст песни</div>
        <div className="hint">Включите музыку — слова появятся и будут подсвечиваться в такт</div>
      </div>
    );
  }
  if (!ready && loaded.status !== "none") {
    return (
      <div className="ly-state" key="loading">
        <div className="ly-dots">
          <i />
          <i />
          <i />
        </div>
        <div className="hint">Ищу текст…</div>
      </div>
    );
  }
  if (loaded.key !== key || (loaded.status === "none") || !data) {
    return (
      <div className="ly-state" key="none">
        <div className="ly-state-title">Текст не найден</div>
        <div className="hint">Для этого трека в базе LRCLIB ничего нет</div>
      </div>
    );
  }
  if (data.instrumental && !lines && !data.plain) {
    return (
      <div className="ly-state" key="inst">
        <div className="ly-state-title">Инструментал</div>
        <div className="hint">В этом треке нет слов</div>
      </div>
    );
  }

  const glow = css(accent);

  if (!lines) {
    return (
      <div className="ly-wrap" key={key} style={{ "--ly-glow": glow } as CSSProperties}>
        <div className="ly-plain">{data.plain}</div>
        <div className="ly-credit">Без синхронизации · LRCLIB</div>
      </div>
    );
  }

  return (
    <div className="ly-wrap" key={key} ref={wrapRef} style={{ "--ly-glow": glow } as CSSProperties}>
      <div className="ly-scroll" ref={scrollRef}>
        {lines.map((l, i) => {
          const d = Math.abs(i - idx);
          return (
            <div
              key={i}
              ref={(el) => {
                lineEls.current[i] = el;
              }}
              className={`ly-line ${i === idx ? "act" : ""}`}
              data-far={d > 5 ? "" : undefined}
              style={{ "--d": d } as CSSProperties}
              onClick={() => call("media_control", { action: "seek", value: l.t }).catch(() => {})}
            >
              {l.words.map((w, k) => (
                <span key={k}>
                  <span className="w">{w.text}</span>{" "}
                </span>
              ))}
            </div>
          );
        })}
      </div>
      <div className="ly-credit">LRCLIB</div>
    </div>
  );
}
