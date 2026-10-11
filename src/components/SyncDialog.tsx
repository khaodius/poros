import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { CalendarClock, FolderOpen, LoaderCircle } from "lucide-react";
import { useOptionCheck } from "../hooks/useOptionCheck";
import { localSource } from "../lib/fileSource";
import { pluralize } from "../lib/format";
import { onSyncProgress, sync, toAppError } from "../lib/ipc";
import { commandPreview, withOption } from "../lib/rsyncOptions";
import { newTask, syncAction } from "../lib/schedule";
import {
  DIRECTION_OPTIONS,
  chosenItems,
  describeRun,
  initialChoices,
  isWithin,
  type Choices,
} from "../lib/sync";
import type {
  OptionCheck,
  SyncDirection,
  SyncOptions,
  SyncPlanView,
  SyncProgress,
} from "../lib/types";
import { lastActivePane, listPanes } from "../state/paneRegistry";
import { useSavedConnections } from "../state/savedConnectionsStore";
import { useSessionStore, type SessionEntry } from "../state/sessionStore";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { useToastStore } from "../state/toastStore";
import { useUiStore } from "../state/uiStore";
import { Dialog } from "./Dialog";
import { OptionCheckView, RsyncArgumentsField, RsyncOptionsEditor } from "./RsyncOptionsEditor";
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
  /** The defaults as the dialog opened; changes made here stay with this synchronization. */
  const [defaults] = useState(() => useSettingsStore.getState().settings.sync);
  const sessionEntries = useSessionStore((state) => state.sessions);
  const savedConnections = useSavedConnections((state) => state.connections);
  const sessions = useMemo(
    () => Object.values(sessionEntries).filter(canSynchronize),
    [sessionEntries],
  );
  const [folders, setFolders] = useState(() => startingFolders(given));
  const [direction, setDirection] = useState<SyncDirection>(defaults.direction);
  const [options, setOptions] = useState<SyncOptions>(defaults.options);
  /** rsync arguments typed for this synchronization alone. */
  const [args, setArgs] = useState("");
  const [stage, setStage] = useState<Stage>({ step: "setup" });
  const [progress, setProgress] = useState<SyncProgress | null>(null);
  const [choices, setChoices] = useState<Choices>(new Map());
  const [error, setError] = useState<string | null>(null);
  /** The comparison or plan the backend holds for this dialog, to let go of on close. */
  const held = useRef<{ requestId?: string; planId?: string }>({});

  const session = sessions.find((entry) => entry.info.id === folders.sessionId);
  const savedConnectionId = session?.info.savedConnectionId;
  const site = savedConnections.find((connection) => connection.id === savedConnectionId);
  const siteArguments = site?.syncArguments ?? "";
  const check = useOptionCheck(options, [args], direction);

  // The options start from the defaults with the site's own arguments applied, and start over
  // when another site is chosen.
  useEffect(() => {
    let current = true;
    sync
      .checkOptions(defaults.options, [defaults.arguments, siteArguments], defaults.direction)
      .then((result) => {
        if (current) setOptions(result.options);
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [defaults, siteArguments]);

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

  const chooseDirection = (next: SyncDirection) => {
    setDirection(next);
    // Both ways, every file is compared by time to find the newer one.
    setOptions((current) => withOption(current, "compare", current.compare, next));
  };

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
    void saveSettingsSection("sync", { direction });
    try {
      const plan = await sync.compare({
        ...options,
        requestId,
        sessionId,
        localPath: folders.localPath.trim(),
        remotePath: folders.remotePath.trim(),
        direction,
        arguments: args,
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
      if (!summary.dryRun) refreshPanes(plan, sessionId);
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

  const filledIn =
    folders.localPath.trim() !== "" && folders.remotePath.trim() !== "" && session !== undefined;
  const canCompare = filledIn && (check?.errors.length ?? 0) === 0;

  /** Moves on to a scheduled task that runs this synchronization through the saved connection. */
  const schedule = () => {
    if (!savedConnectionId) return;
    void saveSettingsSection("sync", { direction });
    useUiStore.getState().open({
      kind: "schedules",
      draft: newTask(
        syncAction({
          connectionId: savedConnectionId,
          localPath: folders.localPath.trim(),
          remotePath: folders.remotePath.trim(),
          direction,
          options,
          arguments: args,
        }),
      ),
    });
  };

  const remote = session ? `${session.info.host}:${folders.remotePath.trim()}` : "";

  return (
    <Dialog title="Synchronize folders" onClose={onClose} width={860}>
      {stage.step === "setup" && (
        <SetupForm
          folders={folders}
          direction={direction}
          options={options}
          args={args}
          siteName={siteArguments.trim() ? site?.name : undefined}
          check={check}
          command={
            check
              ? commandPreview(check.arguments, direction, folders.localPath.trim(), remote)
              : undefined
          }
          sessions={sessions.map((entry) => ({ id: entry.info.id, label: entry.info.label }))}
          error={error}
          canCompare={canCompare}
          onFolders={(change) => setFolders((current) => ({ ...current, ...change }))}
          onSession={chooseSession}
          onBrowse={() => void browseLocal()}
          onDirection={chooseDirection}
          onOptions={setOptions}
          onArgs={setArgs}
          onSubmit={(event) => void compare(event)}
          onSchedule={canCompare && savedConnectionId ? schedule : undefined}
          onCancel={onClose}
        />
      )}
      {stage.step === "comparing" && <ComparingView progress={progress} onStop={stopComparing} />}
      {(stage.step === "preview" || stage.step === "running") && (
        <SyncPlanList
          plan={stage.plan}
          direction={direction}
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
  direction: SyncDirection;
  options: SyncOptions;
  args: string;
  /** The saved connection whose own rsync arguments the options start from, if it has any. */
  siteName?: string;
  check: OptionCheck | null;
  command?: string;
  sessions: { id: string; label: string }[];
  error: string | null;
  canCompare: boolean;
  onFolders: (change: Partial<Folders>) => void;
  onSession: (sessionId: string) => void;
  onBrowse: () => void;
  onDirection: (direction: SyncDirection) => void;
  onOptions: (options: SyncOptions) => void;
  onArgs: (args: string) => void;
  onSubmit: (event: FormEvent) => void;
  /** Absent while the folders are incomplete or the server's connection is not saved. */
  onSchedule?: () => void;
  onCancel: () => void;
}

function SetupForm({
  folders,
  direction,
  options,
  args,
  siteName,
  check,
  command,
  sessions,
  error,
  canCompare,
  onFolders,
  onSession,
  onBrowse,
  onDirection,
  onOptions,
  onArgs,
  onSubmit,
  onSchedule,
  onCancel,
}: SetupFormProps) {
  const chosen = DIRECTION_OPTIONS.find((option) => option.value === direction);

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
              aria-checked={direction === option.value}
              className={direction === option.value ? "is-selected" : ""}
              onClick={() => onDirection(option.value)}
            >
              {option.label}
            </button>
          ))}
        </div>
        {chosen && <small className="field-hint">{chosen.hint}</small>}
      </div>

      <div className="field">
        <span>rsync options</span>
        <small className="field-hint">
          {siteName
            ? `Starting from the defaults in Settings, with the rsync arguments saved for ${siteName}. Changes here apply to this synchronization only.`
            : "Starting from the defaults in Settings. Changes here apply to this synchronization only."}
        </small>
        <RsyncOptionsEditor
          options={options}
          direction={direction}
          onChange={onOptions}
          initiallyOpen={[]}
        />
      </div>
      <RsyncArgumentsField value={args} onChange={onArgs} />
      <OptionCheckView check={check} command={command} />

      {error && <p className="form-error">{error}</p>}

      <div className="dialog-actions">
        <button
          type="button"
          className="button push-left"
          disabled={!onSchedule}
          title={
            onSchedule
              ? "Run this synchronization at set times"
              : "Fill in both folders of a saved connection to schedule this"
          }
          onClick={onSchedule}
        >
          <CalendarClock size={14} /> Schedule...
        </button>
        <button type="button" className="button" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="button button-primary" disabled={!canCompare}>
          {options.dryRun ? "Compare (dry run)" : "Compare"}
        </button>
      </div>
    </form>
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
