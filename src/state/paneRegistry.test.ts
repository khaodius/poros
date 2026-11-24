import { afterEach, describe, expect, it } from "vitest";
import {
  activePane,
  getPane,
  lastActivePane,
  registerPane,
  unregisterPane,
  updatePane,
  type PaneHandle,
} from "./paneRegistry";

/** A server pane as it registers: not on screen and never used, until it says otherwise. */
function serverPane(tabId: string, sessionId: string, path = "/srv"): PaneHandle {
  return {
    tabId,
    kind: "remote",
    sessionId,
    label: `deploy@${sessionId}`,
    path: () => path,
    refresh: () => undefined,
    selected: () => [],
    visible: false,
    activatedAt: 0,
  };
}

afterEach(() => ["first", "second"].forEach(unregisterPane));

describe("a pane that registers again", () => {
  it("keeps what it reported under the session it had before", () => {
    registerPane(serverPane("first", "s1", "/srv/private"));
    updatePane("first", { visible: true, activatedAt: 40 });
    updatePane("first", { locked: true });

    // A reconnect, or going into a vault, gives the tab another session and name.
    registerPane(serverPane("first", "s2", "/srv/private"));

    expect(getPane("first")).toMatchObject({
      sessionId: "s2",
      label: "deploy@s2",
      visible: true,
      activatedAt: 40,
      locked: true,
    });
  });

  it("is still the server pane files go to, ahead of a hidden tab looked at earlier", () => {
    registerPane(serverPane("first", "s1"));
    updatePane("first", { visible: true });
    registerPane(serverPane("second", "other"));
    updatePane("second", { visible: true, activatedAt: 10 });
    updatePane("second", { visible: false });

    registerPane(serverPane("first", "s2"));

    expect(lastActivePane("remote")?.tabId).toBe("first");
    expect(activePane()?.tabId).toBe("first");
  });

  it("starts afresh once its tab's pane was gone", () => {
    registerPane(serverPane("first", "s1"));
    updatePane("first", { visible: true, activatedAt: 40, locked: true });
    unregisterPane("first");
    expect(getPane("first")).toBeUndefined();

    registerPane(serverPane("first", "s2"));
    expect(getPane("first")).toMatchObject({ visible: false, activatedAt: 0 });
    expect(getPane("first")?.locked).toBeUndefined();
  });

  it("is not removed by the registration it replaced", () => {
    const removeFirst = registerPane(serverPane("first", "s1"));
    const removeSecond = registerPane(serverPane("first", "s2"));
    removeFirst();
    expect(getPane("first")?.sessionId).toBe("s2");
    removeSecond();
    expect(getPane("first")).toBeUndefined();
  });
});
