// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ENGLISH, setLanguage, SYSTEM_LANGUAGE, useLanguageStore } from "../i18n";
import type { Translation } from "../i18n/catalog";
import { createTranslate } from "../i18n/runtime";
import { applyUpdate, EMPTY_SNAPSHOT, EMPTY_STATS } from "../lib/transfers";
import type { JobSnapshot } from "../lib/types";
import { useTransferStore } from "../state/transferStore";
import { TransferView } from "./TransferView";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
// jsdom lays nothing out, so the list is given every row.
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 28,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({ index, key: index, start: index * 28 })),
    scrollToIndex: () => undefined,
  }),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const GERMAN = {
  locale: "de",
  name: "Deutsch",
  messages: {
    transfers: {
      queued: "Wartet",
      paused: "Angehalten",
      rowTitle: "{source} nach {target}",
    },
  },
} as unknown as Translation;

function job(id: number, overrides: Partial<JobSnapshot>): JobSnapshot {
  return {
    id,
    rank: `8000000${id}`,
    sessionId: "s",
    direction: "upload",
    kind: "file",
    name: `report-${id}.pdf`,
    source: `/home/me/report-${id}.pdf`,
    target: `/srv/report-${id}.pdf`,
    targetDirectory: "/srv",
    size: 1536,
    transferred: 0,
    speed: 0,
    connections: 0,
    state: "queued",
    attempts: 0,
    ...overrides,
  };
}

let root: Root | null = null;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  act(() => setLanguage(SYSTEM_LANGUAGE));
  useTransferStore.setState({ snapshot: EMPTY_SNAPSHOT });
});

function rows(container: HTMLElement) {
  return [...container.querySelectorAll<HTMLElement>(".transfer-row")].map((row) => ({
    status: row.querySelector(".transfer-status")?.textContent,
    size: row.querySelectorAll(".align-end")[0]?.textContent,
    title: row.title,
  }));
}

describe("a queue row", () => {
  it("follows a change of language, sizes included", async () => {
    useTransferStore.setState({
      snapshot: applyUpdate(EMPTY_SNAPSHOT, {
        sequence: 1,
        jobs: [job(1, {}), job(2, { state: "paused" })],
        removed: [],
        changedDirectories: [],
        stats: EMPTY_STATS,
      }),
    });
    const container = document.createElement("div");
    root = createRoot(container);
    await act(async () =>
      root!.render(
        createElement(TransferView, {
          view: "queue",
          selection: new Set<number>(),
          onSelectionChange: () => undefined,
        }),
      ),
    );
    expect(rows(container)).toEqual([
      { status: "Queued", size: "1.5 KB", title: "/home/me/report-1.pdf\nto /srv/report-1.pdf" },
      { status: "Paused", size: "1.5 KB", title: "/home/me/report-2.pdf\nto /srv/report-2.pdf" },
    ]);

    await act(async () => {
      useLanguageStore.setState({
        choice: "de",
        translation: GERMAN,
        translate: createTranslate(GERMAN, ENGLISH),
      });
    });
    expect(rows(container)).toEqual([
      { status: "Wartet", size: "1,5 KB", title: "/home/me/report-1.pdf nach /srv/report-1.pdf" },
      {
        status: "Angehalten",
        size: "1,5 KB",
        title: "/home/me/report-2.pdf nach /srv/report-2.pdf",
      },
    ]);
  });
});
