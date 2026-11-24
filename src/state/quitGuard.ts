import { busyDrives } from "../lib/mounts";
import { editsHoldExit } from "./externalEditStore";
import { useMountStore } from "./mountStore";
import { useUiStore } from "./uiStore";

/**
 * Whether quitting waits for an answer, asking one question at a time. Files saved in other
 * programs that have not reached their servers come first: agreeing to that question closes
 * the window again, which comes back here and goes on. Drives in use come next, and their
 * question goes on to files with unsaved changes by itself and then closes the window for good,
 * so no question is asked twice.
 */
export async function quitWaits(): Promise<boolean> {
  if (await editsHoldExit()) return true;
  await useMountStore.getState().load();
  const drives = busyDrives(useMountStore.getState().drives);
  if (drives.length === 0) return false;
  useUiStore.getState().open({ kind: "quit", drives });
  return true;
}
