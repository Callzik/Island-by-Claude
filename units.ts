// Unit and currency conversion for the launcher: "5 км в милях", "100 usd в рублях",
// "30 c to f", "2 гб в мб".

import { formatNumber, plainNumber } from "./calc";

interface Unit {
  cat: string;
  /** multiply to get the category base unit */
  k: number;
  /** temperature offset handling */
  temp?: "c" | "f" | "k";
  /** exact aliases (lower case) */
  alias: string[];
  /** word stems for inflected forms (prefix match) */
  stems?: string[];
  /** display: symbol or [one, few, many] */
  show: string | [string, string, string];
}

const U: Unit[] = [
  // length, base metre
  { cat: "len", k: 0.001, alias: ["мм", "mm"], stems: ["миллиметр", "millimet"], show: "мм" },
  { cat: "len", k: 0.01, alias: ["см", "cm"], stems: ["сантиметр", "centimet"], show: "см" },
  { cat: "len", k: 0.1, alias: ["дм", "dm"], stems: ["дециметр"], show: "дм" },
  { cat: "len", k: 1, alias: ["м", "m"], stems: ["метр", "meter", "metre"], show: "м" },
  { cat: "len", k: 1000, alias: ["км", "km"], stems: ["километр", "kilomet"], show: "км" },
  { cat: "len", k: 0.0254, alias: ["in", "inch", "inches", "\"", "дюйм"], stems: ["дюйм"], show: ["дюйм", "дюйма", "дюймов"] },
  { cat: "len", k: 0.3048, alias: ["ft", "foot", "feet", "'"], stems: ["фут"], show: ["фут", "фута", "футов"] },
  { cat: "len", k: 0.9144, alias: ["yd", "yard", "yards"], stems: ["ярд"], show: ["ярд", "ярда", "ярдов"] },
  { cat: "len", k: 1609.344, alias: ["mi", "mile", "miles", "миля"], stems: ["мил"], show: ["миля", "мили", "миль"] },
  {
    cat: "len",
    k: 1852,
    alias: ["nmi", "морская миля", "морские мили", "морских миль", "морских милях", "морскую милю"],
    show: ["морская миля", "морские мили", "морских миль"],
  },
  // mass, base kg
  { cat: "mass", k: 1e-6, alias: ["мг", "mg"], stems: ["миллиграм", "milligram"], show: "мг" },
  { cat: "mass", k: 0.001, alias: ["г", "гр", "g", "gram", "grams"], stems: ["грамм"], show: "г" },
  { cat: "mass", k: 1, alias: ["кг", "kg", "кило"], stems: ["килограм", "kilogram"], show: "кг" },
  { cat: "mass", k: 100, alias: ["ц"], stems: ["центнер"], show: ["центнер", "центнера", "центнеров"] },
  { cat: "mass", k: 1000, alias: ["т", "t", "ton", "tons", "tonne"], stems: ["тонн"], show: "т" },
  { cat: "mass", k: 0.45359237, alias: ["lb", "lbs", "pound", "pounds", "фунт"], stems: ["фунт"], show: ["фунт", "фунта", "фунтов"] },
  { cat: "mass", k: 0.028349523125, alias: ["oz", "ounce", "ounces"], stems: ["унци"], show: ["унция", "унции", "унций"] },
  // volume, base litre
  { cat: "vol", k: 0.001, alias: ["мл", "ml"], stems: ["миллилитр", "millilit"], show: "мл" },
  { cat: "vol", k: 1, alias: ["л", "l", "liter", "liters", "litre"], stems: ["литр"], show: "л" },
  { cat: "vol", k: 1000, alias: ["м3", "м³", "m3", "кубометр", "куб"], stems: ["кубометр", "кубическ"], show: "м³" },
  { cat: "vol", k: 3.785411784, alias: ["gal", "gallon", "gallons"], stems: ["галлон"], show: ["галлон", "галлона", "галлонов"] },
  { cat: "vol", k: 0.473176473, alias: ["pt", "pint", "pints"], stems: ["пинт"], show: ["пинта", "пинты", "пинт"] },
  { cat: "vol", k: 0.0295735295625, alias: ["fl oz", "floz"], show: "fl oz" },
  { cat: "vol", k: 0.24, alias: ["cup", "cups"], stems: ["стакан", "чашк", "чашек"], show: ["стакан", "стакана", "стаканов"] },
  // area, base m²
  { cat: "area", k: 1, alias: ["м2", "м²", "m2", "кв м", "кв. м", "sq m"], stems: ["квадратных метр", "квадратный метр", "квадратного метр", "квадратных метрах"], show: "м²" },
  { cat: "area", k: 1e6, alias: ["км2", "км²", "km2", "кв км", "кв. км"], stems: ["квадратных километр", "квадратный километр"], show: "км²" },
  { cat: "area", k: 10000, alias: ["га", "ha"], stems: ["гектар", "hectare"], show: "га" },
  { cat: "area", k: 100, alias: ["сотка"], stems: ["сотк", "соток"], show: ["сотка", "сотки", "соток"] },
  { cat: "area", k: 4046.8564224, alias: ["acre", "acres"], stems: ["акр"], show: ["акр", "акра", "акров"] },
  { cat: "area", k: 0.09290304, alias: ["ft2", "sq ft", "кв фут"], show: "фут²" },
  // speed, base m/s
  { cat: "speed", k: 1, alias: ["м/с", "m/s", "mps", "метров в секунду"], show: "м/с" },
  { cat: "speed", k: 1 / 3.6, alias: ["км/ч", "km/h", "kmh", "кмч", "км в час", "километров в час"], show: "км/ч" },
  { cat: "speed", k: 0.44704, alias: ["mph", "миль/ч", "миль в час", "мили в час"], show: "миль/ч" },
  { cat: "speed", k: 0.514444, alias: ["kn", "knot", "knots", "узел", "узлы", "узлов", "узлах"], show: ["узел", "узла", "узлов"] },
  // time, base second
  { cat: "time", k: 0.001, alias: ["мс", "ms"], stems: ["миллисекунд", "millisec"], show: "мс" },
  { cat: "time", k: 1, alias: ["с", "сек", "s", "sec"], stems: ["секунд", "second"], show: ["секунда", "секунды", "секунд"] },
  { cat: "time", k: 60, alias: ["мин", "min"], stems: ["минут", "minute"], show: ["минута", "минуты", "минут"] },
  { cat: "time", k: 3600, alias: ["ч", "h", "hr", "hour", "hours"], stems: ["час"], show: ["час", "часа", "часов"] },
  { cat: "time", k: 86400, alias: ["д", "дн", "день", "дня", "дней", "сутки", "суток", "day", "days"], stems: ["днях", "дням", "сутк"], show: ["день", "дня", "дней"] },
  { cat: "time", k: 604800, alias: ["нед", "week", "weeks"], stems: ["недел"], show: ["неделя", "недели", "недель"] },
  { cat: "time", k: 2629746, alias: ["мес", "month", "months"], stems: ["месяц"], show: ["месяц", "месяца", "месяцев"] },
  { cat: "time", k: 31556952, alias: ["г.", "год", "года", "лет", "year", "years", "годах"], stems: ["годах", "годам"], show: ["год", "года", "лет"] },
  // data, base byte
  { cat: "data", k: 1 / 8, alias: ["бит", "bit", "bits", "бита", "битах"], stems: ["битов"], show: ["бит", "бита", "бит"] },
  { cat: "data", k: 1, alias: ["байт", "byte", "bytes", "б"], stems: ["байт"], show: ["байт", "байта", "байт"] },
  { cat: "data", k: 1024, alias: ["кб", "kb", "кбайт"], stems: ["килобайт", "kilobyte"], show: "КБ" },
  { cat: "data", k: 1024 ** 2, alias: ["мб", "mb", "мбайт"], stems: ["мегабайт", "megabyte"], show: "МБ" },
  { cat: "data", k: 1024 ** 3, alias: ["гб", "gb", "гбайт", "гиг", "гига"], stems: ["гигабайт", "gigabyte"], show: "ГБ" },
  { cat: "data", k: 1024 ** 4, alias: ["тб", "tb", "тбайт"], stems: ["терабайт", "terabyte"], show: "ТБ" },
  { cat: "data", k: 125_000, alias: ["мбит", "mbit", "mbps", "мбит/с"], stems: ["мегабит"], show: "Мбит" },
  { cat: "data", k: 125_000_000, alias: ["гбит", "gbit", "gbps", "гбит/с"], stems: ["гигабит"], show: "Гбит" },
  // temperature
  { cat: "temp", k: 1, temp: "c", alias: ["°c", "℃", "c", "°с", "celsius", "цельсий"], stems: ["цельси", "градус цельси", "градусов цельси", "градусах цельси"], show: "°C" },
  { cat: "temp", k: 1, temp: "f", alias: ["°f", "℉", "f", "fahrenheit", "фаренгейт"], stems: ["фаренгейт", "градус фаренгейт", "градусов фаренгейт", "градусах фаренгейт"], show: "°F" },
  { cat: "temp", k: 1, temp: "k", alias: ["k", "kelvin", "кельвин"], stems: ["кельвин"], show: "K" },
];

