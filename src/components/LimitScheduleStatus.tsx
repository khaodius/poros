import { ArrowDown, ArrowUp, CalendarClock, CalendarOff } from "lucide-react";
import { useMinute } from "../hooks/useMinute";
import { transfers } from "../lib/ipc";
import { clockTime, scheduleTooltip, shownLimits } from "../lib/limitSchedule";
import type { LimitStatus } from "../lib/types";
import { openScheduleSettings } from "../state/scheduleSettingsReveal";
import { useTransferStore } from "../state/transferStore";

const DIRECTIONS = {
  upload: { Icon: ArrowUp, label: "Upload" },
  download: { Icon: ArrowDown, label: "Download" },
};

function useLimitStatus(): LimitStatus {
  return useTransferStore((state) => state.snapshot.stats.limits);
}

function overrideText(limits: LimitStatus): { label: string; title: string } {
  return limits.ignored
    ? { label: "Follow schedule", title: "Follow the speed limit schedule again" }
    : { label: "Ignore schedule", title: "Ignore the speed limit schedule until its next change" };
}

/**
 * In the status bar while a rule of the speed limit schedule sets the limits, or is set aside:
 * the limits it sets and until when, then a button that sets it aside or takes it up again.
 */
export function LimitScheduleStatus() {
  const limits = useLimitStatus();
  const now = new Date(useMinute());
  if (limits.rule === null) return null;
  const until = limits.until === null ? "" : ` until ${clockTime(limits.until, now)}`;
  const override = overrideText(limits);
  const OverrideIcon = limits.ignored ? CalendarClock : CalendarOff;
  return (
    <span className={`status-item status-schedule ${limits.ignored ? "is-ignored" : ""}`}>
      <button
        type="button"
        className="status-schedule-limits"
        title={`${scheduleTooltip(limits, now)}\nClick to change the schedule.`}
        onClick={openScheduleSettings}
      >
        {limits.ignored ? (
          `Schedule ignored${until}`
        ) : (
          <>
            {shownLimits(limits).map(({ direction, mode, text }) => {
              const { Icon, label } = DIRECTIONS[direction];
              return (
                <span key={direction} className={`status-schedule-limit is-${mode}`}>
                  <Icon size={12} aria-label={label} />
                  {text}
                </span>
              );
            })}
            {until}
          </>
        )}
      </button>
      <button
        type="button"
        className="status-schedule-override"
        title={override.title}
        aria-label={override.title}
        onClick={() => void transfers.ignoreSchedule(!limits.ignored)}
      >
        <OverrideIcon size={12} />
      </button>
    </span>
  );
}

/** The same choice in the transfer panel, shown while a rule of the schedule applies. */
export function ScheduleOverrideButton() {
  const limits = useLimitStatus();
  if (limits.rule === null) return null;
  const override = overrideText(limits);
  const OverrideIcon = limits.ignored ? CalendarClock : CalendarOff;
  return (
    <>
      <span className="toolbar-divider" />
      <button
        type="button"
        className="button button-small"
        title={override.title}
        onClick={() => void transfers.ignoreSchedule(!limits.ignored)}
      >
        <OverrideIcon size={13} />
        <span className="narrow-hidden">{override.label}</span>
      </button>
    </>
  );
}
