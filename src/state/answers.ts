// Waiting for the user to answer a question in the window's dialog, for steps that go on once
// it is answered, such as closing several tabs one after another.

import { useUiStore, type AppDialog } from "./uiStore";

/**
 * Resolves once the dialog open in this window no longer asks what `asking` looks for: it was
 * answered, dismissed or replaced by another one.
 */
export function untilAnswered(asking: (dialog: AppDialog | null) => boolean): Promise<void> {
  return new Promise((resolve) => {
    if (!asking(useUiStore.getState().dialog)) {
      resolve();
      return;
    }
    const unsubscribe = useUiStore.subscribe((state) => {
      if (asking(state.dialog)) return;
      unsubscribe();
      resolve();
    });
  });
}
