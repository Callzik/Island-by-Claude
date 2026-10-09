import { useEffect, useState } from "react";
import { call } from "../api";

export interface RecordingState {
  active: boolean;
  startedMs: number;
}

/** Island while recording the screen: red dot and timer; a click stops. */
export function RecPill({ rec }: { rec: RecordingState }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(t);
  }, []);
  const s = Math.max(0, Math.floor((now - rec.startedMs) / 1000));
  const time = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  return (
    <button
      className="rec"
      title="Остановить запись"
      onClick={(e) => {
        e.stopPropagation();
        call("record_stop");
      }}
    >
      <i className="rec-dot" />
      <span className="rec-time">{time}</span>
      <span className="rec-stop">Стоп</span>
    </button>
  );
}
