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

function wire() {
  if (wired) return;
  wired = true;
  on<{ id: string; delta: string }>("ai-chunk", ({ id, delta }) => {
    set({ messages: state.messages.map((m) => (m.id === id ? { ...m, text: m.text + delta } : m)) });
  });
  on<{ id: string; error: string | null }>("ai-done", ({ id, error }) => {
    set({
      streaming: state.streaming === id ? null : state.streaming,
      messages: state.messages.map((m) =>
        m.id === id ? { ...m, pending: false, error: error ?? undefined, text: m.text || (error ? "" : "…") } : m,
      ),
    });
    persist();
  });
}

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
  payload(messages: ChatMsg[]) {
    const out: unknown[] = [{ role: "system", content: SYSTEM }];
    for (const m of messages) {
      if (m.pending || (m.role === "assistant" && (m.error || !m.text))) continue;
      if (m.role === "assistant") {
        out.push({ role: "assistant", content: m.text });
        continue;
      }
      const files = m.attachments ?? [];
      const docs = files
        .filter((a) => a.kind === "text" && a.text)
        .map((a) => `\n\nФайл «${a.name}»:\n\`\`\`\n${a.text}\n\`\`\``)
        .join("");
      const images = files.filter((a) => a.kind === "image" && a.dataUrl);
      const text = (m.text || (images.length ? "Что на изображении?" : "")) + docs;
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
    const msgs = [...state.messages];
    while (msgs.length && msgs[msgs.length - 1].role === "assistant") msgs.pop();
    const last = msgs.pop();
    if (!last) return;
    set({ messages: msgs });
    chat.send(last.text, last.attachments ?? []);
  },

  stop() {
    call("ai_stop").catch(() => {});
  },

  clear() {
    if (state.streaming) call("ai_stop").catch(() => {});
    set({ messages: [], streaming: null });
    persist();
  },
};
