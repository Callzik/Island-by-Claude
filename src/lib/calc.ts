// Small safe calculator: + - * / ^ %, parentheses, unary minus, constants and
// common functions. Accepts "," as the decimal separator and "×", "÷", "−".

type Tok = { t: "num"; v: number } | { t: "op"; v: string } | { t: "id"; v: string } | { t: "(" } | { t: ")" } | { t: "," };

const FUNCS: Record<string, (...a: number[]) => number> = {
  sqrt: Math.sqrt,
  корень: Math.sqrt,
  cbrt: Math.cbrt,
  abs: Math.abs,
  sin: (x) => Math.sin(x),
  cos: (x) => Math.cos(x),
  tan: (x) => Math.tan(x),
  tg: (x) => Math.tan(x),
  asin: Math.asin,
  acos: Math.acos,
  atan: Math.atan,
  ln: Math.log,
  log: (x, b) => (b === undefined ? Math.log10(x) : Math.log(x) / Math.log(b)),
  lg: Math.log10,
  log2: Math.log2,
  exp: Math.exp,
  round: (x, d = 0) => Math.round(x * 10 ** d) / 10 ** d,
  floor: Math.floor,
  ceil: Math.ceil,
  min: Math.min,
  max: Math.max,
  pow: Math.pow,
};

const CONSTS: Record<string, number> = { pi: Math.PI, "π": Math.PI, e: Math.E, пи: Math.PI };

function tokenize(src: string): Tok[] | null {
  const s = src
    .replace(/(?<=[\d)\s])[xх×](?=\s*[\d(])/g, "*")
    .replace(/×/g, "*")
    .replace(/÷/g, "/")
    .replace(/[−–]/g, "-")
    .replace(/\*\*/g, "^");
  const out: Tok[] = [];
  let i = 0;
  while (i < s.length) {
    const ch = s[i];
    if (/\s/.test(ch)) {
      i++;
      continue;
    }
    if (/[0-9.,]/.test(ch)) {
      let j = i;
      while (j < s.length && /[0-9.,\s]/.test(s[j])) {
        // a space only continues a number when digits follow ("1 000 000")
        if (/\s/.test(s[j]) && !/^\s\d{3}(?!\d)/.test(s.slice(j))) break;
        j++;
      }
      let raw = s.slice(i, j).replace(/\s/g, "");
      // "1,5" -> 1.5 ; "1,000.5" -> 1000.5
      if (raw.includes(",") && raw.includes(".")) raw = raw.replace(/,/g, "");
      else raw = raw.replace(",", ".");
      const v = Number(raw);
      if (!Number.isFinite(v)) return null;
      out.push({ t: "num", v });
      i = j;
      continue;
    }
    if ("+-*/^%".includes(ch)) {
      out.push({ t: "op", v: ch });
      i++;
      continue;
    }
    if (ch === "(") {
      out.push({ t: "(" });
      i++;
      continue;
    }
    if (ch === ")") {
      out.push({ t: ")" });
      i++;
      continue;
    }
    if (ch === ";") {
      out.push({ t: "," });
      i++;
      continue;
    }
    const m = /^[a-zа-яёπ][a-zа-яё0-9]*/i.exec(s.slice(i));
    if (m) {
      out.push({ t: "id", v: m[0].toLowerCase() });
      i += m[0].length;
      continue;
    }
    return null;
  }
  return out;
}

class Parser {
  i = 0;
  constructor(private toks: Tok[]) {}
  peek() {
    return this.toks[this.i];
  }
  next() {
    return this.toks[this.i++];
  }
  done() {
    return this.i >= this.toks.length;
  }

  expr(): number {
    let v = this.term();
    if (this.lastPercent) {
      v /= 100;
      this.lastPercent = false;
    }
    for (;;) {
      const p = this.peek();
      if (p && p.t === "op" && (p.v === "+" || p.v === "-")) {
        this.next();
        const rhs = this.term();
        // "200 + 15%" -> 230
        const pct = this.lastPercent;
        this.lastPercent = false;
        v = p.v === "+" ? v + (pct ? (v * rhs) / 100 : rhs) : v - (pct ? (v * rhs) / 100 : rhs);
      } else return v;
    }
  }

