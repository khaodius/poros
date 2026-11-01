import { useSyncExternalStore } from "react";

function startOfToday(): number {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
}

function subscribeToNewDays(onChange: () => void): () => void {
  let timer = 0;
  const waitForMidnight = () => {
    const now = new Date();
    const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
    timer = window.setTimeout(
      () => {
        onChange();
        waitForMidnight();
      },
      tomorrow.getTime() - now.getTime() + 1000,
    );
  };
  waitForMidnight();
  // Timers stand still while the computer sleeps, so a night asleep shows on coming back.
  window.addEventListener("focus", onChange);
  return () => {
    window.clearTimeout(timer);
    window.removeEventListener("focus", onChange);
  };
}

/** The start of today in milliseconds, which moves on at midnight. */
export function useToday(): number {
  return useSyncExternalStore(subscribeToNewDays, startOfToday);
}
