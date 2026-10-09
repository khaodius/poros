import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import { application, toAppError, transfers } from "../lib/ipc";
import { useLogStore } from "./logStore";
import { saveSettingsSection, useSettingsStore } from "./settingsStore";
import { useUiStore } from "./uiStore";

export interface InstallProgress {
  received: number;
  /** Null until the server says how large the download is, or when it never does. */
  total: number | null;
  installing: boolean;
}

interface UpdateState {
  /** A newer release found by the last check. */
  update: Update | null;
  checking: boolean;
  /** When the last check finished, in epoch milliseconds. */
  checkedAt: number | null;
  /** Why the last check failed, as a sentence for the user. */
  checkError: string | null;
  progress: InstallProgress | null;
  installError: string | null;
}

export const useUpdateStore = create<UpdateState>(() => ({
  update: null,
  checking: false,
  checkedAt: null,
  checkError: null,
  progress: null,
  installError: null,
}));

const PROGRESS_INTERVAL_MILLIS = 100;
/**
 * What the updater reports when the release server answers without a latest.json, as for a
 * release published before in-app updates existed.
 */
const MISSING_MANIFEST = "Could not fetch a valid release JSON from the remote";

export function describeCheckError(message: string): string {
  return message === MISSING_MANIFEST
    ? "The latest release on GitHub does not offer in-app updates yet"
    : `Could not check for updates: ${message}`;
}

/**
 * Asks the release server for a newer version. A check at start-up only offers a version the
 * user has not skipped, and only when no other dialog is open; the status bar still shows it.
 */
export async function checkForUpdates(trigger: "start" | "manual"): Promise<void> {
  const state = useUpdateStore.getState();
  if (state.checking || state.progress) return;
  useUpdateStore.setState({ checking: true, checkError: null });
  try {
    const found = await check();
    await state.update?.close().catch(() => undefined);
    useUpdateStore.setState({ update: found, checkedAt: Date.now() });
    if (!found) return;
    const ui = useUiStore.getState();
    if (trigger === "manual") {
      ui.open({ kind: "update", returnTo: ui.dialog ?? undefined });
    } else if (!ui.dialog && !isSkipped(found)) {
      ui.open({ kind: "update" });
    }
  } catch (caught) {
    const checkError = describeCheckError(toAppError(caught).message);
    useUpdateStore.setState({ checkError });
    if (trigger === "start") useLogStore.getState().write("warn", checkError);
  } finally {
    useUpdateStore.setState({ checking: false });
  }
}

function isSkipped(update: Update): boolean {
  return useSettingsStore.getState().settings.updates.skippedVersion === update.version;
}

/**
 * Downloads the update, checks its signature against the key built into this version, installs
 * it and starts the new version. On Windows the installer closes Poros and starts it again.
 */
export async function installUpdate(): Promise<void> {
  const { update, progress } = useUpdateStore.getState();
  if (!update || progress) return;
  const current: InstallProgress = { received: 0, total: null, installing: false };
  let shownAt = 0;
  const show = (force: boolean) => {
    const now = performance.now();
    if (!force && now - shownAt < PROGRESS_INTERVAL_MILLIS) return;
    shownAt = now;
    useUpdateStore.setState({ progress: { ...current } });
  };
  const onEvent = (event: DownloadEvent) => {
    switch (event.event) {
      case "Started":
        current.total = event.data.contentLength ?? null;
        show(true);
        break;
      case "Progress":
        current.received += event.data.chunkLength;
        show(false);
        break;
      case "Finished":
        current.installing = true;
        show(true);
        break;
    }
  };

  useUpdateStore.setState({ progress: { ...current }, installError: null });
  try {
    await update.download(onEvent);
    // The Windows installer closes Poros without the normal exit, so the queue is saved first.
    await transfers.saveQueue();
    await update.install();
    await application.restart();
  } catch (caught) {
    useUpdateStore.setState({ progress: null, installError: toAppError(caught).message });
  }
}

export async function skipUpdate(): Promise<void> {
  const { update } = useUpdateStore.getState();
  if (update) await saveSettingsSection("updates", { skippedVersion: update.version });
}
