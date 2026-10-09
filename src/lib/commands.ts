// Commands run on the server from the context menu, with placeholders filled in from the
// selection and quoted for a POSIX shell.

import { formatDuration } from "./format";
import type { CommandResult } from "./types";

export interface CommandTarget {
  /** The folder the pane shows, where the command runs. */
  folder: string;
  /** The selected items, or the one right-clicked. */
  items: { path: string; name: string }[];
}

export const PLACEHOLDERS: { placeholder: string; meaning: string }[] = [
  { placeholder: "{path}", meaning: "each selected item's full path, running once per item" },
  { placeholder: "{name}", meaning: "each selected item's name, running once per item" },
  { placeholder: "{paths}", meaning: "the full paths of all selected items" },
  { placeholder: "{names}", meaning: "the names of all selected items" },
  { placeholder: "{folder}", meaning: "the folder shown" },
];

const PLACEHOLDER_PATTERN = /\{(path|name|paths|names|folder)\}/g;
const PER_ITEM_PATTERN = /\{(path|name)\}/;
const SELECTION_PATTERN = /\{(path|name|paths|names)\}/;

/** Quotes text as a single word for sh, bash and other POSIX shells. */
export function shellQuote(text: string): string {
  return `'${text.split("'").join(`'\\''`)}'`;
}

/** The command works on selected items, so it cannot run without any. */
export function needsSelection(template: string): boolean {
  return SELECTION_PATTERN.test(template);
}

/** The commands to run: one per item when the template has {path} or {name}, else one. */
export function expandCommand(template: string, target: CommandTarget): string[] {
  const allPaths = target.items.map((item) => shellQuote(item.path)).join(" ");
  const allNames = target.items.map((item) => shellQuote(item.name)).join(" ");
  const fill = (item?: CommandTarget["items"][number]) =>
    template.replace(PLACEHOLDER_PATTERN, (_match, key: string) => {
      switch (key) {
        case "path":
          return item ? shellQuote(item.path) : "";
        case "name":
          return item ? shellQuote(item.name) : "";
        case "paths":
          return allPaths;
        case "names":
          return allNames;
        default:
          return shellQuote(target.folder);
      }
    });
  return PER_ITEM_PATTERN.test(template) ? target.items.map(fill) : [fill()];
}

export function commandSucceeded(result: CommandResult): boolean {
  return !result.stopped && result.signal === null && (result.exitStatus ?? 0) === 0;
}

/** How a command ended, as the backend logs it. */
export function describeResult(result: CommandResult): string {
  const elapsed = formatDuration(result.elapsedMillis / 1000);
  if (result.stopped) return `Stopped after ${elapsed}`;
  if (result.signal !== null) return `Ended by signal ${result.signal} after ${elapsed}`;
  if ((result.exitStatus ?? 0) !== 0)
    return `Exited with status ${result.exitStatus} after ${elapsed}`;
  return `Finished in ${elapsed}`;
}
