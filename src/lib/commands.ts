// Commands run on the server from the context menu, with placeholders filled in from the
// selection and quoted for a POSIX shell, so that no file name can act as an option.

import { formatDuration } from "./format";
import type { CommandResult } from "./types";
import { t } from "../i18n";

export interface CommandTarget {
  /** The folder the pane shows, where the command runs. */
  folder: string;
  /** The selected items, or the one right-clicked. */
  items: { path: string; name: string }[];
}

export const PLACEHOLDERS: { placeholder: string; meaning: string }[] = [
  {
    placeholder: "{path}",
    get meaning() {
      return t("automation.eachSelectedItemsFullPath");
    },
  },
  {
    placeholder: "{name}",
    get meaning() {
      return t("automation.eachSelectedItemsNameRunning");
    },
  },
  {
    placeholder: "{paths}",
    get meaning() {
      return t("automation.theFullPathsOfAll");
    },
  },
  {
    placeholder: "{names}",
    get meaning() {
      return t("automation.theNamesOfAllSelected");
    },
  },
  {
    placeholder: "{folder}",
    get meaning() {
      return t("automation.theFolderShown");
    },
  },
];

const PLACEHOLDER_PATTERN = /\{(path|name|paths|names|folder)\}/g;
const PER_ITEM_PATTERN = /\{(path|name)\}/;
const SELECTION_PATTERN = /\{(path|name|paths|names)\}/;

/** Quotes text as a single word for sh, bash and other POSIX shells. */
export function shellQuote(text: string): string {
  return `'${text.split("'").join(`'\\''`)}'`;
}

/**
 * Quotes a file name or path as one shell word that commands cannot read as an option: one that
 * starts with a dash gets `./`, which names the same item in the folder the command runs in.
 */
export function shellArgument(text: string): string {
  return shellQuote(text.startsWith("-") ? `./${text}` : text);
}

/** The command works on selected items, so it cannot run without any. */
export function needsSelection(template: string): boolean {
  return SELECTION_PATTERN.test(template);
}

/** The commands to run: one per item when the template has {path} or {name}, else one. */
export function expandCommand(template: string, target: CommandTarget): string[] {
  const allPaths = target.items.map((item) => shellArgument(item.path)).join(" ");
  const allNames = target.items.map((item) => shellArgument(item.name)).join(" ");
  const fill = (item?: CommandTarget["items"][number]) =>
    template.replace(PLACEHOLDER_PATTERN, (_match, key: string) => {
      switch (key) {
        case "path":
          return item ? shellArgument(item.path) : "";
        case "name":
          return item ? shellArgument(item.name) : "";
        case "paths":
          return allPaths;
        case "names":
          return allNames;
        default:
          return shellArgument(target.folder);
      }
    });
  return PER_ITEM_PATTERN.test(template) ? target.items.map(fill) : [fill()];
}

/** A command with no exit status, as after a dropped connection, is not known to have worked. */
export function commandSucceeded(result: CommandResult): boolean {
  return !result.stopped && result.signal === null && result.exitStatus === 0;
}

/** How a command ended, as the backend logs it. */
export function describeResult(result: CommandResult): string {
  const elapsed = formatDuration(result.elapsedMillis / 1000);
  if (result.stopped) return t("automation.stoppedAfter", { elapsed });
  if (result.signal !== null) {
    return t("automation.endedBySignal", { signal: result.signal, elapsed });
  }
  if (result.exitStatus === null) return t("automation.endedWithoutStatus", { elapsed });
  if (result.exitStatus !== 0) {
    return t("automation.exitedWithStatus", { status: String(result.exitStatus), elapsed });
  }
  return t("automation.finishedIn", { elapsed });
}
