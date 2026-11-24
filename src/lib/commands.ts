// Commands run on the server from the context menu. The server is sent the command with each
// placeholder turned into positional parameters ("${1}" and on) and the selected names apart, as
// the arguments, so that no shell ever reads a name as part of the command and no file name can
// act as an option. What the user sees is that command and what each parameter stands for.

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

/**
 * Said under the placeholders: each is one word to the command, but a shell the command starts
 * reads its own command string, names and all.
 */
export const PLACEHOLDER_ADVICE =
  'Keep placeholders out of a command string another shell reads, such as bash -c "..." or ssh host "...": that shell would read a name as part of its command.';

/**
 * Shown when a command could not be read with certainty, so its placeholders are all quoted the
 * one way that is right outside quotes.
 */
export const UNCERTAIN_COMMAND_WARNING =
  "Poros cannot tell for sure how the shell reads this command: it has a here-document, $'...', or a quote or bracket left open. Each placeholder goes in as \"${1}\" and so on, which keeps a name one word only outside quotes.";

const PLACEHOLDER_PATTERN = /\{(path|name|paths|names|folder)\}/g;
const PER_ITEM_PATTERN = /\{(path|name)\}/;
const SELECTION_PATTERN = /\{(path|name|paths|names)\}/;
/**
 * What the scan below does not follow: here-documents, whose bodies hold any quotes, and bash's
 * `$'...'`, in which `\'` does not end the quotes. `<<` also matches a shift in `$((...))`.
 */
const UNREAD_SYNTAX = /<<|\$'/;

/** Quotes text as a single word for sh, bash and other POSIX shells. */
export function shellQuote(text: string): string {
  return `'${text.split("'").join(`'\\''`)}'`;
}

/**
 * A file name or path as an argument that commands cannot read as an option: one that starts
 * with a dash gets `./`, which names the same item in the folder the command runs in.
 */
function safeArgument(text: string): string {
  return text.startsWith("-") ? `./${text}` : text;
}

/** The command works on selected items, so it cannot run without any. */
export function needsSelection(template: string): boolean {
  return SELECTION_PATTERN.test(template);
}

/** One run of a command. */
export interface PreparedCommand {
  /** What runs, with what each positional parameter stands for, to show. */
  display: string;
  /** What the server runs: the command with placeholders turned into positional parameters. */
  command: string;
  /** What the positional parameters stand for, the first for "${1}". */
  arguments: string[];
}

type Quoting = "none" | "single" | "double";

/** A part of a command that `sh` reads on its own terms. */
interface Frame {
  /** The command itself or `$(...)`; backticks; or double quotes. */
  kind: "command" | "backticks" | "double";
  /** Parentheses opened in it and not closed yet, such as a subshell's. */
  open: number;
  /** `case` statements begun in it and not ended yet, whose patterns end with `)`. */
  cases: number;
}

/** What can stand before a word: there `#` starts a comment, and `case` and `esac` are words. */
const BEFORE_WORD = /[\s;&|()`]/;

/**
 * `case WORD in`, which starts a case statement, whose patterns end with `)`. The word may be
 * quoted or hold `$(...)` or backticks; `case` followed by anything else is only a word.
 */
const CASE_STATEMENT =
  /case[ \t]+(?:[^\s;&|()<>`'"\\]|\\.|'[^']*'|"(?:[^"\\]|\\.)*"|`[^`]*`|\$\([^()]*\))+\s+in(?=[\s;&|()`]|$)/y;

function startsWord(text: string, index: number): boolean {
  return index === 0 || BEFORE_WORD.test(text[index - 1]);
}

function startsCase(text: string, index: number): boolean {
  if (!startsWord(text, index)) return false;
  CASE_STATEMENT.lastIndex = index;
  return CASE_STATEMENT.test(text);
}

function isWord(text: string, index: number, word: string): boolean {
  if (!startsWord(text, index) || !text.startsWith(word, index)) return false;
  const after = text[index + word.length];
  return after === undefined || BEFORE_WORD.test(after);
}

function frame(kind: Frame["kind"]): Frame {
  return { kind, open: 0, cases: 0 };
}

/** How `sh` reads one placeholder. */
interface Spot {
  quoting: Quoting;
  /** A backslash stands right before it, outside single quotes. */
  escaped: boolean;
}

/** How `sh` reads the placeholders of a template. */
interface Reading {
  spots: Spot[];
  /**
   * Whether the scan can be trusted: it ended outside every quote, `$(...)`, backticks,
   * parenthesis and `case`, and the template holds nothing it does not follow.
   */
  certain: boolean;
}

type Match = { index: number; length: number };

function spotsOf(template: string): Match[] {
  return [...template.matchAll(PLACEHOLDER_PATTERN)].map((match) => ({
    index: match.index,
    length: match[0].length,
  }));
}

/**
 * How `sh` reads each of `spots`, the placeholders in `template` in order: inside the user's
 * single quotes, inside double quotes, or neither, which is the command itself, `$(...)` or
 * backticks, inside double quotes or not.
 */
