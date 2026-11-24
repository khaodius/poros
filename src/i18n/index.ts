// The interface's language: which catalog is active, `t` and `useT` to write its messages, and
// the `Intl` formats that go with it. Changing the language re-renders every component that
// called `useT`, so the whole window changes at once without a restart.

import { create } from "zustand";
import type { Translation } from "./catalog";
import * as shipped from "./locales";
import { createTranslate, type Translate } from "./runtime";

export type { MessageKey, PlainMessageKey, Translation } from "./catalog";
export type { Translate } from "./runtime";

export const ENGLISH: Translation = shipped.en;

/** Every shipped language, in the order of their tags: Deutsch, English, Español and so on. */
export const TRANSLATIONS: readonly Translation[] = Object.values(shipped).sort((left, right) =>
  left.locale < right.locale ? -1 : 1,
);

/** The setting that follows the computer's language. */
export const SYSTEM_LANGUAGE = "system";

/** `system`, or the tag of a shipped language. */
export type LanguageChoice = string;

const REMEMBERED_LANGUAGE_KEY = "poros.language";

export function findTranslation(
  locale: string,
  translations: readonly Translation[] = TRANSLATIONS,
): Translation | undefined {
  const wanted = locale.toLowerCase();
  return translations.find((translation) => translation.locale.toLowerCase() === wanted);
}

/** Language and script, so that `zh-CN` finds `zh-Hans` but `zh-TW`, which is `zh-Hant`, does not. */
function languageAndScript(locale: string): string | null {
  try {
    const maximized = new Intl.Locale(locale).maximize();
    return `${maximized.language}-${maximized.script ?? ""}`;
  } catch {
    return null;
  }
}

/** The shipped language for a system locale such as `de-AT`, or English when there is none. */
export function translationForSystem(
  systemLocale: string | undefined,
  translations: readonly Translation[] = TRANSLATIONS,
): Translation {
  if (!systemLocale) return ENGLISH;
  const exact = findTranslation(systemLocale, translations);
  if (exact) return exact;
  const wanted = languageAndScript(systemLocale);
  return (
    translations.find(
      (translation) => wanted !== null && languageAndScript(translation.locale) === wanted,
    ) ?? ENGLISH
  );
}

function systemLocale(): string | undefined {
  return typeof navigator === "undefined" ? undefined : navigator.language;
}

/** The language a setting shows: its own, or the system's for `system` and unknown ones. */
export function resolveLanguage(
  choice: LanguageChoice,
  system: string | undefined = systemLocale(),
): Translation {
  return (
    (choice === SYSTEM_LANGUAGE ? undefined : findTranslation(choice)) ??
    translationForSystem(system)
  );
}

/** A stored language setting: `system`, or a shipped language's tag as that language writes it. */
export function sanitizeLanguage(stored: unknown): LanguageChoice {
  if (typeof stored !== "string") return SYSTEM_LANGUAGE;
  return findTranslation(stored)?.locale ?? SYSTEM_LANGUAGE;
}

const warned = new Set<string>();

function warnMissing(key: string, locale: string): void {
  const id = `${locale}:${key}`;
  if (warned.has(id)) return;
  warned.add(id);
  console.warn(`No ${locale} text for ${key}; showing the English.`);
}

const formatLocales = new Map<string, string>();

/**
 * The tag dates, times and numbers are written in while the interface shows `locale`: the
 * computer's own when the interface is in English or in the computer's language and script, so
 * an en-GB, it-IT or de-AT computer keeps its own formats, and the translation's tag otherwise.
 * Words, such as lists and plural forms, follow the translation whatever this gives.
 */
export function formatLocaleFor(locale: string, system: string | undefined): string {
  if (!system) return locale;
  const id = `${locale} ${system}`;
  let found = formatLocales.get(id);
  if (found === undefined) {
    found = locale;
    try {
      const [canonical] = Intl.getCanonicalLocales(system);
      const ownScript = languageAndScript(locale);
      const sameLanguage = ownScript !== null && languageAndScript(canonical) === ownScript;
      if (canonical && (locale === ENGLISH.locale || sameLanguage)) found = canonical;
    } catch {
      // Not a tag Intl knows: the translation's own formats.
    }
    formatLocales.set(id, found);
  }
  return found;
}

const translators = new Map<string, Translate>();

/** One translator per language, so its identity only changes with the language. */
export function translatorFor(translation: Translation): Translate {
  const numberLocale = formatLocaleFor(translation.locale, systemLocale());
  const id = `${translation.locale} ${numberLocale}`;
  let translate = translators.get(id);
  if (!translate) {
    translate = createTranslate(
      translation,
      ENGLISH,
      import.meta.env.DEV ? warnMissing : undefined,
      numberLocale,
    );
    translators.set(id, translate);
  }
  return translate;
}

