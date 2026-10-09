// «Жильцы»: 25 tiny original creatures drawn in code (SVG, 32×32 grid).
// They share one face style: dot eyes that follow the cursor and close at night.

import type { ReactNode } from "react";

export interface Look {
  x: number; // -1..1
  y: number; // -1..1
}

export type Anim = "idle" | "walk" | "react";

export interface Face {
  look: Look;
  asleep: boolean;
  /** which animation plays and its frame (idle 0–3, walk 0–3, react 0–2) */
  anim?: Anim;
  frame?: number;
}

/** Frames per animation: idle blinks on its last frame, walk alternates steps, react surprises. */
export const FRAMES: Record<Anim, { n: number; fps: number }> = {
  idle: { n: 4, fps: 2.5 },
  walk: { n: 4, fps: 8 },
  react: { n: 3, fps: 9 },
};

const blinking = (f: Face) => (f.anim ?? "idle") === "idle" && f.frame === 3;
const surprised = (f: Face) => f.anim === "react";
/** -1 / +1 alternating step for walk frames, 0 otherwise */
const step = (f: Face) => (f.anim === "walk" ? ((f.frame ?? 0) % 2 ? 1 : -1) : 0);

export interface Resident {
  id: string;
  name: string;
  draw: (f: Face) => ReactNode;
}

const INK = "#1d1b22";

/** Two dot eyes (cursor-following) or sleepy arcs. */
function Eyes({ x1, x2, y, r = 1.55, f, gap = 0.9 }: { x1: number; x2: number; y: number; r?: number; f: Face; gap?: number }) {
  if (f.asleep)
    return (
      <g stroke={INK} strokeWidth={1.1} strokeLinecap="round" fill="none">
        <path d={`M${x1 - r} ${y} q${r} ${r * 0.9} ${r * 2} 0`} />
        <path d={`M${x2 - r} ${y} q${r} ${r * 0.9} ${r * 2} 0`} />
      </g>
    );
  if (blinking(f))
    return (
      <g stroke={INK} strokeWidth={1.1} strokeLinecap="round">
        <path d={`M${x1 - r} ${y} h${r * 2}`} />
        <path d={`M${x2 - r} ${y} h${r * 2}`} />
      </g>
    );
  if (surprised(f)) {
    // wide eyes looking up; they grow on the first frames of the reaction
    const k = 1.15 + Math.min(2, f.frame ?? 0) * 0.12;
    r *= k;
    gap *= 0.3;
  }
  const dx = f.look.x * gap;
  const dy = surprised(f) ? -gap : f.look.y * gap * 0.7;
  return (
    <g>
      {[x1, x2].map((x) => (
        <g key={x}>
          <circle cx={x + dx} cy={y + dy} r={r} fill={INK} />
          <circle cx={x + dx + r * 0.35} cy={y + dy - r * 0.38} r={r * 0.36} fill="#fff" />
        </g>
      ))}
    </g>
  );
}

const Blush = ({ x1, x2, y, c = "#ff8fa8" }: { x1: number; x2: number; y: number; c?: string }) => (
  <g fill={c} opacity={0.55}>
    <ellipse cx={x1} cy={y} rx={1.6} ry={0.9} />
    <ellipse cx={x2} cy={y} rx={1.6} ry={0.9} />
  </g>
);

const Mouth = ({ x, y, w = 2.4, f }: { x: number; y: number; w?: number; f?: Face }) =>
  f && surprised(f) ? (
    <ellipse cx={x} cy={y + 0.6} rx={w * 0.32} ry={w * 0.42} fill={INK} />
  ) : (
    <path d={`M${x - w / 2} ${y} q${w / 2} ${w * 0.55} ${w} 0`} stroke={INK} strokeWidth={1} fill="none" strokeLinecap="round" />
  );

