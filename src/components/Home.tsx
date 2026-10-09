import { useEffect, useRef, useState, type MouseEvent } from "react";
import { call, type PinItem } from "../api";
import { css } from "../lib/color";
import { clock } from "../lib/format";
import { useIcon } from "../lib/icons";
import { Cover } from "./Compact";
import { ICommand, IEraser, ILock, IMute, INext, IPause, IPlay, IPlus, IPrev, IRegion, ISpeaker, IVolume, IClose } from "./Icons";
import type { PanelProps } from "./Panel";

function useTicker(active: boolean, ms = 250) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!active) return;
    const id = window.setInterval(() => setNow(Date.now()), ms);
    return () => window.clearInterval(id);
  }, [active, ms]);
  return now;
}

function MediaCard({ media, mediaAt, cover, accent }: Pick<PanelProps, "media" | "mediaAt" | "cover" | "accent">) {
  const now = useTicker(!!media?.playing || !media, media ? 250 : 1000);
  if (!media) {
    const d = new Date(now);
    return (
      <div className="card media-card empty">
        <div className="big-clock">{d.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" })}</div>
        <div className="big-date">{d.toLocaleDateString("ru-RU", { weekday: "long", day: "numeric", month: "long" })}</div>
        <div className="hint">Включите музыку — она появится здесь</div>
      </div>
    );
  }
  // players report the position with the time it was sampled
  const base = media.updatedMs > 0 && Math.abs(now - media.updatedMs) < 6 * 3600e3 ? media.updatedMs : mediaAt;
  const dur = media.durationMs;
  const pos = Math.min(dur || Infinity, media.positionMs + (media.playing ? Math.max(0, now - base) : 0));
  const pct = dur > 0 ? Math.max(0, Math.min(1, pos / dur)) : 0;

  const seek = (e: MouseEvent<HTMLDivElement>) => {
    if (!dur) return;
    const r = e.currentTarget.getBoundingClientRect();
    const f = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
    call("media_control", { action: "seek", value: f * dur });
  };

  return (
    <div className="card media-card">
      <Cover url={cover} size={128} radius={18} />
      <div className="media-info">
        <div className="media-app">{media.app}</div>
        <div className="media-title" title={media.title}>
          {media.title}
        </div>
        <div className="media-artist" title={media.artist}>
          {media.artist}
        </div>
        <div className="progress" onClick={seek} style={{ cursor: dur ? "pointer" : "default" }}>
          <div className="progress-fill" style={{ width: `${pct * 100}%`, background: css(accent) }} />
        </div>
        <div className="media-times">
          <span>{dur ? clock(pos) : ""}</span>
          <span>{dur ? `-${clock(dur - pos)}` : ""}</span>
        </div>
        <div className="media-controls">
          <button className="ctl" disabled={!media.canPrev} onClick={() => call("media_control", { action: "prev" })} title="Назад">
            <IPrev size={20} />
          </button>
          <button className="ctl ctl-main" onClick={() => call("media_control", { action: "toggle" })} title={media.playing ? "Пауза" : "Играть"}>
            {media.playing ? <IPause size={20} /> : <IPlay size={20} />}
          </button>
          <button className="ctl" disabled={!media.canNext} onClick={() => call("media_control", { action: "next" })} title="Вперёд">
            <INext size={20} />
          </button>
        </div>
      </div>
    </div>
  );
}

function VolumeRow({ volume, setBusy }: Pick<PanelProps, "volume" | "setBusy">) {
  const [local, setLocal] = useState<number | null>(null);
  const last = useRef(0);
  const level = local ?? Math.round(volume.level * 100);
  const send = (v: number, force = false) => {
    const t = Date.now();
    if (force || t - last.current > 40) {
      last.current = t;
      call("volume_set", { level: v / 100 });
    }
  };
  const muted = volume.muted || level === 0;
  return (
    <div className="card volume-row">
      <button className="icon-btn" onClick={() => call("volume_mute", { muted: !volume.muted })} title={volume.muted ? "Включить звук" : "Выключить звук"}>
        {muted ? <IMute size={18} /> : <IVolume size={18} />}
      </button>
      <input
        className="slider"
        type="range"
        min={0}
        max={100}
        value={level}
        style={{ ["--p" as string]: `${level}%` }}
        onPointerDown={() => setBusy(true)}
        onPointerUp={() => {
          setBusy(false);
          if (local !== null) send(local, true);
          window.setTimeout(() => setLocal(null), 600);
        }}
        onChange={(e) => {
          const v = Number(e.target.value);
          setLocal(v);
          send(v);
        }}
      />
      <span className="volume-value">{level}</span>
      {volume.device && (
        <span className="device-chip" title={volume.device}>
          <ISpeaker size={15} />
          {volume.device}
        </span>
      )}
    </div>
  );
}

function Action({ icon: Icon, label, onClick }: { icon: typeof IRegion; label: string; onClick: () => void }) {
  return (
    <button className="action" onClick={onClick}>
      <Icon size={20} />
      <span>{label}</span>
    </button>
  );
}

function PinButton({ pin, onRemove }: { pin: PinItem; onRemove: () => void }) {
  const icon = useIcon(pin.target);
  return (
    <div className="dock-item" title={pin.name}>
      <button
        className="dock-btn"
        onClick={() => call("open_target", { target: pin.target })}
        onContextMenu={(e) => {
          e.preventDefault();
          onRemove();
        }}
      >
        {icon ? <img src={icon} alt="" draggable={false} /> : <span className="dock-letter">{pin.name.charAt(0)}</span>}
      </button>
      <button className="dock-remove" onClick={onRemove} title="Убрать">
        <IClose size={11} />
      </button>
    </div>
  );
}

export function Home(p: PanelProps) {
  const addPins = async () => {
    p.setBusy(true);
    try {
      const files = await call<string[]>("pick_files", { title: "Добавить в лаунчер" });
      if (files.length) p.setPins(await call<PinItem[]>("pins_add", { targets: files }));
    } finally {
      p.setBusy(false);
    }
  };
  const removePin = async (pin: PinItem) => {
    p.setPins(await call<PinItem[]>("pins_remove", { id: pin.id }));
  };

  return (
    <div className="home">
      <div className="home-left">
        <MediaCard media={p.media} mediaAt={p.mediaAt} cover={p.cover} accent={p.accent} />
        <VolumeRow volume={p.volume} setBusy={p.setBusy} />
      </div>
      <div className="actions">
        <Action
          icon={IRegion}
          label="Область"
          onClick={() => {
            p.onClose();
            window.setTimeout(() => call("snip"), 250);
          }}
        />
        <Action
          icon={IEraser}
          label="Чистый текст"
          onClick={async () => {
            p.onClose();
            const ok = await call<boolean>("paste_plain");
            if (!ok) p.toast({ icon: "error", tone: "error", title: "В буфере нет текста" });
          }}
        />
        <Action icon={ICommand} label="Команда" onClick={p.onLauncher} />
        <Action
          icon={ILock}
          label="Блокировка"
          onClick={() => {
            p.onClose();
            call("lock_screen");
          }}
        />
      </div>
      <div className="card dock">
        {p.pins.map((pin) => (
          <PinButton key={pin.id} pin={pin} onRemove={() => removePin(pin)} />
        ))}
        <button className="dock-add" onClick={addPins} title="Добавить программу или файл">
          <IPlus size={18} />
        </button>
        {p.pins.length === 0 && <span className="dock-hint">Перетащите ярлык на остров → «В лаунчер»</span>}
      </div>
    </div>
  );
}
