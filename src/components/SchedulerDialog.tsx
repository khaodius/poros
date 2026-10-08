import { useEffect, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  CalendarClock,
  CircleAlert,
  CircleCheck,
  FolderOpen,
  LoaderCircle,
  Plus,
} from "lucide-react";
import { formatDate } from "../lib/format";
import { toAppError } from "../lib/ipc";
import { protocolInfo } from "../lib/protocols";
import {
  INTERVAL_UNITS,
  MAX_INTERVAL_MINUTES,
  WEEKDAY_NAMES,
  convertTrigger,
  dateTimeInputValue,
  describeTrigger,
  editableTask,
  epochFromDateTimeInput,
  intervalMinutes,
  minuteOfDayFrom,
  newTask,
  sameTask,
  splitInterval,
  suggestedName,
  syncAction,
  timeInputValue,
  type IntervalUnit,
} from "../lib/schedule";
import { TIME_TOLERANCE_LIMITS } from "../lib/settings";
import { COMPARE_OPTIONS, DIRECTION_OPTIONS, excludePatterns } from "../lib/sync";
import type { SavedConnection, ScheduledTask, TaskAction, TaskView, Trigger } from "../lib/types";
import { useSavedConnections } from "../state/savedConnectionsStore";
import { useScheduleStore } from "../state/scheduleStore";
import { useSettingsStore } from "../state/settingsStore";
import { Dialog } from "./Dialog";
import { Stepper } from "./Stepper";

type SyncTaskAction = Extract<TaskAction, { type: "sync" }>;
type CommandTaskAction = Extract<TaskAction, { type: "command" }>;

const TRIGGER_KINDS: { value: Trigger["type"]; label: string }[] = [
  { value: "daily", label: "On days" },
  { value: "every", label: "Repeat" },
  { value: "once", label: "Once" },
];

/** Tasks synchronize and run commands over SSH, so only SFTP connections can run them. */
function taskConnections(connections: SavedConnection[]): SavedConnection[] {
  return connections
    .filter((connection) => connection.protocol === "sftp")
    .sort((first, second) => first.name.localeCompare(second.name));
}

function blankTask(): ScheduledTask {
  const connectionId = taskConnections(useSavedConnections.getState().connections)[0]?.id ?? "";
  const options = useSettingsStore.getState().settings.sync;
  return newTask(syncAction({ connectionId, localPath: "", remotePath: "", options }));
}

function excludesOf(task: ScheduledTask): string {
  return task.action.type === "sync" ? task.action.excludes.join("\n") : "";
}

interface SchedulerDialogProps {
  /** A task to start with instead of a blank one. */
  initialDraft?: ScheduledTask;
  onClose: () => void;
}

