import { useEffect, useState } from "react";
import { COUNTDOWN_SECONDS, COUNTDOWN_TEXT } from "../lib/automation";
import { pluralize } from "../lib/format";
import { cancelCountdown, performNow, useCountdownStore } from "../state/whenDone";
import { Dialog } from "./Dialog";

/** Counts down before closing Poros or locking, sleeping or shutting down the computer. */
export function WhenDoneCountdown() {
  const countdown = useCountdownStore((state) => state.countdown);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!countdown) return;
    const tick = window.setInterval(() => setNow(Date.now()), 250);
    const fire = window.setTimeout(
      () => void performNow(countdown.action),
      countdown.deadline - Date.now(),
    );
    return () => {
      window.clearInterval(tick);
      window.clearTimeout(fire);
    };
  }, [countdown]);

  if (!countdown) return null;
  const text = COUNTDOWN_TEXT[countdown.action];
  const seconds = Math.min(
    COUNTDOWN_SECONDS,
    Math.max(0, Math.ceil((countdown.deadline - now) / 1000)),
  );

  return (
    <Dialog title={text.title} onClose={cancelCountdown} width={400}>
      <p className="countdown-text">
        The transfer queue has finished. {text.title} in{" "}
        <strong>{pluralize(seconds, "second")}</strong>.
      </p>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={() => void performNow(countdown.action)}>
          {text.confirm}
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          onClick={cancelCountdown}
        >
          Cancel
        </button>
      </div>
    </Dialog>
  );
}
