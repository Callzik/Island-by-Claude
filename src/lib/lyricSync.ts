import type { Media } from "../api";
import type { LyricLine } from "./lyrics";

/** how far ahead of the reported position a line lights up (ms) */
const LEAD = 90;

/** Current playback position in ms (players report it together with the time it was sampled). */
export function playbackPos(m: Media | null, mediaAt: number): number | null {
  if (!m) return null;
  const now = Date.now();
  const base = m.updatedMs > 0 && Math.abs(now - m.updatedMs) < 6 * 3600e3 ? m.updatedMs : mediaAt;
  return m.positionMs + (m.playing ? Math.max(0, now - base) : 0);
}

/**
 * Drives the word highlight: finds the active line, reports it, and every frame
 * writes --p (0..1) on the words of the active line element.
 */
export function runLyricSync(
  lines: LyricLine[],
  getPos: () => number | null,
  onIdx: (i: number) => void,
  lineEl: (i: number) => HTMLElement | null,
): () => void {
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
    const base = getPos();
    if (base === null) return;
    const pos = base + LEAD;

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
      onIdx(i);
    }
    if (i < 0) return;

    const el = lineEl(i);
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
}
