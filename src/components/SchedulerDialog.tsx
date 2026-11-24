import { useEffect, useMemo, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  CalendarClock,
  CircleAlert,
  CircleCheck,
  FolderOpen,
  LoaderCircle,
  Plus,
} from "lucide-react";
import { useOptionCheck } from "../hooks/useOptionCheck";
import { formatDate } from "../lib/format";
import { toAppError } from "../lib/ipc";
import { protocolInfo } from "../lib/protocols";
import {
  INTERVAL_UNITS,
  MAX_INTERVAL_MINUTES,
  WEEKDAYS,
  weekdayName,
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
  taskOptions,
  timeInputValue,
  transferAction,
  type IntervalUnit,
  type TransferTaskAction,
} from "../lib/schedule";
import { commandPreview, withOption } from "../lib/rsyncOptions";
import { DIRECTION_OPTIONS } from "../lib/sync";
import type { SavedConnection, ScheduledTask, TaskAction, TaskView, Trigger } from "../lib/types";
import { useSavedConnections } from "../state/savedConnectionsStore";
import { useScheduleStore } from "../state/scheduleStore";
import { useSettingsStore } from "../state/settingsStore";
import { Dialog } from "./Dialog";
import { OptionCheckView, RsyncArgumentsField, RsyncOptionsEditor } from "./RsyncOptionsEditor";
import { Stepper } from "./Stepper";
import { TransferTaskFields } from "./TransferTaskFields";
import { t, useT } from "../i18n";

type SyncTaskAction = Extract<TaskAction, { type: "sync" }>;
type CommandTaskAction = Extract<TaskAction, { type: "command" }>;

const TRIGGER_KINDS: { value: Trigger["type"]; label: string }[] = [
  {
    value: "daily",
    get label() {
      return t("schedule.onDays");
    },
  },
  {
    value: "every",
    get label() {
      return t("schedule.repeat");
    },
  },
  {
    value: "once",
    get label() {
      return t("schedule.once");
    },
  },
];

const TASK_KINDS: { value: TaskAction["type"]; label: string }[] = [
  {
    value: "sync",
    get label() {
      return t("schedule.synchronizeFolders");
    },
  },
  {
    value: "transfer",
    get label() {
      return t("schedule.transferFiles");
    },
  },
  {
    value: "command",
    get label() {
      return t("schedule.runACommand");
    },
  },
];

/** What the connection is to a transfer task, by its direction. */
const TRANSFER_CONNECTION_LABELS: Record<TransferTaskAction["direction"], string> = {
  get upload() {
    return t("schedule.uploadTo");
  },
  get download() {
    return t("schedule.downloadFrom");
  },
  get relay() {
    return t("schedule.copyTo");
  },
};

/**
 * The connections a kind of task can use, by name: any for a transfer, while synchronizing and
 * running commands go over SSH, so only SFTP connections can do them.
 */
function taskConnections(
  connections: SavedConnection[],
  type: TaskAction["type"],
): SavedConnection[] {
  return connections
    .filter((connection) => type === "transfer" || connection.protocol === "sftp")
    .sort((first, second) => first.name.localeCompare(second.name));
}

/** The rsync arguments a new synchronization starts with: the defaults', then the site's own. */
function startingArguments(connectionId: string): string {
  const site = useSavedConnections
    .getState()
    .connections.find((connection) => connection.id === connectionId);
  return [useSettingsStore.getState().settings.sync.arguments, site?.syncArguments ?? ""]
    .map((text) => text.trim())
    .filter(Boolean)
    .join(" ");
}

/** A synchronization with the defaults from Settings. */
function newSyncAction(connectionId: string): TaskAction {
  const defaults = useSettingsStore.getState().settings.sync;
  return syncAction({
    connectionId,
    localPath: "",
    remotePath: "",
    direction: defaults.direction,
    options: defaults.options,
    arguments: startingArguments(connectionId),
  });
}

function blankTask(): ScheduledTask {
  const connectionId =
    taskConnections(useSavedConnections.getState().connections, "sync")[0]?.id ?? "";
  return newTask(newSyncAction(connectionId));
}

interface SchedulerDialogProps {
  /** A task to start with instead of a blank one. */
  initialDraft?: ScheduledTask;
  onClose: () => void;
}

