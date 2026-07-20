import { toast } from "../stores/toast";

/** Unwrap the `string` rejection our Tauri commands return. */
export function errMsg(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

// The site-event stream already surfaces failures for create/push/pull as an
// `error`-stage toast; the command promise then rejects with the same
// underlying message. Remember it briefly so catch arms don't show a
// duplicate toast for the same failure.
let lastEventError: { message: string; at: number } | null = null;

/** Called by the site-event handler when an `error` stage is toasted. */
export function markEventError(message: string): void {
  lastEventError = { message, at: Date.now() };
}

/**
 * Toast an action failure with context ("Start site", "Push DB"). Skips the
 * toast when a site-event error just covered the same underlying message.
 */
export function toastError(e: unknown, context: string): void {
  const msg = errMsg(e);
  const recent = lastEventError;
  if (recent && msg && Date.now() - recent.at < 2000 && recent.message.includes(msg)) return;
  toast.error(context, msg);
}
