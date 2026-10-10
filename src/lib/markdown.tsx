// Tiny markdown for chat replies: paragraphs, headings, lists, quotes,
// fenced code, **bold**, *italic*, `code`, [links](url). No HTML is injected.

import type { MouseEvent, ReactNode } from "react";
import { call, isTauri } from "../api";

/** Links open in the default browser, not inside the island's webview. */
function openLink(e: MouseEvent<HTMLAnchorElement>) {
  if (!isTauri) return;
  e.preventDefault();
  call("open_target", { target: e.currentTarget.href, count: false }).catch(() => {});
}

function inline(text: string, key = 0): ReactNode[] {
  const out: ReactNode[] = [];
  // order matters: code first (its content is literal), then links, bold, italic
  const re = /(`[^`\n]+`)|(\[[^\]\n]+\]\((https?:\/\/[^)\s]+)\))|(\*\*[^*\n]+\*\*|__[^_\n]+__)|(\*[^*\n]+\*|_[^_\n]+_)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let i = 0;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const k = `${key}-${i++}`;
    if (m[1]) out.push(<code key={k}>{m[1].slice(1, -1)}</code>);
    else if (m[2]) {
      const label = m[2].slice(1, m[2].indexOf("]"));
      out.push(
        <a key={k} href={m[3]} rel="noreferrer" onClick={openLink}>
          {label}
        </a>,
      );
    } else if (m[4]) out.push(<strong key={k}>{inline(m[4].slice(2, -2), i)}</strong>);
    else if (m[5]) out.push(<em key={k}>{inline(m[5].slice(1, -1), i)}</em>);
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

type Block =
  | { t: "p"; text: string }
  | { t: "h"; level: number; text: string }
  | { t: "ul" | "ol"; items: string[] }
  | { t: "quote"; text: string }
  | { t: "code"; lang: string; text: string };

function blocks(src: string): Block[] {
  const lines = src.replace(/\r\n/g, "\n").split("\n");
  const out: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const fence = line.match(/^\s*```(\S*)/);
    if (fence) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i])) body.push(lines[i++]);
      i++; // closing fence (may be missing while streaming)
      out.push({ t: "code", lang: fence[1], text: body.join("\n") });
      continue;
    }
    if (!line.trim()) {
      i++;
      continue;
    }
    const h = line.match(/^(#{1,4})\s+(.*)$/);
    if (h) {
      out.push({ t: "h", level: h[1].length, text: h[2] });
      i++;
      continue;
    }
    if (/^\s*[-*•]\s+/.test(line) || /^\s*\d+[.)]\s+/.test(line)) {
      const ordered = /^\s*\d+[.)]\s+/.test(line);
      const items: string[] = [];
      while (i < lines.length && (ordered ? /^\s*\d+[.)]\s+/ : /^\s*[-*•]\s+/).test(lines[i])) {
        let item = lines[i].replace(ordered ? /^\s*\d+[.)]\s+/ : /^\s*[-*•]\s+/, "");
        i++;
        // continuation lines (indented, not a new item)
        while (i < lines.length && /^\s{2,}\S/.test(lines[i]) && !/^\s*([-*•]|\d+[.)])\s+/.test(lines[i])) item += " " + lines[i++].trim();
        items.push(item);
      }
      out.push({ t: ordered ? "ol" : "ul", items });
      continue;
    }
    if (/^\s*>/.test(line)) {
      const q: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) q.push(lines[i++].replace(/^\s*>\s?/, ""));
      out.push({ t: "quote", text: q.join(" ") });
      continue;
    }
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() &&
      !/^\s*```/.test(lines[i]) &&
      !/^#{1,4}\s/.test(lines[i]) &&
      !/^\s*([-*•]|\d+[.)])\s+/.test(lines[i]) &&
      !/^\s*>/.test(lines[i])
    )
      para.push(lines[i++]);
    out.push({ t: "p", text: para.join("\n") });
  }
  return out;
}

function withBreaks(text: string, key: number): ReactNode[] {
  return text.split("\n").flatMap((l, j) => (j ? [<br key={`br${key}-${j}`} />, ...inline(l, key * 100 + j)] : inline(l, key * 100 + j)));
}

export function Markdown({ text, onCopy }: { text: string; onCopy?: (code: string) => void }) {
  return (
    <>
      {blocks(text).map((b, i) => {
        switch (b.t) {
          case "h":
            return (
              <div key={i} className={`md-h md-h${b.level}`}>
                {inline(b.text, i)}
              </div>
            );
          case "ul":
          case "ol": {
            const List = b.t;
            return (
              <List key={i}>
                {b.items.map((it, j) => (
                  <li key={j}>{inline(it, i * 100 + j)}</li>
                ))}
              </List>
            );
          }
          case "quote":
            return <blockquote key={i}>{inline(b.text, i)}</blockquote>;
          case "code":
            return (
              <div key={i} className="md-code">
                <div className="md-code-head">
                  <span>{b.lang || "код"}</span>
                  {onCopy && (
                    <button onClick={() => onCopy(b.text)} title="Скопировать">
                      Копировать
                    </button>
                  )}
                </div>
                <pre>{b.text}</pre>
              </div>
            );
          default:
            return <p key={i}>{withBreaks(b.text, i)}</p>;
        }
      })}
    </>
  );
}

