import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import { pluralize } from "../lib/format";
import { toAppError, updates } from "../lib/ipc";
import type { UpdateProgress } from "../lib/types";
import { useUnsavedStore } from "./editorRegistry";
import { useLogStore } from "./logStore";
import { saveSettingsSection, useSettingsStore } from "./settingsStore";
import { useTransferStore } from "./transferStore";
import { useUiStore } from "./uiStore";

/** Where installing the update stands, once the user chose to install it. */
export type InstallStage =
  | UpdateProgress
  /** Installed, or on Windows downloaded and verified, until Poros starts again. */
  | { stage: "ready" }
  | { stage: "restarting" }
  | { stage: "failed"; message: string };

interface UpdateState {
  /** A newer release found by the last check. */
  update: Update | null;
  /** The release being installed: the update, or another one picked in Settings. */
  target: Update | null;
  checking: boolean;
  /** When the last check finished, in epoch milliseconds. */
  checkedAt: number | null;
  /** Why the last check failed, as a sentence for the user. */
  checkError: string | null;
  install: InstallStage | null;
  /** Whether the notice in the corner shows how installing goes. */
  noticeOpen: boolean;
}

export const useUpdateStore = create<UpdateState>(() => ({
  update: null,
  target: null,
  checking: false,
  checkedAt: null,
  checkError: null,
  install: null,
  noticeOpen: false,
}));

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

/** How much of the download is done, from 0 to 100, or null while its size is unknown. */
export function percentDownloaded(received: number, total: number | null): number | null {
  if (!total) return null;
  return Math.min(100, Math.floor((received / total) * 100));
}

/** Whether an install is under way or done, so there is nothing to check for or start. */
export function isInstalling(install: InstallStage | null): boolean {
  return install !== null && install.stage !== "failed";
}

/**
 * Asks the release server for a newer version. A check at start-up only offers a version the
 * user has not skipped, and only when no other dialog is open; the status bar still shows it.
 */
export async function checkForUpdates(trigger: "start" | "manual"): Promise<void> {
  const state = useUpdateStore.getState();
  if (state.checking || isInstalling(state.install)) return;
  useUpdateStore.setState({ checking: true, checkError: null });
  try {
    const found = await check();
    const { update, target } = useUpdateStore.getState();
    for (const release of new Set([update, target])) await release?.close().catch(() => undefined);
    useUpdateStore.setState({ update: found, target: null, checkedAt: Date.now(), install: null });
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
 * Downloads the release, checks its signature against the key built into this version and
 * installs it, while the notice in the corner shows how far it has got. Poros keeps running the
 * version it started with until the user restarts it from there.
 */
export async function startUpdate(release: Update): Promise<void> {
  const { update, target, install } = useUpdateStore.getState();
  if (isInstalling(install)) return;
  if (target && target !== release && target !== update) {
    await target.close().catch(() => undefined);
  }
  const show = (progress: UpdateProgress) => {
    const current = useUpdateStore.getState().install?.stage;
    // The last progress can arrive after the download has already finished or failed.
    if (current === "downloading" || current === "installing") {
      useUpdateStore.setState({ install: progress });
    }
  };
  useUpdateStore.setState({
    target: release,
    install: { stage: "downloading", received: 0, total: null },
    noticeOpen: true,
  });
  try {
    await updates.download(release.rid, show);
    useUpdateStore.setState({ install: { stage: "ready" }, noticeOpen: true });
  } catch (caught) {
    useUpdateStore.setState({ install: failure(caught), noticeOpen: true });
  }
}

/** The earliest release Settings offers to go back to, as `OLDEST_INSTALLABLE` in update.rs. */
export const OLDEST_INSTALLABLE = "0.4.5";

/** Finds a release picked in Settings, older or newer, and installs it the way an update does. */
export async function installVersion(version: string): Promise<void> {
  void startUpdate(await updates.findRelease(version));
}

/** What restarting now would interrupt, as sentences for the user. */
export function useRestartWarnings(): string[] {
  const pendingTransfers = useTransferStore(
    ({ snapshot }) => snapshot.stats.counts.queued + snapshot.stats.counts.running,
  );
  const keepQueue = useSettingsStore((state) => state.settings.transfers.keepQueue);
  const unsavedFiles = useUnsavedStore((state) => Object.keys(state.tabs).length);
  const warnings: string[] = [];
  if (pendingTransfers > 0) {
    warnings.push(
      `${pluralize(pendingTransfers, "transfer")} in the queue will stop when Poros restarts` +
        (keepQueue ? " and come back paused." : "."),
    );
  }
  if (unsavedFiles > 0) {
    warnings.push(
      `Unsaved changes in ${pluralize(unsavedFiles, "file")} will be lost when Poros restarts.`,
    );
  }
  return warnings;
}

export async function restartToUpdate(): Promise<void> {
  if (useUpdateStore.getState().install?.stage !== "ready") return;
  useUpdateStore.setState({ install: { stage: "restarting" } });
  try {
    await updates.restart();
  } catch (caught) {
    useUpdateStore.setState({ install: failure(caught), noticeOpen: true });
  }
}

function failure(caught: unknown): InstallStage {
  const { message } = toAppError(caught);
  // The updater's own messages start in lower case.
  return { stage: "failed", message: message.charAt(0).toUpperCase() + message.slice(1) };
}

export function showUpdateNotice(): void {
  useUpdateStore.setState({ noticeOpen: true });
}

/** Hides the notice; the status bar brings it back. A failure it showed is done with. */
export function hideUpdateNotice(): void {
  const { install } = useUpdateStore.getState();
  useUpdateStore.setState({
    noticeOpen: false,
    install: install?.stage === "failed" ? null : install,
  });
}

export async function skipUpdate(): Promise<void> {
  const { update } = useUpdateStore.getState();
  if (update) await saveSettingsSection("updates", { skippedVersion: update.version });
}
