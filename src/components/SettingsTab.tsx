import { useState, type KeyboardEvent } from "react";
import { call, type Settings } from "../api";
import { IKeyboard, IPower } from "./Icons";

function Toggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button className={`toggle ${checked ? "on" : ""}`} role="switch" aria-checked={checked} onClick={() => onChange(!checked)}>
      <i />
    </button>
  );
}

const CODE_NAMES: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Tab: "Tab",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

/** KeyboardEvent -> "Ctrl+Shift+K" (format understood by the global-shortcut plugin). */
function comboFrom(e: KeyboardEvent): string | null {
  const code = e.code;
  let key: string | null = null;
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
  else if (/^Digit\d$/.test(code)) key = code.slice(5);
  else if (/^F\d{1,2}$/.test(code)) key = code;
  else if (CODE_NAMES[code]) key = CODE_NAMES[code];
  if (!key) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
  if (mods.length === 0 && !/^F\d/.test(key)) return null;
  return [...mods, key].join("+");
}

function HotkeyField({ value, onChange, setBusy }: { value: string; onChange: (v: string) => void; setBusy: (b: boolean) => void }) {
  const [recording, setRec] = useState(false);
  const setRecording = (r: boolean) => {
    setRec(r);
    setBusy(r);
  };
  return (
    <button
      className={`hotkey ${recording ? "rec" : ""}`}
      onClick={() => setRecording(true)}
      onBlur={() => recording && setRecording(false)}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        e.stopPropagation();
        if (e.key === "Escape") return setRecording(false);
        const combo = comboFrom(e);
        if (combo) {
          setRecording(false);
          onChange(combo);
        }
      }}
    >
      <IKeyboard size={16} />
      {recording ? "Нажмите сочетание…" : value}
    </button>
  );
}

export function SettingsTab({
  settings,
  save,
  version,
  setBusy,
}: {
  settings: Settings;
  save: (s: Settings) => void;
  version: string;
  setBusy: (b: boolean) => void;
}) {
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => save({ ...settings, [k]: v });
  const [confirm, setConfirm] = useState(false);

  const rows: { key: keyof Settings; title: string; sub: string }[] = [
    { key: "autostart", title: "Запускать вместе с Windows", sub: "Остров появится сразу после входа" },
    { key: "hoverExpand", title: "Раскрывать при наведении", sub: "Иначе — по клику" },
    { key: "musicReactive", title: "Волна под музыку", sub: "Край острова дрожит в такт звуку" },
    { key: "cursorPull", title: "Тянуться к курсору", sub: "Жидкий край вытягивается навстречу" },
    { key: "hideFullscreen", title: "Прятать в полноэкранных приложениях", sub: "Игры, видео, презентации" },
  ];

  return (
    <div className="settings">
      <div className="settings-grid">
        {rows.map((r) => (
          <div className="setting" key={r.key}>
            <div>
              <div className="setting-title">{r.title}</div>
              <div className="setting-sub">{r.sub}</div>
            </div>
            <Toggle checked={settings[r.key] as boolean} onChange={(v) => set(r.key, v as never)} />
          </div>
        ))}
        <div className="setting">
          <div>
            <div className="setting-title">Лаунчер</div>
            <div className="setting-sub">Глобальное сочетание клавиш</div>
          </div>
          <HotkeyField value={settings.hotkey} onChange={(v) => set("hotkey", v)} setBusy={setBusy} />
        </div>
        <div className="setting">
          <div>
            <div className="setting-title">История буфера</div>
            <div className="setting-sub">Сколько записей хранить (закреплённые — всегда)</div>
          </div>
          <select
            className="select"
            value={settings.clipLimit}
            onFocus={() => setBusy(true)}
            onBlur={() => setBusy(false)}
            onChange={(e) => {
              setBusy(false);
              set("clipLimit", Number(e.target.value));
            }}
          >
            {[50, 100, 200, 500, 1000].map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </div>
      </div>
      <div className="settings-foot">
        <span className="muted">Island {version} · Alt+Space — лаунчер · перетащите файл на остров — полка</span>
        <button
          className={`text-btn ${confirm ? "danger" : ""}`}
          onClick={() => {
            if (!confirm) {
              setConfirm(true);
              window.setTimeout(() => setConfirm(false), 2500);
              return;
            }
            call("quit");
          }}
        >
          <IPower size={15} /> {confirm ? "Точно выйти?" : "Выйти"}
        </button>
      </div>
    </div>
  );
}
