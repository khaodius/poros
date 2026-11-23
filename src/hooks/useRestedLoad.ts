import { useCallback, useEffect, useRef, useState } from "react";
import { toAppError } from "../lib/ipc";
import { problemOf, type Problem } from "../lib/previewPlan";
import { RestingLoad, isCancelled, loadRetrying } from "../lib/previewScheduler";
import type { PreviewLoad, StorageKind } from "../lib/previewSettings";

/** How long the cursor rests on an item before anything is read for its preview. */
export const REST_MILLIS: Record<StorageKind, number> = { local: 120, server: 250, metered: 250 };
/**
 * A load that found its connection busy with another request asks again this long after, as
 * often as this. The server end waits a few seconds for the connection each time already.
 */
export const BUSY_RETRY_MILLIS = 1000;
export const BUSY_RETRIES = 10;

export type Rested<Result> =
  /** The cursor has just arrived: nothing is read yet. */
  | { status: "resting" }
  /** Waits for "Show preview". */
  | { status: "manual" }
  | { status: "loading" }
  | { status: "ready"; result: Result }
  /** With `retrying`, it loads again by itself shortly. */
  | { status: "failed"; problem: Problem; message: string; retrying: boolean };

export interface RestedLoad<Result> {
  state: Rested<Result>;
  /** Loads now, as "Show preview" and "Try again" do. */
  show: () => void;
}

interface Options<Result> {
  /** Names what is loaded: another item, or the same one changed, has another key. */
  key: string;
  /** What an earlier load left, which shows at once. */
  cached: Result | undefined;
  /** By itself once the cursor rests, or on a click. */
  mode: PreviewLoad;
  storage: StorageKind;
  inVault: boolean;
  /** The user has already asked for this: it loads without waiting for a rest or a click. */
  eager?: boolean;
  load: (signal: AbortSignal) => Promise<Result>;
}

/**
 * Loads what a preview shows of the item under the cursor: once the cursor has rested on it,
 * or when asked where the storage charges for reads. The load is dropped the moment the
 * cursor moves on or the preview closes.
 */
export function useRestedLoad<Result>({
  key,
  cached,
  mode,
  storage,
  inVault,
  eager = false,
  load,
}: Options<Result>): RestedLoad<Result> {
  /** The item the user asked to load, and how often. */
  const [asked, setAsked] = useState({ key: "", attempt: 0 });
  const requested = eager || asked.key === key;
  /** Names one go at loading the item. */
  const run = `${key}:${asked.key === key ? asked.attempt : 0}`;
  const [reached, setReached] = useState<{ run: string; state: Rested<Result> }>({
    run: "",
    state: { status: "resting" },
  });
  const [resting] = useState(() => new RestingLoad(REST_MILLIS[storage]));
  const latest = useRef({ load, inVault });
  useEffect(() => {
    latest.current = { load, inVault };
  });

  const retrying = reached.state.status === "failed" && reached.state.retrying;
  const settled = reached.run === run && reached.state.status !== "loading" && !retrying;
  const waitsForClick = cached === undefined && mode === "click" && !requested;
  const loads = cached === undefined && !waitsForClick && !settled;

  useEffect(() => {
    if (!loads) return;
    const current = latest.current;
    const failed = (caught: unknown, again: boolean): Rested<Result> => {
      const error = toAppError(caught);
      const problem = problemOf(error, current.inVault);
      return { status: "failed", problem, message: error.message, retrying: again };
    };
    resting.schedule((signal) => {
      setReached({ run, state: { status: "loading" } });
      // What failed stays on show while it is tried again.
      loadRetrying(current.load, signal, {
        retries: BUSY_RETRIES,
        delayMillis: BUSY_RETRY_MILLIS,
        passes: (caught) => !isCancelled(caught) && toAppError(caught).kind === "busy",
        meanwhile: (caught) => setReached({ run, state: failed(caught, true) }),
      }).then(
        (result) => {
          if (!signal.aborted) setReached({ run, state: { status: "ready", result } });
        },
        (caught: unknown) => {
          if (signal.aborted || isCancelled(caught)) return;
          setReached({ run, state: failed(caught, false) });
        },
      );
    }, requested);
    return () => resting.cancel();
  }, [run, loads, requested, resting]);

  const show = useCallback(
    () => setAsked((before) => ({ key, attempt: before.attempt + 1 })),
    [key],
  );

  let state: Rested<Result> = { status: "resting" };
  if (settled) state = reached.state;
  else if (cached !== undefined) state = { status: "ready", result: cached };
  else if (waitsForClick) state = { status: "manual" };
  else if (reached.run === run) state = reached.state;
  return { state, show };
}
