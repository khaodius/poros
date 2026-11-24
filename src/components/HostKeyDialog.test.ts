// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { HostKeyApproval, HostKeyInfo } from "../lib/types";
import { useHostKeyPrompt } from "../state/connectFlow";
import { HostKeyDialog } from "./HostKeyDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeAll(() => {
  // The page's dialogs open as modals, which jsdom does not draw.
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
});

let root: Root | null = null;
let container: HTMLElement;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  useHostKeyPrompt.setState({ question: null });
});

/**
 * Asks about a key the way a connection attempt does, with the dialog on the page. The answer
 * comes back wrapped, since awaiting the question itself would wait for the answer.
 */
async function ask(hostKey: HostKeyInfo, changed: boolean) {
  container = document.createElement("div");
  root = createRoot(container);
  await act(async () => root!.render(createElement(HostKeyDialog)));
  let answered!: Promise<HostKeyApproval | null>;
  await act(async () => {
    answered = useHostKeyPrompt.getState().ask(hostKey, changed);
  });
  return { answered };
}

async function press(label: string) {
  const button = [...container.querySelectorAll("button")].find(
    (candidate) => candidate.textContent === label,
  );
  expect(button, label).toBeDefined();
  await act(async () => button!.click());
}

// The server behind a jump host, which the answer about the jump host must not stand for.
const behindJumpHost: HostKeyInfo = {
  host: "10.0.0.5",
  port: 2222,
  algorithm: "ssh-ed25519",
  fingerprint: "SHA256:inner",
};

describe("HostKeyDialog", () => {
  it("answers once for the host and port it asked about", async () => {
    const { answered } = await ask(behindJumpHost, false);
    expect(container.querySelector("h2")?.textContent).toBe("Unknown host");
    expect(container.querySelector("strong")?.textContent).toBe("10.0.0.5:2222");

    await press("Connect once");
    expect(await answered).toEqual({
      host: "10.0.0.5",
      port: 2222,
      fingerprint: "SHA256:inner",
      remember: false,
    });
    expect(useHostKeyPrompt.getState().question).toBeNull();
  });

  it("remembers a new key for that server alone", async () => {
    const { answered } = await ask(behindJumpHost, false);
    await press("Trust and connect");
    expect(await answered).toEqual({
      host: "10.0.0.5",
      port: 2222,
      fingerprint: "SHA256:inner",
      remember: true,
    });
  });

  it("replaces a changed certificate for the server that showed it", async () => {
    const { answered } = await ask(
      {
        host: "dav.example.com",
        port: 8443,
        algorithm: "X.509 certificate",
        fingerprint: "SHA256:replaced",
        certificateProblem: "It is signed by an unknown authority.",
      },
      true,
    );
    expect(container.querySelector("h2")?.textContent).toBe("Certificate changed");
    await press("Replace certificate and connect");
    expect(await answered).toEqual({
      host: "dav.example.com",
      port: 8443,
      fingerprint: "SHA256:replaced",
      remember: true,
    });
  });

  it("gives no answer when the question is turned down", async () => {
    const { answered } = await ask(behindJumpHost, true);
    await press("Cancel");
    expect(await answered).toBeNull();
  });
});