export function SchedulerDialog({ initialDraft, onClose }: SchedulerDialogProps) {
  const t = useT();
  const tasks = useScheduleStore((state) => state.tasks);
  const connections = useSavedConnections((state) => state.connections);
  const [draft, setDraft] = useState<ScheduledTask>(() => initialDraft ?? blankTask());
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
  const candidates = taskConnections(connections, draft.action.type);
  const unusable =
    draft.action.connectionId !== "" &&
    !candidates.some((candidate) => candidate.id === draft.action.connectionId);
  const suggested = suggestedName(draft.action);
  const sourceId =
    draft.action.type === "transfer" && draft.action.direction === "relay"
      ? draft.action.sourceConnectionId
      : null;
  const source = sourceId ? connections.find((candidate) => candidate.id === sourceId) : undefined;
  const signedInWith = [connection, source].filter(
    (candidate): candidate is SavedConnection => candidate !== undefined,
  );

  const select = (task: ScheduledTask) => {
    setDraft(editableTask(task));
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
  const updateTransfer = (change: Partial<TransferTaskAction>) =>
    setDraft((current) =>
      current.action.type === "transfer"
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
          : type === "transfer"
            ? transferAction(connectionId)
            : newSyncAction(connectionId),
    });
  };

  const chooseConnection = (connectionId: string) =>
    setDraft((current) => {
      const action = current.action;
      // A new synchronization takes the site's own rsync arguments until they are edited.
      if (
        !current.id &&
        action.type === "sync" &&
        action.arguments === startingArguments(action.connectionId)
      ) {
        return {
          ...current,
          action: { ...action, connectionId, arguments: startingArguments(connectionId) },
        };
      }
      return { ...current, action: { ...action, connectionId } };
    });

  const taskToSave = (): ScheduledTask => {
    const action: TaskAction =
      draft.action.type === "sync"
        ? {
            ...draft.action,
            localPath: draft.action.localPath.trim(),
            remotePath: draft.action.remotePath.trim(),
          }
        : draft.action.type === "transfer"
          ? {
              ...draft.action,
              sourceConnectionId:
                draft.action.direction === "relay" ? draft.action.sourceConnectionId || null : null,
              destination: draft.action.destination.trim(),
              subfolder: draft.action.subfolder.trim().replace(/^\/+|\/+$/g, ""),
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
    <Dialog title={t("schedule.scheduledTasks")} onClose={onClose} width={880}>
      <div className="site-manager task-manager">
        <aside className="site-list">
          <div className="site-list-header">
            <span>{t("schedule.tasks")}</span>
            <button
              type="button"
              className="icon-button"
              title={t("schedule.newTask")}
              aria-label={t("schedule.newTask")}
              onClick={() => select(blankTask())}
            >
              <Plus size={15} />
            </button>
          </div>
          {tasks.length === 0 ? (
            <p className="site-list-empty">{t("schedule.tasksRunWhilePorosIsOpen")}</p>
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
                          ? t("schedule.runningNow")
                          : task.enabled
                            ? describeTrigger(task.trigger)
                            : t("schedule.turnedOff")}
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
              <span>{t("common.name")}</span>
              <input
                data-autofocus
                value={draft.name}
                placeholder={suggested || t("schedule.shownInTheListOf")}
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
              {t("schedule.turnedOn")}
            </label>
          </div>

          <div className="form-row">
            <div className="field">
              <span>{t("schedule.task")}</span>
              <div className="segmented" role="radiogroup">
                {TASK_KINDS.map((kind) => (
                  <button
                    key={kind.value}
                    type="button"
                    role="radio"
                    aria-checked={draft.action.type === kind.value}
                    className={draft.action.type === kind.value ? "is-selected" : ""}
                    onClick={() => switchKind(kind.value)}
                  >
                    {kind.label}
                  </button>
                ))}
              </div>
            </div>
            <label className="field grow">
              <span>
                {draft.action.type === "transfer"
                  ? TRANSFER_CONNECTION_LABELS[draft.action.direction]
                  : t("schedule.savedConnection")}
              </span>
              <select
                value={draft.action.connectionId}
                onChange={(event) => chooseConnection(event.target.value)}
              >
                <option value="">
                  {candidates.length > 0
                    ? t("schedule.chooseAConnection")
                    : draft.action.type === "transfer"
                      ? t("schedule.noSavedConnections")
                      : t("schedule.noSavedSftpConnections")}
                </option>
                {unusable && (
                  <option value={draft.action.connectionId}>
                    {connection?.name ?? t("schedule.aDeletedConnection")}
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
                ? t("schedule.usesSynchronizingAndRunningCommands", {
                    name: connection.name,
                    label: protocolInfo(connection.protocol).label,
                  })
                : t("schedule.thisTasksSavedConnectionWas")}
            </p>
          )}
          {!unusable && <UnsavedPasswords connections={signedInWith} />}

          {draft.action.type === "sync" ? (
            <SyncFields action={draft.action} host={connection?.host} onChange={updateSync} />
          ) : draft.action.type === "transfer" ? (
            <TransferTaskFields
              action={draft.action}
              connections={candidates}
              onChange={updateTransfer}
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
            {t("schedule.ifPorosIsClosedAt")}
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
                {confirmingDelete ? t("schedule.deleteForGood") : t("common.delete")}
              </button>
            )}
            <button type="button" className="button" onClick={onClose}>
              {t("common.close")}
            </button>
            <button
              type="button"
              className="button"
              disabled={busy !== null || saved?.running}
              onClick={() => void runNow()}
            >
              {busy === "running" ? t("schedule.starting2") : t("schedule.runNow")}
            </button>
            <button type="submit" className="button button-primary" disabled={busy !== null}>
              {busy === "saving" ? t("schedule.saving") : t("common.save")}
            </button>
          </div>
        </form>
      </div>
    </Dialog>
  );
}

