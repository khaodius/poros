import { useEffect, useRef, useState } from "react";
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
  const root = useLayoutStore((state) => state.root);
  const names = tabIds.flatMap((tabId) => {
    const tab = findTab(root, tabId);
    return tab?.kind === "editor" ? [tab.name] : [];
  });

  const finish = () => {
    onClose();
    if (closeWindow && shown.current) closeWindowNow();
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    const saved = await saveAndClose(tabIds);
    setBusy(false);
    if (saved) finish();
    else setError("Some files could not be saved. Their tabs say why.");
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
