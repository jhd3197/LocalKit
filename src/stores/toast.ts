import { create } from "zustand";

export type ToastKind = "success" | "error" | "info";

export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  message?: string;
  /** Pinned toasts (in-flight progress) don't auto-expire. */
  pinned?: boolean;
  spinner?: boolean;
  /**
   * Inline action, e.g. Cancel on a running transfer (plan 19). Lives on the
   * toast because the progress toast is the only thing on screen that knows a
   * long operation is happening — the page behind it may have moved on.
   */
  action?: { label: string; onClick: () => void };
}

interface ToastState {
  toasts: Toast[];
  add: (t: Omit<Toast, "id">) => number;
  update: (id: number, patch: Partial<Omit<Toast, "id">>) => void;
  dismiss: (id: number) => void;
}

const MAX_VISIBLE = 4;
const EXPIRE_MS: Record<ToastKind, number> = { success: 4000, info: 4000, error: 7000 };

let nextId = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

function clearTimer(id: number) {
  const t = timers.get(id);
  if (t) {
    clearTimeout(t);
    timers.delete(id);
  }
}

export const useToast = create<ToastState>((set, get) => {
  const schedule = (id: number, kind: ToastKind) => {
    clearTimer(id);
    timers.set(
      id,
      setTimeout(() => get().dismiss(id), EXPIRE_MS[kind]),
    );
  };

  return {
    toasts: [],

    add: (t) => {
      const id = nextId++;
      set((s) => {
        let toasts = [...s.toasts, { ...t, id }];
        // Cap the stack: drop the oldest non-pinned toast first.
        while (toasts.length > MAX_VISIBLE) {
          const i = toasts.findIndex((x) => !x.pinned);
          const drop = i >= 0 ? i : 0;
          clearTimer(toasts[drop].id);
          toasts = toasts.filter((_, j) => j !== drop);
        }
        return { toasts };
      });
      if (!t.pinned) schedule(id, t.kind);
      return id;
    },

    update: (id, patch) => {
      set((s) => ({
        toasts: s.toasts.map((t) => (t.id === id ? { ...t, ...patch } : t)),
      }));
      const t = get().toasts.find((x) => x.id === id);
      if (!t) return;
      if (t.pinned) clearTimer(id);
      else schedule(id, t.kind);
    },

    dismiss: (id) => {
      clearTimer(id);
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    },
  };
});

/** Module-level helpers, callable from stores and command code alike. */
export const toast = {
  success: (title: string, message?: string) =>
    useToast.getState().add({ kind: "success", title, message }),
  error: (title: string, message?: string) =>
    useToast.getState().add({ kind: "error", title, message }),
  info: (title: string, message?: string) =>
    useToast.getState().add({ kind: "info", title, message }),
  /** Pinned in-flight toast with a spinner; finish it with `toast.resolve`. */
  progress: (title: string, action?: Toast["action"]) =>
    useToast.getState().add({ kind: "info", title, pinned: true, spinner: true, action }),
  /** Turn a pinned progress toast into a final success/error toast. */
  resolve: (id: number, kind: ToastKind, title: string, message?: string) =>
    useToast
      .getState()
      // The action goes with the spinner: there is nothing left to cancel.
      .update(id, { kind, title, message, pinned: false, spinner: false, action: undefined }),
};
