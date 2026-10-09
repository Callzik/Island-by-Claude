// Demo backend used when the UI runs in a normal browser (no Tauri).
// Lets you develop and preview the island without Windows.

type Handler = (payload: any) => void;
const handlers = new Map<string, Set<Handler>>();

export function mockEmit(event: string, payload: unknown) {
  handlers.get(event)?.forEach((h) => h(payload));
}

export async function mockListen<T>(event: string, cb: (p: T) => void): Promise<() => void> {
  if (!handlers.has(event)) handlers.set(event, new Set());
  handlers.get(event)!.add(cb as Handler);
  return () => handlers.get(event)?.delete(cb as Handler);
}

const now = Date.now();
const coverSvg = `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><defs><linearGradient id='g' x1='0' y1='0' x2='0' y2='1'><stop offset='0' stop-color='#f0598a'/><stop offset='.62' stop-color='#7b2b8f'/><stop offset='1' stop-color='#2a1450'/></linearGradient></defs><rect width='100' height='100' fill='url(#g)'/><circle cx='50' cy='52' r='22' fill='#ffd6a0'/><g fill='#2a1450'><rect x='0' y='56' width='100' height='3'/><rect x='0' y='62' width='100' height='3'/><rect x='0' y='68' width='100' height='4'/></g><path d='M0 78 L22 62 L38 72 L58 58 L80 72 L100 64 L100 100 L0 100Z' fill='#1b0d33'/></svg>`;
const cover = "data:image/svg+xml;utf8," + encodeURIComponent(coverSvg);

const state = {
  settings: {
    hotkey: "Alt+Space",
    autostart: true,
    musicReactive: true,
    hideFullscreen: true,
    hoverExpand: true,
    clipLimit: 200,
    cursorPull: true,
  },
  data: {
    shelf: [
      { id: "s1", path: "C:\\Users\\me\\Documents\\Отчёт Q3.pdf", name: "Отчёт Q3.pdf", size: 2_481_233, isDir: false, added: now - 3600e3 },
      { id: "s2", path: "C:\\Users\\me\\Pictures\\Скриншот.png", name: "Скриншот.png", size: 481_233, isDir: false, added: now - 7200e3 },
      { id: "s3", path: "C:\\Users\\me\\Downloads\\design", name: "design", size: 0, isDir: true, added: now - 9600e3 },
    ],
    pins: [
      { id: "p1", target: "app:explorer", name: "Проводник" },
      { id: "p2", target: "app:notepad", name: "Блокнот" },
      { id: "p3", target: "app:powershell", name: "PowerShell" },
      { id: "p4", target: "app:taskmgr", name: "Диспетчер задач" },
    ],
    usage: { "app:powershell": 4 } as Record<string, number>,
  },
  clips: [
    { id: "c1", kind: "text", preview: "https://github.com/tauri-apps/tauri", chars: 35, files: [], thumb: null, width: 0, height: 0, source: "chrome", ts: now - 5 * 3600e3, pinned: true },
    { id: "c2", kind: "text", preview: "npm run tauri dev", chars: 17, files: [], thumb: null, width: 0, height: 0, source: "WindowsTerminal", ts: now - 5 * 3600e3, pinned: false },
    { id: "c3", kind: "text", preview: "#D97757", chars: 7, files: [], thumb: null, width: 0, height: 0, source: "Figma", ts: now - 5.2 * 3600e3, pinned: false },
    { id: "c4", kind: "text", preview: "Встречу перенесли на четверг, 15:00. Повестка та же.", chars: 52, files: [], thumb: null, width: 0, height: 0, source: "Telegram", ts: now - 6 * 3600e3, pinned: false },
    { id: "c5", kind: "files", preview: "Отчёт Q3.pdf", chars: 12, files: ["C:\\Отчёт Q3.pdf"], thumb: null, width: 0, height: 0, source: "explorer", ts: now - 26 * 3600e3, pinned: false },
  ],
  media: {
    app: "Spotify",
    appId: "Spotify.exe",
    title: "Midnight Drive",
    artist: "Neon Harbor",
    album: "Night Lines",
    playing: true,
    positionMs: 85_000,
    durationMs: 214_000,
    updatedMs: now,
    canPrev: true,
    canNext: true,
    coverId: 1,
  },
  volume: { level: 0.62, muted: false, device: "Динамики" },
};

