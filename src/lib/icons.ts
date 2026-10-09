// Shell icons for apps/files, fetched lazily in batches and cached.

import { useEffect, useState } from "react";
import { call } from "../api";

type Entry = string | null;
const cache = new Map<string, Entry>();
const pending = new Set<string>();
const listeners = new Set<() => void>();
let timer: number | null = null;
let queue: { key: string; thumb: boolean }[] = [];

const ck = (key: string, thumb: boolean) => (thumb ? "t|" : "i|") + key;

function flush() {
  timer = null;
  const batch = queue;
  queue = [];
  for (const thumb of [false, true]) {
    const keys = batch.filter((b) => b.thumb === thumb).map((b) => b.key);
    if (!keys.length) continue;
    call<Record<string, Entry>>("get_icons", { keys, size: thumb ? 96 : 64, thumb })
      .then((res) => {
        for (const k of keys) {
          cache.set(ck(k, thumb), res[k] ?? null);
          pending.delete(ck(k, thumb));
        }
        listeners.forEach((l) => l());
      })
      .catch(() => keys.forEach((k) => pending.delete(ck(k, thumb))));
  }
}

function request(key: string, thumb: boolean) {
  const c = ck(key, thumb);
  if (cache.has(c) || pending.has(c)) return;
  pending.add(c);
  queue.push({ key, thumb });
  if (timer === null) timer = window.setTimeout(flush, 16);
}

export function iconOf(key: string, thumb = false): Entry | undefined {
  return cache.get(ck(key, thumb));
}

/** Returns the icon data URL (undefined while loading, null if none). */
export function useIcon(key: string | null | undefined, thumb = false): Entry | undefined {
  const [, force] = useState(0);
  useEffect(() => {
    if (!key) return;
    const l = () => force((n) => n + 1);
    listeners.add(l);
    request(key, thumb);
    return () => {
      listeners.delete(l);
    };
  }, [key, thumb]);
  return key ? cache.get(ck(key, thumb)) : undefined;
}

export function prefetchIcons(keys: string[]) {
  keys.forEach((k) => request(k, false));
}
