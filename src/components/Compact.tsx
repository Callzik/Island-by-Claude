import { useLayoutEffect, useRef } from "react";
import type { Media } from "../api";
import { useLyricSync, type LyricsState } from "../lib/useLyrics";
import { IMusic } from "./Icons";

function Cover({ url, size, radius }: { url: string | null; size: number; radius: number }) {
  return url ? (
    <img className="cover" src={url} alt="" style={{ width: size, height: size, borderRadius: radius }} draggable={false} />
  ) : (
    <div className="cover cover-empty" style={{ width: size, height: size, borderRadius: radius }}>
      <IMusic size={size * 0.55} />
    </div>
  );
}

/** Music playing, island collapsed: artwork on the left, the current lyric line in the middle, bars are drawn on the canvas. */
export function Compact({ cover, media, mediaAt, lyrics }: { cover: string | null; media: Media | null; mediaAt: number; lyrics: LyricsState }) {
  const lines = lyrics.lines;
  const wrapRef = useRef<HTMLDivElement>(null);
  const elRef = useRef<HTMLDivElement>(null);
  const idxRef = useRef(-1);
  const idx = useLyricSync(lines, media, mediaAt, (i) => (i === idxRef.current ? elRef.current : null));
  idxRef.current = idx;
  const line = lines && idx >= 0 ? lines[idx] : null;

  // long lines shrink a little to fit the island
  useLayoutEffect(() => {
    const el = elRef.current;
    const wrap = wrapRef.current;
    if (!el || !wrap) return;
    el.style.setProperty("--fit", "1");
    const fit = Math.max(0.72, Math.min(1, wrap.clientWidth / Math.max(1, el.scrollWidth)));
    el.style.setProperty("--fit", String(fit));
  }, [idx]);

  return (
    <div className="compact">
      <Cover url={cover} size={24} radius={7} />
      {lines && (
        <div className="c-lyric-wrap" ref={wrapRef}>
          {line && (
            <div className="c-lyric" key={idx} ref={elRef}>
              {line.words.map((w, k) => (
                <span key={k}>
                  <span className="w">{w.text}</span>{" "}
                </span>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** Track changed: show title and artist for a moment. */
export function Peek({ media, cover }: { media: Media; cover: string | null }) {
  return (
    <div className="peek">
      <Cover url={cover} size={44} radius={11} />
      <div className="peek-text">
        <div className="peek-title">{media.title}</div>
        <div className="peek-artist">{media.artist || media.app}</div>
      </div>
    </div>
  );
}

export { Cover };