const apps = [
  "PowerShell", "Проводник", "Блокнот", "Диспетчер задач", "Параметры", "Калькулятор", "Google Chrome",
  "Telegram", "Visual Studio Code", "Spotify", "Figma", "Paint", "Терминал", "Microsoft Edge", "Word", "Excel",
].map((name) => ({ target: "app:" + name.toLowerCase(), name }));

function iconFor(key: string): string {
  let h = 0;
  for (const ch of key) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  const hue = h % 360;
  const letter = (key.split(/[:\\]/).pop() || "?").charAt(0).toUpperCase();
  const svg = `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'><rect x='4' y='4' width='56' height='56' rx='14' fill='hsl(${hue} 70% 55%)'/><text x='32' y='42' font-size='28' font-family='Segoe UI,Arial' font-weight='700' text-anchor='middle' fill='white'>${letter}</text></svg>`;
  return "data:image/svg+xml;utf8," + encodeURIComponent(svg);
}

export async function mockInvoke<T>(cmd: string, args: Record<string, any> = {}): Promise<T> {
  const r = (v: unknown) => Promise.resolve(v as T);
  switch (cmd) {
    case "init":
      return r({ ...state, cover: { id: 1, url: cover }, version: "0.1.0-dev", hotkeyError: null });
    case "list_apps":
      return r(apps);
    case "get_icons":
      return r(Object.fromEntries((args.keys as string[]).map((k) => [k, iconFor(k)])));
    case "save_settings":
      state.settings = args.settings;
      return r(state.settings);
    case "shelf_add":
      for (const p of args.paths as string[]) {
        const name = p.split(/[\\/]/).pop() || p;
        state.data.shelf.unshift({ id: String(Math.random()), path: p, name, size: 123456, isDir: false, added: Date.now() });
      }
      return r(state.data.shelf);
    case "shelf_remove":
      state.data.shelf = state.data.shelf.filter((s) => s.id !== args.id);
      return r(state.data.shelf);
    case "shelf_clear":
      state.data.shelf = [];
      return r([]);
    case "pins_add":
      for (const t of args.targets as string[]) state.data.pins.push({ id: String(Math.random()), target: t, name: t.split(/[\\/]/).pop()! });
      return r(state.data.pins);
    case "pins_add_app":
      state.data.pins.push({ id: String(Math.random()), target: args.target, name: args.name });
      return r(state.data.pins);
    case "pins_remove":
      state.data.pins = state.data.pins.filter((p) => p.id !== args.id);
      return r(state.data.pins);
    case "media_control":
      if (args.action === "toggle") {
        state.media = { ...state.media, playing: !state.media.playing, updatedMs: Date.now() };
        mockEmit("media", state.media);
      }
      return r(null);
    case "volume_set":
      state.volume = { ...state.volume, level: args.level };
      mockEmit("volume", state.volume);
      return r(null);
    case "clip_delete":
      state.clips = state.clips.filter((c) => c.id !== args.id);
      mockEmit("clips", state.clips);
      return r(null);
    case "clip_pin":
      state.clips = state.clips.map((c) => (c.id === args.id ? { ...c, pinned: args.pinned } : c));
      mockEmit("clips", state.clips);
      return r(null);
    case "pick_files":
      return r([]);
    case "open_launcher":
      mockEmit("launcher", true);
      return r(null);
    default:
      return r(true);
  }
}

// --- simulated native events -------------------------------------------------

if (typeof window !== "undefined" && (window as any).__TAURI_INTERNALS__ === undefined) {
  let lmb = false;
  window.addEventListener("mousedown", () => (lmb = true));
  window.addEventListener("mouseup", () => (lmb = false));
  window.addEventListener("mousemove", (e) => {
    const hit = (window as any).__islandHit as { x: number; y: number; w: number; h: number } | undefined;
    const inside = !!hit && e.clientX >= hit.x && e.clientX <= hit.x + hit.w && e.clientY >= hit.y && e.clientY <= hit.y + hit.h;
    mockEmit("cursor", { x: e.clientX, y: e.clientY, inside, lmb });
  });
  window.addEventListener("keydown", (e) => {
    if (e.altKey && e.code === "Space") {
      e.preventDefault();
      mockEmit("launcher", true);
    }
  });
  // fake audio level
  let t = 0;
  setInterval(() => {
    t += 0.05;
    if (!state.media.playing) return;
    const beat = Math.abs(Math.sin(t * 3.1)) * 0.55 + Math.random() * 0.35;
    mockEmit("audio", Math.min(1, beat));
  }, 50);
  (window as any).__mock = { emit: mockEmit, state };
}
