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
    <Dialog title="Quit Poros?" onClose={onClose}>
      <div className="confirm-body">
        <DrivesGoing drives={drives} when="Poros quits" />
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose}>
          Cancel
        </button>
        <button type="button" data-autofocus className="button button-primary" onClick={quit}>
          Unmount and quit
        </button>
      </div>
    </Dialog>
  );
}
