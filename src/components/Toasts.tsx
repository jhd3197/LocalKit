import { useToast, type ToastKind } from "../stores/toast";

const KIND_CLASSES: Record<ToastKind, string> = {
  success: "border-emerald-800 bg-emerald-950/90",
  error: "border-red-800 bg-red-950/90",
  info: "border-zinc-700 bg-zinc-900/95",
};

/** Global toast viewport — fixed bottom-right stack, above modals. */
export default function Toasts() {
  const toasts = useToast((s) => s.toasts);
  const dismiss = useToast((s) => s.dismiss);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 flex w-full max-w-sm flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`flex items-start gap-3 rounded-lg border px-4 py-3 shadow-xl ${KIND_CLASSES[t.kind]}`}
        >
          {t.spinner && (
            <span className="mt-0.5 inline-block h-4 w-4 animate-spin rounded-full border-2 border-zinc-500 border-t-violet-400" />
          )}
          <div className="flex-1">
            <p className="text-sm">{t.title}</p>
            {t.message && <p className="mt-0.5 text-xs text-zinc-400">{t.message}</p>}
          </div>
          <button
            onClick={() => dismiss(t.id)}
            className="text-zinc-500 hover:text-zinc-300"
            aria-label="Dismiss"
          >
            ✕
          </button>
        </div>
      ))}
    </div>
  );
}
