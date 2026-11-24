import { describe, expect, it } from "vitest";
import type { Translation } from "./catalog";
import { checkTranslation, KNOWN_PLURAL_CATEGORIES, pluralCategoriesOf } from "./check";
import { ENGLISH, TRANSLATIONS } from "./index";

/** A copy of English under another tag, which each case below spoils in one way. */
function copyOfEnglish(locale: string): {
  translation: Translation;
  messages: Record<string, Record<string, unknown>>;
} {
  const messages = JSON.parse(JSON.stringify(ENGLISH.messages)) as Record<
    string,
    Record<string, unknown>
  >;
  return {
    translation: { locale, name: "Test", messages } as unknown as Translation,
    messages,
  };
}

describe("shipped catalogs", () => {
  it("ships English", () => {
    expect(TRANSLATIONS).toContain(ENGLISH);
  });

  it.each(TRANSLATIONS.map((translation) => [translation.locale, translation] as const))(
    "%s fits the English catalog",
    (_locale, translation) => {
      expect(checkTranslation(translation, ENGLISH)).toEqual([]);
    },
  );

  it("names every language in itself and gives every tag once", () => {
    const tags = TRANSLATIONS.map((translation) => translation.locale.toLowerCase());
    expect(new Set(tags).size).toBe(tags.length);
    for (const translation of TRANSLATIONS) expect(translation.name.trim()).not.toBe("");
  });
});

describe("checkTranslation", () => {
  it("passes a faithful copy", () => {
    expect(checkTranslation(copyOfEnglish("en-GB").translation, ENGLISH)).toEqual([]);
  });

  it("finds a missing key", () => {
    const { translation, messages } = copyOfEnglish("en-GB");
    delete messages.common.cancel;
    expect(checkTranslation(translation, ENGLISH)).toEqual(["common.cancel: missing"]);
  });

  it("finds an extra key and an extra area", () => {
    const { translation, messages } = copyOfEnglish("en-GB");
    messages.common.leftOver = "Left over";
    messages.nowhere = { text: "Text" };
    expect(checkTranslation(translation, ENGLISH)).toEqual([
      "nowhere: an area English does not have",
      "common.leftOver: a key English does not have",
    ]);
  });

  it("finds a placeholder that differs", () => {
    const { translation, messages } = copyOfEnglish("en-GB");
    messages.dialogs.alreadyExistsIn = "{name} already exists in {directory}.";
    expect(checkTranslation(translation, ENGLISH)).toEqual([
      "dialogs.alreadyExistsIn: placeholders {directory}, {name}; English has {folder}, {name}",
    ]);
  });

  it("finds a plural written as one text, and one text written as a plural", () => {
    const { translation, messages } = copyOfEnglish("en-GB");
    messages.pane.items = "{count} items";
    messages.common.cancel = { one: "Cancel", other: "Cancel" };
    expect(checkTranslation(translation, ENGLISH)).toEqual([
      "common.cancel: must not be a plural",
      "pane.items: must be a plural",
    ]);
  });

  it("finds plural forms that do not fit the language", () => {
    const french = copyOfEnglish("fr");
    // French also has a form for millions: "1 000 000 de fichiers".
    expect(checkTranslation(french.translation, ENGLISH)).toContain(
      "pane.items: plural forms one, other; fr uses one, many, other",
    );
    const japanese = copyOfEnglish("ja");
    expect(checkTranslation(japanese.translation, ENGLISH)).toContain(
      "pane.items: plural forms one, other; ja uses other",
    );
  });

  it("asks for the count in the general plural form", () => {
    const { translation, messages } = copyOfEnglish("en-GB");
    messages.pane.items = { one: "One item", other: "Some items" };
    expect(checkTranslation(translation, ENGLISH)).toEqual([
      'pane.items: the "other" form must show {count}',
    ]);
  });

  it("finds a brace that is not a placeholder", () => {
    const { translation, messages } = copyOfEnglish("en-GB");
    messages.common.cancel = "Cancel {";
    expect(checkTranslation(translation, ENGLISH)).toEqual([
      "common.cancel: a brace that is not a placeholder",
    ]);
  });

  it("refuses a tag Intl does not know", () => {
    expect(checkTranslation(copyOfEnglish("not a tag").translation, ENGLISH)).toEqual([
      "not a tag: not a language tag Intl knows",
    ]);
  });
});

describe("pluralCategoriesOf", () => {
  it("knows the forms of the languages Poros is translated into", () => {
    expect(pluralCategoriesOf("en")).toEqual(["one", "other"]);
    expect(pluralCategoriesOf("de")).toEqual(["one", "other"]);
    expect(pluralCategoriesOf("es")).toEqual(["one", "many", "other"]);
    expect(pluralCategoriesOf("fr")).toEqual(["one", "many", "other"]);
    expect(pluralCategoriesOf("ja")).toEqual(["other"]);
    expect(pluralCategoriesOf("zh-Hans")).toEqual(["other"]);
  });

  it("asks the engine about languages it does not know", () => {
    expect(pluralCategoriesOf("en-GB")).toEqual(["one", "other"]);
  });

  it.each(Object.entries(KNOWN_PLURAL_CATEGORIES))(
    "agrees with the engine's rules for %s",
    (locale, known) => {
      const rules = new Intl.PluralRules(locale);
      // An older ICU may know fewer forms, never others.
      for (const category of rules.resolvedOptions().pluralCategories) {
        expect(known).toContain(category);
      }
      const samples = [0, 1, 2, 5, 21, 100, 1000, 1_000_000, 0.5, 1.5];
      for (const count of samples) expect(known).toContain(rules.select(count));
    },
  );
});
