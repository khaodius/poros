import { createElement, isValidElement, type ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";
import type { Translation } from "./catalog";
import { pluralCategoriesOf } from "./check";
import { ENGLISH, translatorFor } from "./index";
import { createTranslate, placeholdersOf } from "./runtime";

/** A catalog with only the given messages, which is all a test of the runtime needs. */
function catalog(locale: string, messages: Record<string, Record<string, unknown>>): Translation {
  return { locale, name: locale, messages } as unknown as Translation;
}

/** One message in each plural form `locale` has, naming the form it is. */
function formsOf(locale: string): Record<string, string> {
  return Object.fromEntries(
    pluralCategoriesOf(locale).map((category) => [category, `${category}: {count}`]),
  );
}

describe("createTranslate", () => {
  it("fills in named placeholders, in any order the language puts them", () => {
    const german = catalog("de", {
      dialogs: { alreadyExistsIn: "In {folder} gibt es {name} schon." },
    });
    const translate = createTranslate(german, ENGLISH);
    expect(translate("dialogs.alreadyExistsIn", { name: "a.txt", folder: "/srv" })).toBe(
      "In /srv gibt es a.txt schon.",
    );
  });

  it("writes numbers the way the language does", () => {
    const messages = { dialogs: { andMoreListedInThe: { one: "{count}", other: "{count}" } } };
    expect(
      createTranslate(catalog("en", messages), ENGLISH)("dialogs.andMoreListedInThe", {
        count: 1234567,
      }),
    ).toBe("1,234,567");
    expect(
      createTranslate(catalog("de", messages), ENGLISH)("dialogs.andMoreListedInThe", {
        count: 1234567,
      }),
    ).toBe("1.234.567");
  });

  it("leaves a placeholder it was not given as written", () => {
    const translate = createTranslate(
      catalog("en", { connect: { foundFiles: "Found {files}" } }),
      ENGLISH,
    );
    expect(
      (translate as unknown as (key: string, params: object) => string)("connect.foundFiles", {}),
    ).toBe("Found {files}");
  });

  it.each([
    [
      "en",
      [
        [0, "other"],
        [1, "one"],
        [2, "other"],
      ],
    ],
    [
      "de",
      [
        [0, "other"],
        [1, "one"],
        [2, "other"],
      ],
    ],
    [
      "es",
      [
        [0, "other"],
        [1, "one"],
        [2, "other"],
        [1_000_000, "many"],
      ],
    ],
    [
      "fr",
      [
        [0, "one"],
        [1, "one"],
        [2, "other"],
        [1_000_000, "many"],
      ],
    ],
    [
      "ja",
      [
        [0, "other"],
        [1, "other"],
        [2, "other"],
      ],
    ],
    [
      "zh-Hans",
      [
        [0, "other"],
        [1, "other"],
        [2, "other"],
      ],
    ],
  ] as const)("picks the plural forms of %s", (locale, cases) => {
    const translate = createTranslate(
      catalog(locale, { pane: { items: formsOf(locale) } }),
      ENGLISH,
    );
    for (const [count, form] of cases) {
      expect(translate("pane.items", { count })).toBe(
        `${form}: ${new Intl.NumberFormat(locale).format(count)}`,
      );
    }
  });

  it("falls back to the general form when a language leaves one out", () => {
    const translate = createTranslate(
      catalog("fr", { pane: { items: { one: "{count} élément", other: "{count} éléments" } } }),
      ENGLISH,
    );
    expect(translate("pane.items", { count: 1_000_000 })).toBe("1\u202f000\u202f000 éléments");
  });

  it("writes a key the language does not have yet in English, and says so", () => {
    const missing = vi.fn();
    const translate = createTranslate(catalog("de", { common: {} }), ENGLISH, missing);
    expect(translate("common.cancel")).toBe("Cancel");
    expect(translate("pane.items", { count: 1 })).toBe("1 item");
    expect(missing).toHaveBeenCalledWith("common.cancel", "de");
    expect(missing).toHaveBeenCalledWith("pane.items", "de");
  });

  it("puts elements into a sentence without breaking it apart", () => {
    const translate = createTranslate(ENGLISH, ENGLISH);
    const bold = createElement("strong", null, "notes.txt");
    const parts = translate.rich("dialogs.alreadyExistsIn", { name: bold, folder: "/srv" });
    expect(Array.isArray(parts)).toBe(true);
    const nodes = parts as ReactElement<{ children: unknown }>[];
    const shown = nodes.map((node) => (isValidElement(node) ? node.props.children : node));
    expect(shown).toEqual(["", bold, " already exists in ", "/srv", "."]);
  });

  it("tells which language it writes", () => {
    expect(createTranslate(catalog("ja", {}), ENGLISH).locale).toBe("ja");
  });
});

describe("placeholdersOf", () => {
  it("lists each placeholder once, in every plural form", () => {
    expect(placeholdersOf("{b} and {a} and {b}")).toEqual(["a", "b"]);
    expect(placeholdersOf({ one: "{count} file in {folder}", other: "{count} files" })).toEqual([
      "count",
      "folder",
    ]);
    expect(placeholdersOf("no {1} or { spaced } names")).toEqual([]);
  });
});

describe("translatorFor", () => {
  it("warns once for each key a language lacks while developing", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const partial = catalog("de-AT", { common: { cancel: "Abbrechen" } });
    const translate = translatorFor(partial);
    expect(translate("common.cancel")).toBe("Abbrechen");
    expect(translate("common.close")).toBe("Close");
    expect(translate("common.close")).toBe("Close");
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalledWith("No de-AT text for common.close; showing the English.");
    expect(translatorFor(partial)).toBe(translate);
    warn.mockRestore();
  });
});
