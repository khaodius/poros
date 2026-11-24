// What makes a catalog fit the English one: the same areas and keys, a plural wherever the
// English has one, the same placeholders, and the plural forms its own language uses. The
// catalog test runs it on every shipped language; a translator can run it on theirs alone.

import { PLURAL_CATEGORIES, type Message, type PluralCategory, type Translation } from "./catalog";
import { placeholdersOf } from "./runtime";

type Messages = Readonly<Record<string, Readonly<Record<string, Message>>>>;

/** A brace that opens no `{name}` placeholder, which the runtime would show as written. */
const STRAY_BRACE = /\{(?![A-Za-z][A-Za-z0-9]*\})|(?<!\{[A-Za-z][A-Za-z0-9]*)\}/;

/** The plural forms `locale` picks from, in CLDR's order. */
export function pluralCategoriesOf(locale: string): PluralCategory[] {
  const used = new Intl.PluralRules(locale).resolvedOptions().pluralCategories;
  return PLURAL_CATEGORIES.filter((category) => used.includes(category));
}

function isPlural(message: Message): message is Exclude<Message, string> {
  return typeof message === "object" && message !== null;
}

function textsOf(message: Message): string[] {
  return typeof message === "string" ? [message] : Object.values(message);
}

/** Placeholders other than `count`, which a plural form may leave out, as English's "one" does. */
function namedPlaceholders(message: Message): string[] {
  return placeholdersOf(message).filter((name) => !isPlural(message) || name !== "count");
}

function sameNames(first: string[], second: string[]): boolean {
  return first.length === second.length && first.every((name, index) => name === second[index]);
}

/** Every way `translation` does not fit `english`, one line each; empty when it fits. */
export function checkTranslation(translation: Translation, english: Translation): string[] {
  const problems: string[] = [];
  const { locale } = translation;
  try {
    if (Intl.getCanonicalLocales(locale).length !== 1) problems.push(`${locale}: not one tag`);
  } catch {
    problems.push(`${locale}: not a language tag Intl knows`);
    return problems;
  }
  if (!translation.name.trim()) problems.push(`${locale}: the language has no name`);

  const categories = pluralCategoriesOf(locale);
  const theirs = translation.messages as unknown as Messages;
  const ours = english.messages as unknown as Messages;

  for (const area of Object.keys(theirs)) {
    if (!(area in ours)) problems.push(`${area}: an area English does not have`);
  }
  for (const [area, sources] of Object.entries(ours)) {
    const messages = theirs[area] ?? {};
    for (const key of Object.keys(messages)) {
      if (!(key in sources)) problems.push(`${area}.${key}: a key English does not have`);
    }
    for (const [key, source] of Object.entries(sources)) {
      const id = `${area}.${key}`;
      if (!Object.prototype.hasOwnProperty.call(messages, key)) {
        problems.push(`${id}: missing`);
        continue;
      }
      const message = messages[key];
      if (isPlural(source) !== isPlural(message)) {
        problems.push(`${id}: ${isPlural(source) ? "must be" : "must not be"} a plural`);
        continue;
      }
      if (isPlural(message)) {
        const given = PLURAL_CATEGORIES.filter((category) => category in message);
        if (!sameNames(given, categories)) {
          problems.push(
            `${id}: plural forms ${given.join(", ")}; ${locale} uses ${categories.join(", ")}`,
          );
        }
      }
      if (isPlural(source) && isPlural(message)) {
        const counted = placeholdersOf(source.other).includes("count");
        if (counted && !placeholdersOf(message.other).includes("count")) {
          problems.push(`${id}: the "other" form must show {count}`);
        }
      }
      const wanted = namedPlaceholders(source);
      const found = namedPlaceholders(message);
      if (!sameNames(wanted, found)) {
        problems.push(
          `${id}: placeholders {${found.join("}, {")}}; English has {${wanted.join("}, {")}}`,
        );
      }
      if (textsOf(message).some((text) => text.trim() === "")) problems.push(`${id}: empty`);
      if (textsOf(message).some((text) => STRAY_BRACE.test(text))) {
        problems.push(`${id}: a brace that is not a placeholder`);
      }
    }
  }
  return problems;
}
