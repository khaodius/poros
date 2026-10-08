// Picks syntax highlighting for a file by its name. The language packages load on demand.

import { LanguageDescription } from "@codemirror/language";
import { languages } from "@codemirror/language-data";

/** Names the language list does not know, mostly the config files servers are full of. */
const NAME_FALLBACKS: [RegExp, string][] = [
  [/^\.(bash|zsh|ksh)?(rc|_profile|_login|_logout|_aliases|env)$|^\.?profile$|\.zsh$/i, "Shell"],
  [
    /\.(conf|cfg|cnf|env|service|socket|timer|mount|target|desktop)$|^\.env(\..+)?$/i,
    "Properties files",
  ],
  [/^(Containerfile|Dockerfile\..+)$/, "Dockerfile"],
];

export function languageFor(fileName: string): LanguageDescription | null {
  const known = LanguageDescription.matchFilename(languages, fileName);
  if (known) return known;
  const fallback = NAME_FALLBACKS.find(([pattern]) => pattern.test(fileName));
  return fallback ? LanguageDescription.matchLanguageName(languages, fallback[1], false) : null;
}
