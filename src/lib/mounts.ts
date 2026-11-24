import { formatSize, pluralize } from "./format";
import type { DriveUpload, DriveView, SessionInfo } from "./types";

/** The drives that show any of the sessions. */
export function drivesShowing(drives: DriveView[], sessionIds: string[]): DriveView[] {
  return drives.filter((drive) => sessionIds.includes(drive.sessionId));
}

/** Whether programs have files open on the drive, or uploads are pending. */
function inUse(drive: DriveView): boolean {
  return drive.openFiles.length > 0 || drive.uploads.length > 0;
}

/** Where the drives are that programs have files open on or that have uploads pending. */
export function busyDrives(drives: DriveView[]): string[] {
  return drives.filter(inUse).map((drive) => drive.mountPoint);
}

/** What restarting does to the drives in use, as a sentence for the user, if any is. */
export function drivesInUseWarning(drives: DriveView[]): string | null {
  const busy = drives.filter(inUse);
  if (busy.length === 0) return null;
  const one = busy.length === 1;
  const fromVault = busy.some((drive) => drive.protocol === "cryptomator");
  return (
    `${one ? "A drive has" : `${busy.length} drives have`} files open in programs or uploads ` +
    `pending. Restarting unmounts ${one ? "it" : "them"}, and what is not uploaded yet is kept ` +
    "on this computer" +
    (fromVault ? ", unencrypted for files from a vault." : ".")
  );
}

export function failedUploads(drives: DriveView[]): number {
  return drives.reduce(
    (total, drive) => total + drive.uploads.filter((upload) => upload.error).length,
    0,
  );
}

export function pendingUploads(drives: DriveView[]): number {
  return drives.reduce(
    (total, drive) => total + drive.uploads.filter((upload) => !upload.error).length,
    0,
  );
}

export type IndicatorTone = "idle" | "busy" | "failed";

/** What the status bar says about the mounted drives, and in which tone. */
export function indicatorSummary(drives: DriveView[]): { text: string; tone: IndicatorTone } {
  const count = pluralize(drives.length, "drive");
  const failed = failedUploads(drives);
  if (failed > 0) {
    return { text: `${count}, ${pluralize(failed, "upload")} failed`, tone: "failed" };
  }
  const pending = pendingUploads(drives);
  if (pending > 0) return { text: `${count}, uploading ${pending}`, tone: "busy" };
  return { text: count, tone: "idle" };
}

/** One line on what a drive is busy with. */
export function driveActivity(drive: DriveView): string {
  const parts: string[] = [];
  if (drive.openFiles.length > 0) parts.push(`${pluralize(drive.openFiles.length, "file")} open`);
  const pending = drive.uploads.filter((upload) => !upload.error).length;
  if (pending > 0) parts.push(`${pluralize(pending, "upload")} pending`);
  const moved = [
    drive.bytesRead > 0 && `${formatSize(drive.bytesRead)} read`,
    drive.bytesWritten > 0 && `${formatSize(drive.bytesWritten)} written`,
  ].filter((part): part is string => Boolean(part));
  if (parts.length === 0 && moved.length === 0) return "Idle";
  const text = [...parts, ...moved].join(", ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** How far one upload has got, or why it failed. */
export function uploadText(upload: DriveUpload): string {
  if (upload.error) return `Failed: ${upload.error}`;
  if (!upload.uploading) return "Waiting";
  if (upload.size === 0) return "Uploading";
  const percent = Math.min(100, Math.floor((upload.sent / upload.size) * 100));
  return `Uploading, ${percent}%`;
}

/** A local path with the home folder shown as `~`. */
export function shortPath(path: string, home: string | null): string {
  if (!home) return path;
  const trimmedHome = home.replace(/[\\/]+$/, "");
  if (path === trimmedHome) return "~";
  for (const separator of ["/", "\\"]) {
    if (path.startsWith(trimmedHome + separator)) return `~${path.slice(trimmedHome.length)}`;
  }
  return path;
}

/** The name a session's drive is known by: the vault's for a vault, else the connection's. */
export function driveName(info: SessionInfo): string {
  return info.vault?.name ?? info.label;
}

/** The top of what a session shows: a vault's own folder, or the server's root. */
export function connectionRoot(info: SessionInfo): string {
  return info.vault?.root ?? "/";
}

/** What is wrong with a folder typed for a drive, or null if nothing is. */
export function mountPointProblem(text: string, usesDriveLetters: boolean): string | null {
  const trimmed = text.trim();
  if (trimmed === "") return usesDriveLetters ? "Choose a drive letter." : "Choose a folder.";
  if (usesDriveLetters) {
    return /^[A-Za-z]:\\?$/.test(trimmed) ? null : "A drive letter, such as P:";
  }
  return trimmed.startsWith("/") || trimmed === "~" || trimmed.startsWith("~/")
    ? null
    : "A full path, such as ~/Poros/server";
}
