import { describe, expect, it } from "vitest";
import {
  downloadBlocker,
  taskFromFiles,
  taskFromJobs,
  taskExistsAction,
  type SessionFacts,
  type TaskDraft,
} from "./scheduleDraft";
import type { JobSnapshot, TaskAction } from "./types";

function job(id: number, overrides: Partial<JobSnapshot> = {}): JobSnapshot {
  return {
    id,
    rank: String(id),
    sessionId: "web",
    direction: "upload",
    kind: "file",
    name: `file-${id}`,
    source: `/home/me/file-${id}`,
    target: `/srv/file-${id}`,
    targetDirectory: "/srv",
    size: 100,
    transferred: 100,
    speed: 0,
    connections: 0,
    state: "done",
    attempts: 0,
    ...overrides,
  };
}

const SESSIONS: Record<string, SessionFacts> = {
  web: { label: "Web", savedConnectionId: "saved-web", vault: false },
  backup: { label: "Backup", savedConnectionId: "saved-backup", vault: false },
  typed: { label: "me@example.com", vault: false },
  vault: { label: "Vault", savedConnectionId: "saved-web", vault: true },
};
const facts = (sessionId: string) => SESSIONS[sessionId];

function action(draft: TaskDraft): Extract<TaskAction, { type: "transfer" }> {
  if ("problem" in draft) throw new Error(draft.problem);
  if (draft.task.action.type !== "transfer") throw new Error("not a transfer task");
  return draft.task.action;
}

function problem(draft: TaskDraft): string {
  return "problem" in draft ? draft.problem : "";
}

describe("taskFromJobs", () => {
  it("repeats transfers into one folder", () => {
    const made = action(
      taskFromJobs(
        [job(1), job(2, { kind: "folder", source: "/home/me/assets" })],
        facts,
        "overwriteIfNewer",
      ),
    );
    expect(made).toMatchObject({
      connectionId: "saved-web",
      sourceConnectionId: null,
      direction: "upload",
      destination: "/srv",
      exists: "overwriteIfNewer",
      items: [
        { path: "/home/me/file-1", isDir: false },
        { path: "/home/me/assets", isDir: true },
      ],
    });
  });

  it("schedules the folder whose files were queued", () => {
    const made = action(
      taskFromJobs(
        [
          job(1, { source: "/home/me/site/css/site.css", targetDirectory: "/srv/site/css" }),
          job(2, { source: "/home/me/site/index.html", targetDirectory: "/srv/site" }),
          job(3, { source: "/home/me/notes.txt", targetDirectory: "/srv" }),
        ],
        facts,
        "ask",
      ),
    );
    expect(made.destination).toBe("/srv");
    expect(made.items).toEqual([
      { path: "/home/me/site", isDir: true },
      { path: "/home/me/notes.txt", isDir: false },
    ]);
    // No one is there to answer.
    expect(made.exists).toBe("overwrite");
  });

  it("reads Windows paths on this computer", () => {
    const made = action(
      taskFromJobs(
        [
          job(1, {
            direction: "download",
            source: "/var/www/site/css/site.css",
            targetDirectory: "C:\\Users\\me\\Downloads\\site\\css",
          }),
          job(2, {
            direction: "download",
            source: "/var/www/robots.txt",
            targetDirectory: "C:\\Users\\me\\Downloads",
          }),
        ],
        facts,
        "skip",
      ),
    );
    expect(made.destination).toBe("C:\\Users\\me\\Downloads");
    expect(made.items).toEqual([
      { path: "/var/www/site", isDir: true },
      { path: "/var/www/robots.txt", isDir: false },
    ]);
    const upload = action(
      taskFromJobs(
        [job(1, { source: "C:\\site\\css\\site.css", targetDirectory: "/srv/site/css" })],
        facts,
        "skip",
      ),
    );
    expect(upload.items).toEqual([{ path: "C:\\site\\css\\site.css", isDir: false }]);
  });

  it("copies between the servers of a relay", () => {
    const made = action(
      taskFromJobs(
        [job(1, { direction: "relay", sessionId: "backup", sourceSessionId: "web" })],
        facts,
        "rename",
      ),
    );
    expect(made).toMatchObject({
      direction: "relay",
      connectionId: "saved-backup",
      sourceConnectionId: "saved-web",
    });
  });

  it("says why transfers cannot be scheduled", () => {
    expect(problem(taskFromJobs([], facts, "skip"))).toContain("Select the transfers");
    expect(problem(taskFromJobs([job(1), job(2, { direction: "download" })], facts, "skip"))).toBe(
      "Select transfers that go the same way between the same places to schedule them.",
    );
    expect(problem(taskFromJobs([job(1, { sessionId: "typed" })], facts, "skip"))).toBe(
      "Save the connection to me@example.com first: a scheduled task signs in with a saved connection.",
    );
    expect(problem(taskFromJobs([job(1, { sessionId: "vault" })], facts, "skip"))).toContain(
      "vault",
    );
    expect(problem(taskFromJobs([job(1, { sessionId: "closed" })], facts, "skip"))).toContain(
      "no longer knows",
    );
    expect(
      problem(
        taskFromJobs(
          [job(1, { direction: "relay", sessionId: "web", sourceSessionId: "typed" })],
          facts,
          "skip",
        ),
      ),
    ).toContain("me@example.com");
    // Side by side, or renamed on the way: no one folder holds them.
    expect(
      problem(
        taskFromJobs(
          [job(1, { targetDirectory: "/srv/a" }), job(2, { targetDirectory: "/srv/b" })],
          facts,
          "skip",
        ),
      ),
    ).toContain("different folders");
    expect(
      problem(
        taskFromJobs(
          [job(1), job(2, { source: "/home/me/site/index.html", targetDirectory: "/srv/renamed" })],
          facts,
          "skip",
        ),
      ),
    ).toContain("different folders");
  });
});

