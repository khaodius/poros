import { create } from "zustand";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { defaultOpenWith, editsOfSessions } from "../lib/externalEditor";
import { localSource } from "../lib/fileSource";
import { external, toAppError } from "../lib/ipc";
import type {
  DocumentLocation,
  ExternalEdit,
  ExternalEditsUpdate,
  ExternalLeftover,
  OpenWith,
} from "../lib/types";
import { openInEditor } from "./editorActions";
import { useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";
import { useUiStore } from "./uiStore";

/** A file to open, on this computer or on the server of a session. */
export type OpenTarget = DocumentLocation & { name: string };

/** What a file opens in: a program, or the built-in editor. */
export type OpenChoice = OpenWith | { kind: "builtIn" };

/** A question this window asks before a file opens. */
export type OpenPrompt =
  /**
   * The file is of a type that runs, so only an editor opens it, once the user names one.
   * `editId` is a server file already downloaded and held back for its content.
   */
  | { kind: "executable"; target: OpenTarget; editId?: string }
  /** Double-click asks each time how to open a file. */
  | { kind: "choose"; target: OpenTarget; nearGroupId?: string };

/** Asked before a server tab closes, or a vault locks, with files from it still open in other
 * programs. `label` names the server or the vault. */
export type ClosePrompt =
  | { kind: "tab"; tabId: string; label: string; sessionIds: string[] }
  | { kind: "lockVault"; sessionId: string; label: string; sessionIds: string[] };

interface ExternalEditState {
  edits: ExternalEdit[];
  /** Files an earlier run left saved but never uploaded. */
  leftovers: ExternalLeftover[];
  prompt: OpenPrompt | null;
  /** The edit whose changed server copy this window asks about. */
  resolving: string | null;
  /** Edits whose saves wait for the user's word, the first one asked about first. */
  askingUpload: string[];
  /** The files left from an earlier run are shown. */
  showingLeftovers: boolean;
  closing: ClosePrompt | null;
  /** The app is about to quit with these edits holding saves their servers do not have. */
  exiting: string[] | null;
  apply: (update: ExternalEditsUpdate) => void;
  load: () => Promise<void>;
  loadLeftovers: () => Promise<void>;
}

export const useExternalEditStore = create<ExternalEditState>((set, get) => ({
  edits: [],
  leftovers: [],
  prompt: null,
  resolving: null,
  askingUpload: [],
  showingLeftovers: false,
  closing: null,
  exiting: null,
  apply: (update) => {
    const { resolving, askingUpload } = get();
    const inState = (id: string | null, state: ExternalEdit["state"]) =>
      update.edits.some((edit) => edit.id === id && edit.state === state);
    set({
      edits: update.edits,
      // Answered, here or in another window.
      resolving: inState(resolving, "conflict") ? resolving : null,
      askingUpload: askingUpload.filter((id) => inState(id, "changed")),
    });
  },
  load: async () => {
    const edits = await external.list().catch(() => null);
    if (edits) set({ edits });
  },
  loadLeftovers: async () => {
    const leftovers = await external.leftovers().catch(() => null);
    if (leftovers) set({ leftovers });
  },
}));

function showError(caught: unknown): void {
  useToastStore.getState().show("error", toAppError(caught).message);
}

/** Opens a file in another program: a server file through a working copy that is uploaded as
 * it is saved, a local file where it is. */
export async function openFile(
  target: OpenTarget,
  openWith: OpenWith,
  confirmedExecutable = false,
): Promise<void> {
  try {
    const outcome =
      target.side === "local"
        ? await external.openLocal(target.path, openWith)
        : await external.open({
            sessionId: target.sessionId,
            path: target.path,
            with: openWith,
            confirmedExecutable,
          });
    if (outcome.kind === "needsConfirmation") {
      useExternalEditStore.setState({ prompt: { kind: "executable", target } });
    } else if (outcome.kind === "alreadyOpen") {
      useToastStore
        .getState()
        .show("info", `${outcome.edit.name} is open already. Its working copy opens again.`);
    }
  } catch (caught) {
    showError(caught);
  }
}

/**
 * Open: the program set for the file's kind, else the system's own. Setting a program for a
 * kind of file is the user's word that such files open there, scripts among them, so they are
 * not asked about each time; the system's own program never gets a script.
 */
export function openWithDefault(target: OpenTarget): Promise<void> {
  const programs = useSettingsStore.getState().settings.editing.programs;
  const openWith = defaultOpenWith(target.name, programs);
  return openFile(target, openWith, openWith.kind === "program");
}

/** Asks for a program file; null when the user picks none. */
export async function pickProgram(): Promise<string | null> {
  const windows = localSource.pathStyle === "windows";
  const chosen = await openFileDialog({
    multiple: false,
    directory: false,
    title: "Choose a program",
    filters: windows ? [{ name: "Programs", extensions: ["exe", "cmd", "bat", "com"] }] : undefined,
  }).catch((caught: unknown) => {
    showError(caught);
    return null;
  });
  return typeof chosen === "string" ? chosen : null;
}

let programSettingsAsked = false;

/** Opens the settings at the programs files open in. */
export function openProgramSettings(): void {
  programSettingsAsked = true;
  useUiStore.getState().open({ kind: "settings", section: "interface" });
}

/** Whether the settings were opened for their programs, which is told once. */
export function takeProgramSettingsAsk(): boolean {
  const asked = programSettingsAsked;
  programSettingsAsked = false;
  return asked;
}

/** Opens a file the way the user picked. */
export async function openFileWith(
  target: OpenTarget,
  choice: OpenChoice | "choose",
  options: { nearGroupId?: string; confirmedExecutable?: boolean } = {},
): Promise<void> {
  if (choice === "choose") {
    const command = await pickProgram();
    if (command) await openFile(target, { kind: "custom", command }, options.confirmedExecutable);
    return;
  }
  if (choice.kind === "builtIn") {
    const location: DocumentLocation =
      target.side === "remote"
        ? { side: "remote", sessionId: target.sessionId, path: target.path }
        : { side: "local", path: target.path };
    await openInEditor(location, options.nearGroupId);
    return;
  }
  await openFile(target, choice, options.confirmedExecutable);
}

/** Opens the working copy of a held-back file in the editor the user named. */
export async function openHeldBackWith(editId: string, choice: OpenWith | "choose"): Promise<void> {
  const openWith =
    choice === "choose"
      ? await pickProgram().then((command) =>
          command ? ({ kind: "custom", command } satisfies OpenWith) : null,
        )
      : choice;
  if (openWith) await external.reopen(editId, openWith).catch(showError);
}

/** Runs an action on an edit, telling the user when it fails. */
export function editAction(action: () => Promise<void>): void {
  void action().catch(showError);
}

/** The tabs and vaults the user has agreed to close with their files still being edited, while
 * that close goes on. */
const closesAllowed = new Set<string>();

function allowanceOf(prompt: ClosePrompt): string {
  return prompt.kind === "tab" ? `tab:${prompt.tabId}` : `vault:${prompt.sessionId}`;
}

/** Whether the close has to wait for the user, because files from it are open in other
 * programs. Asks when it does. */
function holdsClose(prompt: ClosePrompt): boolean {
  if (closesAllowed.has(allowanceOf(prompt))) return false;
  const editing = editsOfSessions(useExternalEditStore.getState().edits, prompt.sessionIds);
  if (editing.length === 0) return false;
  useExternalEditStore.setState({ closing: prompt });
  return true;
}

/**
 * Whether the server tab `tabId` has to wait for the user before it closes, because files from
 * its server are open in other programs. Asks when it does.
 */
export function editsHoldClose(tabId: string, label: string, sessionIds: string[]): boolean {
  return holdsClose({ kind: "tab", tabId, label, sessionIds });
}

/**
 * Whether the vault of session `sessionId` has to wait for the user before it locks, because
 * files from it are open in other programs, which locking ends. Asks when it does.
 */
export function editsHoldLock(sessionId: string, name: string): boolean {
  return holdsClose({ kind: "lockVault", sessionId, label: name, sessionIds: [sessionId] });
}

/**
 * Runs `close` past the question about the files being edited, which the user has answered.
 * The answer is for this close alone: once `close` is done, having closed or stopped at another
 * question, the next close asks again.
 */
export async function closePastEdits(
  prompt: ClosePrompt,
  close: () => Promise<void>,
): Promise<void> {
  const allowance = allowanceOf(prompt);
  closesAllowed.add(allowance);
  try {
    await close();
  } finally {
    closesAllowed.delete(allowance);
  }
}

let exitAllowed = false;

/**
 * Whether quitting has to wait for the user, because files hold saves the servers do not have
 * yet. Asks when it does. A save made a moment ago is waited for, so it is asked about too.
 */
export async function editsHoldExit(): Promise<boolean> {
  // The user's word to quit is for the one quit it started.
  if (exitAllowed) {
    exitAllowed = false;
    return false;
  }
  const unsaved = await external.unsaved().catch(() => []);
  if (unsaved.length === 0) return false;
  useExternalEditStore.setState({ exiting: unsaved.map((edit) => edit.id) });
  return true;
}

/** Quits now, whatever the files being edited hold. */
export function exitNow(): void {
  exitAllowed = true;
  useExternalEditStore.setState({ exiting: null });
  void getCurrentWindow()
    .close()
    .catch((caught: unknown) => {
      exitAllowed = false;
      showError(caught);
    });
}
