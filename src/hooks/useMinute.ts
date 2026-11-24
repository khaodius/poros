import { useSyncExternalStore } from "react";

const MILLIS_PER_MINUTE = 60_000;

function startOfMinute(): number {
  return Math.floor(Date.now() / MILLIS_PER_MINUTE) * MILLIS_PER_MINUTE;
}

function subscribeToNewMinutes(onChange: () => void): () => void {
  let timer = 0;
  const waitForNextMinute = () => {
    timer = window.setTimeout(
      () => {
        onChange();
        waitForNextMinute();
      },
      startOfMinute() + MILLIS_PER_MINUTE - Date.now() + 50,
    );
  };
  waitForNextMinute();
  // Timers stand still while the computer sleeps, so the time is read again on coming back.
  window.addEventListener("focus", onChange);
  return () => {
    window.clearTimeout(timer);
    window.removeEventListener("focus", onChange);
  };
}

/** The start of the current minute in milliseconds, for views that show the time of day. */
export function useMinute(): number {
  return useSyncExternalStore(subscribeToNewMinutes, startOfMinute);
}
