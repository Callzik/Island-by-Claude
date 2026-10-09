import { useEffect, useRef, useState } from "react";
import { call, on } from "../api";
import { IClose, IMic, IStop } from "./Icons";

export interface VoiceState {
  state: "idle" | "recording" | "processing";
  startedMs: number;
}

const BARS = 22;

/** Island while dictating: red mic, live wave, timer, ■ done, × cancel. */
export function VoicePill({ voice }: { voice: VoiceState }) {
  const [now, setNow] = useState(Date.now());
  const barsRef = useRef<HTMLDivElement>(null);
  const hist = useRef<number[]>(Array(BARS).fill(0));

  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(t);
  }, []);

  // level history drives the bars directly (no re-render per sample)
  useEffect(() => {
    const un = on<number>("voice-level", (l) => {
      const h = hist.current;
      h.shift();
      h.push(Math.max(0, Math.min(1, l)));
      const el = barsRef.current;
      if (!el) return;
      for (let i = 0; i < el.children.length; i++) {
        const v = h[i] ?? 0;
        (el.children[i] as HTMLElement).style.transform = `scaleY(${0.12 + v * 0.88})`;
      }
    });
    return () => {
      un.then((u) => u());
    };
  }, []);

  const secs = Math.max(0, Math.floor((now - voice.startedMs) / 1000));
  const timer = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
  const busy = voice.state === "processing";

  return (
    <div className={`voice ${busy ? "busy" : ""}`}>
      <span className="voice-mic">
        <IMic size={17} />
      </span>
      <div className="voice-bars" ref={barsRef}>
        {Array.from({ length: BARS }, (_, i) => (
          <i key={i} />
        ))}
      </div>
      <span className="voice-time">{busy ? "Распознаю…" : timer}</span>
      {!busy && (
        <>
          <button
            className="voice-btn done"
            title="Готово — вставить текст"
            onClick={(e) => {
              e.stopPropagation();
              call("voice", { action: "stop" });
            }}
          >
            <IStop size={13} />
          </button>
          <button
            className="voice-btn"
            title="Отмена"
            onClick={(e) => {
              e.stopPropagation();
              call("voice", { action: "cancel" });
            }}
          >
            <IClose size={14} />
          </button>
        </>
      )}
    </div>
  );
}
