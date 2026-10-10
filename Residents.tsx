// Little residents living at the island: sitters perch at the top edge of the
// screen beside it, walkers stroll along its bottom edge. They hop to the music,
// look at the cursor and fall asleep at night.

import { useEffect, useRef, useState, type MutableRefObject } from "react";
import { on, type Cursor } from "../api";
import { byId, FRAMES, ResidentSvg, type Anim, type Face } from "../lib/residents";

export interface IslandBox {
  cx: number;
  w: number;
  h: number;
}

const SIZE = 28;

interface Mood {
  reactStart: number;
  reactUntil: number;
  calmUntil: number;
}

interface Walker {
  u: number; // -1..1 along the edge
  dir: 1 | -1;
  speed: number; // edge-widths per second
  pauseUntil: number;
}

const isNight = (d = new Date()) => d.getHours() >= 23 || d.getHours() < 6;

export function Residents({
  ids,
  boxRef,
  visible,
  playing,
}: {
  ids: string[];
  boxRef: MutableRefObject<IslandBox>;
  visible: boolean;
  playing: boolean;
}) {
  const list = ids.map(byId).filter(Boolean) as NonNullable<ReturnType<typeof byId>>[];
  const els = useRef<(HTMLDivElement | null)[]>([]);
  const [faces, setFaces] = useState<Face[]>([]);
  const [asleep, setAsleep] = useState(isNight());
  const audio = useRef(0);
  const cursor = useRef<{ x: number; y: number; at: number } | null>(null);
  const playingRef = useRef(playing);
  playingRef.current = playing;
  const asleepRef = useRef(asleep);
  asleepRef.current = asleep;
  const walkers = useRef<Walker[]>([]);
  const moods = useRef<Mood[]>([]);

  useEffect(() => {
    const subs = [
      on<number>("audio", (l) => (audio.current = l)),
      on<Cursor>("cursor", (c) => (cursor.current = { x: c.x, y: c.y, at: Date.now() })),
    ];
    const t = window.setInterval(() => setAsleep(isNight()), 60e3);
    return () => {
      window.clearInterval(t);
      subs.forEach((p) => p.then((u) => u()));
    };
  }, []);

  useEffect(() => {
    if (!list.length) return;
    let raf = 0;
    let last = performance.now();
    let lastFace = 0;
    // hidden (panel open, fullscreen app): finish the fade-out, then stop
    const stopAt = visible ? Infinity : last + 450;
    // frame budget: smooth while hopping to music or reacting, much lower when
    // they only breathe — this loop runs all day, so idle cost matters
    let interval = 1000 / 30;
    const centers: { x: number; y: number; flip: boolean }[] = [];
    const anims: { anim: Anim; frame: number }[] = [];

    const tick = (now: number) => {
      if (now > stopAt) {
        raf = 0;
        return;
      }
      raf = requestAnimationFrame(tick);
      if (now - last < interval - 3) return;
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      let busy = false;
      const t = now / 1000;
      const box = boxRef.current;
      const W = window.innerWidth;
      const sleeping = asleepRef.current;
      const amp = playingRef.current && !sleeping ? Math.min(1, audio.current * 1.6) : 0;

      let sitL = 0;
      let sitR = 0;
      list.forEach((_, i) => {
        const el = els.current[i];
        if (!el) return;
        let x: number;
        let y: number;
        let flip = false;
        let moving = false;
        const walker = i % 3 === 0;
        if (walker) {
          const w = (walkers.current[i] ??= { u: (i * 0.37) % 1 - 0.5, dir: i % 2 ? 1 : -1, speed: 0.08 + ((i * 7) % 5) * 0.015, pauseUntil: 0 });
          if (!sleeping && now > w.pauseUntil) {
            moving = true;
            w.u += w.dir * w.speed * dt * (1 + amp);
            if (Math.abs(w.u) > 1) {
              w.u = Math.sign(w.u);
              w.dir = (-w.dir) as 1 | -1;
            }
            if (Math.random() < dt * 0.25) w.pauseUntil = now + 1200 + Math.random() * 2500;
          }
          const half = Math.max(0, box.w / 2 - SIZE / 2 - 10);
          x = box.cx + w.u * half - SIZE / 2;
          y = box.h + 1;
          flip = w.dir < 0;
        } else {
          const right = (i % 3 === 1) !== (i % 2 === 0);
          const k = right ? sitR++ : sitL++;
          const off = box.w / 2 + 8 + k * (SIZE + 4);
          x = right ? box.cx + off : box.cx - off - SIZE;
          y = 1;
          flip = right; // look toward the island
        }
        // which animation and frame: react (cursor came close) > walk > idle
        const mood = (moods.current[i] ??= { reactStart: 0, reactUntil: 0, calmUntil: 0 });
        const c = cursor.current;
        const near = !!c && Date.now() - c.at < 1500 && Math.hypot(c.x - (x + SIZE / 2), c.y - (y + SIZE / 2)) < 55;
        if (near && !sleeping && now > mood.calmUntil) {
          mood.reactStart = now;
          mood.reactUntil = now + 900;
          mood.calmUntil = now + 3500;
        }
        const anim: Anim = sleeping ? "idle" : now < mood.reactUntil ? "react" : moving ? "walk" : "idle";
        if (anim !== "idle") busy = true;
        const fr = FRAMES[anim];
        const frame =
          sleeping ? 0 : anim === "react" ? Math.min(fr.n - 1, Math.floor(((now - mood.reactStart) / 1000) * fr.fps)) : Math.floor(t * fr.fps + i * 1.7) % fr.n;
        anims[i] = { anim, frame };

        const hop = -Math.abs(Math.sin(t * 7.5 + i * 1.3)) * amp * 8 + (anim === "react" ? -[5, 7, 3][frame] : 0) + (anim === "walk" && frame % 2 ? -1.2 : 0);
        const tilt = anim === "walk" ? (frame % 2 ? 5 : -5) : 0;
        const stretch = anim === "react" ? [1.12, 1.05, 0.96][frame] : 1;
        const breathe = sleeping ? 1 + Math.sin(t * 1.4 + i) * 0.03 : 1 + Math.sin(t * 2.2 + i) * 0.015;
        const hidden = box.w <= 0 || x < 2 || x > W - SIZE - 2;
        el.style.opacity = hidden ? "0" : "";
        el.style.transform = `translate(${x}px, ${y + hop}px) rotate(${tilt}deg) scale(${(flip ? -1 : 1) / Math.sqrt(stretch)}, ${breathe * stretch})`;
        centers[i] = { x: x + SIZE / 2, y: y + SIZE / 2, flip };
      });

      interval = amp > 0.02 ? 1000 / 60 : busy ? 1000 / 30 : 1000 / 15;

      // faces (eye direction) a few times a second
      if (now - lastFace > 60) {
        lastFace = now;
        const c = cursor.current && Date.now() - cursor.current.at < 4000 ? cursor.current : null;
        setFaces((prev) => {
          const next = list.map((_, i) => {
            const p = centers[i];
            let lx = 0;
            let ly = 0.25;
            if (c && p) {
              lx = Math.max(-1, Math.min(1, (c.x - p.x) / 90));
              ly = Math.max(-1, Math.min(1, (c.y - p.y) / 90));
              if (p.flip) lx = -lx;
            }
            const a = anims[i] ?? { anim: "idle" as Anim, frame: 0 };
            return { look: { x: Math.round(lx * 10) / 10, y: Math.round(ly * 10) / 10 }, asleep: sleeping, anim: a.anim, frame: a.frame };
          });
          const same =
            prev.length === next.length &&
            prev.every((f, i) => f.look.x === next[i].look.x && f.look.y === next[i].look.y && f.asleep === next[i].asleep && f.anim === next[i].anim && f.frame === next[i].frame);
          return same ? prev : next;
        });
      }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ids.join(","), visible]);

  if (!list.length) return null;
  return (
    <div className={`residents ${visible ? "" : "away"}`} aria-hidden="true">
      {list.map((r, i) => (
        <div
          key={r.id}
          className="res"
          ref={(el) => {
            els.current[i] = el;
          }}
        >
          <ResidentSvg r={r} face={faces[i] ?? { look: { x: 0, y: 0.25 }, asleep }} size={SIZE} />
          {asleep && (
            <span className="res-z" style={{ animationDelay: `${(i * 0.37) % 2}s` }}>
              z
            </span>
          )}
        </div>
      ))}
    </div>
  );
}
