// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { HostKeyDialog } from "../components/HostKeyDialog";
import type { HostKeyInfo } from "../lib/types";
import { useHostKeyPrompt } from "../state/connectFlow";
import type { Translation } from "./catalog";
import { ENGLISH, setLanguage, SYSTEM_LANGUAGE, useLanguageStore } from "./index";
import { createTranslate } from "./runtime";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The few German texts the host key question shows; the rest falls back to English. */
const GERMAN = {
  locale: "de",
  name: "Deutsch",
  messages: {
    common: { cancel: "Abbrechen" },
    connect: {
      unknownHost: "Unbekannter Host",
      connectOnce: "Einmal verbinden",
      trustAndConnect: "Vertrauen und verbinden",
      hasNotBeenSeenBefore:
        "{target} ist noch unbekannt. Vergleichen Sie den Fingerabdruck mit dem, den Ihnen die Verwaltung des Servers gegeben hat.",
      keyType: "Schlüsseltyp",
      fingerprint: "Fingerabdruck",
    },
  },
} as unknown as Translation;

const hostKey: HostKeyInfo = {
  host: "files.example.com",
  port: 22,
  algorithm: "ssh-ed25519",
  fingerprint: "SHA256:abc",
};

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

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  act(() => setLanguage(SYSTEM_LANGUAGE));
  useHostKeyPrompt.setState({ question: null });
});

function buttons(container: HTMLElement): string[] {
  return [...container.querySelectorAll("button")].map((button) => button.textContent ?? "");
}

describe("switching the language", () => {
  it("changes an open dialog at once, without closing it", async () => {
    const container = document.createElement("div");
    root = createRoot(container);
    await act(async () => root!.render(createElement(HostKeyDialog)));
    await act(async () => {
      void useHostKeyPrompt.getState().ask(hostKey, false);
    });
    expect(container.querySelector("h2")?.textContent).toBe("Unknown host");
    expect(buttons(container)).toContain("Connect once");

    await act(async () => {
      useLanguageStore.setState({
        choice: "de",
        translation: GERMAN,
        translate: createTranslate(GERMAN, ENGLISH),
      });
    });
    expect(container.querySelector("h2")?.textContent).toBe("Unbekannter Host");
    expect(buttons(container)).toEqual(
      expect.arrayContaining(["Abbrechen", "Einmal verbinden", "Vertrauen und verbinden"]),
    );
    expect(container.querySelector("p")?.textContent).toMatch(/^files\.example\.com ist noch/);
    expect(useHostKeyPrompt.getState().question).not.toBeNull();

    await act(async () => setLanguage("en"));
    expect(container.querySelector("h2")?.textContent).toBe("Unknown host");
  });

  it("marks the page with the language it shows", () => {
    act(() => setLanguage("en"));
    expect(document.documentElement.lang).toBe("en");
    expect(localStorage.getItem("poros.language")).toBe("en");
  });
});
