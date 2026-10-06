import { useCallback, useEffect, useLayoutEffect, useRef, type PointerEvent } from "react";

const FIRST_REPEAT_DELAY_MILLIS = 380;
/** Every ten steps of a hold, the next ten go about three times faster. */
const REPEAT_INTERVALS_MILLIS = [130, 45, 15, 5];
export const STEPS_PER_SPEED_UP = 10;

/** The wait before the step after `stepsTaken` steps of one hold. */
export function repeatDelay(stepsTaken: number): number {
  if (stepsTaken <= 1) return FIRST_REPEAT_DELAY_MILLIS;
  const speed = Math.floor(stepsTaken / STEPS_PER_SPEED_UP);
  return REPEAT_INTERVALS_MILLIS[Math.min(speed, REPEAT_INTERVALS_MILLIS.length - 1)];
}

/**
 * Steps once on press and keeps stepping while the button is held, faster after every ten
 * steps. `onRelease` runs when the hold ends, so a caller can save once instead of every step.
 */
export function useHoldRepeat(onStep: () => void, onRelease: () => void) {
  const callbacks = useRef({ onStep, onRelease });
  useLayoutEffect(() => {
    callbacks.current = { onStep, onRelease };
  });
  const stopHold = useRef<(() => void) | null>(null);
  useEffect(() => () => stopHold.current?.(), []);

  const start = useCallback((event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    // Keeps the press from moving focus or starting a text selection.
    event.preventDefault();
    stopHold.current?.();
    let steps = 0;
    let timer = 0;
    const step = () => {
      callbacks.current.onStep();
      steps += 1;
      timer = window.setTimeout(step, repeatDelay(steps));
    };
    const stop = () => {
      window.clearTimeout(timer);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      stopHold.current = null;
      callbacks.current.onRelease();
    };
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
    stopHold.current = stop;
    step();
  }, []);

  const stop = useCallback(() => stopHold.current?.(), []);
  return { onPointerDown: start, onPointerLeave: stop };
}
