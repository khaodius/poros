import { describe, expect, it } from "vitest";
import {
  connectionRoot,
  driveActivity,
  driveName,
  drivesShowing,
  indicatorSummary,
  mountPointProblem,
  shortPath,
  uploadText,
} from "./mounts";
import type { DriveUpload, DriveView, SessionInfo } from "./types";

function drive(id: string, changes: Partial<DriveView> = {}): DriveView {
  return {
    id,
    sessionId: `session-${id}`,
    label: `deploy@${id}.example.com`,
    protocol: "sftp",
    mountPoint: `/home/me/Poros/${id}`,
    remoteRoot: "/srv",
    readOnly: false,
    bytesRead: 0,
    bytesWritten: 0,
    openFiles: [],
    uploads: [],
    ...changes,
  };
}

function upload(path: string, changes: Partial<DriveUpload> = {}): DriveUpload {
  return { path, size: 1000, sent: 0, uploading: false, ...changes };
}

const server: SessionInfo = {
  id: "server",
  label: "deploy@build.example.com",
  host: "build.example.com",
  port: 22,
  username: "deploy",
  home: "/home/deploy",
  initialPath: "/home/deploy",
  protocol: "sftp",
};

const vault: SessionInfo = {
  ...server,
  id: "vault",
  label: "Taxes",
  protocol: "cryptomator",
  vault: { parentSessionId: "server", root: "/home/deploy/Taxes", name: "Taxes" },
};

describe("drivesShowing", () => {
  it("keeps the drives of the sessions asked for", () => {
    const drives = [drive("a"), drive("b"), drive("c")];
    expect(drivesShowing(drives, ["session-a", "session-c"]).map((shown) => shown.id)).toEqual([
      "a",
      "c",
    ]);
    expect(drivesShowing(drives, [])).toEqual([]);
  });
});

describe("indicatorSummary", () => {
  it("counts the drives when nothing is pending", () => {
    expect(indicatorSummary([drive("a")])).toEqual({ text: "1 drive", tone: "idle" });
    expect(indicatorSummary([drive("a"), drive("b")])).toEqual({ text: "2 drives", tone: "idle" });
  });

  it("says how many uploads are under way", () => {
    const busy = drive("a", { uploads: [upload("a.txt"), upload("b.txt", { uploading: true })] });
    expect(indicatorSummary([busy, drive("b")])).toEqual({
      text: "2 drives, uploading 2",
      tone: "busy",
    });
  });

  it("puts failed uploads first", () => {
    const failing = drive("a", {
      uploads: [upload("a.txt", { error: "Permission denied" }), upload("b.txt")],
    });
    expect(indicatorSummary([failing])).toEqual({
      text: "1 drive, 1 upload failed",
      tone: "failed",
    });
  });
});

describe("driveActivity", () => {
  it("is idle before anything is read or written", () => {
    expect(driveActivity(drive("a"))).toBe("Idle");
  });

  it("names open files, pending uploads and what has moved", () => {
    const busy = drive("a", {
      openFiles: ["report.odt", "notes.txt"],
      uploads: [upload("report.odt", { uploading: true }), upload("old.txt", { error: "No" })],
      bytesRead: 2048,
      bytesWritten: 0,
    });
    expect(driveActivity(busy)).toBe("2 files open, 1 upload pending, 2.0 KB read");
    expect(driveActivity(drive("b", { bytesWritten: 1024 }))).toBe("1.0 KB written");
  });
});

describe("uploadText", () => {
  it("tells waiting, sending and failed uploads apart", () => {
    expect(uploadText(upload("a"))).toBe("Waiting");
    expect(uploadText(upload("a", { uploading: true, sent: 425 }))).toBe("Uploading, 42%");
    expect(uploadText(upload("a", { uploading: true, size: 0 }))).toBe("Uploading");
    expect(uploadText(upload("a", { error: "Disk full" }))).toBe("Failed: Disk full");
  });
});

describe("shortPath", () => {
  it("shows the home folder as a tilde", () => {
    expect(shortPath("/home/me/Poros/build", "/home/me")).toBe("~/Poros/build");
    expect(shortPath("/home/me", "/home/me/")).toBe("~");
    expect(shortPath("/home/mean/Poros", "/home/me")).toBe("/home/mean/Poros");
    expect(shortPath("C:\\Users\\me\\Poros", "C:\\Users\\me")).toBe("~\\Poros");
    expect(shortPath("/mnt/drive", null)).toBe("/mnt/drive");
  });
});

describe("driveName and connectionRoot", () => {
  it("use the vault's name and folder inside a vault", () => {
    expect(driveName(server)).toBe("deploy@build.example.com");
    expect(connectionRoot(server)).toBe("/");
    expect(driveName(vault)).toBe("Taxes");
    expect(connectionRoot(vault)).toBe("/home/deploy/Taxes");
  });
});

describe("mountPointProblem", () => {
  it("wants a full path for a folder", () => {
    expect(mountPointProblem("~/Poros/build", false)).toBeNull();
    expect(mountPointProblem("/mnt/build", false)).toBeNull();
    expect(mountPointProblem("~", false)).toBeNull();
    expect(mountPointProblem("Poros/build", false)).toBe("A full path, such as ~/Poros/server");
    expect(mountPointProblem("  ", false)).toBe("Choose a folder.");
  });

  it("wants a drive letter on Windows", () => {
    expect(mountPointProblem("P:", true)).toBeNull();
    expect(mountPointProblem("p:\\", true)).toBeNull();
    expect(mountPointProblem("C:\\Mount", true)).toBe("A drive letter, such as P:");
    expect(mountPointProblem("", true)).toBe("Choose a drive letter.");
  });
});
