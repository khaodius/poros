import { beforeEach, describe, expect, it, vi } from "vitest";
import { allTabs, group, type TerminalTab } from "../lib/layout";
import type { TerminalInfo } from "../lib/types";
import { useLayoutStore } from "./layoutStore";
import { openShell, type TerminalScreen } from "./terminalActions";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

const shell: TerminalTab = {
  id: "shell",
  kind: "terminal",
  sessionId: "s1",
  label: "deploy@web.example.com",
};

/** Holds the shell's opening until `finish` is called. */
let finish: (info: TerminalInfo) => void = () => undefined;

/** Starts the shell of the terminal tab, whose pane stays open. */
const start = (screen: TerminalScreen) =>
  openShell(
    "shell",
    screen,
    () => undefined,
    () => false,
  );

const calls = (name: string) =>
  invoke.mock.calls.flatMap(([command, args]) => (command === name ? [args] : []));

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((command: string) =>
    command === "terminal_open"
      ? new Promise<TerminalInfo>((resolve) => (finish = resolve))
      : Promise.resolve(),
  );
  useLayoutStore.getState().setRoot(group([shell]));
});

describe("a shell that opens", () => {
  it("is told the size its terminal took while it opened", async () => {
    const screen = { cols: 80, rows: 24 };
    const opening = start(screen);
    expect(calls("terminal_open")).toMatchObject([{ sessionId: "s1", columns: 80, rows: 24 }]);

    // The window is made larger while the server starts the shell.
    screen.cols = 132;
    screen.rows = 40;
    finish({ id: "t1", label: shell.label });

    expect(await opening).toBe("t1");
    expect(calls("terminal_resize")).toEqual([{ terminalId: "t1", columns: 132, rows: 40 }]);
    expect(allTabs(useLayoutStore.getState().root)).toEqual([{ ...shell, terminalId: "t1" }]);
  });

  it("is not resized when its terminal kept its size", async () => {
    const opening = start({ cols: 80, rows: 24 });
    finish({ id: "t1", label: shell.label });
    expect(await opening).toBe("t1");
    expect(calls("terminal_resize")).toEqual([]);
  });

  it("is closed again when its tab closed meanwhile", async () => {
    const opening = start({ cols: 80, rows: 24 });
    useLayoutStore.getState().closeTab("shell");
    finish({ id: "t1", label: shell.label });
    expect(await opening).toBeNull();
    expect(calls("terminal_close")).toEqual([{ terminalId: "t1" }]);
  });
});