export function SchedulerDialog({ initialDraft, onClose }: SchedulerDialogProps) {
  const tasks = useScheduleStore((state) => state.tasks);
  const connections = useSavedConnections((state) => state.connections);
  const [draft, setDraft] = useState<ScheduledTask>(() => initialDraft ?? blankTask());
  /** Kept as typed, since blank lines vanish once split into patterns. */
  const [excludes, setExcludes] = useState(() => excludesOf(draft));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"saving" | "running" | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  useEffect(() => {
    void useScheduleStore
      .getState()
      .load()
      .catch((caught) => setError(toAppError(caught).message));
  }, []);

  const saved = draft.id ? tasks.find((task) => task.id === draft.id) : undefined;
  const connection = connections.find((candidate) => candidate.id === draft.action.connectionId);
  const candidates = taskConnections(connections);
  const unusable =
    draft.action.connectionId !== "" &&
    !candidates.some((candidate) => candidate.id === draft.action.connectionId);
  const suggested = suggestedName(draft.action);

  const select = (task: ScheduledTask) => {
    setDraft(editableTask(task));
    setExcludes(excludesOf(task));
    setError(null);
    setConfirmingDelete(false);
  };

  const update = (change: Partial<ScheduledTask>) =>
    setDraft((current) => ({ ...current, ...change }));
  const updateSync = (change: Partial<SyncTaskAction>) =>
    setDraft((current) =>
      current.action.type === "sync"
        ? { ...current, action: { ...current.action, ...change } }
        : current,
    );
  const updateCommand = (change: Partial<CommandTaskAction>) =>
    setDraft((current) =>
      current.action.type === "command"
        ? { ...current, action: { ...current.action, ...change } }
        : current,
    );

  const switchKind = (type: TaskAction["type"]) => {
    if (draft.action.type === type) return;
    const connectionId = draft.action.connectionId;
    update({
      action:
        type === "command"
          ? { type, connectionId, command: "", directory: null }
          : syncAction({
              connectionId,
              localPath: "",
              remotePath: "",
              options: useSettingsStore.getState().settings.sync,
            }),
    });
  };

  const taskToSave = (): ScheduledTask => {
    const action: TaskAction =
      draft.action.type === "sync"
        ? {
            ...draft.action,
            localPath: draft.action.localPath.trim(),
            remotePath: draft.action.remotePath.trim(),
            excludes: excludePatterns(excludes),
          }
        : {
            ...draft.action,
            command: draft.action.command.trim(),
            directory: draft.action.directory?.trim() || null,
          };
    return { ...draft, name: draft.name.trim() || suggestedName(action), action };
  };

  const save = async (): Promise<TaskView | null> => {
    setBusy("saving");
    setError(null);
    try {
      const stored = await useScheduleStore.getState().save(taskToSave());
      select(stored);
      return stored;
    } catch (caught) {
      setError(toAppError(caught).message);
      return null;
    } finally {
      setBusy(null);
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!busy) void save();
  };

  const runNow = async () => {
    let id = draft.id;
    if (!saved || !sameTask(taskToSave(), saved)) {
      const stored = await save();
      if (!stored) return;
      id = stored.id;
    }
    setBusy("running");
    try {
      await useScheduleStore.getState().runNow(id);
    } catch (caught) {
      setError(toAppError(caught).message);
    } finally {
      setBusy(null);
    }
  };

  const deleteTask = async () => {
    if (!draft.id) return;
    if (!confirmingDelete) {
      setConfirmingDelete(true);
      return;
    }
    try {
      await useScheduleStore.getState().remove(draft.id);
      select(blankTask());
    } catch (caught) {
      setError(toAppError(caught).message);
    }
  };

  return (
    <Dialog title="Scheduled tasks" onClose={onClose} width={880}>
      <div className="site-manager task-manager">
        <aside className="site-list">
          <div className="site-list-header">
            <span>Tasks</span>
            <button
              type="button"
              className="icon-button"
              title="New task"
              aria-label="New task"
              onClick={() => select(blankTask())}
            >
              <Plus size={15} />
            </button>
          </div>
          {tasks.length === 0 ? (
            <p className="site-list-empty">
              Tasks run while Poros is open. Ones you save appear here.
            </p>
          ) : (
            <ul>
              {tasks.map((task) => (
                <li key={task.id}>
                  <button
                    type="button"
                    className={`site-item ${draft.id === task.id ? "is-selected" : ""} ${task.enabled ? "" : "is-off"}`}
                    onClick={() => select(task)}
                  >
                    <TaskIcon task={task} />
                    <span className="site-item-text">
                      <span className="site-item-name">{task.name}</span>
                      <span className="site-item-detail">
                        {task.running
                          ? "Running now"
                          : task.enabled
                            ? describeTrigger(task.trigger)
                            : "Turned off"}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>

        <form className="form site-form task-form" onSubmit={submit}>
          <div className="form-row">
            <label className="field grow">
              <span>Name</span>
              <input
                data-autofocus
                value={draft.name}
                placeholder={suggested || "Shown in the list of tasks"}
                spellCheck={false}
                onChange={(event) => update({ name: event.target.value })}
              />
            </label>
            <label className="check task-enabled">
              <input
                type="checkbox"
                checked={draft.enabled}
                onChange={(event) => update({ enabled: event.target.checked })}
              />
              Turned on
            </label>
          </div>

          <div className="form-row">
            <div className="field">
              <span>Task</span>
              <div className="segmented" role="radiogroup">
                {(
                  [
                    ["sync", "Synchronize folders"],
                    ["command", "Run a command"],
                  ] as const
                ).map(([type, label]) => (
                  <button
                    key={type}
                    type="button"
                    role="radio"
                    aria-checked={draft.action.type === type}
                    className={draft.action.type === type ? "is-selected" : ""}
                    onClick={() => switchKind(type)}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
            <label className="field grow">
              <span>Saved connection</span>
              <select
                value={draft.action.connectionId}
                onChange={(event) =>
                  update({ action: { ...draft.action, connectionId: event.target.value } })
                }
              >
                <option value="">
                  {candidates.length === 0 ? "No saved SFTP connections" : "Choose a connection"}
                </option>
                {unusable && (
                  <option value={draft.action.connectionId}>
                    {connection?.name ?? "A deleted connection"}
                  </option>
                )}
                {candidates.map((candidate) => (
                  <option key={candidate.id} value={candidate.id}>
                    {candidate.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {unusable && (
            <p className="warning-text">
              {connection
                ? `${connection.name} uses ${protocolInfo(connection.protocol).label}; scheduled tasks need an SFTP connection.`
                : "This task's saved connection was deleted. Choose another."}
            </p>
          )}
          {!unusable && connection?.authType === "password" && !connection.saveSecret && (
            <p className="warning-text">
              This connection's password is not saved in the system keychain, so the task cannot log
              in. Save it in the connection's settings.
            </p>
          )}

          {draft.action.type === "sync" ? (
            <SyncFields
              action={draft.action}
              excludes={excludes}
              onChange={updateSync}
              onExcludes={setExcludes}
            />
          ) : (
            <CommandFields action={draft.action} onChange={updateCommand} />
          )}

          <TriggerFields trigger={draft.trigger} onChange={(trigger) => update({ trigger })} />

          <label className="check">
            <input
              type="checkbox"
              checked={draft.runMissed}
              onChange={(event) => update({ runMissed: event.target.checked })}
            />
            If Poros is closed at the time, run when it next starts
          </label>

          {saved && <TaskStatus task={saved} />}
          {error && <p className="form-error">{error}</p>}

          <div className="dialog-actions">
            {draft.id && (
              <button
                type="button"
                className={`button ${confirmingDelete ? "button-danger" : ""} push-left`}
                onClick={() => void deleteTask()}
              >
                {confirmingDelete ? "Delete for good" : "Delete"}
              </button>
            )}
            <button type="button" className="button" onClick={onClose}>
              Close
            </button>
            <button
              type="button"
              className="button"
              disabled={busy !== null || saved?.running}
              onClick={() => void runNow()}
            >
              {busy === "running" ? "Starting..." : "Run now"}
            </button>
            <button type="submit" className="button button-primary" disabled={busy !== null}>
              {busy === "saving" ? "Saving..." : "Save"}
            </button>
          </div>
        </form>
      </div>
    </Dialog>
  );
}

function TaskIcon({ task }: { task: TaskView }) {
  if (task.running) return <LoaderCircle size={14} className="spin" />;
  if (task.lastRun && !task.lastRun.succeeded) {
    return <CircleAlert size={14} className="task-failed" />;
  }
  return <CalendarClock size={14} />;
}

function TaskStatus({ task }: { task: TaskView }) {
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const next = !task.enabled
    ? "Turned off"
    : task.nextRun
      ? formatDate(task.nextRun, dateFormat)
      : "None, the time has passed";
  return (
    <div className="task-status">
      <span>
        <strong>Next run</strong> {next}
      </span>
      {task.running ? (
        <span>
          <LoaderCircle size={13} className="spin" /> Running now
        </span>
      ) : (
        task.lastRun && (
          <span className={task.lastRun.succeeded ? "" : "task-failed"}>
            {task.lastRun.succeeded ? <CircleCheck size={13} /> : <CircleAlert size={13} />}
            <strong>Last run</strong> {formatDate(task.lastRun.finished, dateFormat)}:{" "}
            {task.lastRun.message}
          </span>
        )
      )}
    </div>
  );
}

interface SyncFieldsProps {
  action: SyncTaskAction;
  excludes: string;
  onChange: (change: Partial<SyncTaskAction>) => void;
  onExcludes: (excludes: string) => void;
}

function SyncFields({ action, excludes, onChange, onExcludes }: SyncFieldsProps) {
  const both = action.direction === "both";
  const target = action.direction === "download" ? "in the local folder" : "on the server";
  const direction = DIRECTION_OPTIONS.find((option) => option.value === action.direction);

  const browse = async () => {
    const selected = await openFileDialog({
      title: "Choose the local folder",
      directory: true,
      multiple: false,
      defaultPath: action.localPath || undefined,
    });
    if (typeof selected === "string") onChange({ localPath: selected });
  };

  return (
    <>
      <div className="form-row">
        <div className="field grow">
          <span>Local folder</span>
          <div className="input-with-button">
            <input
              value={action.localPath}
              placeholder="The folder on this computer"
              spellCheck={false}
              onChange={(event) => onChange({ localPath: event.target.value })}
            />
            <button type="button" className="button" onClick={() => void browse()}>
              <FolderOpen size={14} /> Browse
            </button>
          </div>
        </div>
        <label className="field grow">
          <span>Server folder</span>
          <input
            value={action.remotePath}
            placeholder="/home/me/site"
            spellCheck={false}
            onChange={(event) => onChange({ remotePath: event.target.value })}
          />
        </label>
      </div>

      <div className="field">
        <span>Direction</span>
        <div className="segmented" role="radiogroup">
          {DIRECTION_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={action.direction === option.value}
              className={action.direction === option.value ? "is-selected" : ""}
              onClick={() =>
                onChange(
                  option.value === "both" && action.compare === "always"
                    ? { direction: option.value, compare: "sizeAndTime" }
                    : { direction: option.value },
                )
              }
            >
              {option.label}
            </button>
          ))}
        </div>
        {direction && (
          <small className="field-hint">
            {direction.hint} Conflicts are left alone and reported.
          </small>
        )}
      </div>

      <div className="form-row sync-compare">
        <label className="field grow">
          <span>Compare files by</span>
          <select
            value={action.compare}
            onChange={(event) =>
              onChange({ compare: event.target.value as SyncTaskAction["compare"] })
            }
          >
            {COMPARE_OPTIONS.map((option) => (
              <option
                key={option.value}
                value={option.value}
                disabled={both && option.value === "always"}
              >
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <div className="field">
          <span>Times this close count as equal</span>
          <Stepper
            value={action.timeToleranceSecs}
            min={TIME_TOLERANCE_LIMITS.min}
            max={TIME_TOLERANCE_LIMITS.max}
            label="Times this close count as equal"
            unit="s"
            onChange={(timeToleranceSecs) => onChange({ timeToleranceSecs })}
          />
        </div>
      </div>

      <div className="checks">
        <label className={`check ${both ? "is-disabled" : ""}`}>
          <input
            type="checkbox"
            checked={!both && action.deleteExtraneous}
            disabled={both}
            onChange={(event) => onChange({ deleteExtraneous: event.target.checked })}
          />
          Delete files {target} that the other side does not have
        </label>
        <label className={`check ${both ? "is-disabled" : ""}`}>
          <input
            type="checkbox"
            checked={!both && action.skipNewerOnTarget}
            disabled={both}
            onChange={(event) => onChange({ skipNewerOnTarget: event.target.checked })}
          />
          Skip files that are newer {target}
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={action.ignoreExisting}
            onChange={(event) => onChange({ ignoreExisting: event.target.checked })}
          />
          {both
            ? "Only copy files the other side does not have"
            : `Skip files that already exist ${target}`}
        </label>
      </div>

      <label className="field">
        <span>Exclude</span>
        <textarea
          value={excludes}
          rows={2}
          spellCheck={false}
          placeholder={"*.tmp\nnode_modules/"}
          onChange={(event) => onExcludes(event.target.value)}
        />
      </label>
    </>
  );
}

interface CommandFieldsProps {
  action: CommandTaskAction;
  onChange: (change: Partial<CommandTaskAction>) => void;
}

function CommandFields({ action, onChange }: CommandFieldsProps) {
  return (
    <div className="form-row">
      <label className="field grow">
        <span>Command</span>
        <input
          className="mono"
          value={action.command}
          placeholder="./backup.sh"
          spellCheck={false}
          autoCapitalize="off"
          onChange={(event) => onChange({ command: event.target.value })}
        />
      </label>
      <label className="field grow">
        <span>In folder (optional)</span>
        <input
          value={action.directory ?? ""}
          placeholder="The login folder"
          spellCheck={false}
          onChange={(event) => onChange({ directory: event.target.value })}
        />
      </label>
    </div>
  );
}

interface TriggerFieldsProps {
  trigger: Trigger;
  onChange: (trigger: Trigger) => void;
}

function TriggerFields({ trigger, onChange }: TriggerFieldsProps) {
  return (
    <div className="field">
      <span>When</span>
      <div className="trigger-row">
        <div className="segmented" role="radiogroup">
          {TRIGGER_KINDS.map((kind) => (
            <button
              key={kind.value}
              type="button"
              role="radio"
              aria-checked={trigger.type === kind.value}
              className={trigger.type === kind.value ? "is-selected" : ""}
              onClick={() => onChange(convertTrigger(trigger, kind.value, new Date()))}
            >
              {kind.label}
            </button>
          ))}
        </div>
        {trigger.type === "once" && (
          <DateTimeInput
            label="Date and time"
            value={trigger.at}
            onChange={(at) => onChange({ ...trigger, at })}
          />
        )}
        {trigger.type === "every" && <IntervalFields trigger={trigger} onChange={onChange} />}
        {trigger.type === "daily" && <DailyFields trigger={trigger} onChange={onChange} />}
      </div>
    </div>
  );
}

function DateTimeInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <input
      type="datetime-local"
      aria-label={label}
      value={dateTimeInputValue(value)}
      onChange={(event) => {
        const parsed = epochFromDateTimeInput(event.target.value);
        if (parsed !== null) onChange(parsed);
      }}
    />
  );
}

function IntervalFields({
  trigger,
  onChange,
}: {
  trigger: Extract<Trigger, { type: "every" }>;
  onChange: (trigger: Trigger) => void;
}) {
  const { amount, unit } = splitInterval(trigger.minutes);
  const perUnit = INTERVAL_UNITS.find((option) => option.value === unit)?.minutes ?? 1;
  const changeInterval = (nextAmount: number, nextUnit: IntervalUnit) =>
    onChange({
      ...trigger,
      minutes: Math.min(MAX_INTERVAL_MINUTES, intervalMinutes(nextAmount, nextUnit)),
    });
  return (
    <>
      <span className="trigger-label">every</span>
      <Stepper
        value={amount}
        min={1}
        max={Math.floor(MAX_INTERVAL_MINUTES / perUnit)}
        label="Repeat every"
        onChange={(next) => changeInterval(next, unit)}
      />
      <select
        value={unit}
        aria-label="Unit"
        onChange={(event) => changeInterval(amount, event.target.value as IntervalUnit)}
      >
        {INTERVAL_UNITS.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <span className="trigger-label">from</span>
      <DateTimeInput
        label="Starting"
        value={trigger.start}
        onChange={(start) => onChange({ ...trigger, start })}
      />
    </>
  );
}

function DailyFields({
  trigger,
  onChange,
}: {
  trigger: Extract<Trigger, { type: "daily" }>;
  onChange: (trigger: Trigger) => void;
}) {
  const toggleDay = (day: number) => {
    const chosen = trigger.weekdays.includes(day);
    // At least one day stays chosen.
    if (chosen && trigger.weekdays.length === 1) return;
    onChange({
      ...trigger,
      weekdays: chosen
        ? trigger.weekdays.filter((existing) => existing !== day)
        : [...trigger.weekdays, day].sort((first, second) => first - second),
    });
  };
  return (
    <>
      <span className="trigger-label">at</span>
      <input
        type="time"
        aria-label="Time of day"
        value={timeInputValue(trigger.minuteOfDay)}
        onChange={(event) => {
          const minuteOfDay = minuteOfDayFrom(event.target.value);
          if (minuteOfDay !== null) onChange({ ...trigger, minuteOfDay });
        }}
      />
      <div className="toggle-group" role="group" aria-label="Days of the week">
        {WEEKDAY_NAMES.map((name, day) => {
          const chosen = trigger.weekdays.includes(day);
          return (
            <button
              key={name}
              type="button"
              className={`toggle ${chosen ? "is-on" : ""}`}
              aria-pressed={chosen}
              onClick={() => toggleDay(day)}
            >
              {name}
            </button>
          );
        })}
      </div>
    </>
  );
}
