export function ago(ts: number, now = Date.now()): string {
  const s = Math.max(0, (now - ts) / 1000);
  if (s < 60) return "сейчас";
  const m = s / 60;
  if (m < 60) return `${Math.floor(m)} мин`;
  const h = m / 60;
  if (h < 24) return `${Math.floor(h)} ч`;
  const d = h / 24;
  if (d < 7) return `${Math.floor(d)} д`;
  return new Date(ts).toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
}

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} Б`;
  const units = ["КБ", "МБ", "ГБ", "ТБ"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toLocaleString("ru-RU", { maximumFractionDigits: v < 10 ? 1 : 0 })} ${units[i]}`;
}

export function clock(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}
