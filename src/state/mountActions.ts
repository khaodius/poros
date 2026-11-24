import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { mounts } from "../lib/ipc";
import { baseName } from "../lib/path";
import type { DriveView, MountRequest, UnsentFile } from "../lib/types";
import { useLogStore } from "./logStore";
import { useMountStore } from "./mountStore";
import { useToastStore } from "./toastStore";
import { useUiStore } from "./uiStore";

/** Asks where to mount a session's server or vault, starting from the folder `path` shows. */
export function requestMount(sessionId: string, path: string | null): void {
  useUiStore.getState().open({ kind: "mount", sessionId, path });
}

/** Mounts a drive, and opens it in the file manager when asked. Rejects with the backend's error. */
export async function mountDrive(request: MountRequest, openWhenMounted: boolean): Promise<void> {
  const drive = await mounts.start(request);
  await useMountStore.getState().load();
  if (openWhenMounted) await openDrive(drive);
}

function reportFailure(what: string, caught: unknown, sessionId?: string): void {
  const message = (caught as { message?: string }).message ?? String(caught);
  useToastStore.getState().show("error", message);
  useLogStore.getState().write("error", `${what}: ${message}`, sessionId);
}

export async function openDrive(drive: DriveView): Promise<void> {
  await mounts
    .open(drive.id)
    .catch((caught) =>
      reportFailure(`Could not open ${drive.mountPoint}`, caught, drive.sessionId),
    );
}

/** Unmounts a drive; without `force` the backend refuses while files are open or uploads wait. */
export async function unmountDrive(drive: DriveView, force: boolean): Promise<void> {
  try {
    await mounts.unmount(drive.id, force);
  } catch (caught) {
    reportFailure(`Could not unmount ${drive.mountPoint}`, caught, drive.sessionId);
  }
  await useMountStore.getState().load();
}

export async function retryUploads(drive: DriveView): Promise<void> {
  await mounts
    .retry(drive.id)
    .catch((caught) => reportFailure("Could not send the uploads again", caught, drive.sessionId));
  await useMountStore.getState().load();
}

/** Saves a copy of a kept file where the user chooses, under the name it had on the server. */
export async function saveUnsentCopy(file: UnsentFile): Promise<void> {
  const destination = await saveFileDialog({ defaultPath: baseName(file.remotePath) });
  if (typeof destination !== "string") return;
  try {
    await mounts.saveUnsent(file.path, destination);
    useToastStore.getState().show("info", `Saved a copy at ${destination}`);
  } catch (caught) {
    reportFailure("Could not save the kept file", caught);
  }
}

export async function discardUnsent(file: UnsentFile): Promise<void> {
  try {
    await mounts.discardUnsent(file.path);
    useLogStore.getState().write("info", `Discarded the kept copy of ${file.remotePath}`);
  } catch (caught) {
    reportFailure("Could not discard the kept file", caught);
  }
  await useMountStore.getState().load();
}