interface LanguageState {
  choice: LanguageChoice;
  translation: Translation;
  translate: Translate;
}

export const useLanguageStore = create<LanguageState>(() => ({
  choice: SYSTEM_LANGUAGE,
  translation: ENGLISH,
  translate: translatorFor(ENGLISH),
}));

/**
 * The translator for components. It changes with the language, so a component re-renders then,
 * and memos and effects that list it in their dependencies run again.
 */
export function useT(): Translate {
  return useLanguageStore((state) => state.translate);
}

function active(): Translate {
  return useLanguageStore.getState().translate;
}

/** For code outside components, such as toasts and notifications: the language at the call. */
export const t: Translate = Object.defineProperty(
  Object.assign(
    (key: string, params?: Record<string, unknown>) =>
      (active() as unknown as (key: string, params?: Record<string, unknown>) => string)(
        key,
        params,
      ),
    {
      rich: (key: string, params?: Record<string, unknown>) =>
        (active().rich as unknown as (key: string, params?: Record<string, unknown>) => unknown)(
          key,
          params,
        ),
    },
  ),
  "locale",
  { get: () => active().locale },
) as unknown as Translate;

/** A label as it reads inside a sentence or a list, in the active language. */
export function midSentence(label: string): string {
  const { translation } = useLanguageStore.getState();
  return translation.lowercaseMidSentence ? label.toLocaleLowerCase(translation.locale) : label;
}

/** The tag of the language the interface shows, which its words and plurals follow: `de`. */
export function activeLocale(): string {
  return useLanguageStore.getState().translation.locale;
}

/** The tag dates, times and numbers follow, such as `de-AT`; see `formatLocaleFor`. */
export function formatLocale(): string {
  return formatLocaleFor(activeLocale(), systemLocale());
}

function remember(choice: LanguageChoice): void {
  try {
    if (localStorage.getItem(REMEMBERED_LANGUAGE_KEY) !== choice) {
      localStorage.setItem(REMEMBERED_LANGUAGE_KEY, choice);
    }
  } catch {
    // Storage can be unavailable; the next start then shows English until the settings load.
  }
}

/** Shows the interface in the language `choice` resolves to, in this window. */
export function setLanguage(choice: LanguageChoice): void {
  const translation = resolveLanguage(choice);
  if (typeof document !== "undefined") document.documentElement.lang = translation.locale;
  if (typeof localStorage !== "undefined") remember(choice);
  const state = useLanguageStore.getState();
  if (state.choice === choice && state.translation === translation) return;
  useLanguageStore.setState({ choice, translation, translate: translatorFor(translation) });
}

/** Applies the language the last run used, so the first frame is already in it. */
export function applyRememberedLanguage(): void {
  let remembered: string | null;
  try {
    remembered = localStorage.getItem(REMEMBERED_LANGUAGE_KEY);
  } catch {
    return;
  }
  setLanguage(sanitizeLanguage(remembered));
}

const numberFormats = new Map<string, Intl.NumberFormat>();

/** A number format as the user's region writes numbers, made once for each set of options. */
export function numberFormat(options: Intl.NumberFormatOptions = {}): Intl.NumberFormat {
  const locale = formatLocale();
  const id = `${locale} ${JSON.stringify(options)}`;
  let format = numberFormats.get(id);
  if (!format) {
    format = new Intl.NumberFormat(locale, options);
    numberFormats.set(id, format);
  }
  return format;
}

/** `1,234` in the United States, `1.234` in Germany. */
export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  return numberFormat(options).format(value);
}

const listFormats = new Map<string, Intl.ListFormat>();

/** `a, b, and c` in English, `a, b und c` in German: words, so in the interface's language. */
export function formatList(items: readonly string[], options?: Intl.ListFormatOptions): string {
  const locale = activeLocale();
  const id = `${locale} ${JSON.stringify(options ?? {})}`;
  let format = listFormats.get(id);
  if (!format) {
    format = new Intl.ListFormat(locale, options);
    listFormats.set(id, format);
  }
  return format.format(items);
}

const dateTimeFormats = new Map<string, Intl.DateTimeFormat>();

function cachedDateTimeFormat(
  locale: string,
  options: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormat {
  const id = `${locale} ${JSON.stringify(options)}`;
  let format = dateTimeFormats.get(id);
  if (!format) {
    format = new Intl.DateTimeFormat(locale, options);
    dateTimeFormats.set(id, format);
  }
  return format;
}

/** A date and time format as the user's region writes them, made once for each set of options. */
export function dateTimeFormat(options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  return cachedDateTimeFormat(formatLocale(), options);
}

/**
 * Names of weekdays and months on their own, as words a message puts in a sentence, so in the
 * interface's language: `Mon` and `May` in English, `Mo` and `Mai` in German.
 */
export function dateWordFormat(options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  return cachedDateTimeFormat(activeLocale(), options);
}
