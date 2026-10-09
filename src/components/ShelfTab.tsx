import { useRef, useState, type PointerEvent } from "react";
import { call, dragOut, type ShelfItem } from "../api";
import { extOf, fileSize } from "../lib/format";
import { iconOf, useIcon } from "../lib/icons";
import { ICopy, IFile, IFolder, IInbox, IReveal, ITrash, IClose } from "./Icons";
import type { ToastData } from "./Toast";

let fallbackPng: string | null = null;
function fallbackIcon(): string {
  if (fallbackPng) return fallbackPng;
  const c = document.createElement("canvas");
  c.width = c.height = 64;
  const g = c.getContext("2d")!;
  g.fillStyle = "#2b2b31";
  g.beginPath();
  g.roundRect(10, 4, 44, 56, 8);
  g.fill();
  g.fillStyle = "#8b7cf6";
  g.fillRect(18, 22, 28, 4);
  g.fillRect(18, 32, 22, 4);
  g.fillRect(18, 42, 26, 4);
  fallbackPng = c.toDataURL("image/png");
  return fallbackPng;
}

const IMAGE_EXT = new Set(["png", "jpg", "jpeg", "gif", "webp", "bmp", "heic", "tif", "tiff", "mp4", "mov", "mkv", "avi", "webm", "pdf"]);

function Tile({
  item,
  onRemove,
  onCopy,
  setBusy,
}: {
  item: ShelfItem;
  onRemove: () => void;
  onCopy: () => void;
  setBusy: (b: boolean) => void;
}) {
  const thumb = IMAGE_EXT.has(extOf(item.name));
  const icon = useIcon(item.path, thumb);
  const start = useRef<{ x: number; y: number } | null>(null);
  const dragged = useRef(false);

  const onDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    start.current = { x: e.clientX, y: e.clientY };
    dragged.current = false;
  };
  const onMove = (e: PointerEvent) => {
    const s = start.current;
    if (!s || dragged.current || !(e.buttons & 1)) return;
    if (Math.hypot(e.clientX - s.x, e.clientY - s.y) < 6) return;
    dragged.current = true;
    start.current = null;
    setBusy(true);
    const png = (thumb ? iconOf(item.path, false) : icon) || icon;
    const image = png && png.startsWith("data:image/png;base64,") ? png : fallbackIcon();
    dragOut([item.path], image).finally(() => setBusy(false));
  };

  return (
    <div
      className="tile"
      title={item.path}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={() => (start.current = null)}
      onClick={() => {
        if (!dragged.current) call("open_target", { target: item.path, count: false });
      }}
    >
      <div className={`tile-icon ${thumb && icon ? "is-thumb" : ""}`}>
        {icon ? <img src={icon} alt="" draggable={false} /> : item.isDir ? <IFolder size={30} /> : <IFile size={30} />}
      </div>
      <div className="tile-name">{item.name}</div>
      <div className="tile-size">{item.isDir ? "папка" : fileSize(item.size)}</div>
      <div className="tile-actions" onClick={(e) => e.stopPropagation()} onPointerDown={(e) => e.stopPropagation()}>
        <button className="icon-btn small" title="Скопировать файл" onClick={onCopy}>
          <ICopy size={14} />
        </button>
        <button className="icon-btn small" title="Показать в папке" onClick={() => call("reveal", { path: item.path })}>
          <IReveal size={14} />
        </button>
        <button className="icon-btn small" title="Убрать с полки" onClick={onRemove}>
          <IClose size={14} />
        </button>
      </div>
    </div>
  );
}

export function ShelfTab({
  shelf,
  setShelf,
  toast,
  setBusy,
}: {
  shelf: ShelfItem[];
  setShelf: (s: ShelfItem[]) => void;
  toast: (t: ToastData) => void;
  setBusy: (b: boolean) => void;
}) {
  const [confirm, setConfirm] = useState(false);

  if (shelf.length === 0) {
    return (
      <div className="shelf-empty">
        <IInbox size={34} />
        <div className="shelf-empty-title">Полка пуста</div>
        <div className="hint">Перетащите файлы на остров и отпустите над «На полку» — они подождут здесь. Отсюда их можно утащить в любое окно.</div>
      </div>
    );
  }

  const copy = async (ids: string[]) => {
    const ok = await call<boolean>("shelf_copy", { ids });
    toast(ok ? { icon: "copy", title: "Файлы в буфере", subtitle: "Вставьте их через Ctrl+V" } : { icon: "error", tone: "error", title: "Не удалось скопировать" });
  };

  return (
    <div className="shelf">
      <div className="shelf-head">
        <span className="muted">
          {shelf.length} {shelf.length === 1 ? "файл" : shelf.length < 5 ? "файла" : "файлов"} · тащите карточку в любое окно
        </span>
        <div className="shelf-buttons">
          <button className="text-btn" onClick={() => copy(shelf.map((s) => s.id))}>
            <ICopy size={15} /> Скопировать все
          </button>
          <button
            className={`text-btn ${confirm ? "danger" : ""}`}
            onClick={async () => {
              if (!confirm) {
                setConfirm(true);
                window.setTimeout(() => setConfirm(false), 2500);
                return;
              }
              setShelf(await call<ShelfItem[]>("shelf_clear"));
            }}
          >
            <ITrash size={15} /> {confirm ? "Точно очистить?" : "Очистить"}
          </button>
        </div>
      </div>
      <div className="tiles">
        {shelf.map((item) => (
          <Tile
            key={item.id}
            item={item}
            setBusy={setBusy}
            onCopy={() => copy([item.id])}
            onRemove={async () => setShelf(await call<ShelfItem[]>("shelf_remove", { id: item.id }))}
          />
        ))}
      </div>
    </div>
  );
}