interface Currency {
  code: string;
  alias: string[];
  stems: string[];
  sym: string;
}

const CUR: Currency[] = [
  { code: "RUB", alias: ["rub", "руб", "р", "₽", "rur"], stems: ["рубл"], sym: "₽" },
  { code: "USD", alias: ["usd", "$", "бакс", "баксов", "баксах"], stems: ["доллар", "dollar"], sym: "$" },
  { code: "EUR", alias: ["eur", "€", "евро", "euro"], stems: [], sym: "€" },
  { code: "CNY", alias: ["cny", "¥", "rmb"], stems: ["юан", "yuan"], sym: "¥" },
  { code: "KZT", alias: ["kzt", "₸", "тенге"], stems: [], sym: "₸" },
  { code: "BYN", alias: ["byn", "бел руб", "белорусских рублей"], stems: ["белорусск"], sym: "Br" },
  { code: "UAH", alias: ["uah", "₴"], stems: ["гривн", "hryvn"], sym: "₴" },
  { code: "GBP", alias: ["gbp", "£", "фунт стерлингов", "фунтов стерлингов", "фунтах стерлингов"], stems: ["фунт стерлинг"], sym: "£" },
  { code: "JPY", alias: ["jpy", "иена", "иен", "йен", "йена"], stems: ["иен", "йен"], sym: "¥" },
  { code: "TRY", alias: ["try", "лира", "лир", "лиры", "лирах"], stems: [], sym: "₺" },
  { code: "GEL", alias: ["gel", "лари", "₾"], stems: [], sym: "₾" },
  { code: "AMD", alias: ["amd", "драм", "драмов", "драмах"], stems: [], sym: "֏" },
  { code: "CHF", alias: ["chf"], stems: ["франк"], sym: "Fr" },
  { code: "AED", alias: ["aed"], stems: ["дирхам"], sym: "AED" },
  { code: "UZS", alias: ["uzs", "сум", "сумов", "сумах"], stems: [], sym: "сум" },
  { code: "KRW", alias: ["krw", "₩"], stems: ["вон"], sym: "₩" },
  { code: "INR", alias: ["inr", "₹"], stems: ["рупи"], sym: "₹" },
];

