import { afterEach, describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, sanitizeSettings } from "../lib/settings";
import { formatDate, formatDuration, formatPercent, formatSize, formatSpeed } from "../lib/format";
import type { Translation } from "./catalog";
import {
  ENGLISH,
  formatList,
  formatNumber,
  midSentence,
  resolveLanguage,
  sanitizeLanguage,
  setLanguage,
  SYSTEM_LANGUAGE,
  translationForSystem,
  useLanguageStore,
} from "./index";
import { createTranslate } from "./runtime";

function catalog(locale: string, name: string): Translation {
  return { locale, name, messages: {} } as unknown as Translation;
}

const GERMAN = catalog("de", "Deutsch");
const SPANISH = catalog("es", "Español");
const SIMPLIFIED_CHINESE = catalog("zh-Hans", "简体中文");
const SHIPPED_ELSEWHERE = [ENGLISH, GERMAN, SPANISH, SIMPLIFIED_CHINESE];

/** Shows the interface in `translation`, as picking it in Settings would. */
function show(translation: Translation) {
  useLanguageStore.setState({
    choice: translation.locale,
    translation,
    translate: createTranslate(translation, ENGLISH),
  });
}

afterEach(() => setLanguage(SYSTEM_LANGUAGE));

describe("translationForSystem", () => {
  it("finds a shipped language for the computer's own", () => {
    expect(translationForSystem("de-AT", SHIPPED_ELSEWHERE)).toBe(GERMAN);
    expect(translationForSystem("es-419", SHIPPED_ELSEWHERE)).toBe(SPANISH);
    expect(translationForSystem("zh-CN", SHIPPED_ELSEWHERE)).toBe(SIMPLIFIED_CHINESE);
    expect(translationForSystem("ZH-HANS", SHIPPED_ELSEWHERE)).toBe(SIMPLIFIED_CHINESE);
  });

  it("falls back to English for a language or script not shipped", () => {
    expect(translationForSystem("zh-TW", SHIPPED_ELSEWHERE)).toBe(ENGLISH);
    expect(translationForSystem("pt-BR", SHIPPED_ELSEWHERE)).toBe(ENGLISH);
    expect(translationForSystem("not a tag", SHIPPED_ELSEWHERE)).toBe(ENGLISH);
    expect(translationForSystem(undefined, SHIPPED_ELSEWHERE)).toBe(ENGLISH);
  });
});

describe("resolveLanguage", () => {
  it("shows the system's language for System and for a language no longer shipped", () => {
    expect(resolveLanguage(SYSTEM_LANGUAGE, "en-US")).toBe(ENGLISH);
    expect(resolveLanguage("xx", "en-US")).toBe(ENGLISH);
    expect(resolveLanguage("en", "de-DE")).toBe(ENGLISH);
  });
});

describe("sanitizeLanguage", () => {
  it("keeps System and the tags of shipped languages, as they write them", () => {
    expect(sanitizeLanguage("system")).toBe("system");
    expect(sanitizeLanguage("en")).toBe("en");
    expect(sanitizeLanguage("EN")).toBe("en");
  });

  it("turns anything else into System", () => {
    for (const stored of [undefined, null, 42, "", "xx", "<script>", { locale: "en" }]) {
      expect(sanitizeLanguage(stored)).toBe("system");
    }
  });
});

describe("the Language setting", () => {
  it("follows the system in a settings file from 0.7.0, which has no language", () => {
    const stored = { interface: { showHiddenFiles: true, dateFormat: "seconds" } };
    const settings = sanitizeSettings(stored);
    expect(settings.interface.language).toBe("system");
    expect(settings.interface.showHiddenFiles).toBe(true);
    expect(sanitizeSettings({}).interface.language).toBe("system");
    expect(DEFAULT_SETTINGS.interface.language).toBe("system");
  });

  it("keeps a shipped language and drops an unknown one", () => {
    expect(sanitizeSettings({ interface: { language: "en" } }).interface.language).toBe("en");
    expect(sanitizeSettings({ interface: { language: "tlh" } }).interface.language).toBe("system");
    expect(sanitizeSettings({ interface: { language: 7 } }).interface.language).toBe("system");
  });

  it("applies a shipped language when it is picked", () => {
    setLanguage("en");
    expect(useLanguageStore.getState().choice).toBe("en");
    expect(useLanguageStore.getState().translation).toBe(ENGLISH);
  });
});

describe("formats in the active language", () => {
  it("writes numbers, sizes, speeds and percentages in English", () => {
    show(ENGLISH);
    expect(formatNumber(1234567.5)).toBe("1,234,567.5");
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(5 * 1024 ** 3)).toBe("5.0 GB");
    expect(formatSpeed(2048)).toBe("2.0 KB/s");
    expect(formatPercent(42)).toBe("42%");
    expect(formatList(["a", "b", "c"])).toBe("a, b, and c");
  });

  it("writes them in German, units unchanged", () => {
    show(GERMAN);
    expect(formatNumber(1234567.5)).toBe("1.234.567,5");
    expect(formatSize(1536)).toBe("1,5 KB");
    expect(formatSize(5 * 1024 ** 3)).toBe("5,0 GB");
    expect(formatSpeed(2048)).toBe("2,0 KB/s");
    expect(formatPercent(42)).toBe("42\u00a0%");
    expect(formatList(["a", "b", "c"])).toBe("a, b und c");
  });

  it("writes dates in the order and words of the language", () => {
    const moment = new Date(2026, 4, 14, 18, 30).getTime() / 1000;
    show(ENGLISH);
    // ICU puts a narrow or a plain space before PM, depending on its version.
    expect(formatDate(moment, "locale")).toMatch(/^May 14, 2026, 6:30\s+PM$/);
    show(GERMAN);
    expect(formatDate(moment, "locale")).toBe("14.05.2026, 18:30");
    // The fixed formats stay as they are in every language.
    expect(formatDate(moment, "minutes")).toBe("2026-05-14 18:30");
  });

  it("writes durations with the language's units", () => {
    show(ENGLISH);
    expect(formatDuration(3725)).toBe("1h 2m");
    expect(formatDuration(65)).toBe("1m 5s");
  });

  it("lowercases labels inside sentences only where the language does", () => {
    show(ENGLISH);
    expect(midSentence("Files only")).toBe("files only");
    show(GERMAN);
    expect(midSentence("Nur Dateien")).toBe("Nur Dateien");
  });
});
