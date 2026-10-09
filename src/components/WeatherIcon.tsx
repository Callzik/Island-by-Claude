// Weather glyphs (own SVG): sun, moon, clouds, rain, snow, thunder, fog.

import type { ReactNode } from "react";
import { sky, type Sky } from "../lib/weather";

const SUN = "#ffc94a";
const MOON = "#e9e3c9";
const CLOUD = "#dfe3ea";
const CLOUD_DARK = "#a9b0bc";
const RAIN = "#6fb3ff";
const SNOW = "#ffffff";
const BOLT = "#ffd34d";

function Sun({ x = 12, y = 12, r = 4.6 }: { x?: number; y?: number; r?: number }) {
  const rays = Array.from({ length: 8 }, (_, i) => {
    const a = (i * Math.PI) / 4;
    const r1 = r + 2.2;
    const r2 = r + 4.4;
    return <line key={i} x1={x + Math.cos(a) * r1} y1={y + Math.sin(a) * r1} x2={x + Math.cos(a) * r2} y2={y + Math.sin(a) * r2} />;
  });
  return (
    <g stroke={SUN} strokeWidth={1.8} strokeLinecap="round">
      <circle cx={x} cy={y} r={r} fill={SUN} stroke="none" />
      {rays}
    </g>
  );
}

function Moon({ x = 12, y = 12, r = 6.5 }: { x?: number; y?: number; r?: number }) {
  // crescent: a circle with a bite taken out by a shifted circle
  return (
    <path
      d={`M${x + r * 0.35} ${y - r} A${r} ${r} 0 1 0 ${x + r} ${y + r * 0.35} A${r * 0.82} ${r * 0.82} 0 0 1 ${x + r * 0.35} ${y - r}Z`}
      fill={MOON}
    />
  );
}

function Cloud({ dx = 0, dy = 0, dark = false, s = 1 }: { dx?: number; dy?: number; dark?: boolean; s?: number }) {
  return (
    <path
      transform={`translate(${dx} ${dy}) scale(${s})`}
      d="M7.2 18.5h10.3a3.7 3.7 0 0 0 .5-7.37A5.2 5.2 0 0 0 8.1 9.8 4.35 4.35 0 0 0 7.2 18.5z"
      fill={dark ? CLOUD_DARK : CLOUD}
    />
  );
}

function Drops({ n = 3, snow = false, dy = 0 }: { n?: number; snow?: boolean; dy?: number }) {
  const xs = n === 2 ? [10, 15] : [8.5, 12.5, 16.5];
  return (
    <g>
      {xs.map((x, i) =>
        snow ? (
          <circle key={i} cx={x - 0.5} cy={21 + dy + (i % 2) * 1.2} r={1.15} fill={SNOW} />
        ) : (
          <line key={i} x1={x} y1={19.8 + dy} x2={x - 1.3} y2={22.6 + dy} stroke={RAIN} strokeWidth={1.7} strokeLinecap="round" />
        ),
      )}
    </g>
  );
}

const ART: Record<Sky, (day: boolean) => ReactNode> = {
  clear: (day) => (day ? <Sun /> : <Moon />),
  partly: (day) => (
    <>
      {day ? <Sun x={9} y={9} r={3.8} /> : <Moon x={9} y={8.5} r={5} />}
      <Cloud dx={1.5} dy={1.5} />
    </>
  ),
  cloudy: () => (
    <>
      <Cloud dx={2.5} dy={-3.5} s={0.85} dark />
      <Cloud dx={-0.5} dy={0} />
    </>
  ),
  fog: () => (
    <g stroke={CLOUD} strokeWidth={1.8} strokeLinecap="round">
      <line x1="4" y1="8" x2="17" y2="8" />
      <line x1="7" y1="12" x2="20" y2="12" />
      <line x1="4" y1="16" x2="15" y2="16" />
      <line x1="9" y1="20" x2="19" y2="20" />
    </g>
  ),
  drizzle: () => (
    <>
      <Cloud dy={-3} />
      <Drops n={2} dy={-3} />
    </>
  ),
  rain: () => (
    <>
      <Cloud dy={-3} />
      <Drops dy={-3} />
    </>
  ),
  snow: () => (
    <>
      <Cloud dy={-3} />
      <Drops snow dy={-3} />
    </>
  ),
  thunder: () => (
    <>
      <Cloud dy={-3.5} dark />
      <path d="M12.6 14.2 9.8 18.6h2.6l-1.3 4.2 4.2-5.6h-2.7l1.6-3z" fill={BOLT} />
    </>
  ),
};

export function WeatherIcon({ code, day = true, size = 22 }: { code: number; day?: boolean; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" className="wx-icon">
      {ART[sky(code)](day)}
    </svg>
  );
}
