import type { MessageArea } from "../catalog";

export const editor = {
  // components/EditorPane
  lfLinuxMacos: "LF (Linux, macOS)",
  crlfWindows: "CRLF (Windows)",
  unsavedChanges: "Unsaved changes",
  findAndReplaceCtrlF: "Find and replace (Ctrl+F)",
  saveCtrlS: "Save (Ctrl+S)",
  reloadTheirs: "Reload theirs",
  couldNotSave: "Could not save: {saveError}",
  opening: "Opening {name}...",
  couldNotOpen: "Could not open {name}",
  encodingUsedWhenSaving: "Encoding used when saving",
  lineEndingsUsedWhenSaving: "Line endings used when saving",
  stopWrappingLongLines: "Stop wrapping long lines",
  wrapLongLines: "Wrap long lines",
  reload: "Reload {name}?",
  discardAndReload: "Discard and reload",
  yourUnsavedChangesWillBe: "Your unsaved changes will be lost.",

  // components/UnsavedChangesDialog
  someFilesCouldNotBe: "Some files could not be saved. Their tabs say why.",
  changesYouDontSaveAre: "Changes you don't save are lost.",
  dontSave: "Don't save",
  // components/EditorPane
  utf8WithBom: "UTF-8 with BOM",
  latin1: "Latin-1",
  // components/EditorPane
  wasDeletedAfterYouOpened: "{name} was deleted {where} after you opened it.",
  changedSinceYouOpenedIt: "{name} changed {where} since you opened it.",
  saveItAgain: "Save it again",
  saving: "Saving...",
  saved: "Saved",

  // components/UnsavedChangesDialog
  saveAll: "Save all",
} as const satisfies MessageArea;
