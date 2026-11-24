// The shape every catalog has. English is the source: its modules fix the keys, which keys are
// plural, and the placeholders each message takes. Every other language has the same keys.

import type { english } from "./en";

/** The plural categories `Intl.PluralRules` can pick, in the order CLDR lists them. */
export const PLURAL_CATEGORIES = ["zero", "one", "two", "few", "many", "other"] as const;
export type PluralCategory = (typeof PLURAL_CATEGORIES)[number];

/**
 * A message that follows a number, `{count}`: one text for each plural category the language
 * uses. Every language has `other`; English adds `one`, Japanese and Chinese need nothing else.
 */
export type PluralMessage = { readonly other: string } & {
  readonly [Category in Exclude<PluralCategory, "other">]?: string;
};

export type Message = string | PluralMessage;

/** One area of the interface: its keys and their messages. */
export type MessageArea = Readonly<Record<string, Message>>;

export type EnglishMessages = typeof english;
export type AreaName = keyof EnglishMessages;

/** An area in another language: every English key, a plural wherever the English is one. */
export type TranslatedArea<Area extends AreaName> = {
  readonly [Key in keyof EnglishMessages[Area]]: EnglishMessages[Area][Key] extends string
    ? string
    : PluralMessage;
};

export type TranslatedMessages = { readonly [Area in AreaName]: TranslatedArea<Area> };

export interface Translation {
  /** A BCP 47 tag `Intl` understands, such as `de` or `zh-Hans`. */
  readonly locale: string;
  /** The language's name in itself, as the Language list shows it. */
  readonly name: string;
  readonly messages: TranslatedMessages;
}

/** Checks a language's catalog against the English one as it is written. */
export function defineTranslation(translation: Translation): Translation {
  return translation;
}

/** `area.key`, for every message there is. */
export type MessageKey = {
  [Area in AreaName]: `${Area}.${keyof EnglishMessages[Area] & string}`;
}[AreaName];

type EnglishMessage<Key extends MessageKey> =
  Key extends `${infer Area extends AreaName}.${infer Name}`
    ? EnglishMessages[Area][Name & keyof EnglishMessages[Area]]
    : never;

type PlaceholdersInText<Text> = Text extends `${string}{${infer Name}}${infer Rest}`
  ? Name | PlaceholdersInText<Rest>
  : never;

type PlaceholdersIn<Source> = Source extends string
  ? PlaceholdersInText<Source>
  : "count" | PlaceholdersInText<Source[keyof Source]>;

export type Placeholder<Key extends MessageKey> = PlaceholdersIn<EnglishMessage<Key>>;

/** What a placeholder takes: numbers are written the way the language writes them. */
export type ParamValue = string | number;

export type MessageParams<Key extends MessageKey, Value = ParamValue> = {
  readonly [Name in Placeholder<Key>]: Name extends "count" ? number : Value;
};

/** No second argument for a message without placeholders, and a required one otherwise. */
export type MessageArguments<Key extends MessageKey, Value = ParamValue> = [
  Placeholder<Key>,
] extends [never]
  ? []
  : [params: MessageParams<Key, Value>];