const norm = (s: string) =>
  s
    .toLowerCase()
    .replace(/ё/g, "е")
    .replace(/\s+/g, " ")
    .replace(/[.?!]+$/, "")
    .trim();

function findUnit(text: string): Unit | null {
  const t = norm(text);
  if (!t) return null;
  for (const u of U) if (u.alias.includes(t)) return u;
  let best: Unit | null = null;
  let bestLen = 0;
  for (const u of U)
    for (const s of u.stems ?? [])
      if (t.startsWith(s) && s.length > bestLen) {
        best = u;
        bestLen = s.length;
      }
  return best;
}

function findCurrency(text: string): Currency | null {
  const t = norm(text);
  if (!t) return null;
  const up = t.toUpperCase();
  for (const c of CUR) if (c.code === up || c.alias.includes(t)) return c;
  let best: Currency | null = null;
  let bestLen = 0;
  for (const c of CUR)
    for (const s of c.stems)
      if (t.startsWith(s) && s.length > bestLen) {
        best = c;
        bestLen = s.length;
      }
  return best;
}

function plural(n: number, forms: [string, string, string]): string {
  if (!Number.isInteger(n)) return forms[1];
  const a = Math.abs(n) % 100;
  const b = a % 10;
  if (a > 10 && a < 20) return forms[2];
  if (b === 1) return forms[0];
  if (b >= 2 && b <= 4) return forms[1];
  return forms[2];
}

function toBase(v: number, u: Unit): number {
  if (u.temp === "c") return v;
  if (u.temp === "f") return ((v - 32) * 5) / 9;
  if (u.temp === "k") return v - 273.15;
  return v * u.k;
}

function fromBase(v: number, u: Unit): number {
  if (u.temp === "c") return v;
  if (u.temp === "f") return (v * 9) / 5 + 32;
  if (u.temp === "k") return v + 273.15;
  return v / u.k;
}

/** rounds to 7 significant digits to hide float noise */
function tidy(v: number): number {
  if (v === 0 || !Number.isFinite(v)) return v;
  return parseFloat(v.toPrecision(7));
}

