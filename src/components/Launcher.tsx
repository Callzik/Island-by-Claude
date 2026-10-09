import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { call, type AppItem, type PinItem, type ShelfItem } from "../api";
import { calculate, formatNumber, plainNumber } from "../lib/calc";
import { score } from "../lib/fuzzy";
import { useIcon } from "../lib/icons";
import { convertCurrency, convertUnits, ensureRates } from "../lib/units";
import { ICalc, IEnter, IFile, IFolder, IGlobe, ISearch, ISwap } from "./Icons";
import type { ToastData } from "./Toast";

interface Row {
  id: string;
  /** shell icon of an app / file */
  target?: string;
  /** or a line icon */
  glyph?: ReactNode;
  title: string;
  hint: string;
  run: () => void;
}

const ROW_H = 46;
const MAX_ROWS = 7;

function RowIcon({ target, glyph }: { target?: string; glyph?: ReactNode }) {
  const url = useIcon(target);
  if (!target) return <span className="row-icon glyph">{glyph}</span>;
  if (url) return <img className="row-icon" src={url} alt="" draggable={false} />;
  return <span className="row-icon glyph">{/\.[a-z0-9]+$/i.test(target) && !target.startsWith("app:") ? <IFile size={18} /> : <IFolder size={18} />}</span>;
}

const looksLikeUrl = (q: string) => /^(https?:\/\/)?([\w-]+\.)+[a-zа-я]{2,}(\/\S*)?$/i.test(q.trim()) && !/\s/.test(q.trim());

export function Launcher({
  apps,
  pins,
  shelf,
  usage,
  hotkey,
  onHeight,
  onClose,
  toast,
  onLaunch,
}: {
  apps: AppItem[];
  pins: PinItem[];
  shelf: ShelfItem[];
  usage: Record<string, number>;
  hotkey: string;
  onHeight: (h: number) => void;
  onClose: (restoreFocus: boolean) => void;
  toast: (t: ToastData) => void;
  onLaunch: (target: string) => void;
}) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const [, setRatesTick] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focus = () => inputRef.current?.focus();
    focus();
    const t = window.setTimeout(focus, 60);
    window.addEventListener("focus", focus);
    return () => {
      window.clearTimeout(t);
      window.removeEventListener("focus", focus);
    };
  }, []);

  const launch = (target: string) => {
    call("open_target", { target });
    onLaunch(target);
    onClose(false);
  };

  const copy = (text: string, label: string) => {
    call("copy_text", { text });
    toast({ icon: "copy", title: "Скопировано", subtitle: label });
    onClose(true);
  };

  const rows = useMemo<Row[]>(() => {
    const query = q.trim();
    const out: Row[] = [];

    if (!query) {
      const recent = [...apps]
        .filter((a) => (usage[a.target] ?? 0) > 0 && !pins.some((p) => p.target === a.target))
        .sort((a, b) => (usage[b.target] ?? 0) - (usage[a.target] ?? 0))
        .slice(0, 4);
      for (const p of pins.slice(0, 4)) out.push({ id: "pin:" + p.id, target: p.target, title: p.name, hint: "Закреплено", run: () => launch(p.target) });
      for (const a of recent) out.push({ id: a.target, target: a.target, title: a.name, hint: "Недавнее", run: () => launch(a.target) });
      return out;
    }

    const value = calculate(query);
    if (value !== null) {
      const shown = formatNumber(value);
      out.push({ id: "calc", glyph: <ICalc size={19} />, title: `= ${shown}`, hint: "Калькулятор · Enter — скопировать", run: () => copy(plainNumber(value), `= ${shown}`) });
    }

    const conv = convertUnits(query);
    if (conv) out.push({ id: "conv", glyph: <ISwap size={19} />, title: conv.text, hint: "Перевод величин · Enter — скопировать", run: () => copy(conv.copy, conv.text) });
    else {
      const cur = convertCurrency(query);
      if (cur === "pending") out.push({ id: "cur", glyph: <ISwap size={19} />, title: "Загружаю курсы валют…", hint: "", run: () => {} });
      else if (cur) out.push({ id: "cur", glyph: <ISwap size={19} />, title: cur.text, hint: "Курс валют · Enter — скопировать", run: () => copy(cur.copy, cur.text) });
    }

    type Cand = { target: string; name: string; hint: string; s: number };
    const cands: Cand[] = [];
    for (const p of pins) {
      const s = score(p.name, query);
      if (s >= 0) cands.push({ target: p.target, name: p.name, hint: "Ярлык", s: s + 60 });
    }
    for (const a of apps) {
      if (pins.some((p) => p.target === a.target)) continue;
      const s = score(a.name, query);
      if (s >= 0) cands.push({ target: a.target, name: a.name, hint: "Программа", s: s + Math.min(150, (usage[a.target] ?? 0) * 15) });
    }
    for (const f of shelf) {
      const s = score(f.name, query);
      if (s >= 0) cands.push({ target: f.path, name: f.name, hint: "С полки", s: s - 40 });
    }
    cands.sort((a, b) => b.s - a.s);
    for (const c of cands.slice(0, 5)) out.push({ id: c.target, target: c.target, title: c.name, hint: c.hint, run: () => launch(c.target) });

    if (looksLikeUrl(query)) {
      const url = /^https?:\/\//i.test(query) ? query : "https://" + query;
      out.push({ id: "url", glyph: <IGlobe size={19} />, title: `Открыть ${query}`, hint: "Сайт", run: () => launch(url) });
    }
    out.push({
      id: "web",
      glyph: <ISearch size={19} />,
      title: `Найти в Яндексе: «${query}»`,
      hint: "Поиск",
      run: () => launch("https://yandex.ru/search/?text=" + encodeURIComponent(query)),
    });
    return out.slice(0, MAX_ROWS);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q, apps, pins, shelf, usage]);

  // exchange rates arrive asynchronously
  useEffect(() => {
    if (rows.some((r) => r.id === "cur" && r.title.startsWith("Загружаю"))) {
      ensureRates().then(() => setRatesTick((n) => n + 1));
    }
  }, [rows]);

  useEffect(() => setSel(0), [q]);

  useLayoutEffect(() => {
    onHeight(64 + rows.length * ROW_H + (rows.length ? 12 : 0));
  }, [rows.length, onHeight]);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose(true);
    } else if (e.key === "ArrowDown" || (e.key === "Tab" && !e.shiftKey)) {
      e.preventDefault();
      setSel((s) => (rows.length ? (s + 1) % rows.length : 0));
    } else if (e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey)) {
      e.preventDefault();
      setSel((s) => (rows.length ? (s - 1 + rows.length) % rows.length : 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      rows[sel]?.run();
    }
  };

  return (
    <div className="launcher">
      <div className="launcher-input">
        <ISearch size={22} />
        <input
          ref={inputRef}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={onKey}
          placeholder="Программа, расчёт, 5 км в милях…"
          spellCheck={false}
          autoComplete="off"
        />
        <kbd>{hotkey}</kbd>
      </div>
      {rows.length > 0 && (
        <div className="launcher-rows">
          {rows.map((r, i) => (
            <div key={r.id} className={`row ${i === sel ? "sel" : ""}`} onMouseMove={() => i !== sel && setSel(i)} onClick={() => r.run()}>
              <RowIcon target={r.target} glyph={r.glyph} />
              <span className="row-title">{r.title}</span>
              <span className="row-hint">{r.hint}</span>
              <span className="row-enter">{i === sel && <IEnter size={16} />}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
