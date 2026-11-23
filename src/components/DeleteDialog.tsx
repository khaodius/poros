import { useMemo, type ReactNode } from "react";
import { entriesStillListed } from "../lib/deletion";
import type { DirListing, FileEntry } from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";

interface DeleteDialogProps {
  /** The entries picked to delete, as the listing had them then. */
  chosen: readonly FileEntry[];
  /** The folder's listing on show now. */
  listing: DirListing | null;
  /** Set while that listing is a remembered one the server has yet to confirm. */
  checking: boolean;
  /** Describes the entries about to be deleted. */
  summary: (entries: FileEntry[]) => ReactNode;
  onConfirm: (entries: FileEntry[]) => Promise<void>;
  onClose: () => void;
}

/**
 * Asks before deleting. What it counts and deletes is what the folder lists now: over a
 * remembered listing it waits for the server's answer, which is one request away, before it
 * lets anything be deleted.
 */
export function DeleteDialog({
  chosen,
  listing,
  checking,
  summary,
  onConfirm,
  onClose,
}: DeleteDialogProps) {
  const entries = useMemo(
    () => entriesStillListed(chosen, listing?.entries ?? []),
    [chosen, listing],
  );
  return (
    <ConfirmDialog
      title={
        entries.length === 0
          ? "Nothing to delete"
          : entries.length === 1
            ? "Delete item"
            : `Delete ${entries.length} items`
      }
      confirmLabel="Delete"
      danger
      waiting={checking ? "Checking the folder..." : undefined}
      disabled={entries.length === 0}
      onClose={onClose}
      onConfirm={() => onConfirm(entries)}
    >
      {entries.length > 0 ? (
        summary(entries)
      ) : (
        <p>{checking ? "Checking the folder..." : "These items are no longer in the folder."}</p>
      )}
    </ConfirmDialog>
  );
}
