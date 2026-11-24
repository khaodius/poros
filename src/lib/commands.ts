// Commands run on the server from the context menu. The server is sent the command with each
// placeholder turned into positional parameters ("${1}" and on) and the selected names apart, as
// the arguments, so that no shell ever reads a name as part of the command and no file name can
// act as an option. What the user sees is the command with the names filled in and quoted.

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

/**
 * A file name or path as an argument that commands cannot read as an option: one that starts
 * with a dash gets `./`, which names the same item in the folder the command runs in.
 */
function safeArgument(text: string): string {
  return text.startsWith("-") ? `./${text}` : text;
}

/** Quotes a file name or path as one shell word that commands cannot read as an option. */
export function shellArgument(text: string): string {
  return shellQuote(safeArgument(text));
}

/** The command works on selected items, so it cannot run without any. */
export function needsSelection(template: string): boolean {
  return SELECTION_PATTERN.test(template);
}

/** One run of a command. */
export interface PreparedCommand {
  /** The command with the names filled in and quoted, to show. */
  display: string;
  /** What the server runs: the command with placeholders turned into positional parameters. */
  command: string;
  /** What the positional parameters stand for, the first for "${1}". */
  arguments: string[];
}

type Quoting = "none" | "single" | "double";

/** Where `sh` is while it reads a command: inside nested `$(...)` and double quotes, or not. */
interface QuoteState {
  frames: ("command" | "double")[];
  single: boolean;
}

/** Moves `state` past `text`, as `sh` reads it. */
function readPast(state: QuoteState, text: string): void {
  for (let index = 0; index < text.length; index++) {
    const character = text[index];
    if (state.single) {
      if (character === "'") state.single = false;
      continue;
    }
    if (character === "\\") {
      index++;
      continue;
    }
    if (character === "$" && text[index + 1] === "(") {
      state.frames.push("command");
      index++;
      continue;
    }
    if (state.frames[state.frames.length - 1] === "double") {
      if (character === '"') state.frames.pop();
    } else if (character === "'") {
      state.single = true;
    } else if (character === '"') {
      state.frames.push("double");
    } else if (character === ")" && state.frames.length > 1) {
      state.frames.pop();
    }
  }
}

function quotingOf(state: QuoteState): Quoting {
  if (state.single) return "single";
  return state.frames[state.frames.length - 1] === "double" ? "double" : "none";
}

/**
 * Positional parameters `positions` as they belong where a placeholder stands: each quoted on
 * its own, bare inside the user's double quotes, and out of the user's single quotes.
 */
function reference(positions: number[], quoting: Quoting): string {
  const bare = positions.map((position) => `\${${position}}`);
  const quoted = bare.map((parameter) => `"${parameter}"`).join(" ");
  if (quoting === "double") return bare.join(" ");
  if (quoting === "single") return `'${quoted}'`;
  return quoted;
}

/**
 * The runs of a command: one per item when the template has {path} or {name}, else one. Each
 * value a placeholder stands for becomes one argument, so a name stays one word whatever it holds.
 */
export function prepareCommands(template: string, target: CommandTarget): PreparedCommand[] {
  const displays = expandCommand(template, target);
  const prepare = (display: string, item?: CommandTarget["items"][number]): PreparedCommand => {
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
    const positions = new Map<string, number[]>();
    const state: QuoteState = { frames: ["command"], single: false };
    let command = "";
    let read = 0;
    for (const match of template.matchAll(PLACEHOLDER_PATTERN)) {
      const before = template.slice(read, match.index);
      readPast(state, before);
      command += before;
      const key = match[1];
      let taken = positions.get(key);
      if (!taken) {
        taken = values(key).map((value) => parameters.push(value));
        positions.set(key, taken);
      }
      command += reference(taken, quotingOf(state));
      read = match.index + match[0].length;
    }
    command += template.slice(read);
    return { display, command, arguments: parameters };
  };
  return PER_ITEM_PATTERN.test(template)
    ? target.items.map((item, index) => prepare(displays[index], item))
    : [prepare(displays[0])];
}

/** The commands as they read with the names filled in: one per item for {path} or {name}. */
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
  if (result.stopped) return `Stopped after ${elapsed}`;
  if (result.signal !== null) return `Ended by signal ${result.signal} after ${elapsed}`;
  if (result.exitStatus === null) return `Ended without an exit status after ${elapsed}`;
  if (result.exitStatus !== 0) return `Exited with status ${result.exitStatus} after ${elapsed}`;
  return `Finished in ${elapsed}`;
}
