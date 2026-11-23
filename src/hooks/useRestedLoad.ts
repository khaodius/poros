import { useCallback, useEffect, useRef, useState } from "react";
import { toAppError } from "../lib/ipc";
import { problemOf, type Problem } from "../lib/previewPlan";
import { RestingLoad, isCancelled } from "../lib/previewScheduler";
import type { PreviewLoad, StorageKind } from "../lib/previewSettings";

/** How long the cursor rests on an item before anything is read for its preview. */
export const REST_MILLIS: Record<StorageKind, number> = { local: 120, server: 250, metered: 250 };

export type Rested<Result> =
  /** The cursor has just arrived: nothing is read yet. */
  | { status: "resting" }
  /** Waits for "Show preview". */
  | { status: "manual" }
  | { status: "loading" }
  | { status: "ready"; result: Result }
  | { status: "failed"; problem: Problem; message: string };

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

  const settled = reached.run === run && reached.state.status !== "loading";
  const waitsForClick = cached === undefined && mode === "click" && !requested;
  const loads = cached === undefined && !waitsForClick && !settled;

  useEffect(() => {
    if (!loads) return;
    const current = latest.current;
    resting.schedule((signal) => {
      setReached({ run, state: { status: "loading" } });
      current.load(signal).then(
        (result) => {
          if (!signal.aborted) setReached({ run, state: { status: "ready", result } });
        },
        (caught: unknown) => {
          if (signal.aborted || isCancelled(caught)) return;
          const error = toAppError(caught);
          const problem = problemOf(error, current.inVault);
          setReached({ run, state: { status: "failed", problem, message: error.message } });
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