/** Says which connections cannot sign in on their own, because their password is not kept. */
function UnsavedPasswords({ connections }: { connections: SavedConnection[] }) {
  const t = useT();
  const unsaved = connections.filter(
    (connection) => connection.authType === "password" && !connection.saveSecret,
  );
  if (unsaved.length === 0) return null;
  const names = unsaved.map((connection) => connection.name).join(" and ");
  return (
    <p className="warning-text">
      {connections.length === 1
        ? t("schedule.thisConnectionsPasswordIsNotSaved")
        : unsaved.length === 1
          ? t("schedule.thePasswordOfIsNot", { names })
          : t("schedule.thePasswordsOfAreNot", { names })}{" "}
      {unsaved.length === 1
        ? t("schedule.saveItInTheConnections")
        : t("schedule.saveThemInTheConnections")}
    </p>
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
  const t = useT();
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const next = !task.enabled
    ? t("schedule.turnedOff")
    : task.nextRun
      ? formatDate(task.nextRun, dateFormat)
      : t("schedule.noneTheTimeHasPassed");
  return (
    <div className="task-status">
      <span>
        <strong>{t("schedule.nextRun")}</strong> {next}
      </span>
      {task.running ? (
        <span>
          <LoaderCircle size={13} className="spin" /> {t("schedule.runningNow")}
        </span>
      ) : (
        task.lastRun && (
          <span className={task.lastRun.succeeded ? "" : "task-failed"}>
            {task.lastRun.succeeded ? <CircleCheck size={13} /> : <CircleAlert size={13} />}
            <strong>{t("schedule.lastRun")}</strong> {formatDate(task.lastRun.finished, dateFormat)}
            : {task.lastRun.message}
          </span>
        )
      )}
    </div>
  );
}

interface SyncFieldsProps {
  action: SyncTaskAction;
  /** The saved connection's server, for the command line shown. */
  host?: string;
  onChange: (change: Partial<SyncTaskAction>) => void;
}

function SyncFields({ action, host, onChange }: SyncFieldsProps) {
  const t = useT();
  const options = useMemo(() => taskOptions(action), [action]);
  const check = useOptionCheck(options, [action.arguments], action.direction);
  const direction = DIRECTION_OPTIONS.find((option) => option.value === action.direction);
  const remote = host ? `${host}:${action.remotePath.trim()}` : action.remotePath.trim();

  const browse = async () => {
    const selected = await openFileDialog({
      title: t("schedule.chooseTheLocalFolder"),
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
          <span>{t("schedule.localFolder")}</span>
          <div className="input-with-button">
            <input
              value={action.localPath}
              placeholder={t("schedule.theFolderOnThisComputer")}
              spellCheck={false}
              onChange={(event) => onChange({ localPath: event.target.value })}
            />
            <button type="button" className="button" onClick={() => void browse()}>
              <FolderOpen size={14} /> {t("common.browse")}
            </button>
          </div>
        </div>
        <label className="field grow">
          <span>{t("schedule.serverFolder")}</span>
          <input
            value={action.remotePath}
            placeholder={t("schedule.homeMeSite")}
            spellCheck={false}
            onChange={(event) => onChange({ remotePath: event.target.value })}
          />
        </label>
      </div>

      <div className="field">
        <span>{t("schedule.direction")}</span>
        <div className="segmented" role="radiogroup">
          {DIRECTION_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={action.direction === option.value}
              className={action.direction === option.value ? "is-selected" : ""}
              onClick={() =>
                onChange({
                  ...withOption(options, "compare", options.compare, option.value),
                  direction: option.value,
                })
              }
            >
              {option.label}
            </button>
          ))}
        </div>
        {direction && (
          <small className="field-hint">
            {t("schedule.conflictsAreLeftAloneAnd", { hint: direction.hint })}
          </small>
        )}
      </div>

      <div className="field">
        <span>{t("schedule.rsyncOptions")}</span>
        <RsyncOptionsEditor
          options={options}
          direction={action.direction}
          onChange={(next) => onChange(next)}
          initiallyOpen={[]}
        />
      </div>
      <RsyncArgumentsField
        value={action.arguments}
        hint={t("schedule.typedAsOnRsyncsCommand")}
        onChange={(value) => onChange({ arguments: value })}
      />
      <OptionCheckView
        check={check}
        command={
          check
            ? commandPreview(check.arguments, action.direction, action.localPath.trim(), remote)
            : undefined
        }
      />
    </>
  );
}

interface CommandFieldsProps {
  action: CommandTaskAction;
  onChange: (change: Partial<CommandTaskAction>) => void;
}

function CommandFields({ action, onChange }: CommandFieldsProps) {
  const t = useT();
  return (
    <div className="form-row">
      <label className="field grow">
        <span>{t("schedule.command")}</span>
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
        <span>{t("schedule.inFolderOptional")}</span>
        <input
          value={action.directory ?? ""}
          placeholder={t("schedule.theLoginFolder")}
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
  const t = useT();
  return (
    <div className="field">
      <span>{t("schedule.when")}</span>
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
            label={t("schedule.dateAndTime")}
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
  const t = useT();
  const { amount, unit } = splitInterval(trigger.minutes);
  const perUnit = INTERVAL_UNITS.find((option) => option.value === unit)?.minutes ?? 1;
  const changeInterval = (nextAmount: number, nextUnit: IntervalUnit) =>
    onChange({
      ...trigger,
      minutes: Math.min(MAX_INTERVAL_MINUTES, intervalMinutes(nextAmount, nextUnit)),
    });
  return (
    <>
      <span className="trigger-label">{t("schedule.every")}</span>
      <Stepper
        value={amount}
        min={1}
        max={Math.floor(MAX_INTERVAL_MINUTES / perUnit)}
        label={t("schedule.repeatEvery")}
        onChange={(next) => changeInterval(next, unit)}
      />
      <select
        value={unit}
        aria-label={t("schedule.unit")}
        onChange={(event) => changeInterval(amount, event.target.value as IntervalUnit)}
      >
        {INTERVAL_UNITS.map((option) => (
          <option key={option.value} value={option.value}>
            {t(option.label)}
          </option>
        ))}
      </select>
      <span className="trigger-label">{t("schedule.from")}</span>
      <DateTimeInput
        label={t("schedule.starting")}
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
  const t = useT();
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
      <span className="trigger-label">{t("schedule.at")}</span>
      <input
        type="time"
        aria-label={t("schedule.timeOfDay")}
        value={timeInputValue(trigger.minuteOfDay)}
        onChange={(event) => {
          const minuteOfDay = minuteOfDayFrom(event.target.value);
          if (minuteOfDay !== null) onChange({ ...trigger, minuteOfDay });
        }}
      />
      <div className="toggle-group" role="group" aria-label={t("schedule.daysOfTheWeek")}>
        {WEEKDAYS.map((day) => {
          const name = weekdayName(day);
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
