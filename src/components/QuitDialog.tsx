import { useT } from "../i18n";
import { askToSave, closeWindowNow, unsavedEditorTabs } from "../state/editorActions";
import { Dialog } from "./Dialog";
import { DrivesGoing } from "./DrivesGoing";

interface QuitDialogProps {
  /** Where the drives are that programs have files open on or that have uploads pending. */
  drives: string[];
  onClose: () => void;
}

/** Asks before Poros quits while drives are in use, as closing their tab does. Asked after the
 * question about files saved in other programs, and goes on to files with unsaved changes. */
export function QuitDialog({ drives, onClose }: QuitDialogProps) {
  const t = useT();
  const quit = () => {
    onClose();
    // Files with unsaved changes are asked about next; the window closes once they are.
    const unsaved = unsavedEditorTabs();
    if (unsaved.length > 0)
      askToSave(
        unsaved.map((tab) => tab.id),
        true,
      );
    else closeWindowNow();
  };

  return (
    <Dialog title={t("drives.quitPoros")} onClose={onClose}>
      <div className="confirm-body">
        <DrivesGoing drives={drives} when="quit" />
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button type="button" data-autofocus className="button button-primary" onClick={quit}>
          {t("drives.unmountAndQuit")}
        </button>
      </div>
    </Dialog>
  );
}
