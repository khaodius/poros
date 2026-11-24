import { useUiStore } from "./uiStore";

/** The element id of the schedule block in Settings > Transfers. */
export const LIMIT_SCHEDULE_GROUP_ID = "limit-schedule";

let revealPending = false;

/** Opens Settings > Transfers scrolled to the speed limit schedule. */
export function openScheduleSettings(): void {
  revealPending = true;
  useUiStore.getState().open({ kind: "settings", section: "transfers" });
}

/** Whether the schedule block was asked for, so it brings itself into view as it appears. */
export function takeScheduleReveal(): boolean {
  const pending = revealPending;
  revealPending = false;
  return pending;
}
