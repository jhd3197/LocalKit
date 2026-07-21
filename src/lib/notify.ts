import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { getOsNotificationsEnabled } from "../stores/settings";

/**
 * OS desktop notifications for completed long operations (plan 25).
 *
 * Fired *only* when the window can't already show the outcome — i.e. it's
 * unfocused or closed to tray. While the window is focused the in-app toast
 * owns feedback, and double-notifying is worse than either alone. Respects the
 * `osNotifications` setting (default on) and treats a permission denial as
 * "off" without nagging.
 */

// Permission is resolved at most once per session; a denial sticks.
let permission: "granted" | "denied" | "unknown" = "unknown";

async function ensurePermission(): Promise<boolean> {
  if (permission === "granted") return true;
  if (permission === "denied") return false;
  try {
    if (await isPermissionGranted()) {
      permission = "granted";
      return true;
    }
    // macOS requires a runtime request; a denial is remembered, not re-asked.
    permission = (await requestPermission()) === "granted" ? "granted" : "denied";
    return permission === "granted";
  } catch {
    permission = "denied";
    return false;
  }
}

/** Notify about a finished long op, but only when the window is in the
 *  background (unfocused or hidden). No-op when focused, off, or denied. */
export async function notifyIfBackground(title: string, body?: string): Promise<void> {
  if (!getOsNotificationsEnabled()) return;
  // Focused window → the toast already covers it.
  if (typeof document !== "undefined" && document.hasFocus()) return;
  if (!(await ensurePermission())) return;
  try {
    sendNotification(body ? { title, body } : { title });
  } catch {
    /* notifications unavailable — ignore */
  }
}