  lastPercent = false;

  term(): number {
    let v = this.power();
    for (;;) {
      const p = this.peek();
      if (p && p.t === "op" && (p.v === "*" || p.v === "/")) {
        this.next();
        const rhs = this.power();
        if (this.lastPercent) {
          this.lastPercent = false;
          v = p.v === "*" ? (v * rhs) / 100 : v / (rhs / 100);
        } else v = p.v === "*" ? v * rhs : v / rhs;
      } else if (p && (p.t === "(" || p.t === "id")) {
        // implicit multiplication: 2(3+4), 2pi
        v *= this.power();
      } else return v;
    }
  }

  power(): number {
    const base = this.unary();
    const p = this.peek();
    if (p && p.t === "op" && p.v === "^") {
      this.next();
      return Math.pow(base, this.power());
    }
    return base;
  }

  unary(): number {
    const p = this.peek();
    if (p && p.t === "op" && (p.v === "-" || p.v === "+")) {
      this.next();
      const v = this.unary();
      return p.v === "-" ? -v : v;
    }
    return this.postfix();
  }

  postfix(): number {
    const v = this.atom();
    const p = this.peek();
    if (p && p.t === "op" && p.v === "%") {
      this.next();
      this.lastPercent = true;
    }
    return v;
  }

  atom(): number {
    const tk = this.next();
    if (!tk) throw new Error("eof");
    if (tk.t === "num") return tk.v;
    if (tk.t === "(") {
      const v = this.expr();
      if (this.next()?.t !== ")") throw new Error(")");
      return v;
    }
    if (tk.t === "id") {
      if (tk.v in CONSTS) return CONSTS[tk.v];
      const fn = FUNCS[tk.v];
      if (!fn) throw new Error("id");
      const n = this.peek();
      const args: number[] = [];
      if (n && n.t === "(") {
        this.next();
        if (this.peek()?.t !== ")") {
          args.push(this.expr());
          while (this.peek()?.t === ",") {
            this.next();
            args.push(this.expr());
          }
        }
        if (this.next()?.t !== ")") throw new Error(")");
      } else args.push(this.power());
      return fn(...args);
    }
    throw new Error("tok");
  }
}

/** Returns the result when `q` looks like a calculation, otherwise null. */
export function calculate(q: string): number | null {
  const s = q.trim().replace(/=$/, "").trim();
  if (!s || !/\d|pi|π|пи/i.test(s)) return null;
  // must contain an operator or a function call, a bare number is not a calculation
  if (!/[+\-*/^%×÷−()]|\d\s*[xх]\s*\d|\b(sqrt|корень|cbrt|abs|sin|cos|tan|tg|ln|log|lg|exp|round|min|max|pow)\b/i.test(s)) return null;
  const toks = tokenize(s);
  if (!toks || toks.length < 2) return null;
  try {
    const p = new Parser(toks);
    const v = p.expr();
    if (!p.done() || !Number.isFinite(v)) return null;
    return v;
  } catch {
    return null;
  }
}

export function formatNumber(v: number): string {
  if (!Number.isFinite(v)) return String(v);
  const abs = Math.abs(v);
  if (abs !== 0 && (abs >= 1e15 || abs < 1e-6)) return v.toExponential(6).replace(".", ",");
  const opts: Intl.NumberFormatOptions = abs >= 1000 ? { maximumFractionDigits: 4 } : { maximumSignificantDigits: 10 };
  return new Intl.NumberFormat("ru-RU", opts).format(v).replace(/ | /g, " ");
}

/** Plain (no grouping) number for copying. */
export function plainNumber(v: number): string {
  const s = Number.isInteger(v) ? String(v) : String(parseFloat(v.toPrecision(12)));
  return s.replace(".", ",");
}
