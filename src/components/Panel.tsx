import type { Clip, Media, PinItem, Settings, ShelfItem, Volume } from "../api";
import type { RGB } from "../lib/color";
import { IClipboard, IHome, IInbox, ILyrics, IPin, ISliders } from "./Icons";
import { Home } from "./Home";
import { Lyrics } from "./Lyrics";
import { ClipboardTab } from "./ClipboardTab";
import { ShelfTab } from "./ShelfTab";
import { SettingsTab } from "./SettingsTab";
import type { ToastData } from "./Toast";

export type Tab = "home" | "lyrics" | "clipboard" | "shelf" | "settings";

const TABS: { id: Exclude<Tab, "settings">; label: string; Icon: typeof IHome }[] = [
  { id: "home", label: "Главная", Icon: IHome },
  { id: "lyrics", label: "Текст", Icon: ILyrics },
  { id: "clipboard", label: "Буфер", Icon: IClipboard },
  { id: "shelf", label: "Полка", Icon: IInbox },
];

export interface PanelProps {
  tab: Tab;
  setTab: (t: Tab) => void;
  pinned: boolean;
  setPinned: (p: boolean) => void;
  settings: Settings;
  saveSettings: (s: Settings) => void;
  media: Media | null;
  mediaAt: number;
  cover: string | null;
  accent: RGB;
  volume: Volume;
  clips: Clip[];
  shelf: ShelfItem[];
  setShelf: (s: ShelfItem[]) => void;
  pins: PinItem[];
  setPins: (p: PinItem[]) => void;
  version: string;
  onClose: () => void;
  onLauncher: () => void;
  toast: (t: ToastData, ms?: number) => void;
  setBusy: (b: boolean) => void;
}

export function Panel(p: PanelProps) {
  return (
    <div className="panel">
      <header className="panel-head">
        <nav className="tabs">
          {TABS.map(({ id, label, Icon }) => (
            <button key={id} className={`tab ${p.tab === id ? "active" : ""}`} onClick={() => p.setTab(id)} title={label}>
              <Icon size={18} />
              {p.tab === id && <span>{label}</span>}
              {id === "shelf" && p.shelf.length > 0 && p.tab !== id && <i className="tab-badge">{p.shelf.length}</i>}
            </button>
          ))}
        </nav>
        <div className="head-actions">
          <button
            className={`icon-btn ${p.pinned ? "on" : ""}`}
            onClick={() => p.setPinned(!p.pinned)}
            title={p.pinned ? "Открепить" : "Закрепить открытым"}
          >
            <IPin size={18} />
          </button>
          <button
            className={`icon-btn ${p.tab === "settings" ? "on" : ""}`}
            onClick={() => p.setTab(p.tab === "settings" ? "home" : "settings")}
            title="Настройки"
          >
            <ISliders size={18} />
          </button>
        </div>
      </header>
      <section className="panel-body">
        {p.tab === "home" && <Home {...p} />}
        {p.tab === "lyrics" && <Lyrics media={p.media} mediaAt={p.mediaAt} accent={p.accent} />}
        {p.tab === "clipboard" && <ClipboardTab clips={p.clips} onClose={p.onClose} toast={p.toast} />}
        {p.tab === "shelf" && <ShelfTab shelf={p.shelf} setShelf={p.setShelf} toast={p.toast} setBusy={p.setBusy} />}
        {p.tab === "settings" && <SettingsTab settings={p.settings} save={p.saveSettings} version={p.version} setBusy={p.setBusy} />}
      </section>
    </div>
  );
}
