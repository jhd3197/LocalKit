import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc } from "./ipc";
import { useToast } from "../stores/toast";
import {
  UPDATE_CHECK_INTERVAL_MS,
  getSnoozedUpdate,
  getUpdateLastChecked,
  markUpdateChecked,
  snoozeUpdate,
} from "../stores/settings";
import type { UpdateInfo } from "./types";

/**
 * In-app update awareness (plan 25). The backend does the GitHub fetch +
 * version compare; this module owns the *when* — a daily launch throttle, a
 * once-per-version toast, and the cached result Settings → General reads so it
 * doesn't re-hit GitHub every time it opens.
 */

/** Last check result, so the Settings row renders without another round-trip. */
let lastResult: UpdateInfo | null = null;
export function getLastUpdateResult(): UpdateInfo | null {
  return lastResult;
}

/** Run a check now (Settings "Check now"); records the time and caches it. */
export async function checkForUpdate(): Promise<UpdateInfo> {
  const info = await ipc.checkForUpdate();
  lastResult = info;
  markUpdateChecked(Date.now());
  return info;
}

/**
 * On launch: if we haven't checked in the last day, ask GitHub. When an update
 * is available and this version hasn't already been announced, show a pinned,
 * dismissible toast linking to the release page — and snooze it immediately so
 * the nudge appears at most once per version (the Settings row stays as the
 * durable surface). Any failure is swallowed: a background check must never
 * surface an error toast.
 */
export async function checkForUpdateOnLaunch(): Promise<void> {
  if (Date.now() - getUpdateLastChecked() < UPDATE_CHECK_INTERVAL_MS) return;

  let info: UpdateInfo;
  try {
    info = await checkForUpdate();
  } catch {
    return;
  }
  if (!info.update_available || getSnoozedUpdate() === info.latest) return;

  // Announce once per version.
  snoozeUpdate(info.latest);
  useToast.getState().add({
    kind: "info",
    title: `LocalKit ${info.latest} is available`,
    message: `You're on ${info.current}.`,
    pinned: true,
    action: {
      label: "View release",
      onClick: () => void openUrl(info.url).catch(() => {}),
    },
  });
}
