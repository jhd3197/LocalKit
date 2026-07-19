// Terminal instance registry. xterm instances — and the DOM node each one
// renders into — live here, OUTSIDE React, keyed by site id. React components
// are thin viewports: on mount they `attach(el)` the cached node, on unmount
// they `detach()` it; the instance (and its scrollback, PTY, and listeners)
// survives page switches and HMR. Disposal is explicit (`disposeTerminal`),
// never a side effect of a React unmount.
//
// Simplified from Faro's terminalRegistry (no splits/popouts there→here).

import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc, onTerminalData, onTerminalExit } from "./ipc";
import { attachSuggestions } from "./termSuggest";
import { getTerminalFontSize, getTerminalScrollback, useSettings } from "../stores/settings";

export type TermStatus = "opening" | "ready" | "exited";

export interface TermState {
  status: TermStatus;
  error: string | null;
  exitCode: number | null;
}

export interface TermEntry {
  siteId: string;
  term: XTerm;
  attach(container: HTMLElement): void;
  detach(container?: HTMLElement): void;
  refit(): void;
  subscribe(cb: (s: TermState) => void): () => void;
}

interface InternalEntry extends TermEntry {
  element: HTMLDivElement;
  state: TermState;
  listeners: Set<(s: TermState) => void>;
  terminalId: string | null;
  unlistenData: (() => void) | null;
  unlistenExit: (() => void) | null;
  disposables: Array<{ dispose: () => void }>;
  onWindowResize: () => void;
  disposed: boolean;
}

const terminals = new Map<string, InternalEntry>();

export function hasTerminal(siteId: string): boolean {
  return terminals.has(siteId);
}

export function getTerminal(siteId: string): TermEntry | undefined {
  return terminals.get(siteId);
}

/** Get the terminal for `siteId`, creating it (and opening its PTY) if absent.
 *  Idempotent: a second call with the same id returns the live instance. */
