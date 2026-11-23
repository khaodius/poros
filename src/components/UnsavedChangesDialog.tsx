import { useEffect, useRef, useState } from "react";
import { useLatest } from "../hooks/useLatest";
import { findTab } from "../lib/layout";
import { closeEditorTab, closeWindowNow, saveAndClose } from "../state/editorActions";
import { useLayoutStore } from "../state/layoutStore";
import { Dialog } from "./Dialog";

interface UnsavedChangesDialogProps {
  tabIds: string[];
  /** The window closes once the files are saved or discarded. */
  closeWindow: boolean;
  onClose: () => void;
}

export function UnsavedChangesDialog({ tabIds, closeWindow, onClose }: UnsavedChangesDialogProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Dismissed while the files save, the question leaves the window open.
  const shown = useRef(true);
  useEffect(() => {
    shown.current = true;
    return () => {
      shown.current = false;
    };
  }, []);
  // The question as it is now: tabs, and the window, can join it while the files save.
  const latest = useLatest({ tabIds, closeWindow, onClose });
  const root = useLayoutStore((state) => state.root);
  const names = tabIds.flatMap((tabId) => {
    const tab = findTab(root, tabId);
    return tab?.kind === "editor" ? [tab.name] : [];
  });

  const finish = () => {
    latest.current.onClose();
    if (latest.current.closeWindow && shown.current) closeWindowNow();
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    const saved = await saveAndClose(tabIds);
    setBusy(false);
    if (!saved) setError("Some files could not be saved. Their tabs say why.");
    // Tabs that joined meanwhile are still to be asked about, and the question stays for them.
    else if (latest.current.tabIds.every((tabId) => tabIds.includes(tabId))) finish();
  };

  const discard = () => {
    if (!closeWindow) tabIds.forEach(closeEditorTab);
    finish();
  };

  const title =
    names.length === 1 ? `Save changes to ${names[0]}?` : `Save changes to ${names.length} files?`;
  return (
    <Dialog title={title} onClose={onClose}>
      <div className="confirm-body">
        {names.length > 1 && (
          <ul className="unsaved-files">
            {names.map((name, index) => (
              <li key={`${name}-${index}`}>{name}</li>
            ))}
          </ul>
        )}
        <p>Changes you don't save are lost.</p>
      </div>
      {error && <p className="form-error">{error}</p>}
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button type="button" className="button button-danger" onClick={discard} disabled={busy}>
          Don't save
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          onClick={() => void save()}
          disabled={busy}
        >
          {busy ? "Saving..." : names.length === 1 ? "Save" : "Save all"}
        </button>
      </div>
    </Dialog>
  );
}
