import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS } from "../lib/settings";
import { openWithDefault, type OpenTarget } from "./externalEditStore";
import { useSettingsStore } from "./settingsStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

function onServer(name: string): OpenTarget {
  return { side: "remote", sessionId: "s1", path: `/var/www/${name}`, name };
}

/** The request each call to open a server file sent. */
const openRequests = () =>
  invoke.mock.calls.flatMap(([command, args]) =>
    command === "external_edit_open" ? [(args as { request: unknown }).request] : [],
  );

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue({ kind: "opened" });
  useSettingsStore.setState({
    settings: {
      ...DEFAULT_SETTINGS,
      editing: {
        ...DEFAULT_SETTINGS.editing,
        programs: [
          {
            id: "code",
            name: "VS Code",
            command: "/usr/bin/code",
            arguments: "--wait {file}",
            patterns: ["*.php"],
          },
        ],
      },
    },
  });
});

describe("Open", () => {
  it("opens a script in the program set for its kind without asking again", async () => {
    await openWithDefault(onServer("index.php"));
    expect(openRequests()).toEqual([
      {
        sessionId: "s1",
        path: "/var/www/index.php",
        with: { kind: "program", id: "code" },
        confirmedExecutable: true,
      },
    ]);
  });

  it("leaves the system's own program to ask about a file of a type that runs", async () => {
    await openWithDefault(onServer("deploy.sh"));
    expect(openRequests()).toEqual([
      {
        sessionId: "s1",
        path: "/var/www/deploy.sh",
        with: { kind: "default" },
        confirmedExecutable: false,
      },
    ]);
  });
});
