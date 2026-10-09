import { fileSize } from "../lib/format";

export interface DownloadProgress {
  name: string;
  bytes: number;
  speed: number;
  count: number;
  total?: number | null;
}

/** Island while the browser downloads: progress ring, file name, "2,4 ГБ · 14 МБ/с". */
export function DownloadPill({ d }: { d: DownloadProgress }) {
  const known = !!d.total && d.total > 0;
  const frac = known ? Math.min(1, d.bytes / d.total!) : 0.28;
  const R = 11;
  const C = 2 * Math.PI * R;
  return (
    <div className="dl">
      <svg className={`dl-ring ${known ? "" : "spin"}`} width="28" height="28" viewBox="0 0 28 28" aria-hidden="true">
        <circle cx="14" cy="14" r={R} className="dl-track" />
        <circle cx="14" cy="14" r={R} className="dl-arc" strokeDasharray={`${frac * C} ${C}`} />
        <path d="M14 9.5v8M10.8 14.6 14 17.8l3.2-3.2" className="dl-arrow" />
      </svg>
      <div className="dl-text">
        <div className="dl-name">{d.name}</div>
        <div className="dl-sub">
          {fileSize(d.bytes)}
          {d.speed > 1024 && ` · ${fileSize(d.speed)}/с`}
          {d.count > 1 && ` · ещё ${d.count - 1}`}
        </div>
      </div>
    </div>
  );
}
