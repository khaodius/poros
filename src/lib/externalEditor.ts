// Opening files in the user's own programs: which program Open picks, and how the files being
// edited are described. The backend (src-tauri/src/external_editor) does the opening.

import type { ExternalProgram } from "./settings";
import type {
  ExternalEdit,
  ExternalEditState,
  ExternalLeftover,
  OpenWith,
  SessionInfo,
} from "./types";
import { t } from "../i18n";
import { formatPercent } from "./format";

/** Stands for the file in a program's arguments. Mirrored in command_line.rs. */
export const FILE_PLACEHOLDER = "{file}";

function patternExpression(pattern: string): RegExp {
  const escaped = [...pattern]
    .map((character) => {
      if (character === "*") return ".*";
      if (character === "?") return ".";
      return character.replace(/[.+^${}()|[\]\\]/g, "\\$&");
    })
    .join("");
  return new RegExp(`^${escaped}$`, "iu");
}

/**
 * Whether a file called `name` matches a pattern such as `*.psd`, `report-??.csv` or `Makefile`,
 * in any case. A pattern that is only an extension (`.psd`) stands for every file with it.
 */
export function matchesPattern(name: string, pattern: string): boolean {
  const wanted = pattern.trim();
  if (wanted === "") return false;
  const glob = /^\.[^*?./\\]+$/.test(wanted) ? `*${wanted}` : wanted;
  return patternExpression(glob).test(name);
}

/** The patterns typed in one box, split at commas, semicolons and spaces, without repeats. */
export function parsePatterns(text: string): string[] {
  const patterns = text
    .split(/[\s,;]+/)
    .map((pattern) => pattern.trim())
    .filter((pattern) => pattern !== "");
  return [...new Set(patterns)];
}

export function formatPatterns(patterns: string[]): string {
  return patterns.join(", ");
}

/** The first program a pattern of which names the file. */
export function programFor(
  name: string,
  programs: readonly ExternalProgram[],
): ExternalProgram | undefined {
  return programs.find((program) =>
    program.patterns.some((pattern) => matchesPattern(name, pattern)),
  );
}

/** What Open uses for a file: the program set for its kind, else the system's own. */
export function defaultOpenWith(name: string, programs: readonly ExternalProgram[]): OpenWith {
  const program = programFor(name, programs);
  return program ? { kind: "program", id: program.id } : { kind: "default" };
}

/** A program's name for menus: the one given, else its file's name without the extension. */
export function programLabel(program: Pick<ExternalProgram, "name" | "command">): string {
  const named = program.name.trim();
  if (named) return named;
  const file = program.command.trim().split(/[/\\]/).pop() ?? "";
  const dot = file.lastIndexOf(".");
  return dot > 0 ? file.slice(0, dot) : file;
}

export const EDIT_STATE_LABELS: Record<ExternalEditState, string> = {
  get downloading() {
    return t("openWith.downloading");
  },
  get watching() {
    return t("openWith.watchingForSaves");
  },
  get changed() {
    return t("openWith.savedNotUploaded");
  },
  get waiting() {
    return t("openWith.waitingToUpload");
  },
  get conflict() {
    return t("openWith.changedOnTheServer2");
  },
  get uploading() {
    return t("openWith.uploading");
  },
  get failed() {
    return t("openWith.uploadFailed");
  },
};

export function editStateLabel(edit: Pick<ExternalEdit, "state" | "blocked">): string {
  if (edit.blocked) return t("openWith.notOpenedItIsAProgram");
  return EDIT_STATE_LABELS[edit.state];
}

/** How far a download or upload is, from 0 to 1; null when nothing moves. */
export function transferFraction(
  edit: Pick<ExternalEdit, "state" | "transferred" | "size">,
): number | null {
  if (edit.state !== "downloading" && edit.state !== "uploading") return null;
  if (edit.size <= 0) return 0;
  return Math.min(1, edit.transferred / edit.size);
}

/** The working copy holds something the server does not, or is held back to be looked at. */
export function needsAttention(edit: Pick<ExternalEdit, "state" | "blocked">): boolean {
  return edit.blocked || edit.state === "conflict" || edit.state === "failed";
}

/** The edits whose uploads go through one of `sessionIds`. */
export function editsOfSessions(
  edits: readonly ExternalEdit[],
  sessionIds: readonly string[],
): ExternalEdit[] {
  return edits.filter((edit) => sessionIds.includes(edit.sessionId));
}

/** What the status bar says about the files being edited; null when there are none. */
export function editsSummary(
  edits: readonly ExternalEdit[],
  leftovers: readonly ExternalLeftover[] = [],
): string | null {
  const downloading = edits.find((edit) => edit.state === "downloading");
  if (downloading) {
    const fraction = transferFraction(downloading) ?? 0;
    return t("openWith.openingNamePercent", {
      name: downloading.name,
      percent: formatPercent(Math.floor(fraction * 100)),
    });
  }
  const uploading = edits.find((edit) => edit.state === "uploading");
  if (uploading) return t("openWith.uploadingName", { name: uploading.name });
  const attention = edits.filter(needsAttention);
  if (attention.length === 1) {
    const [edit] = attention;
    const { name } = edit;
    if (edit.blocked) return t("openWith.nameNotOpenedItIsAProgram", { name });
    return edit.state === "conflict"
      ? t("openWith.nameChangedOnTheServer", { name })
      : t("openWith.nameUploadFailed", { name });
  }
  if (attention.length > 1)
    return t("openWith.editedFilesNeedAttention", { count: attention.length });
  if (edits.length === 1) return t("openWith.editingName", { name: edits[0].name });
  if (edits.length > 1) return t("openWith.editingFiles", { count: edits.length });
  if (leftovers.length > 0) return t("openWith.filesNotUploaded", { count: leftovers.length });
  return null;
}

/**
 * An open session that leads where a file left from an earlier run came from: the same account
 * on the same server, and the same vault there or none.
 */
export function sessionForLeftover(
  leftover: ExternalLeftover,
  sessions: readonly SessionInfo[],
): SessionInfo | undefined {
  const byId = new Map(sessions.map((info) => [info.id, info]));
  return sessions.find((info) => {
    const server = info.vault ? byId.get(info.vault.parentSessionId) : info;
    if (!server || (info.vault?.root ?? null) !== (leftover.vaultRoot ?? null)) return false;
    return (
      server.protocol === leftover.protocol &&
      server.host === leftover.host &&
      server.port === leftover.port &&
      server.username === leftover.username
    );
  });
}

const TEXT_EXTENSIONS = new Set([
  "c",
  "cfg",
  "conf",
  "cpp",
  "cs",
  "css",
  "csv",
  "env",
  "go",
  "h",
  "hpp",
  "htm",
  "html",
  "ini",
  "java",
  "js",
  "json",
  "jsx",
  "kt",
  "less",
  "log",
  "lua",
  "md",
  "php",
  "pl",
  "properties",
  "py",
  "rb",
  "rs",
  "sass",
  "scss",
  "sh",
  "sql",
  "srt",
  "svg",
  "swift",
  "tex",
  "toml",
  "ts",
  "tsv",
  "tsx",
  "txt",
  "vue",
  "xml",
  "yaml",
  "yml",
]);

/** Whether a file's name says it holds text, which the file compare shows line by line. */
export function looksLikeText(name: string): boolean {
  const dot = name.lastIndexOf(".");
  return dot > 0 && TEXT_EXTENSIONS.has(name.slice(dot + 1).toLowerCase());
}
