import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { CircleAlert, FolderOpen, LoaderCircle } from "lucide-react";
import { localSource } from "../lib/fileSource";
import { pluralize } from "../lib/format";
import { onSyncProgress, sync, toAppError } from "../lib/ipc";
import { TIME_TOLERANCE_LIMITS, type SyncSettings } from "../lib/settings";
import {
  COMPARE_OPTIONS,
  DIRECTION_OPTIONS,
  chosenItems,
  describeRun,
  excludePatterns,
  initialChoices,
  isWithin,
  type Choices,
} from "../lib/sync";
import type { SyncPlanView, SyncProgress } from "../lib/types";
import { lastActivePane, listPanes } from "../state/paneRegistry";
import { useSessionStore, type SessionEntry } from "../state/sessionStore";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { useToastStore } from "../state/toastStore";
import { Dialog } from "./Dialog";
import { Stepper } from "./Stepper";
import { SyncPlanList } from "./SyncPlanList";

interface Folders {
  localPath: string;
  sessionId: string;
  remotePath: string;
}

type Stage =
  | { step: "setup" }
  | { step: "comparing"; requestId: string }
  | { step: "preview"; plan: SyncPlanView; sessionId: string }
  | { step: "running"; plan: SyncPlanView; sessionId: string };

interface SyncDialogProps {
  localPath?: string;
  sessionId?: string;
  remotePath?: string;
  onClose: () => void;
}

function remoteFolderOf(sessionId: string): string {
  const pane = lastActivePane("remote", sessionId);
  return pane?.path() ?? useSessionStore.getState().sessions[sessionId]?.info.initialPath ?? "";
}

/** Synchronization lists and runs commands over SSH, so it needs an SFTP connection. */
function canSynchronize(entry: SessionEntry): boolean {
  return entry.status === "connected" && entry.info.protocol === "sftp";
}

/** The folders given, with the panes looked at last filling in the rest. */
function startingFolders(given: Omit<SyncDialogProps, "onClose">): Folders {
  const connected = Object.values(useSessionStore.getState().sessions)
    .filter(canSynchronize)
    .map((entry) => entry.info.id);
  const sessionId =
    [given.sessionId, lastActivePane("remote")?.sessionId, connected[0]].find(
      (candidate) => candidate !== undefined && connected.includes(candidate),
    ) ?? "";
  return {
    localPath: given.localPath ?? lastActivePane("local")?.path() ?? "",
    sessionId,
    remotePath: given.remotePath ?? (sessionId ? remoteFolderOf(sessionId) : ""),
  };
}

/** Panes showing either folder, or a folder inside them, reload after a run. */
function refreshPanes(plan: SyncPlanView, sessionId: string): void {
  for (const pane of listPanes()) {
    const path = pane.path();
    if (path === null) continue;
    const inside =
      pane.kind === "local"
        ? isWithin(path, plan.localRoot, localSource.pathStyle)
        : pane.sessionId === sessionId && isWithin(path, plan.remoteRoot, "posix");
    if (inside) pane.refresh();
  }
}