export const RESIDENTS: Resident[] = [
  {
    id: "jelly",
    name: "Медуза",
    draw: (f) => (
      <>
        <g stroke="#f7a8d8" strokeWidth={1.4} fill="none" strokeLinecap="round" transform={`skewX(${((f.frame ?? 0) % 2 ? 6 : -6) * (f.asleep ? 0 : 1)})`} style={{ transformOrigin: "16px 20px" }}>
          <path d="M10 20 q-1.5 3 0 5.5 q1.5 2.5 0 5" />
          <path d="M14 21 q1.5 3 0 5.5" />
          <path d="M18 21 q-1.5 3 0 5.5" />
          <path d="M22 20 q1.5 3 0 5.5 q-1.5 2.5 0 5" />
        </g>
        <path d="M5.5 20 C5.5 9 26.5 9 26.5 20 q-2.6 1.6 -5.2 0 q-2.6 1.6 -5.3 0 q-2.6 1.6 -5.3 0 q-2.6 1.6 -5.2 0z" fill="#f4b6e3" />
        <path d="M9 13.5 q3-3.5 7-3.6" stroke="#fff" strokeWidth={1.1} fill="none" opacity={0.6} strokeLinecap="round" />
        <Eyes x1={12.5} x2={19.5} y={16} f={f} />
        <Blush x1={9.6} x2={22.4} y={18.4} />
      </>
    ),
  },
  {
    id: "cat",
    name: "Кот",
    draw: (f) => (
      <>
        <path d="M24 25 q6 -1 4 -7" stroke="#8d8a96" strokeWidth={2.4} fill="none" strokeLinecap="round" />
        <path d="M7 11 L8.5 4.5 L13 9 M25 11 L23.5 4.5 L19 9" fill="#a9a6b3" stroke="#a9a6b3" strokeWidth={1.2} strokeLinejoin="round" />
        <ellipse cx="16" cy="18" rx="10" ry="9" fill="#a9a6b3" />
        <path d="M9 7.4 L9.6 5.8 L11.2 7.6z M23 7.4 L22.4 5.8 L20.8 7.6z" fill="#f2b6c2" />
        <Eyes x1={12.4} x2={19.6} y={16.5} f={f} />
        <path d="M15.2 19.2 h1.6 l-0.8 0.9z" fill="#f28fa5" />
        <g stroke="#f3f1f7" strokeWidth={0.6} strokeLinecap="round">
          <path d="M8 19 h-3.5 M8 20.5 l-3.2 1" />
          <path d="M24 19 h3.5 M24 20.5 l3.2 1" />
        </g>
      </>
    ),
  },
  {
    id: "ghost",
    name: "Призрак",
    draw: (f) => (
      <>
        <path d="M6.5 27 V15 a9.5 9.5 0 0 1 19 0 V27 q-1.6 -1.8 -3.2 0 t-3.2 0 t-3.1 0 t-3.2 0 t-3.1 0 t-3.2 0z" fill="#f4f2fb" />
        <path d="M6.5 19 q-3 1 -2.5 4 M25.5 19 q3 1 2.5 4" stroke="#f4f2fb" strokeWidth={2.2} strokeLinecap="round" fill="none" />
        <Eyes x1={12.6} x2={19.4} y={14.5} f={f} r={1.7} />
        <Blush x1={10.4} x2={21.6} y={17.4} />
        <ellipse cx="16" cy="19" rx="1.3" ry="1.6" fill={INK} opacity={f.asleep ? 0 : 0.85} />
      </>
    ),
  },
  {
    id: "coffee",
    name: "Кофе",
    draw: (f) => (
      <>
        <g className="res-steam" stroke="#e9e4dc" strokeWidth={1.2} fill="none" strokeLinecap="round" opacity={0.75}>
          <path d="M12 9 q-2 -2.5 0 -5" />
          <path d="M16 9 q2 -2.5 0 -5" />
          <path d="M20 9 q-2 -2.5 0 -5" />
        </g>
        <path d="M24.5 15 h1.8 a3.3 3.3 0 0 1 0 6.6 h-2.3" stroke="#f0ebe3" strokeWidth={2} fill="none" />
        <path d="M6.5 11 h18 v9.5 a6 6 0 0 1 -6 6 h-6 a6 6 0 0 1 -6 -6z" fill="#f0ebe3" />
        <ellipse cx="15.5" cy="11.2" rx="9" ry="1.6" fill="#7a4a2b" />
        <Eyes x1={12.4} x2={18.6} y={17} f={f} />
        <Mouth x={15.5} y={20} f={f} />
        <Blush x1={10} x2={21} y={19.3} />
      </>
    ),
  },
  {
    id: "cactus",
    name: "Кактус",
    draw: (f) => (
      <>
        <path d="M8.5 23 h15 l-1.6 7 h-11.8z" fill="#d9774f" />
        <rect x="8" y="21.5" width="16" height="2.6" rx="1" fill="#e98b62" />
        <path d="M9.5 15 v-2.5 a2 2 0 0 1 4 0 M22.5 12.5 v-2 a2 2 0 0 0 -4 0" stroke="#5fae6b" strokeWidth={3.2} fill="none" strokeLinecap="round" />
        <rect x="11" y="6" width="10" height="16.5" rx="5" fill="#6cc078" />
        <circle cx="16" cy="5.6" r="1.6" fill="#ff9ec7" />
        <Eyes x1={13.8} x2={18.2} y={12.5} f={f} r={1.3} />
        <Mouth x={16} y={15} w={2} f={f} />
      </>
    ),
  },
  {
    id: "snail",
    name: "Улитка",
    draw: (f) => (
      <>
        <path d="M3 26 h20 q4 0 4.5 -4 l0.5 -7" stroke="#c9d98a" strokeWidth={4} fill="none" strokeLinecap="round" />
        <path d="M26.5 15 l-1.5 -5 M29 15 l1 -5" stroke="#c9d98a" strokeWidth={1.2} strokeLinecap="round" />
        <circle cx="13" cy="17" r="8" fill="#f0a35e" />
        <path d="M13 17 m-5 0 a5 5 0 1 0 5 -5 a3.4 3.4 0 1 0 3 4.5 a1.6 1.6 0 1 0 -2 -1.5" stroke="#c97a39" strokeWidth={1.4} fill="none" strokeLinecap="round" />
        <g transform="translate(13.8 -3)">
          <Eyes x1={11.2} x2={15.2} y={12.6} f={f} r={1.2} gap={0.5} />
        </g>
      </>
    ),
  },
  {
    id: "octo",
    name: "Осьминог",
    draw: (f) => (
      <>
        <g stroke="#a98be6" strokeWidth={2.4} fill="none" strokeLinecap="round">
          <path d="M8 20 q-3 5 1 7" />
          <path d="M12.5 22 q-1 5 2 6" />
          <path d="M19.5 22 q1 5 -2 6" />
          <path d="M24 20 q3 5 -1 7" />
        </g>
        <ellipse cx="16" cy="14.5" rx="10" ry="9" fill="#b89cf0" />
        <circle cx="10" cy="10" r="1.1" fill="#d6c6fa" />
        <circle cx="22.5" cy="9" r="0.8" fill="#d6c6fa" />
        <Eyes x1={12.6} x2={19.4} y={15.5} f={f} />
        <Blush x1={10} x2={22} y={18.4} />
      </>
    ),
  },
  {
    id: "fox",
    name: "Лис",
    draw: (f) => (
      <>
        <path d="M5 8 L9 2.5 L13 8z M27 8 L23 2.5 L19 8z" fill="#f08a3c" />
        <path d="M7.2 7.2 L9 4.8 L10.8 7.2z M24.8 7.2 L23 4.8 L21.2 7.2z" fill="#3a2a25" />
        <path d="M4.5 9 Q16 3 27.5 9 L22 22 Q16 27 10 22z" fill="#f08a3c" />
        <path d="M6 12 Q12 15 16 25.5 Q20 15 26 12 L22 22 Q16 27 10 22z" fill="#fff6ea" />
        <Eyes x1={12.2} x2={19.8} y={13.4} f={f} />
        <ellipse cx="16" cy="22.6" rx="1.4" ry="1" fill={INK} />
      </>
    ),
  },
  {
    id: "robot",
    name: "Робот",
    draw: (f) => (
      <>
        <line x1="16" y1="7" x2="16" y2="3.5" stroke="#8f9aa8" strokeWidth={1.2} />
        <circle cx="16" cy="3" r="1.5" className="res-blink" fill="#ff6b6b" />
        <rect x="4" y="13" width="2.5" height="6" rx="1" fill="#8f9aa8" />
        <rect x="25.5" y="13" width="2.5" height="6" rx="1" fill="#8f9aa8" />
        <rect x="6" y="7" width="20" height="17" rx="5" fill="#c8d1dc" />
        <rect x="8.5" y="10" width="15" height="9" rx="3" fill="#1f2a36" />
        {f.asleep ? (
          <g stroke="#6ee7c8" strokeWidth={1.1} strokeLinecap="round">
            <path d="M11.3 14.8 h3 M17.7 14.8 h3" />
          </g>
        ) : (
          <g fill="#6ee7c8">
            <rect x={11.6 + f.look.x * 1.1} y={12.6 + f.look.y * 0.8} width="2.4" height="3.6" rx="1" />
            <rect x={18 + f.look.x * 1.1} y={12.6 + f.look.y * 0.8} width="2.4" height="3.6" rx="1" />
          </g>
        )}
        <rect x="12" y="25" width="8" height="4" rx="1.5" fill="#a9b3bf" />
      </>
    ),
  },
  {
    id: "shroom",
    name: "Гриб",
    draw: (f) => (
      <>
        <path d="M11.5 18 h9 l1 9.5 a2 2 0 0 1 -2 2 h-7 a2 2 0 0 1 -2 -2z" fill="#f3e6cf" />
        <path d="M3.5 18.5 C3.5 7 28.5 7 28.5 18.5 q-12.5 2.6 -25 0z" fill="#3fb3a6" />
        <circle cx="10" cy="12.5" r="1.6" fill="#8fe0d5" />
        <circle cx="21.5" cy="11" r="2.1" fill="#8fe0d5" />
        <circle cx="16" cy="9" r="1.1" fill="#8fe0d5" />
        <Eyes x1={13} x2={19} y={15.4} f={f} r={1.35} />
        <Blush x1={10.5} x2={21.5} y={17} c="#ffb3c7" />
      </>
    ),
  },
  {
    id: "penguin",
    name: "Пингвин",
    draw: (f) => (
      <>
        <ellipse cx="16" cy="17" rx="10" ry="11.5" fill="#2f3f5c" />
        <ellipse cx="16" cy="19.5" rx="6.6" ry="8" fill="#f5f3ee" />
        <path d="M6.5 15 q-3 4 -1 8 M25.5 15 q3 4 1 8" stroke="#2f3f5c" strokeWidth={2.6} fill="none" strokeLinecap="round" />
        <path d="M8.5 21 q7.5 3 15 0 l0.5 2.8 q-8 3 -16 0z" fill="#e2524f" />
        <path d="M21 23 l2.5 4.5 l-2.8 0.2z" fill="#e2524f" />
        <Eyes x1={12.6} x2={19.4} y={13} f={f} r={1.45} />
        <path d="M14.6 15.6 h2.8 l-1.4 1.8z" fill="#f5a03a" />
        <path d={`M10.5 ${28.6 + step(f) * 0.8} h4 M17.5 ${28.6 - step(f) * 0.8} h4`} stroke="#f5a03a" strokeWidth={1.8} strokeLinecap="round" />
      </>
    ),
  },
  {
    id: "whale",
    name: "Кит",
    draw: (f) => (
      <>
        <g className="res-steam" stroke="#9fd2ff" strokeWidth={1.3} fill="none" strokeLinecap="round">
          <path d="M14 8 v-3.5 M14 5 q-2 -2.5 -4 -1.5 M14 5 q2 -2.5 4 -1.5" />
        </g>
        <path d="M3 18 C3 9 23 8 25 16 q1.5 -4 5 -5 q-1 5 -3 7 q2 2 3 6 q-4 -1 -5.5 -4 C22 27 3 28 3 18z" fill="#5d9ee8" />
        <path d="M4.5 20 q10 6 19 -0.5 q-1.5 5.5 -9.5 6 q-8 0 -9.5 -5.5z" fill="#d6e9ff" />
        <Eyes x1={9} x2={15} y={15.6} f={f} r={1.35} />
        <Blush x1={7} x2={17} y={18.2} />
      </>
    ),
  },
  {
    id: "owlet",
    name: "Совёнок",
    draw: (f) => (
      <>
        <path d="M6 7 L9 11 M26 7 L23 11" stroke="#8a6248" strokeWidth={2.6} strokeLinecap="round" />
        <ellipse cx="16" cy="17.5" rx="10.5" ry="11" fill="#a0745a" />
        <ellipse cx="16" cy="21.5" rx="6" ry="6.5" fill="#e9d2b4" />
        <circle cx="11.7" cy="14" r="4" fill="#f6ead7" />
        <circle cx="20.3" cy="14" r="4" fill="#f6ead7" />
        <Eyes x1={11.7} x2={20.3} y={14} f={f} r={1.9} gap={1.2} />
        <path d="M14.8 17.5 h2.4 l-1.2 2z" fill="#f5a03a" />
        <path d="M12.5 21.5 l1 1 l1-1 M17.5 21.5 l1 1 l1-1 M15 24 l1 1 l1-1" stroke="#c7a582" strokeWidth={0.8} fill="none" />
      </>
    ),
  },
  {
    id: "slug",
    name: "Слизень",
    draw: (f) => (
      <>
        <path d="M11 13 l-2 -6 M15 13 l1 -6" stroke="#9bd35a" strokeWidth={1.4} strokeLinecap="round" />
        <path d="M3 27 Q3 21 9 18 Q12 13 18 15 Q26 17 29 27z" fill="#a8de68" />
        <path d="M9 19.5 q4 -3 8 -2" stroke="#d6f2a8" strokeWidth={1.2} fill="none" strokeLinecap="round" />
        <g transform="translate(0 -1)">
          <Eyes x1={8.6} x2={16.4} y={7.6} f={f} r={1.25} gap={0.5} />
        </g>
        <Mouth x={13} y={23} f={f} />
      </>
    ),
  },
  {
    id: "pumpkin",
    name: "Тыква",
    draw: (f) => (
      <>
        <path d="M16 9 q0 -3 3 -4.5" stroke="#5d8a3a" strokeWidth={2} fill="none" strokeLinecap="round" />
        <path d="M18.5 6 q3 -1 4.5 1 q-3 1.5 -4.5 -1z" fill="#76b046" />
        <ellipse cx="10.5" cy="18" rx="6.5" ry="9" fill="#f08a2c" />
        <ellipse cx="21.5" cy="18" rx="6.5" ry="9" fill="#f08a2c" />
        <ellipse cx="16" cy="18" rx="6.5" ry="9.5" fill="#f59a3e" />
        <Eyes x1={13} x2={19} y={16.5} f={f} />
        <Mouth x={16} y={20.5} w={3} f={f} />
      </>
    ),
  },
  {
    id: "star",
    name: "Звёздочка",
    draw: (f) => (
      <>
        <path
          d="M16 3.5 L19.3 10.6 L27 11.5 L21.3 16.8 L22.9 24.5 L16 20.7 L9.1 24.5 L10.7 16.8 L5 11.5 L12.7 10.6z"
          fill="#c9b8ff"
          stroke="#c9b8ff"
          strokeWidth={2.4}
          strokeLinejoin="round"
        />
        <Eyes x1={13.6} x2={18.4} y={14} f={f} r={1.3} />
        <Blush x1={11.4} x2={20.6} y={16.6} />
        <circle cx="25" cy="5" r="0.9" fill="#fff" className="res-blink" />
      </>
    ),
  },
  {
    id: "cloud",
    name: "Облачко",
    draw: (f) => (
      <>
        <path d="M8 24 h16 a5.5 5.5 0 0 0 0.6 -11 a7.6 7.6 0 0 0 -14.5 -1.2 a6.2 6.2 0 0 0 -2.1 12.2z" fill="#f1f4fa" />
        <Eyes x1={13} x2={19} y={17} f={f} r={1.35} />
        <Blush x1={10.4} x2={21.6} y={19.4} />
        <path className="res-drop" d="M16 26.5 q-1.4 2 0 3 q1.4 -1 0 -3z" fill="#7fbaff" />
      </>
    ),
  },
  {
    id: "frog",
    name: "Лягушка",
    draw: (f) => (
      <>
        <ellipse cx="16" cy="20" rx="11.5" ry="8" fill="#6cc46e" />
        <circle cx="10.5" cy="11.5" r="4.4" fill="#6cc46e" />
        <circle cx="21.5" cy="11.5" r="4.4" fill="#6cc46e" />
        <circle cx="10.5" cy="11.5" r="2.7" fill="#fdfdf5" />
        <circle cx="21.5" cy="11.5" r="2.7" fill="#fdfdf5" />
        <Eyes x1={10.5} x2={21.5} y={11.5} f={f} r={1.4} gap={0.8} />
        <path d="M10 20 q6 4 12 0" stroke={INK} strokeWidth={1.1} fill="none" strokeLinecap="round" />
        <Blush x1={7.6} x2={24.4} y={18.6} />
      </>
    ),
  },
  {
    id: "rocket",
    name: "Ракета",
    draw: (f) => (
      <>
        <path
          d={`M13 25 q3 ${4 + ((f.frame ?? 0) % 3)} 6 0z`}
          fill="#ffb347"
          opacity={f.asleep ? 0 : 1}
        />
        <path d={`M14.3 25 q1.7 ${2.2 + ((f.frame ?? 0) % 2)} 3.4 0z`} fill="#fff1a8" opacity={f.asleep ? 0 : 1} />
        <path d="M10.5 18 l-3.5 5 v2 h4z M21.5 18 l3.5 5 v2 h-4z" fill="#ef5f5f" />
        <path d="M16 2.5 C22 7 22 18 20.5 25 h-9 C10 18 10 7 16 2.5z" fill="#eef1f6" />
        <path d="M13.4 7 q2.6 -3 5.2 0z" fill="#ef5f5f" />
        <circle cx="16" cy="13.5" r="4" fill="#3a4b66" />
        <circle cx="16" cy="13.5" r="4" fill="none" stroke="#c4ccd8" strokeWidth={1} />
        <Eyes x1={14.4} x2={17.6} y={13.6} f={f} r={0.95} gap={0.6} />
        <rect x="14.5" y="23" width="3" height="2" rx="0.6" fill="#ef5f5f" />
      </>
    ),
  },
  {
    id: "turtle",
    name: "Черепаха",
    draw: (f) => (
      <>
        <g fill="#8ccf7e">
          <ellipse cx={9.5 + step(f) * 1.2} cy="25.5" rx="2.6" ry="2" />
          <ellipse cx={20.5 - step(f) * 1.2} cy="25.5" rx="2.6" ry="2" />
          <circle cx="26.5" cy="17.5" r="4.4" />
          <path d="M3 21 l-1.5 2 l3 -0.5z" />
        </g>
        <path d="M3.5 22 C3.5 10 23.5 10 23.5 22z" fill="#5f9e5a" />
        <path d="M8 21.5 l2 -5 h7 l2 5 M10 16.5 l-1.5 -2.5 M17 16.5 l1.5 -2.5 M13.5 16.5 v-4" stroke="#7fbf6f" strokeWidth={1} fill="none" />
        <rect x="3" y="21" width="21" height="2.4" rx="1.2" fill="#d9c58a" />
        <g transform="translate(0 0)">
          <Eyes x1={25.2} x2={28.6} y={16.6} f={f} r={1} gap={0.5} />
        </g>
        <Mouth x={27} y={19} w={1.8} f={f} />
      </>
    ),
  },
  {
    id: "comet",
    name: "Комета",
    draw: (f) => (
      <>
        <g stroke="#ffd98a" strokeLinecap="round" opacity={0.8}>
          <path d="M4 6 L14 13" strokeWidth={2.4} />
          <path d="M2 13 L12.5 16.5" strokeWidth={1.8} />
          <path d="M5 21 L13 19" strokeWidth={1.4} />
        </g>
        {[0, 1, 2].map((i) => (
          <circle key={i} cx={4 + i * 3.5} cy={((f.frame ?? 0) + i) % 2 ? 9 + i * 4 : 11 + i * 3} r={0.7} fill="#fff6d0" opacity={f.asleep ? 0.3 : 1} />
        ))}
        <circle cx="20" cy="16" r="8.5" fill="#ffe7a3" />
        <circle cx="20" cy="16" r="8.5" fill="none" stroke="#fff4cf" strokeWidth={1.2} opacity={0.7} />
        <Eyes x1={17.4} x2={22.6} y={15.4} f={f} r={1.35} />
        <Blush x1={15.4} x2={24.6} y={18.2} c="#ffae7a" />
      </>
    ),
  },
  {
    id: "candle",
    name: "Свеча",
    draw: (f) => {
      const fl = f.asleep ? 0 : (f.frame ?? 0) % 3;
      return (
        <>
          {!f.asleep && (
            <>
              <path d={`M16 ${2.5 + fl * 0.4} C${19.5 - fl * 0.5} 7 18.5 10.5 16 10.5 C13.5 10.5 ${12.5 + fl * 0.4} 7 16 ${2.5 + fl * 0.4}z`} fill="#ffb84d" />
              <path d="M16 6 C17.4 8 17 10 16 10 C15 10 14.6 8 16 6z" fill="#fff3b0" />
            </>
          )}
          <line x1="16" y1="10.5" x2="16" y2="12.5" stroke="#4a3b30" strokeWidth={1} />
          <rect x="10" y="12" width="12" height="16" rx="2.5" fill="#f6efe0" />
          <path d="M10 14 q2 0 2 3 q0 2 1.4 2 q1.4 0 1.4 -3 q0 -2 1.2 -2" fill="#fbf7ee" />
          <rect x="8" y="27" width="16" height="2.5" rx="1.2" fill="#c9a777" />
          <Eyes x1={13.6} x2={18.4} y={19.5} f={f} r={1.2} />
          <Mouth x={16} y={22.4} w={2} f={f} />
        </>
      );
    },
  },
  {
    id: "cube",
    name: "Кубик",
    draw: (f) => (
      <>
        <path d="M16 4 L27 9.5 L16 15 L5 9.5z" fill="#9fe3ff" />
        <path d="M5 9.5 L16 15 V27.5 L5 22z" fill="#4fb4e0" />
        <path d="M27 9.5 L16 15 V27.5 L27 22z" fill="#3797c4" />
        <path d="M16 4 L27 9.5 L16 15 L5 9.5z" fill="none" stroke="#c9f1ff" strokeWidth={0.6} />
        <g transform="translate(-4.6 4.4) skewY(26)" style={{ transformOrigin: "10.5px 15px" }}>
          <Eyes x1={9} x2={13} y={14.5} f={f} r={1.15} gap={0.6} />
        </g>
        <circle cx={22.5} cy={14 + step(f) * 0.6} r="1" fill="#d7f4ff" opacity={0.8} />
      </>
    ),
  },
  {
    id: "drop",
    name: "Капелька",
    draw: (f) => (
      <>
        <path d="M16 3 C21 10 26 15 26 20.5 a10 10 0 0 1 -20 0 C6 15 11 10 16 3z" fill="#6fb3ff" />
        <path d="M10 18 q0.5 -4 3.5 -7" stroke="#cfe6ff" strokeWidth={1.6} fill="none" strokeLinecap="round" />
        <Eyes x1={12.8} x2={19.2} y={19} f={f} />
        <Mouth x={16} y={22.4} f={f} />
      </>
    ),
  },
  {
    id: "bee",
    name: "Пчела",
    draw: (f) => (
      <>
        <g className="res-wings" fill="#e7f3ff" opacity={(f.frame ?? 0) % 2 ? 0.65 : 0.95}>
          <ellipse cx="12" cy="9" rx="3.6" ry="5" transform="rotate(-25 12 9)" />
          <ellipse cx="20" cy="9" rx="3.6" ry="5" transform="rotate(25 20 9)" />
        </g>
        <ellipse cx="16" cy="18.5" rx="10.5" ry="8.5" fill="#ffcd3c" />
        <path d="M19.5 10.6 a10.5 8.5 0 0 1 3 1.3 v13.3 a10.5 8.5 0 0 1 -3 1.3z" fill={INK} />
        <path d="M26.5 18.5 l2.5 0" stroke={INK} strokeWidth={1.6} strokeLinecap="round" />
        <Eyes x1={9.6} x2={15} y={17.2} f={f} r={1.35} />
        <Mouth x={12.3} y={20.6} w={2} f={f} />
        <path d="M10 10.4 l-1.6 -3 M13.6 10 l0.4 -3.2" stroke={INK} strokeWidth={0.9} strokeLinecap="round" />
      </>
    ),
  },
];

export const RESIDENT_IDS = RESIDENTS.map((r) => r.id);
export const byId = (id: string) => RESIDENTS.find((r) => r.id === id);

export function ResidentSvg({ r, face, size = 28 }: { r: Resident; face: Face; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
      {r.draw(face)}
    </svg>
  );
}
