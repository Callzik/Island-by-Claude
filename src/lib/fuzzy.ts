// Ranking for the launcher. Also understands queries typed in the wrong
// keyboard layout ("зщцукырудд" -> "powershell").

const EN = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./";
const RU = "ёйцукенгшщзхъфывапролджэячсмитьбю.";

const toRu = new Map<string, string>();
const toEn = new Map<string, string>();
for (let i = 0; i < EN.length; i++) {
  toRu.set(EN[i], RU[i]);
  toEn.set(RU[i], EN[i]);
}

export function swapLayout(s: string): string {
  let out = "";
  for (const ch of s.toLowerCase()) out += toEn.get(ch) ?? toRu.get(ch) ?? ch;
  return out;
}

function scoreOne(name: string, q: string): number {
  if (!q) return 0;
  const n = name.toLowerCase().replace(/ё/g, "е");
  if (n === q) return 1000;
  if (n.startsWith(q)) return 900 - Math.min(n.length, 100);
  const words = n.split(/[\s\-_.()]+/).filter(Boolean);
  if (words.some((w) => w.startsWith(q))) return 700 - Math.min(n.length, 100);
  const initials = words.map((w) => w[0]).join("");
  if (initials.startsWith(q) && q.length >= 2) return 600;
  const idx = n.indexOf(q);
  if (idx >= 0) return 450 - idx;
  // subsequence
  let j = 0;
  let gaps = 0;
  let lastHit = -1;
  for (let i = 0; i < n.length && j < q.length; i++) {
    if (n[i] === q[j]) {
      if (lastHit >= 0 && i - lastHit > 1) gaps++;
      lastHit = i;
      j++;
    }
  }
  if (j === q.length && q.length >= 2) return 200 - gaps * 15 - Math.min(n.length, 60);
  return -1;
}

/** Higher is better, -1 = no match. Tries the query as typed and in the other layout. */
export function score(name: string, query: string): number {
  const q = query.toLowerCase().replace(/ё/g, "е").trim();
  const a = scoreOne(name, q);
  const b = scoreOne(name, swapLayout(q)) - 30;
  return Math.max(a, b < -1 ? -1 : b);
}
