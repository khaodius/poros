// Commands run on the server from the context menu. The server is sent the command with each
// placeholder turned into positional parameters ("${1}" and on) and the selected names apart, as
// the arguments, so that no shell ever reads a name as part of the command and no file name can
// act as an option. What the user sees is that command and what each parameter stands for.

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

/**
 * Said under the placeholders: each is one word to the command, but a shell the command starts
 * reads its own command string, names and all.
 */
export const PLACEHOLDER_ADVICE =
  'Keep placeholders out of a command string another shell reads, such as bash -c "..." or ssh host "...": that shell would read a name as part of its command.';

const PLACEHOLDER_PATTERN = /\{(path|name|paths|names|folder)\}/g;
const PER_ITEM_PATTERN = /\{(path|name)\}/;
const SELECTION_PATTERN = /\{(path|name|paths|names)\}/;

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

function startsWord(text: string, index: number): boolean {
  return index === 0 || BEFORE_WORD.test(text[index - 1]);
}

function isWord(text: string, index: number, word: string): boolean {
  if (!startsWord(text, index) || !text.startsWith(word, index)) return false;
  const after = text[index + word.length];
  return after === undefined || BEFORE_WORD.test(after);
}

function frame(kind: Frame["kind"]): Frame {
  return { kind, open: 0, cases: 0 };
}

/**
 * How `sh` reads each of `spots`, the placeholders in `template` in order: inside the user's
 * single quotes, inside double quotes, or neither, which is the command itself, `$(...)` or
 * backticks, inside double quotes or not.
 */
function quotingAt(template: string, spots: { index: number; length: number }[]): Quoting[] {
  const frames: Frame[] = [frame("command")];
  const found: Quoting[] = [];
  let single = false;
  let comment = false;
  let spot = 0;
  for (let index = 0; index < template.length; index++) {
    const top = frames[frames.length - 1];
    if (spots[spot]?.index === index) {
      found.push(single ? "single" : top.kind === "double" ? "double" : "none");
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
      if (spots[spot]?.index !== index + 1) index++;
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
    } else if (isWord(template, index, "case")) {
      top.cases++;
      index += "case".length - 1;
    } else if (isWord(template, index, "esac")) {
      top.cases = Math.max(0, top.cases - 1);
      index += "esac".length - 1;
    }
  }
  return found;
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
 */
export function prepareCommands(template: string, target: CommandTarget): PreparedCommand[] {
  const matches = [...template.matchAll(PLACEHOLDER_PATTERN)];
  const quoting = quotingAt(
    template,
    matches.map((match) => ({ index: match.index, length: match[0].length })),
  );
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
      command += template.slice(read, match.index);
      const key = match[1];
      let taken = references.get(key);
      if (!taken) {
        taken =
          key === "folder" && folderOnly
            ? ["$PWD"]
            : values(key).map((value) => `\${${parameters.push(value)}}`);
        references.set(key, taken);
      }
      command += reference(taken, quoting[index]);
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
  if (result.stopped) return `Stopped after ${elapsed}`;
  if (result.signal !== null) return `Ended by signal ${result.signal} after ${elapsed}`;
  if (result.exitStatus === null) return `Ended without an exit status after ${elapsed}`;
  if (result.exitStatus !== 0) return `Exited with status ${result.exitStatus} after ${elapsed}`;
  return `Finished in ${elapsed}`;
}
