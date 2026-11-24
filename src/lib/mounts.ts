import { t } from "../i18n";
import { formatSize } from "./format";
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
  const fromVault = busy.some((drive) => drive.protocol === "cryptomator");
  return fromVault
    ? t("drives.restartUnmountsDrivesInUseWithVault", { count: busy.length })
    : t("drives.restartUnmountsDrivesInUse", { count: busy.length });
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
  const count = t("drives.driveCount", { count: drives.length });
  const failed = failedUploads(drives);
  if (failed > 0) {
    return {
      text: t("drives.drivesWithFailedUploads", { drives: count, count: failed }),
      tone: "failed",
    };
  }
  const pending = pendingUploads(drives);
  if (pending > 0) {
    return { text: t("drives.drivesUploading", { drives: count, count: pending }), tone: "busy" };
  }
  return { text: count, tone: "idle" };
}

/** One line on what a drive is busy with. */
export function driveActivity(drive: DriveView): string {
  const parts: string[] = [];
  if (drive.openFiles.length > 0) {
    parts.push(t("drives.filesOpen", { count: drive.openFiles.length }));
  }
  const pending = drive.uploads.filter((upload) => !upload.error).length;
  if (pending > 0) parts.push(t("drives.uploadsPending", { count: pending }));
  if (drive.bytesRead > 0) parts.push(t("drives.sizeRead", { size: formatSize(drive.bytesRead) }));
  if (drive.bytesWritten > 0) {
    parts.push(t("drives.sizeWritten", { size: formatSize(drive.bytesWritten) }));
  }
  if (parts.length === 0) return t("drives.idle");
  const text = parts.join(t("common.listSeparator"));
  return text.charAt(0).toLocaleUpperCase(t.locale) + text.slice(1);
}

/** How far one upload has got, or why it failed. */
export function uploadText(upload: DriveUpload): string {
  if (upload.error) return t("drives.uploadFailed", { error: upload.error });
  if (!upload.uploading) return t("drives.uploadWaiting");
  if (upload.size === 0) return t("drives.uploading");
  const percent = Math.min(100, Math.floor((upload.sent / upload.size) * 100));
  return t("drives.uploadingPercent", { percent });
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

/** Shown as the mount point's placeholder and in what is wrong with one typed. */
export const DRIVE_LETTER_EXAMPLE = "P:";
export const FOLDER_EXAMPLE = "~/Poros/server";

/** What is wrong with a folder typed for a drive, or null if nothing is. */
export function mountPointProblem(text: string, usesDriveLetters: boolean): string | null {
  const trimmed = text.trim();
  if (trimmed === "") {
    return usesDriveLetters ? t("drives.chooseADriveLetter") : t("drives.chooseAFolder");
  }
  if (usesDriveLetters) {
    return /^[A-Za-z]:\\?$/.test(trimmed)
      ? null
      : t("drives.driveLetterSuchAs", { example: DRIVE_LETTER_EXAMPLE });
  }
  return trimmed.startsWith("/") || trimmed === "~" || trimmed.startsWith("~/")
    ? null
    : t("drives.fullPathSuchAs", { example: FOLDER_EXAMPLE });
}