export function SyncDialog({ onClose, ...given }: SyncDialogProps) {
  const defaults = useSettingsStore((state) => state.settings.sync);
  const preserveTimestamps = useSettingsStore(
    (state) => state.settings.transfers.preserveTimestamps,
  );
  const sessionEntries = useSessionStore((state) => state.sessions);
  const sessions = useMemo(
    () => Object.values(sessionEntries).filter(canSynchronize),
    [sessionEntries],
  );
  const [folders, setFolders] = useState(() => startingFolders(given));
  const [options, setOptions] = useState<SyncSettings>(defaults);
  const [stage, setStage] = useState<Stage>({ step: "setup" });
  const [progress, setProgress] = useState<SyncProgress | null>(null);
  const [choices, setChoices] = useState<Choices>(new Map());
  const [error, setError] = useState<string | null>(null);
  /** The comparison or plan the backend holds for this dialog, to let go of on close. */
  const held = useRef<{ requestId?: string; planId?: string }>({});

  useEffect(() => {
    if (stage.step !== "comparing") return;
    const subscription = onSyncProgress((update) => {
      if (update.requestId === stage.requestId) setProgress(update);
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, [stage]);

  useEffect(() => {
    const backend = held;
    return () => {
      const { requestId, planId } = backend.current;
      // Cleared, so a comparison finishing after this discards its plan.
      backend.current = {};
      if (requestId) void sync.cancel(requestId).catch(() => undefined);
      if (planId) void sync.discard(planId).catch(() => undefined);
    };
  }, []);

  const setOption = <K extends keyof SyncSettings>(key: K, value: SyncSettings[K]) =>
    setOptions((current) => {
      const next = { ...current, [key]: value };
      if (next.direction === "both" && next.compare === "always") next.compare = "sizeAndTime";
      return next;
    });

  const chooseSession = (sessionId: string) =>
    setFolders((current) => ({ ...current, sessionId, remotePath: remoteFolderOf(sessionId) }));

  const browseLocal = async () => {
    const selected = await openFileDialog({
      title: "Choose the local folder",
      directory: true,
      multiple: false,
      defaultPath: folders.localPath || undefined,
    });
    if (typeof selected === "string") {
      setFolders((current) => ({ ...current, localPath: selected }));
    }
  };

  const compare = async (event?: FormEvent) => {
    event?.preventDefault();
    const requestId = crypto.randomUUID();
    const sessionId = folders.sessionId;
    held.current = { requestId };
    setError(null);
    setProgress(null);
    setStage({ step: "comparing", requestId });
    void saveSettingsSection("sync", options);
    try {
      const plan = await sync.compare({
        requestId,
        sessionId,
        localPath: folders.localPath.trim(),
        remotePath: folders.remotePath.trim(),
        direction: options.direction,
        compare: options.compare,
        deleteExtraneous: options.deleteExtraneous,
        skipNewerOnTarget: options.skipNewerOnTarget,
        ignoreExisting: options.ignoreExisting,
        timeToleranceSecs: options.timeToleranceSecs,
        excludes: excludePatterns(options.excludes),
      });
      if (held.current.requestId !== requestId) {
        void sync.discard(plan.planId);
        return;
      }
      held.current = { planId: plan.planId };
      setChoices(initialChoices(plan.items));
      setStage({ step: "preview", plan, sessionId });
    } catch (caught) {
      if (held.current.requestId !== requestId) return;
      held.current = {};
      const failure = toAppError(caught);
      if (failure.kind !== "cancelled") setError(failure.message);
      setStage({ step: "setup" });
    }
  };

  const stopComparing = () => {
    const { requestId } = held.current;
    held.current = {};
    if (requestId) void sync.cancel(requestId);
    setStage({ step: "setup" });
  };

  const backToSetup = () => {
    const { planId } = held.current;
    held.current = {};
    if (planId) void sync.discard(planId);
    setError(null);
    setStage({ step: "setup" });
  };

  const run = async (plan: SyncPlanView, sessionId: string) => {
    setError(null);
    setStage({ step: "running", plan, sessionId });
    try {
      const summary = await sync.run({
        planId: plan.planId,
        choices: chosenItems(plan.items, choices),
      });
      held.current = {};
      refreshPanes(plan, sessionId);
      useToastStore
        .getState()
        .show(summary.failures.length > 0 ? "error" : "info", describeRun(summary));
      onClose();
    } catch (caught) {
      held.current = {};
      setError(toAppError(caught).message);
      setStage({ step: "preview", plan, sessionId });
    }
  };

  const canCompare =
    folders.localPath.trim() !== "" &&
    folders.remotePath.trim() !== "" &&
    sessions.some((entry) => entry.info.id === folders.sessionId);

  return (
    <Dialog title="Synchronize folders" onClose={onClose} width={860}>
      {stage.step === "setup" && (
        <SetupForm
          folders={folders}
          options={options}
          sessions={sessions.map((entry) => ({ id: entry.info.id, label: entry.info.label }))}
          preserveTimestamps={preserveTimestamps}
          error={error}
          canCompare={canCompare}
          onFolders={(change) => setFolders((current) => ({ ...current, ...change }))}
          onSession={chooseSession}
          onBrowse={() => void browseLocal()}
          onOption={setOption}
          onSubmit={(event) => void compare(event)}
          onCancel={onClose}
        />
      )}
      {stage.step === "comparing" && <ComparingView progress={progress} onStop={stopComparing} />}
      {(stage.step === "preview" || stage.step === "running") && (
        <SyncPlanList
          plan={stage.plan}
          direction={options.direction}
          choices={choices}
          running={stage.step === "running"}
          error={error}
          onChoices={setChoices}
          onBack={backToSetup}
          onCancel={onClose}
          onRun={() => void run(stage.plan, stage.sessionId)}
        />
      )}
    </Dialog>
  );
}

interface SetupFormProps {
  folders: Folders;
  options: SyncSettings;
  sessions: { id: string; label: string }[];
  preserveTimestamps: boolean;
  error: string | null;
  canCompare: boolean;
  onFolders: (change: Partial<Folders>) => void;
  onSession: (sessionId: string) => void;
  onBrowse: () => void;
  onOption: <K extends keyof SyncSettings>(key: K, value: SyncSettings[K]) => void;
  onSubmit: (event: FormEvent) => void;
  onCancel: () => void;
}

function SetupForm({
  folders,
  options,
  sessions,
  preserveTimestamps,
  error,
  canCompare,
  onFolders,
  onSession,
  onBrowse,
  onOption,
  onSubmit,
  onCancel,
}: SetupFormProps) {
  const both = options.direction === "both";
  const target = options.direction === "download" ? "in the local folder" : "on the server";
  const source = options.direction === "download" ? "on the server" : "in the local folder";
  const usesTimes = options.compare === "sizeAndTime" || both || options.skipNewerOnTarget;
  const direction = DIRECTION_OPTIONS.find((option) => option.value === options.direction);

  return (
    <form className="form sync-form" onSubmit={onSubmit}>
      <div className="field">
        <span>Local folder</span>
        <div className="input-with-button">
          <input
            data-autofocus
            value={folders.localPath}
            placeholder="The folder on this computer"
            spellCheck={false}
            onChange={(event) => onFolders({ localPath: event.target.value })}
          />
          <button type="button" className="button" onClick={onBrowse}>
            <FolderOpen size={14} /> Browse
          </button>
        </div>
      </div>
      <div className="form-row">
        <label className="field sync-server">
          <span>Server</span>
          <select
            value={folders.sessionId}
            disabled={sessions.length === 0}
            onChange={(event) => onSession(event.target.value)}
          >
            {sessions.length === 0 && <option value="">No SFTP server connected</option>}
            {sessions.map((session) => (
              <option key={session.id} value={session.id}>
                {session.label}
              </option>
            ))}
          </select>
        </label>
        <label className="field grow">
          <span>Server folder</span>
          <input
            value={folders.remotePath}
            placeholder="/home/me/site"
            spellCheck={false}
            onChange={(event) => onFolders({ remotePath: event.target.value })}
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
              aria-checked={options.direction === option.value}
              className={options.direction === option.value ? "is-selected" : ""}
              onClick={() => onOption("direction", option.value)}
            >
              {option.label}
            </button>
          ))}
        </div>
        {direction && <small className="field-hint">{direction.hint}</small>}
      </div>

      <div className="form-row sync-compare">
        <label className="field grow">
          <span>Compare files by</span>
          <select
            value={options.compare}
            onChange={(event) => onOption("compare", event.target.value as SyncSettings["compare"])}
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
        <div className={`field ${usesTimes ? "" : "is-disabled"}`}>
          <span>Times this close count as equal</span>
          <Stepper
            value={options.timeToleranceSecs}
            min={TIME_TOLERANCE_LIMITS.min}
            max={TIME_TOLERANCE_LIMITS.max}
            label="Times this close count as equal"
            unit="s"
            disabled={!usesTimes}
            onChange={(seconds) => onOption("timeToleranceSecs", seconds)}
          />
        </div>
      </div>

      <div className="checks">
        <Check
          label={`Delete files ${target} that are not ${source}`}
          checked={!both && options.deleteExtraneous}
          disabled={both}
          onChange={(checked) => onOption("deleteExtraneous", checked)}
        />
        <Check
          label={`Skip files that are newer ${target}`}
          checked={!both && options.skipNewerOnTarget}
          disabled={both}
          onChange={(checked) => onOption("skipNewerOnTarget", checked)}
        />
        <Check
          label={
            both
              ? "Only copy files the other side does not have"
              : `Skip files that already exist ${target}`
          }
          checked={options.ignoreExisting}
          onChange={(checked) => onOption("ignoreExisting", checked)}
        />
      </div>

      <label className="field">
        <span>Exclude</span>
        <textarea
          value={options.excludes}
          rows={3}
          spellCheck={false}
          placeholder={"*.tmp\nnode_modules/\n/build"}
          onChange={(event) => onOption("excludes", event.target.value)}
        />
        <small className="field-hint">
          One pattern per line, as in rsync: * and ? match within a name, ** across folders, a
          trailing / matches only folders and a leading / anchors to the top folder.
        </small>
      </label>

      {!preserveTimestamps && options.compare === "sizeAndTime" && (
        <p className="warning-text sync-warning">
          <CircleAlert size={14} />
          Keep modification times is off in the transfer settings, so copied files get new times and
          will look changed the next time you compare.
        </p>
      )}
      {error && <p className="form-error">{error}</p>}

      <div className="dialog-actions">
        <button type="button" className="button" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="button button-primary" disabled={!canCompare}>
          Compare
        </button>
      </div>
    </form>
  );
}

interface CheckProps {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}

function Check({ label, checked, disabled, onChange }: CheckProps) {
  return (
    <label className={`check ${disabled ? "is-disabled" : ""}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      {label}
    </label>
  );
}

function ComparingView({
  progress,
  onStop,
}: {
  progress: SyncProgress | null;
  onStop: () => void;
}) {
  const comparing = progress?.stage === "comparing";
  const fraction = comparing && progress.toCompare > 0 ? progress.compared / progress.toCompare : 0;
  return (
    <div className="sync-comparing">
      <LoaderCircle size={22} className="spin" />
      <p>
        {comparing
          ? `Comparing contents: ${progress.compared} of ${pluralize(progress.toCompare, "file")}`
          : `Listing folders: ${progress?.localEntries ?? 0} local and ${progress?.remoteEntries ?? 0} server entries`}
      </p>
      {comparing && (
        <span className="progress-track sync-comparing-track">
          <span className="progress-fill" style={{ width: `${fraction * 100}%` }} />
        </span>
      )}
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onStop}>
          Stop
        </button>
      </div>
    </div>
  );
}
