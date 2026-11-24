// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ENGLISH, setLanguage, SYSTEM_LANGUAGE, useLanguageStore } from "../i18n";
import type { Translation } from "../i18n/catalog";
import { createTranslate } from "../i18n/runtime";
import type { Trigger } from "../lib/types";
import { TriggerFields } from "./SchedulerDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A language that puts the start first and the unit after the number, as Japanese does. */
const START_FIRST = {
  locale: "ja",
  name: "Test",
  messages: {
    schedule: {
      everyAmountUnitFromStart: "{start}から{amount}{unit}ごと",
      unitMinutes: { other: "分" },
      atTimeOnDays: "{days}の{time}",
    },
  },
} as unknown as Translation;

let root: Root | null = null;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  act(() => setLanguage(SYSTEM_LANGUAGE));
});

/** The parts after the kind buttons, in order: a label's words, or the kind of field. */
function partsOf(container: HTMLElement): string[] {
  const row = container.querySelector(".trigger-row")!;
  return [...row.children].slice(1).map((part) => {
    if (part.classList.contains("trigger-label")) return part.textContent!;
    if (part.classList.contains("stepper")) return "[amount]";
    if (part.classList.contains("toggle-group")) return "[days]";
    if (part instanceof HTMLSelectElement) {
      return `[unit: ${part.selectedOptions[0].textContent}]`;
    }
    return `[${(part as HTMLInputElement).type}]`;
  });
}

async function render(trigger: Trigger): Promise<HTMLElement> {
  const container = document.createElement("div");
  root = createRoot(container);
  await act(async () =>
    root!.render(createElement(TriggerFields, { trigger, onChange: () => undefined })),
  );
  return container;
}

describe("the trigger editor", () => {
  it("writes the interval as one sentence, with the unit agreeing with the number", async () => {
    let container = await render({ type: "every", minutes: 1, start: 0 });
    expect(partsOf(container)).toEqual([
      "every",
      "[amount]",
      "[unit: minute]",
      "from",
      "[datetime-local]",
    ]);
    act(() => root?.unmount());
    container = await render({ type: "every", minutes: 5, start: 0 });
    expect(partsOf(container)).toContain("[unit: minutes]");
  });

  it("puts the fields where the language's sentence has them", async () => {
    const container = await render({ type: "every", minutes: 5, start: 0 });
    await act(async () => {
      useLanguageStore.setState({
        choice: "ja",
        translation: START_FIRST,
        translate: createTranslate(START_FIRST, ENGLISH),
      });
    });
    expect(partsOf(container)).toEqual([
      "[datetime-local]",
      "から",
      "[amount]",
      "[unit: 分]",
      "ごと",
    ]);
  });

  it("does the same for the time of a daily task", async () => {
    const container = await render({ type: "daily", minuteOfDay: 120, weekdays: [0] });
    expect(partsOf(container)).toEqual(["at", "[time]", "[days]"]);
    await act(async () => {
      useLanguageStore.setState({
        choice: "ja",
        translation: START_FIRST,
        translate: createTranslate(START_FIRST, ENGLISH),
      });
    });
    expect(partsOf(container)).toEqual(["[days]", "の", "[time]"]);
  });
});
