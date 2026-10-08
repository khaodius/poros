import { create } from "zustand";
import {
  commandSucceeded,
  describeResult,
  expandCommand,
  type CommandTarget,
} from "../lib/commands";
import { onCommandOutput, serverCommands, toAppError } from "../lib/ipc";
import type { ServerCommand } from "../lib/settings";
import type { CommandResult, OutputChunk } from "../lib/types";
import { listPanes } from "./paneRegistry";
import { useToastStore } from "./toastStore";

/** Output kept in the window; the oldest text goes first. */
const MAX_OUTPUT_CHARS = 512 * 1024;
const FLUSH_DELAY_MILLIS = 50;
/** Output events can arrive just after the command's result, so handlers stay a little longer. */
const LATE_OUTPUT_MILLIS = 1000;

export interface CommandRun {
  title: string;
  sessionId: string;
  /** The folder the commands run in. */
  directory: string;
  /** Run one after another. */
  commands: string[];
  /** Reloads the session's panes afterwards. */
  refresh: boolean;
}

export interface OutputPart {
  id: number;
  /** `note` is Poros's own text, such as each command before it runs. */
  stream: OutputChunk["stream"] | "note";
  text: string;
}

export interface RunSummary {
  succeeded: boolean;
  text: string;
}

interface CommandState {
  /** The run shown in the output window. */
  run: CommandRun | null;
  output: OutputPart[];
  /** Older output was dropped to keep the window responsive. */
  truncated: boolean;
  running: boolean;
  summary: RunSummary | null;
  /** Starts a run and shows its output. */
  show: (run: CommandRun) => void;
  stop: () => void;
  /** Stops the run if it is still going and closes the window. */
  dismiss: () => void;
}

interface RunControl {
  stopped: boolean;
  runId: string | null;
}

const outputHandlers = new Map<string, (chunk: OutputChunk) => void>();
let listening: Promise<unknown> | null = null;

function listenForOutput(): Promise<unknown> {
  listening ??= onCommandOutput((chunk) => outputHandlers.get(chunk.runId)?.(chunk));
  return listening;
}

interface RunHooks {
  control: RunControl;
  note: (text: string) => void;
  output: (chunk: OutputChunk) => void;
}

/** Runs each command in turn, going on after a failure until stopped. */
async function runEach(run: CommandRun, hooks: RunHooks): Promise<RunSummary> {
  await listenForOutput();
  const several = run.commands.length > 1;
  let failed = 0;
  let finished = 0;
  let last: CommandResult | null = null;
  let lastError: string | null = null;
  for (const command of run.commands) {
    if (hooks.control.stopped) break;
    const runId = crypto.randomUUID();
    outputHandlers.set(runId, hooks.output);
    hooks.control.runId = runId;
    if (several) hooks.note(`$ ${command}\n`);
    try {
      last = await serverCommands.run({
        runId,
        sessionId: run.sessionId,
        command,
        directory: run.directory || null,
      });
      if (!commandSucceeded(last)) failed++;
      if (several && !last.stopped) hooks.note(`${describeResult(last)}\n`);
    } catch (caught) {
      failed++;
      lastError = toAppError(caught).message;
      hooks.note(`${lastError}\n`);
    } finally {
      hooks.control.runId = null;
      window.setTimeout(() => outputHandlers.delete(runId), LATE_OUTPUT_MILLIS);
    }
    finished++;
  }
  if (hooks.control.stopped) return { succeeded: false, text: "Stopped" };
  if (!several) {
    return {
      succeeded: failed === 0,
      text: lastError ?? (last ? describeResult(last) : "Nothing to run"),
    };
  }
  return failed === 0
    ? { succeeded: true, text: `All ${finished} commands finished` }
    : { succeeded: false, text: `${failed} of ${finished} commands failed` };
}

export function refreshServerPanes(sessionId: string): void {
  for (const pane of listPanes()) {
    if (pane.kind === "remote" && pane.sessionId === sessionId) pane.refresh();
  }
}

let current: RunControl | null = null;

export const useCommandStore = create<CommandState>((set, get) => {
  let pending: OutputPart[] = [];
  let flushTimer = 0;
  let nextPartId = 1;

  const flush = () => {
    flushTimer = 0;
    const added = pending;
    pending = [];
    set((state) => {
      const output = [...state.output, ...added];
      let total = output.reduce((sum, part) => sum + part.text.length, 0);
      let dropped = 0;
      while (total > MAX_OUTPUT_CHARS && dropped < output.length - 1) {
        total -= output[dropped].text.length;
        dropped++;
      }
      return dropped > 0
        ? { output: output.slice(dropped), truncated: true }
        : { output, truncated: state.truncated };
    });
  };

  return {
    run: null,
    output: [],
    truncated: false,
    running: false,
    summary: null,
    show: (run) => {
      get().stop();
      const control: RunControl = { stopped: false, runId: null };
      current = control;
      pending = [];
      set({ run, output: [], truncated: false, running: true, summary: null });
      const append = (stream: OutputPart["stream"], text: string) => {
        if (current !== control) return;
        pending.push({ id: nextPartId++, stream, text });
        if (!flushTimer) flushTimer = window.setTimeout(flush, FLUSH_DELAY_MILLIS);
      };
      void runEach(run, {
        control,
        note: (text) => append("note", text),
        output: (chunk) => append(chunk.stream, chunk.text),
      }).then((summary) => {
        if (run.refresh) refreshServerPanes(run.sessionId);
        if (current === control) set({ running: false, summary });
      });
    },
    stop: () => {
      if (!current || current.stopped) return;
      current.stopped = true;
      if (current.runId) void serverCommands.stop(current.runId).catch(() => undefined);
    },
    dismiss: () => {
      get().stop();
      current = null;
      pending = [];
      set({ run: null, output: [], truncated: false, running: false, summary: null });
    },
  };
});

/** Runs a saved command on the selection: in the output window, or quietly with a toast. */
export function runServerCommand(
  command: Pick<ServerCommand, "name" | "command" | "showOutput" | "refresh">,
  sessionId: string,
  target: CommandTarget,
): void {
  const run: CommandRun = {
    title: command.name.trim() || command.command.trim(),
    sessionId,
    directory: target.folder,
    commands: expandCommand(command.command, target),
    refresh: command.refresh,
  };
  if (command.showOutput) {
    useCommandStore.getState().show(run);
    return;
  }
  const control: RunControl = { stopped: false, runId: null };
  void runEach(run, { control, note: () => undefined, output: () => undefined }).then((summary) => {
    if (run.refresh) refreshServerPanes(sessionId);
    useToastStore
      .getState()
      .show(summary.succeeded ? "info" : "error", `${run.title}: ${summary.text}`);
  });
}
