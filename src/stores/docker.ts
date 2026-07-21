import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { DockerStatus } from "../lib/types";

/**
 * Docker daemon health (plan 23). Polled by App from the cached `check_docker`
 * command so the sidebar can show a global "Docker unavailable" pill the moment
 * Docker Desktop goes down, and clear it when it comes back — without spawning
 * a `docker info` every few seconds (the backend caches for 30 s).
 */
interface DockerState {
  status: DockerStatus | null;
  refresh: (force?: boolean) => Promise<void>;
}

export const useDocker = create<DockerState>((set) => ({
  status: null,
  refresh: async (force = false) => {
    try {
      set({ status: await ipc.checkDocker(force) });
    } catch {
      set({ status: { available: false, version: null, error: "Docker check failed." } });
    }
  },
}));
