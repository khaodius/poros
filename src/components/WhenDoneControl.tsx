import { WHEN_DONE_LABELS } from "../lib/automation";
import { WHEN_DONE_ACTIONS, type WhenDoneAction } from "../lib/settings";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";

/** Picks what happens when the queue finishes; the details are in the automation settings. */
export function WhenDoneControl() {
  const whenDone = useSettingsStore((state) => state.settings.automation.whenDone);
  const hasCommand = useSettingsStore(
    (state) => state.settings.automation.whenDoneCommand.trim() !== "",
  );
  return (
    <label className="when-done-control" title="What happens when the queue finishes">
      <span className="narrow-hidden">When done</span>
      <select
        value={whenDone}
        aria-label="When the queue finishes"
        className={whenDone === "nothing" ? "" : "is-set"}
        onChange={(event) =>
          void saveSettingsSection("automation", { whenDone: event.target.value as WhenDoneAction })
        }
      >
        {WHEN_DONE_ACTIONS.map((action) => (
          <option
            key={action}
            value={action}
            disabled={action === "runCommand" && !hasCommand && whenDone !== action}
          >
            {WHEN_DONE_LABELS[action]}
          </option>
        ))}
      </select>
    </label>
  );
}