export interface Conversion {
  /** "3,106856 миль" */
  text: string;
  /** "3,106856" for the clipboard */
  copy: string;
  kind: "unit" | "currency";
}

const SEP = /\s+(?:в|во|to|in|into|=|->|→|на)\s+/gi;

function splits(q: string): { num: number; from: string; to: string }[] {
  const m = /^\s*(-?\d[\d\s]*(?:[.,]\d+)?|-?[.,]\d+)\s*(.+)$/.exec(q);
  if (!m) return [];
  const num = Number(m[1].replace(/\s/g, "").replace(",", "."));
  if (!Number.isFinite(num)) return [];
  const rest = m[2];
  const out: { num: number; from: string; to: string }[] = [];
  SEP.lastIndex = 0;
  let mm: RegExpExecArray | null;
  while ((mm = SEP.exec(rest))) {
    out.push({ num, from: rest.slice(0, mm.index), to: rest.slice(mm.index + mm[0].length) });
  }
  return out;
}

export function convertUnits(q: string): Conversion | null {
  for (const { num, from, to } of splits(q)) {
    const a = findUnit(from);
    const b = findUnit(to);
    if (!a || !b || a.cat !== b.cat || a === b) continue;
    const v = tidy(fromBase(toBase(num, a), b));
    const label = typeof b.show === "string" ? b.show : plural(v, b.show);
    return { text: `${formatNumber(v)} ${label}`, copy: plainNumber(v), kind: "unit" };
  }
  return null;
}

// --- currency -----------------------------------------------------------------

const RATES_KEY = "island.rates.v1";
let rates: Record<string, number> | null = null;
let ratesAt = 0;
let loading: Promise<void> | null = null;
/** when the last attempt to load rates failed (no rates at all yet) */
let failedAt = 0;

try {
  const raw = localStorage.getItem(RATES_KEY);
  if (raw) {
    const parsed = JSON.parse(raw);
    rates = parsed.rates;
    ratesAt = parsed.at;
  }
} catch {
  /* no storage */
}

async function fetchRates(): Promise<void> {
  const sources: [string, (j: any) => Record<string, number> | null][] = [
    ["https://open.er-api.com/v6/latest/USD", (j) => (j && j.rates ? j.rates : null)],
    [
      "https://www.cbr-xml-daily.ru/latest.js",
      // rates relative to RUB -> convert to "per USD"
      (j) => {
        if (!j || !j.rates || !j.rates.USD) return null;
        const perRub = j.rates as Record<string, number>;
        const usd = perRub.USD;
        const out: Record<string, number> = { RUB: 1 / usd };
        for (const [k, v] of Object.entries(perRub)) out[k] = v / usd;
        out.USD = 1;
        return out;
      },
    ],
  ];
  for (const [url, parse] of sources) {
    try {
      const res = await fetch(url, { cache: "no-store" });
      if (!res.ok) continue;
      const r = parse(await res.json());
      if (r) {
        rates = r;
        ratesAt = Date.now();
        try {
          localStorage.setItem(RATES_KEY, JSON.stringify({ rates, at: ratesAt }));
        } catch {
          /* ignore */
        }
        failedAt = 0;
        return;
      }
    } catch {
      /* try next source */
    }
  }
  failedAt = Date.now();
}

/** Starts loading exchange rates (cached for 6 hours). */
export function ensureRates(): Promise<void> {
  if (rates && Date.now() - ratesAt < 6 * 3600e3) return Promise.resolve();
  // offline: don't hammer the servers on every keystroke
  if (!loading && failedAt && Date.now() - failedAt < 60e3) return Promise.resolve();
  if (!loading) loading = fetchRates().finally(() => (loading = null));
  return loading;
}

export function convertCurrency(q: string): Conversion | "pending" | "failed" | null {
  for (const { num, from, to } of splits(q)) {
    const a = findCurrency(from);
    const b = findCurrency(to);
    if (!a || !b || a === b) continue;
    // old cached rates are still shown, fresh ones load in the background
    ensureRates();
    if (!rates) return failedAt && !loading ? "failed" : "pending";
    const ra = rates[a.code];
    const rb = rates[b.code];
    if (!ra || !rb) return null;
    const v = (num / ra) * rb;
    const rounded = Math.round(v * 100) / 100;
    const text = new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 2, minimumFractionDigits: Math.abs(rounded) < 100 ? 2 : 0 })
      .format(rounded)
      .replace(/ | /g, " ");
    return { text: `${text} ${b.sym}`, copy: plainNumber(rounded), kind: "currency" };
  }
  return null;
}
