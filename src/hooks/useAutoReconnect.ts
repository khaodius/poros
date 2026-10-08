import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { toAppError } from "../lib/ipc";
import type { AppError, ErrorKind, SessionInfo } from "../lib/types";
import { useSessionStore } from "../state/sessionStore";
import { useSettingsStore } from "../state/settingsStore";

/** Quick at first, to ride out a blip, then every half minute. */
const RETRY_DELAYS_SECS = [1, 2, 4, 8, 15, 30];

/** Answers that trying again cannot change; only the user can. */
const NEEDS_THE_USER: ErrorKind[] = [
  "hostKeyUnknown",
  "hostKeyChanged",
  "authFailed",
  "passphraseRequired",
  "keychain",
  "invalidInput",
  "sessionNotFound",
];

/** The wait before the next attempt, after `failures` failed ones. */
export function retryDelaySecs(failures: number): number {
  return RETRY_DELAYS_SECS[Math.min(failures, RETRY_DELAYS_SECS.length - 1)];
}

export function canRetryAlone(error: AppError): boolean {
  return !NEEDS_THE_USER.includes(error.kind);
}

/** Tabs that lost the same session share one attempt at a time. */
const attempts = new Map<string, Promise<SessionInfo>>();

function reconnectOnce(sessionId: string): Promise<SessionInfo> {
  let attempt = attempts.get(sessionId);
  if (!attempt) {
    attempt = useSessionStore
      .getState()
      .reconnect(sessionId)
      .finally(() => attempts.delete(sessionId));
    attempts.set(sessionId, attempt);
  }
  return attempt;
}

/** The automatic attempt under way for a session, so a manual one can wait for it. */
export function attemptUnderWay(sessionId: string): Promise<SessionInfo> | undefined {
  return attempts.get(sessionId);
}

export interface AutoReconnect {
  /** Still trying without being asked. */
  active: boolean;
  failures: number;
  /** Until the next attempt; zero while one is under way. */
  secondsLeft: number;
  lastError: string | null;
  stop: () => void;
}

/**
 * Reconnects a lost session on its own, with growing delays, for as long as transfers wait for
 * a lost server. Gives up early on answers only the user can change, such as a wrong password.
 */
export function useAutoReconnect(
  sessionId: string,
  onReconnected: (session: SessionInfo) => void,
): AutoReconnect {
  const enabled = useSettingsStore((state) => state.settings.connection.autoReconnect);
  const windowMinutes = useSettingsStore((state) => state.settings.transfers.reconnectMinutes);
  const [stopped, setStopped] = useState(false);
  const [progress, setProgress] = useState({
    failures: 0,
    secondsLeft: retryDelaySecs(0),
    lastError: null as string | null,
  });
  const callback = useRef(onReconnected);
  useLayoutEffect(() => {
    callback.current = onReconnected;
  });
  const active = enabled && windowMinutes > 0 && !stopped;

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    let failures = 0;
    let lastError: string | null = null;
    let timer = 0;
    const deadline = Date.now() + windowMinutes * 60_000;
    let nextAttemptAt = Date.now() + retryDelaySecs(0) * 1000;
    const secondsLeft = () => Math.max(0, Math.ceil((nextAttemptAt - Date.now()) / 1000));
    const ticker = window.setInterval(
      () => setProgress({ failures, lastError, secondsLeft: secondsLeft() }),
      1000,
    );

    const attempt = async () => {
      nextAttemptAt = 0;
      setProgress({ failures, lastError, secondsLeft: 0 });
      try {
        const session = await reconnectOnce(sessionId);
        if (!cancelled) callback.current(session);
      } catch (caught) {
        if (cancelled) return;
        const error = toAppError(caught);
        failures += 1;
        lastError = error.message;
        const delay = retryDelaySecs(failures) * 1000;
        if (!canRetryAlone(error) || Date.now() + delay > deadline) {
          setProgress({ failures, lastError, secondsLeft: 0 });
          setStopped(true);
          return;
        }
        nextAttemptAt = Date.now() + delay;
        setProgress({ failures, lastError, secondsLeft: secondsLeft() });
        timer = window.setTimeout(attempt, delay);
      }
    };
    timer = window.setTimeout(attempt, retryDelaySecs(0) * 1000);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      window.clearInterval(ticker);
    };
  }, [active, sessionId, windowMinutes]);

  return { active, ...progress, stop: () => setStopped(true) };
}
