// Chat state lives outside React so a reply keeps streaming while another tab is open.

import { call, on } from "../api";

export interface ChatAttachment {
  name: string;
  kind: "image" | "text";
  label: string;
  dataUrl?: string;
  text?: string;
}

export interface ChatMsg {
  id: string;
  role: "user" | "assistant";
  text: string;
  attachments?: ChatAttachment[];
  error?: string;
  pending?: boolean;
}

interface State {
  loaded: boolean;
  messages: ChatMsg[];
  streaming: string | null;
}

const SYSTEM =
  "Ты — помощник, встроенный в Island (Dynamic Island для Windows). Отвечай на языке пользователя, по-русски — если он пишет по-русски. " +
  "Пиши кратко и по делу; для списков и кода используй markdown.";

let state: State = { loaded: false, messages: [], streaming: null };
const listeners = new Set<() => void>();
let wired = false;

function set(next: Partial<State>) {
  state = { ...state, ...next };
  listeners.forEach((l) => l());
}

const newId = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 6);

function persist() {
  const keep = state.messages.filter((m) => !m.pending).map(({ pending: _p, ...m }) => m);
  call("chat_save", { messages: keep }).catch(() => {});
}

// Streamed pieces are applied at most once per frame: a fast model sends dozens
// of tiny chunks per second and each one would otherwise re-render the chat.
const queued = new Map<string, string>();
let flushT = 0;
function flush() {
  flushT = 0;
  if (!queued.size) return;
  const add = new Map(queued);
  queued.clear();
  // chunks of a reply that was stopped on our side are dropped
  set({ messages: state.messages.map((m) => (m.pending && add.has(m.id) ? { ...m, text: m.text + add.get(m.id) } : m)) });
}

function wire() {
  if (wired) return;
  wired = true;
  on<{ id: string; delta: string }>("ai-chunk", ({ id, delta }) => {
    queued.set(id, (queued.get(id) ?? "") + delta);
    if (!flushT) flushT = window.setTimeout(flush, 40);
  });
  on<{ id: string; error: string | null }>("ai-done", ({ id, error }) => {
    window.clearTimeout(flushT);
    flush();
    const msg = state.messages.find((m) => m.id === id);
    set({
      streaming: state.streaming === id ? null : state.streaming,
      messages: msg?.pending
        ? state.messages.map((m) => (m.id === id ? { ...m, pending: false, error: error ?? undefined, text: m.text || (error ? "" : "…") } : m))
        : state.messages,
    });
    if (msg?.pending) persist();
  });
}

/** Older turns are sent without their files: every request carries the whole
 *  dialogue, and a few screenshots would soon overflow a local model's context. */
const FILES_FOR_LAST = 3;
/** at most this many earlier messages go with a question */
const HISTORY = 30;

export const chat = {
  get: () => state,
  subscribe(fn: () => void) {
    listeners.add(fn);
    return () => listeners.delete(fn);
  },
  async load() {
    wire();
    if (state.loaded) return;
    const saved = await call<ChatMsg[]>("chat_load").catch(() => [] as ChatMsg[]);
    if (!state.loaded) set({ loaded: true, messages: Array.isArray(saved) ? saved : [] });
  },

  /** OpenAI-style messages for the whole dialogue. */
  payload(all: ChatMsg[]) {
    const out: unknown[] = [{ role: "system", content: SYSTEM }];
    const messages = all.filter((m) => !(m.pending || (m.role === "assistant" && (m.error || !m.text)))).slice(-HISTORY);
    // a reply can't open the list
    while (messages.length && messages[0].role === "assistant") messages.shift();
    let userLeft = messages.filter((m) => m.role === "user").length;
    for (const m of messages) {
      if (m.role === "assistant") {
        out.push({ role: "assistant", content: m.text });
        continue;
      }
      const recent = userLeft-- <= FILES_FOR_LAST;
      const files = recent ? (m.attachments ?? []) : [];
      const dropped = recent ? [] : (m.attachments ?? []);
      const note = dropped.length ? `\n\n[Ранее прикреплено: ${dropped.map((a) => `«${a.name}»`).join(", ")}]` : "";
      const docs = files
        .filter((a) => a.kind === "text" && a.text)
        .map((a) => `\n\nФайл «${a.name}»:\n\`\`\`\n${a.text}\n\`\`\``)
        .join("");
      const images = files.filter((a) => a.kind === "image" && a.dataUrl);
      const text = (m.text || (images.length ? "Что на изображении?" : "")) + docs + note;
      if (!images.length) out.push({ role: "user", content: text });
      else
        out.push({
          role: "user",
          content: [{ type: "text", text }, ...images.map((a) => ({ type: "image_url", image_url: { url: a.dataUrl } }))],
        });
    }
    return out;
  },

  send(text: string, attachments: ChatAttachment[] = []) {
    wire();
    const t = text.trim();
    if ((!t && !attachments.length) || state.streaming) return false;
    const user: ChatMsg = { id: newId(), role: "user", text: t, attachments: attachments.length ? attachments : undefined };
    const reply: ChatMsg = { id: newId(), role: "assistant", text: "", pending: true };
    const messages = [...state.messages, user, reply];
    set({ messages, streaming: reply.id });
    call("ai_send", { id: reply.id, messages: chat.payload([...state.messages.slice(0, -1)]) }).catch((e) => {
      set({
        streaming: null,
        messages: state.messages.map((m) => (m.id === reply.id ? { ...m, pending: false, error: String(e) } : m)),
      });
    });
    return true;
  },

  /** Sends the last question again after an error. */
  retry() {
    if (state.streaming) return;
    const msgs = [...state.messages];
    while (msgs.length && msgs[msgs.length - 1].role === "assistant") msgs.pop();
    const last = msgs.pop();
    if (!last) return;
    set({ messages: msgs });
    chat.send(last.text, last.attachments ?? []);
  },

  /** Finishes the reply right away — the server may be stuck loading a model
   *  and never send another byte. */
  stop() {
    const id = state.streaming;
    if (!id) return;
    call("ai_stop", { id }).catch(() => {});
    window.clearTimeout(flushT);
    flush();
    set({
      streaming: null,
      messages: state.messages.map((m) => (m.id === id ? { ...m, pending: false, error: m.text ? undefined : "Остановлено" } : m)),
    });
    persist();
  },

  clear() {
    if (state.streaming) call("ai_stop", { id: state.streaming }).catch(() => {});
    queued.clear();
    set({ messages: [], streaming: null });
    persist();
  },
};
