// Mock of @tauri-apps/plugin-opener for the mock build: opening URLs is a no-op
// in the browser (screenshots must not spawn real browser tabs).

export async function openUrl(_url: string, _openWith?: string): Promise<void> {
  // no-op
}
