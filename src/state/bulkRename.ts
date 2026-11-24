// Renaming several items of one folder at once: the rules last used, what names the folder
// takes, the run itself, and what to show when it stops part way.

import { create } from "zustand";
import {
  DEFAULT_RULES,
  caseProbe,
  formProbe,
  planRenames,
  rememberFind,
  renameOutcomes,
  targetTraits,
  type NameTarget,
  type OutcomeRow,
  type PlannedRename,
  type RenameRules,
} from "../lib/bulkRename";
import { toLocation, type FileOrigin } from "../lib/fileOrigin";
import { englishCount } from "../lib/format";
import { fileOperations, local, toAppError } from "../lib/ipc";
import { baseName, childPath } from "../lib/path";
import type { RenameSummary } from "../lib/types";
import { pathStyleOf, refreshFolder } from "./fileOperations";
import { useLogStore } from "./logStore";
import { track } from "./operationStore";
import { saveSettingsSection, useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";
import { t } from "../i18n";

/** A bulk rename that stopped before it was done, for the user to read. */
export interface RenameReport {
  folder: string;
  /** Every rename asked for, with what became of it. */
  outcomes: OutcomeRow[];
  summary: RenameSummary;
}

interface BulkRenameState {
  /** The rules of the last bulk rename, kept until the app closes. */
  rules: RenameRules;
  report: RenameReport | null;
}

export const useBulkRenameStore = create<BulkRenameState>(() => ({
  rules: DEFAULT_RULES,
  report: null,
}));

export function closeRenameReport(): void {
  useBulkRenameStore.setState({ report: null });
}

/**
 * Whether the folder answers for `name`, another spelling of a name it lists, which means it
 * holds the two spellings to be one name. Not knowing counts as no: a rename onto a name in use
 * is refused when it runs, so guessing that way never overwrites anything.
 */
async function answersFor(
  origin: FileOrigin,
  folder: string,
  name: string | undefined,
): Promise<boolean> {
  if (name === undefined) return false;
  try {
    if (origin.kind === "local") {
      const path = childPath(folder, name, pathStyleOf(origin));
      return (await local.stat([path])).length > 0;
    }
    const taken = await fileOperations.conflicts(toLocation(origin), [name], folder);
    return taken.length > 0;
  } catch {
    return false;
  }
}

/** What the kind of place says of the names `folder` takes, before the folder is asked. */
export function knownTraits(origin: FileOrigin, folder: string) {
  return targetTraits({
    pathStyle: pathStyleOf(origin),
    protocol: origin.kind === "remote" ? (origin.server?.protocol ?? "sftp") : undefined,
    folder,
  });
}

/**
 * What names `folder` takes, asking the folder itself where its kind does not say: for one of
 * its names spelled in the other case, and in the other Unicode form.
 */
export async function nameTargetOf(
  origin: FileOrigin,
  folder: string,
  names: readonly string[],
): Promise<NameTarget> {
  const traits = knownTraits(origin, folder);
  const [caseInsensitive, normalizes] = await Promise.all([
    traits.caseInsensitive ?? answersFor(origin, folder, caseProbe(names)?.swapped),
    traits.normalizes ?? answersFor(origin, folder, formProbe(names)?.otherForm),
  ]);
  return { ...traits, caseInsensitive, normalizes };
}

const TOKEN_BYTES = 4;

/** Names for items waiting while others trade places: plain, short and unlikely to be in use. */
function temporaryNames(): (attempt: number) => string {
  const bytes = crypto.getRandomValues(new Uint8Array(TOKEN_BYTES));
  const token = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return (attempt) => `poros-rename-${token}-${attempt}.tmp`;
}

export interface BulkRename {
  origin: FileOrigin;
  /** The folder the items are in. */
  folder: string;
  renames: PlannedRename[];
  /** Every name the folder holds, none of which a temporary name may be. */
  held: readonly string[];
  target: NameTarget;
  /** The rules that gave the names, remembered for the next time. */
  rules: RenameRules;
}

function remember(rules: RenameRules): void {
  useBulkRenameStore.setState({ rules });
  const history = useSettingsStore.getState().settings.interface.renameHistory;
  const next = rememberFind(history, rules.find);
  if (JSON.stringify(next) !== JSON.stringify(history)) {
    void saveSettingsSection("interface", { renameHistory: next }).catch(() => undefined);
  }
}

/**
 * Renames the items in one operation, which shows in the tray and can be cancelled. Afterwards
 * the folder is reloaded with the renamed items selected. A run that stops early opens a report
 * of which items have their new name and which do not.
 */
export async function runBulkRename(request: BulkRename): Promise<void> {
  const { origin, folder, renames, held, target, rules } = request;
  if (renames.length === 0) return;
  const sessionId = origin.sessionId;
  const write = useLogStore.getState().write;
  const showToast = useToastStore.getState().show;
  remember(rules);
  const shownFolder = baseName(folder) || folder;
  const title = `Renaming ${englishCount(renames.length, "item")} in ${shownFolder}`;
  let renamedPaths: string[] = [];
  try {
    const groups = planRenames(renames, held, target, temporaryNames());
    const summary = await track(
      t("rename.renamingItemsIn", { count: renames.length, folder: shownFolder }),
      (operationId) =>
        fileOperations.rename({
          operationId,
          location: toLocation(origin),
          directory: folder,
          groups,
        }),
      "rename",
    );
    renamedPaths = summary.renamed.map((item) => item.path);
    const count = `${summary.renamed.length} of ${englishCount(renames.length, "item")}`;
    if (summary.failure || summary.stranded) {
      const { failure, stranded } = summary;
      write("error", `Renaming stopped in ${folder} with ${count} renamed`, sessionId);
      if (failure) {
        write("error", `${failure.from} to ${failure.to}: ${failure.message}`, sessionId);
      }
      if (stranded) {
        write(
          "error",
          `${stranded.from} is left named ${stranded.temporary}: ${stranded.message}`,
          sessionId,
        );
      }
      useBulkRenameStore.setState({
        report: { folder, outcomes: renameOutcomes(renames, summary), summary },
      });
    } else if (summary.cancelled) {
      write("info", `Cancelled: ${title}, with ${count} renamed`, sessionId);
      showToast(
        "info",
        t("rename.renamingCancelledRenamed", {
          renamed: summary.renamed.length,
          count: renames.length,
        }),
      );
    } else {
      write(
        "info",
        `Renamed ${englishCount(summary.renamed.length, "item")} in ${folder}`,
        sessionId,
      );
    }
  } catch (caught) {
    const error = toAppError(caught);
    showToast("error", error.message);
    write("error", `${title}: ${error.message}`, sessionId);
  } finally {
    // Whatever was renamed before a failure or cancel shows too. On a server the folder's
    // remembered listing is forgotten, and so are those of renamed folders, under either name.
    const style = pathStyleOf(origin);
    const changed = renames.flatMap(({ from, to }) => [
      childPath(folder, from, style),
      childPath(folder, to, style),
    ]);
    refreshFolder(origin, folder, renamedPaths, changed);
  }
}
