import { useEffect, useState } from "react";
import { COUNTDOWN_MESSAGE, COUNTDOWN_SECONDS, COUNTDOWN_TEXT } from "../lib/automation";
import { cancelCountdown, performNow, useCountdownStore } from "../state/whenDone";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

/** Counts down before closing Poros or locking, sleeping or shutting down the computer. */
export function WhenDoneCountdown() {
  const t = useT();
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
        {t.rich(COUNTDOWN_MESSAGE[countdown.action], {
          seconds: <strong>{t("automation.seconds", { count: seconds })}</strong>,
        })}
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
          {t("common.cancel")}
        </button>
      </div>
    </Dialog>
  );
}
