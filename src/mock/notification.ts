// Mock of @tauri-apps/plugin-notification for the mock build: there are no OS
// notifications in the browser, so permission is never granted and sends are
// no-ops. (In mock mode the window is focused anyway, so notify.ts short-
// circuits before it ever reaches these.)

export async function isPermissionGranted(): Promise<boolean> {
  return false;
}

export async function requestPermission(): Promise<"granted" | "denied" | "default"> {
  return "denied";
}

export function sendNotification(_options: unknown): void {
  // no-op
}