export function acquireTerminal(siteId: string): TermEntry {
  const existing = terminals.get(siteId);
  if (existing) return existing;

  const term = new XTerm({
    fontFamily: "'JetBrains Mono', monospace",
    fontSize: getTerminalFontSize(),
    scrollback: getTerminalScrollback(), // new terminals only; live terms keep theirs
    lineHeight: 1.35,
    cursorBlink: true,
    theme: {
      background: "#08090E",
      foreground: "#D8DBE6",
      cursor: "#B8AFFA",
      cursorAccent: "#08090E",
      selectionBackground: "rgba(108, 92, 231, 0.35)",
      black: "#151822",
      red: "#FF8A8A",
      green: "#3AD99A",
      yellow: "#F0B34A",
      blue: "#7FB0FF",
      magenta: "#B8AFFA",
      cyan: "#8FE3D8",
      white: "#D8DBE6",
      brightBlack: "#4A5168",
      brightRed: "#FF8A8A",
      brightGreen: "#3AD99A",
      brightYellow: "#F0B34A",
      brightBlue: "#7FB0FF",
      brightMagenta: "#B8AFFA",
      brightCyan: "#8FE3D8",
      brightWhite: "#FFFFFF",
    },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  // Ctrl-clickable URLs (wp-cli output, pasted logs) via the OS browser.
  term.loadAddon(
    new WebLinksAddon((event, uri) => {
      event.preventDefault();
      void openUrl(uri).catch(() => {});
    })
  );

  // The xterm renders into this cached node; React only re-parents it.
  const element = document.createElement("div");
  element.className = "h-full w-full px-3 py-2";
  term.open(element);

  const entry: InternalEntry = {
    siteId,
    term,
    element,
    state: { status: "opening", error: null, exitCode: null },
    listeners: new Set(),
    terminalId: null,
    unlistenData: null,
    unlistenExit: null,
    disposables: [],
    onWindowResize: () => {},
    disposed: false,
    attach: (container) => {
      if (element.parentElement !== container) container.appendChild(element);
    },
    detach: (container) => {
      if (container && element.parentElement !== container) return;
      element.parentElement?.removeChild(element);
    },
    refit: () => {
      try {
        fit.fit();
      } catch {
        /* hidden or zero-size host */
      }
    },
    subscribe: (cb) => {
      entry.listeners.add(cb);
      cb(entry.state);
      return () => entry.listeners.delete(cb);
    },
  };

  const setState = (patch: Partial<TermState>) => {
    entry.state = { ...entry.state, ...patch };
    for (const cb of entry.listeners) cb(entry.state);
  };

  entry.disposables.push(
    term.onData((data) => {
      if (entry.terminalId) ipc.terminalWrite(entry.terminalId, data).catch(() => {});
    })
  );
  entry.disposables.push(
    term.onResize(({ cols, rows }) => {
      if (entry.terminalId) ipc.terminalResize(entry.terminalId, cols, rows).catch(() => {});
    })
  );
  // Copy-on-select (PuTTY-style): a non-empty selection goes straight to the
  // clipboard; an empty selection never clobbers it.
  entry.disposables.push(
    term.onSelectionChange(() => {
      const sel = term.getSelection();
      if (sel) void navigator.clipboard.writeText(sel).catch(() => {});
    })
  );

  // Ghost-text history suggestions, MRU keyed per site; →/End accepts.
  entry.disposables.push(
    attachSuggestions(term, {
      historyKey: siteId,
      send: (data) => {
        if (entry.terminalId) void ipc.terminalWrite(entry.terminalId, data).catch(() => {});
      },
    })
  );

  entry.onWindowResize = () => entry.refit();
  window.addEventListener("resize", entry.onWindowResize);

  // Open the PTY and wire its lifecycle. Runs once per terminal.
  (async () => {
    try {
      entry.unlistenData = await onTerminalData((e) => {
        if (e.terminalId === entry.terminalId) term.write(e.data);
      });
      entry.unlistenExit = await onTerminalExit((e) => {
        if (e.terminalId === entry.terminalId) {
          setState({ status: "exited", exitCode: e.code ?? null });
          term.writeln(
            `\r\n\x1b[33m[session ended${e.code !== null ? ` (exit ${e.code})` : ""}]\x1b[0m`
          );
        }
      });
      const id = await ipc.terminalOpen(siteId, term.cols || 80, term.rows || 24);
      entry.terminalId = id;
      if (entry.disposed) {
        ipc.terminalClose(id).catch(() => {});
        return;
      }
      setState({ status: "ready" });
      term.focus();
    } catch (e) {
      setState({ status: "exited", error: String(e) });
    }
  })();

  terminals.set(siteId, entry);
  return entry;
}

/** Fully tear down a site's terminal (closes the PTY). */
export function disposeTerminal(siteId: string): void {
  const entry = terminals.get(siteId);
  if (!entry) return;
  terminals.delete(siteId);
  entry.disposed = true;
  window.removeEventListener("resize", entry.onWindowResize);
  for (const d of entry.disposables) d.dispose();
  entry.unlistenData?.();
  entry.unlistenExit?.();
  if (entry.terminalId) ipc.terminalClose(entry.terminalId).catch(() => {});
  entry.detach();
  entry.term.dispose();
}

/** Restart a site's terminal (e.g. after the session ended). */
export function restartTerminal(siteId: string): TermEntry {
  disposeTerminal(siteId);
  return acquireTerminal(siteId);
}

// Live-apply the terminal font-size pref to every open terminal (scrollback
// intentionally applies to newly created terminals only — xterm can't grow
// an existing buffer's scrollback without dropping it).
useSettings.subscribe((state, prev) => {
  if (state.values.terminalFontSize === prev.values.terminalFontSize) return;
  const px = getTerminalFontSize();
  for (const entry of terminals.values()) {
    entry.term.options.fontSize = px;
    entry.refit();
  }
});

// HMR: dispose every live terminal so a hot reload doesn't leak xterm
// instances or orphan PTYs. Pages re-acquire on the next render.
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    for (const id of [...terminals.keys()]) disposeTerminal(id);
  });
}