function quotingAt(template: string, spots: Match[]): Reading {
  const frames: Frame[] = [frame("command")];
  const found: Spot[] = [];
  let single = false;
  let comment = false;
  let escaped = false;
  let spot = 0;
  for (let index = 0; index < template.length; index++) {
    const top = frames[frames.length - 1];
    if (spots[spot]?.index === index) {
      const quoting = single ? "single" : top.kind === "double" ? "double" : "none";
      found.push({ quoting, escaped });
      escaped = false;
      index += spots[spot].length - 1;
      spot++;
      continue;
    }
    const character = template[index];
    if (comment) {
      comment = character !== "\n";
      continue;
    }
    if (single) {
      single = character !== "'";
      continue;
    }
    if (character === "\\") {
      if (spots[spot]?.index === index + 1) escaped = true;
      else index++;
      continue;
    }
    if (character === "`") {
      if (top.kind === "backticks") frames.pop();
      else frames.push(frame("backticks"));
    } else if (character === "$" && template[index + 1] === "(") {
      frames.push(frame("command"));
      index++;
    } else if (top.kind === "double") {
      if (character === '"') frames.pop();
    } else if (character === "'") {
      single = true;
    } else if (character === '"') {
      frames.push(frame("double"));
    } else if (character === "#" && startsWord(template, index)) {
      comment = true;
    } else if (character === "(") {
      top.open++;
    } else if (character === ")") {
      if (top.open > 0) top.open--;
      else if (top.cases === 0 && top.kind === "command" && frames.length > 1) frames.pop();
    } else if (startsCase(template, index)) {
      top.cases++;
      index += "case".length - 1;
    } else if (isWord(template, index, "esac")) {
      top.cases = Math.max(0, top.cases - 1);
      index += "esac".length - 1;
    }
  }
  const closed = !single && frames.length === 1 && frames[0].open === 0 && frames[0].cases === 0;
  return { spots: found, certain: closed && !UNREAD_SYNTAX.test(template) };
}

/**
 * Whether `template` has placeholders and could not be read with certainty, so that each is
 * quoted as `"${1}"` whatever stands around it.
 */
export function readsUncertainly(template: string): boolean {
  const spots = spotsOf(template);
  return spots.length > 0 && !quotingAt(template, spots).certain;
}

/**
 * Parameters as they belong where a placeholder stands: each quoted on its own, bare inside the
 * user's double quotes, and out of the user's single quotes.
 */
function reference(parameters: string[], quoting: Quoting): string {
  const quoted = parameters.map((parameter) => `"${parameter}"`).join(" ");
  if (quoting === "double") return parameters.join(" ");
  if (quoting === "single") return `'${quoted}'`;
  return quoted;
}

/** What runs, and what each positional parameter stands for. */
function describeCommand(command: string, values: string[]): string {
  if (values.length === 0) return command;
  const meanings = values.map((value, index) => `$${index + 1} = ${shellQuote(value)}`);
  return `${command} with ${meanings.join(", ")}`;
}

/**
 * The runs of a command: one per item when the template has {path} or {name}, else one. Each
 * value a placeholder stands for becomes one argument, so a name stays one word whatever it holds.
 * A command that names no selected item gets no arguments: it runs in the folder, so {folder} is
 * "$PWD" there, and the login shell runs it as it does a command without placeholders.
 *
 * A backslash right before a placeholder applies to the name's first character, as it would
 * were the name typed there: outside quotes it is dropped, and inside double quotes, where it
 * stays a backslash, it is doubled so it does not quote the `$`. A template that could not be
 * read with certainty gets every placeholder as "${1}" and so on, as outside quotes.
 */
export function prepareCommands(template: string, target: CommandTarget): PreparedCommand[] {
  const matches = [...template.matchAll(PLACEHOLDER_PATTERN)];
  const reading = quotingAt(template, spotsOf(template));
  const folderOnly = !needsSelection(template);
  const prepare = (item?: CommandTarget["items"][number]): PreparedCommand => {
    const values = (key: string): string[] => {
      switch (key) {
        case "path":
          return item ? [safeArgument(item.path)] : [];
        case "name":
          return item ? [safeArgument(item.name)] : [];
        case "paths":
          return target.items.map((each) => safeArgument(each.path));
        case "names":
          return target.items.map((each) => safeArgument(each.name));
        default:
          return [safeArgument(target.folder)];
      }
    };
    const parameters: string[] = [];
    const references = new Map<string, string[]>();
    let command = "";
    let read = 0;
    matches.forEach((match, index) => {
      const { quoting, escaped } = reading.spots[index];
      const where = reading.certain ? quoting : "none";
      const before = template.slice(read, match.index);
      if (!escaped || where === "single") command += before;
      else if (where === "none") command += before.slice(0, -1);
      else command += `${before}\\`;
      const key = match[1];
      let taken = references.get(key);
      if (!taken) {
        taken =
          key === "folder" && folderOnly
            ? ["$PWD"]
            : values(key).map((value) => `\${${parameters.push(value)}}`);
        references.set(key, taken);
      }
      command += reference(taken, where);
      read = match.index + match[0].length;
    });
    command += template.slice(read);
    return { display: describeCommand(command, parameters), command, arguments: parameters };
  };
  return PER_ITEM_PATTERN.test(template) ? target.items.map((item) => prepare(item)) : [prepare()];
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
