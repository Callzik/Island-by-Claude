import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { call, type Clip } from "../api";
import { ago } from "../lib/format";
import { swapLayout } from "../lib/fuzzy";
import { ICopy, IFolder, IPin, ISearch, IText, ITrash } from "./Icons";
import type { ToastData } from "./Toast";

function ClipIcon({ clip }: { clip: Clip }) {
  if (clip.kind === "image" && clip.thumb) return <img className="clip-thumb" src={clip.thumb} alt="" draggable={false} />;
  return <div className="clip-icon">{clip.kind === "files" ? <IFolder size={18} /> : <IText size={18} />}</div>;
}

export function ClipboardTab({ clips, onClose, toast }: { clips: Clip[]; onClose: () => void; toast: (t: ToastData) => void }) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const [confirm, setConfirm] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);

  const shown = useMemo(() => {
    const s = q.trim().toLowerCase();
    if (!s) return clips;
    const alt = swapLayout(s);
    return clips.filter((c) => {
      const hay = (c.preview + " " + c.source).toLowerCase();
      return hay.includes(s) || hay.includes(alt);
    });
  }, [clips, q]);

  useEffect(() => setSel(0), [q]);
  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(`[data-i="${sel}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const paste = (c: Clip) => {
    onClose();
    call("clip_paste", { id: c.id, paste: true });
  };
  const copy = (c: Clip) => {
    call<boolean>("clip_paste", { id: c.id, paste: false }).then((ok) =>
      toast(ok ? { icon: "copy", title: "Скопировано", subtitle: c.preview.slice(0, 60) } : { icon: "error", tone: "error", title: "Не удалось скопировать" }),
    );
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(shown.length - 1, s + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(0, s - 1));
    } else if (e.key === "Enter" && shown[sel]) {
      e.preventDefault();
      if (e.ctrlKey) copy(shown[sel]);
      else paste(shown[sel]);
    }
  };

  return (
    <div className="clipboard">
      <div className="search-row">
        <label className="search">
          <ISearch size={17} />
          <input value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} placeholder="Поиск по истории" spellCheck={false} />
        </label>
        <button
          className={`icon-btn square ${confirm ? "danger" : ""}`}
          title={confirm ? "Нажмите ещё раз, чтобы очистить (закреплённые останутся)" : "Очистить историю"}
          onClick={() => {
            if (!confirm) {
              setConfirm(true);
              window.setTimeout(() => setConfirm(false), 2500);
              return;
            }
            setConfirm(false);
            call("clip_clear");
          }}
        >
          <ITrash size={18} />
        </button>
      </div>
      <div className="clip-list" ref={listRef}>
        {shown.length === 0 && (
          <div className="empty">{clips.length ? "Ничего не найдено" : "Скопируйте что-нибудь — история появится здесь"}</div>
        )}
        {shown.map((c, i) => (
          <div
            key={c.id}
            data-i={i}
            className={`clip ${i === sel ? "sel" : ""}`}
            onMouseEnter={() => setSel(i)}
            onClick={() => paste(c)}
            title="Клик — вставить, Ctrl+Enter — только скопировать"
          >
            <ClipIcon clip={c} />
            <div className="clip-body">
              <div className="clip-text">
                {c.kind === "image" ? `Изображение ${c.width}×${c.height}` : c.preview.replace(/\s+/g, " ").trim()}
              </div>
              <div className="clip-meta">
                {c.pinned && <IPin size={12} className="pin-mark" />}
                {c.source || "—"} · {ago(c.ts)}
                {c.kind === "text" && c.chars > 200 ? ` · ${c.chars.toLocaleString("ru-RU")} симв.` : ""}
              </div>
            </div>
            <div className="clip-actions" onClick={(e) => e.stopPropagation()}>
              <button className="icon-btn small" title="Скопировать" onClick={() => copy(c)}>
                <ICopy size={15} />
              </button>
              <button
                className={`icon-btn small ${c.pinned ? "on" : ""}`}
                title={c.pinned ? "Открепить" : "Закрепить"}
                onClick={() => call("clip_pin", { id: c.id, pinned: !c.pinned })}
              >
                <IPin size={15} />
              </button>
              <button className="icon-btn small" title="Удалить" onClick={() => call("clip_delete", { id: c.id })}>
                <ITrash size={15} />
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
