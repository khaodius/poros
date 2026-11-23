// Extracting archives from a pane's context menu: on a server with the programs it has, or on
// this computer with the program Settings > Archives picks. Each archive shows in the tray
// while it unpacks, and the panes showing its folder reload with what came out selected.

import { archiveFormatOf, CUSTOM_EXTRACTOR, extractorOf } from "../lib/archives";
import { pluralize } from "../lib/format";
import { fileOperations, toAppError } from "../lib/ipc";
import { baseName } from "../lib/path";
import type { ExtractInto, FileEntry } from "../lib/types";
import { paneOrigin, refreshFolder } from "./fileOperations";
import { useLogStore } from "./logStore";
import { track } from "./operationStore";
import type { PaneHandle } from "./paneRegistry";
import { useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";

/** The entries a pane can offer to extract: files whose names say they are archives. */
export function archivesAmong(entries: readonly FileEntry[]): FileEntry[] {
  return entries.filter(
    (entry) =>
      (entry.kind === "file" || (entry.kind === "symlink" && entry.linkTarget === "file")) &&
      archiveFormatOf(entry.name) !== null,
  );
}

/** Extracts each archive in turn, beside itself or into a folder named after it. */
export async function extractArchives(
  pane: Pick<PaneHandle, "kind" | "sessionId">,
  archives: readonly FileEntry[],
  into: ExtractInto,
): Promise<void> {
  const origin = paneOrigin(pane);
  const location =
    pane.kind === "remote" && pane.sessionId
      ? { kind: "remote" as const, sessionId: pane.sessionId }
      : { kind: "local" as const };
  const settings = useSettingsStore.getState().settings.interface.extractor;
  const write = useLogStore.getState().write;
  const showToast = useToastStore.getState().show;
  if (location.kind === "local" && settings.choice === CUSTOM_EXTRACTOR && !settings.program) {
    showToast("info", "Name the program that extracts archives in Settings > Archives.");
    return;
  }
  const extractor = location.kind === "local" ? extractorOf(settings) : undefined;
  for (const archive of archives) {
    const title = `Extracting ${archive.name}`;
    try {
      const summary = await track(
        title,
        (operationId) =>
          fileOperations.extract({ operationId, location, path: archive.path, into, extractor }),
        "extract",
      );
      const items =
        summary.placed.length > 1 ? ` (${pluralize(summary.placed.length, "item")})` : "";
      write(
        "info",
        `Extracted ${archive.name} to ${summary.destination}${items} with ${summary.tool}`,
        pane.sessionId,
      );
      if (summary.warning) write("warn", `${summary.tool}: ${summary.warning}`, pane.sessionId);
      // What came out is selected, so it shows among what the folder already held.
      refreshFolder(origin, summary.folder, summary.placed, summary.placed);
    } catch (caught) {
      const error = toAppError(caught);
      if (error.kind === "cancelled") {
        write("info", `Cancelled: ${title}`, pane.sessionId);
        return;
      }
      showToast("error", `${baseName(archive.path)}: ${error.message}`);
      write("error", `${title}: ${error.message}`, pane.sessionId);
    }
  }
}