describe("taskFromFiles", () => {
  const entries = [{ path: "/home/me/site", isDir: true }, { path: "/home/me/notes.txt" }];

  it("uploads to the server in view", () => {
    const made = action(
      taskFromFiles({
        direction: "upload",
        entries,
        server: SESSIONS.web,
        destination: "/var/www",
        exists: "ask",
      }),
    );
    expect(made).toMatchObject({
      connectionId: "saved-web",
      destination: "/var/www",
      exists: "overwrite",
      items: [
        { path: "/home/me/site", isDir: true },
        { path: "/home/me/notes.txt", isDir: false },
      ],
    });
  });

  it("leaves the server to the dialog when the one in view cannot be used", () => {
    for (const server of [SESSIONS.typed, SESSIONS.vault, undefined]) {
      const made = action(
        taskFromFiles({ direction: "upload", entries, server, destination: "/x", exists: "skip" }),
      );
      expect(made).toMatchObject({ connectionId: "", destination: "" });
    }
  });

  it("downloads into the local folder in view", () => {
    const made = action(
      taskFromFiles({
        direction: "download",
        entries: [{ path: "/var/www", isDir: true }],
        server: SESSIONS.web,
        destination: "/home/me/backups",
        exists: "resume",
      }),
    );
    expect(made).toMatchObject({
      direction: "download",
      connectionId: "saved-web",
      destination: "/home/me/backups",
      exists: "resume",
    });
    expect(
      problem(
        taskFromFiles({ direction: "download", entries, server: SESSIONS.typed, exists: "skip" }),
      ),
    ).toContain("Save the connection");
    expect(problem(taskFromFiles({ direction: "upload", entries: [], exists: "skip" }))).toContain(
      "Select the files",
    );
  });

  it("names what stops a download in a few words", () => {
    expect(downloadBlocker(SESSIONS.web)).toBeNull();
    expect(downloadBlocker(SESSIONS.typed)).toBe("save the connection first");
    expect(downloadBlocker(SESSIONS.vault)).toBe("not in a vault");
    expect(downloadBlocker(undefined)).toBe("no server");
    expect(taskExistsAction("ask")).toBe("overwrite");
    expect(taskExistsAction("skip")).toBe("skip");
  });
});
