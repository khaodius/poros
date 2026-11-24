import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, sanitizeSettings } from "../lib/settings";
import { formatDate, formatDuration, formatPercent, formatSize, formatSpeed } from "../lib/format";
import type { Translation } from "./catalog";
import {
  dateTimeFormat,
  dateWordFormat,
  ENGLISH,
  formatList,
  formatLocale,
  formatLocaleFor,
  formatNumber,
  midSentence,
  resolveLanguage,
  sanitizeLanguage,
  setLanguage,
  SYSTEM_LANGUAGE,
  translationForSystem,
  useLanguageStore,
} from "./index";
import { weekdayName } from "../lib/schedule";
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

/** The computer's language, as the system reports it to the page. */
function onComputerIn(language: string) {
  vi.stubGlobal("navigator", { language });
}

afterEach(() => vi.unstubAllGlobals());

describe("formats in the active language", () => {
  beforeEach(() => onComputerIn("en-US"));

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

describe("formatLocaleFor", () => {
  it("keeps the computer's formats under the English interface", () => {
    expect(formatLocaleFor("en", "en-GB")).toBe("en-GB");
    expect(formatLocaleFor("en", "it-IT")).toBe("it-IT");
    expect(formatLocaleFor("en", "sv-se")).toBe("sv-SE");
  });

  it("keeps the computer's region when it speaks the interface's language", () => {
    expect(formatLocaleFor("de", "de-AT")).toBe("de-AT");
    expect(formatLocaleFor("de", "de-CH")).toBe("de-CH");
    expect(formatLocaleFor("zh-Hans", "zh-CN")).toBe("zh-CN");
  });

  it("uses the translation's formats on a computer in another language or script", () => {
    expect(formatLocaleFor("de", "en-US")).toBe("de");
    expect(formatLocaleFor("zh-Hans", "zh-TW")).toBe("zh-Hans");
  });

  it("uses the translation's formats when the computer gives no usable tag", () => {
    expect(formatLocaleFor("en", undefined)).toBe("en");
    expect(formatLocaleFor("de", "")).toBe("de");
    expect(formatLocaleFor("en", "C")).toBe("en");
  });
});

describe("formats of the user's region", () => {
  const moment = new Date(2026, 4, 14, 18, 30).getTime() / 1000;

  it("writes British dates under the English interface on an en-GB computer", () => {
    onComputerIn("en-GB");
    setLanguage("en");
    expect(formatLocale()).toBe("en-GB");
    expect(formatDate(moment, "locale")).toBe("14 May 2026, 18:30");
    expect(formatNumber(1234567.5)).toBe("1,234,567.5");
    // Words follow the interface.
    expect(formatList(["a", "b", "c"])).toBe("a, b, and c");
  });

  it("writes Italian numbers and dates under the English interface on an it-IT computer", () => {
    onComputerIn("it-IT");
    setLanguage("en");
    expect(formatNumber(1234567.5)).toBe("1.234.567,5");
    expect(formatSize(1536)).toBe("1,5 KB");
    expect(formatDate(moment, "locale")).toBe("14 mag 2026, 18:30");
    // Italian groups numbers from five digits on.
    expect(useLanguageStore.getState().translate("drives.driveCount", { count: 12345 })).toBe(
      "12.345 drives",
    );
    expect(weekdayName(0)).toBe("Mon");
  });

  it("writes Austrian formats under German on a de-AT computer, words in German", () => {
    onComputerIn("de-AT");
    show(GERMAN);
    expect(formatLocale()).toBe("de-AT");
    expect(formatNumber(1234567.5)).toBe(new Intl.NumberFormat("de-AT").format(1234567.5));
    expect(dateTimeFormat({ dateStyle: "medium" }).resolvedOptions().locale).toBe("de-AT");
    expect(dateWordFormat({ weekday: "short" }).resolvedOptions().locale).toBe("de");
    expect(formatList(["a", "b", "c"])).toBe("a, b und c");
  });
});
