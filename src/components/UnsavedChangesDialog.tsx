import { useCallback, useEffect, useRef, useState } from "react";
import { useLatest } from "../hooks/useLatest";
import { findTab } from "../lib/layout";
import { closeEditorTab, closeWindowNow, saveAndClose } from "../state/editorActions";
import { useLayoutStore } from "../state/layoutStore";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

interface UnsavedChangesDialogProps {
  tabIds: string[];
  /** The window closes once the files are saved or discarded. */
  closeWindow: boolean;
  onClose: () => void;
}

export function UnsavedChangesDialog({ tabIds, closeWindow, onClose }: UnsavedChangesDialogProps) {
  const t = useT();
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

  const finished = useRef(false);
  const finish = useCallback(() => {
    if (finished.current) return;
    finished.current = true;
    latest.current.onClose();
    if (latest.current.closeWindow && shown.current) closeWindowNow();
  }, [latest]);

  // Its tabs closed meanwhile, as when a save started from a question dismissed since ends:
  // nothing is left to ask about, and a window waiting on the answer closes.
  const noneLeft = names.length === 0;
  useEffect(() => {
    if (noneLeft && !busy) finish();
  }, [noneLeft, busy, finish]);

  const save = async () => {
    setBusy(true);
    setError(null);
    const saved = await saveAndClose(tabIds);
    setBusy(false);
    if (!saved) setError(t("editor.someFilesCouldNotBe"));
    // Tabs that joined meanwhile are still to be asked about, and the question stays for them.
    else if (latest.current.tabIds.every((tabId) => tabIds.includes(tabId))) finish();
  };

  const discard = () => {
    if (!closeWindow) tabIds.forEach(closeEditorTab);
    finish();
  };

  const title =
    names.length === 1
      ? t("editor.saveChangesTo", { name: names[0] })
      : t("editor.saveChangesToFiles", { count: names.length });
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
        <p>{t("editor.changesYouDontSaveAre")}</p>
      </div>
      {error && <p className="form-error">{error}</p>}
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose} disabled={busy}>
          {t("common.cancel")}
        </button>
        <button type="button" className="button button-danger" onClick={discard} disabled={busy}>
          {t("editor.dontSave")}
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          onClick={() => void save()}
          disabled={busy}
        >
          {busy ? t("editor.saving") : names.length === 1 ? t("common.save") : t("editor.saveAll")}
        </button>
      </div>
    </Dialog>
  );
}
