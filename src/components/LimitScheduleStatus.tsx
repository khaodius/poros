import { ArrowDown, ArrowUp, CalendarClock, CalendarOff } from "lucide-react";
import { useMinute } from "../hooks/useMinute";
import { transfers } from "../lib/ipc";
import { clockTime, scheduleTooltip, shownLimits } from "../lib/limitSchedule";
import type { LimitStatus } from "../lib/types";
import { openScheduleSettings } from "../state/scheduleSettingsReveal";
import { useTransferStore } from "../state/transferStore";
import { t, useT } from "../i18n";

const DIRECTIONS = {
  upload: {
    Icon: ArrowUp,
    get label() {
      return t("common.upload");
    },
  },
  download: {
    Icon: ArrowDown,
    get label() {
      return t("common.download");
    },
  },
};

function useLimitStatus(): LimitStatus {
  return useTransferStore((state) => state.snapshot.stats.limits);
}

function overrideText(limits: LimitStatus): { label: string; title: string } {
  return limits.ignored
    ? { label: t("limits.followSchedule"), title: t("limits.followTheSpeedLimitSchedule") }
    : { label: t("limits.ignoreSchedule"), title: t("limits.ignoreTheSpeedLimitSchedule") };
}

/**
 * In the status bar while a rule of the speed limit schedule sets the limits, or is set aside:
 * the limits it sets and until when, then a button that sets it aside or takes it up again.
 */
export function LimitScheduleStatus() {
  const t = useT();
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
        title={t("limits.clickToChangeTheSchedule", {
          scheduleTooltip: scheduleTooltip(limits, now),
        })}
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
