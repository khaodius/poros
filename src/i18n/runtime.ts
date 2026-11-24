import { createElement, Fragment, type ReactNode } from "react";
import type {
  Message,
  MessageArguments,
  MessageKey,
  ParamValue,
  PluralMessage,
  Translation,
} from "./catalog";

/** Writes a message of the active language, its placeholders filled in. */
export interface Translate {
  <Key extends MessageKey>(key: Key, ...params: MessageArguments<Key>): string;
  /** The same, with React elements allowed in the placeholders, such as a name set in bold. */
  rich<Key extends MessageKey>(key: Key, ...params: MessageArguments<Key, ReactNode>): ReactNode;
  /** The language the messages are in, which numbers and dates follow too. */
  readonly locale: string;
}

/** `{name}`: a letter, then letters and digits. Braces around anything else stay as written. */
const PLACEHOLDER = /\{([A-Za-z][A-Za-z0-9]*)\}/g;

type Params = Readonly<Record<string, unknown>>;
type Catalog = Readonly<Record<string, Readonly<Record<string, Message>>>>;

function own(catalog: Catalog, key: string): Message | undefined {
  const separator = key.indexOf(".");
  const area = catalog[key.slice(0, separator)];
  const name = key.slice(separator + 1);
  return area && Object.prototype.hasOwnProperty.call(area, name) ? area[name] : undefined;
}

/** The placeholder names a message uses, in every form for a plural one. */
export function placeholdersOf(message: Message): string[] {
  const texts = typeof message === "string" ? [message] : Object.values(message);
  const names = texts.flatMap((text) =>
    [...(text ?? "").matchAll(PLACEHOLDER)].map((match) => match[1]),
  );
  return [...new Set(names)].sort();
}

/**
 * The translator for one language. A key the language does not have yet is written in
 * `fallback` (English), and `onMissing` hears about it.
 */
export function createTranslate(
  translation: Translation,
  fallback: Translation,
  onMissing?: (key: string, locale: string) => void,
): Translate {
  const plurals = new Intl.PluralRules(translation.locale);
  const fallbackPlurals = new Intl.PluralRules(fallback.locale);
  const numbers = new Intl.NumberFormat(translation.locale);

  const textOf = (key: string, params: Params | undefined): string => {
    let message = own(translation.messages, key);
    let rules = plurals;
    if (message === undefined) {
      onMissing?.(key, translation.locale);
      message = own(fallback.messages, key) ?? key;
      rules = fallbackPlurals;
    }
    if (typeof message === "string") return message;
    const count = Number(params?.count ?? 0);
    return (message as PluralMessage)[rules.select(count)] ?? message.other;
  };

  const written = (value: unknown): unknown =>
    typeof value === "number" ? numbers.format(value) : value;

  const translate = (key: string, params?: Params): string => {
    const text = textOf(key, params);
    if (!params) return text;
    return text.replace(PLACEHOLDER, (whole, name: string) =>
      Object.prototype.hasOwnProperty.call(params, name)
        ? String(written(params[name] as ParamValue))
        : whole,
    );
  };

  const rich = (key: string, params?: Params): ReactNode => {
    const text = textOf(key, params);
    if (!params) return text;
    // Splitting on a pattern with a group keeps the names at the odd positions.
    const parts = text.split(PLACEHOLDER);
    return parts.map((part, index) => {
      if (index % 2 === 0) return part;
      const value = Object.prototype.hasOwnProperty.call(params, part)
        ? (written(params[part]) as ReactNode)
        : `{${part}}`;
      return createElement(Fragment, { key: index }, value);
    });
  };

  return Object.assign(translate, { rich, locale: translation.locale }) as unknown as Translate;
}
