import { useEffect, useRef, useState, type FormEvent } from "react";
import { formatDate, formatSize, pluralize } from "../lib/format";
import { fileOperations, toAppError } from "../lib/ipc";
import { baseName } from "../lib/path";
import {
  PERMISSION_CLASSES,
  PERMISSION_KINDS,
  SPECIAL_BITS,
  bitStates,
  isUnchanged,
  nextState,
  parseOctal,
  statesFromMode,
  symbolic,
  toModeChange,
  toOctal,
  type BitState,
  type BitStates,
} from "../lib/permissions";
import { typeLabel } from "../lib/sort";
import type {
  FileDetails,
  FileEntry,
  FolderUsage,
  ModeChange,
  OperationFailure,
  PermissionSummary,
} from "../lib/types";
import { useLogStore } from "../state/logStore";
import { track, useOperationStore } from "../state/operationStore";
import { useSettingsStore } from "../state/settingsStore";
import { Dialog } from "./Dialog";

interface PropertiesDialogProps {
  sessionId: string;
  /** The folder the entries are in. */
  folder: string;
  entries: FileEntry[];
  onClose: () => void;
  /** After a change went through, even in part. */
  onApplied: () => void;
}

type MaskKind = "folders" | "files";

const NO_CHANGE: ModeChange = { set: 0, clear: 0 };
const SHOWN_FAILURES = 4;

/** What a selection agrees on, or an empty string when its items differ. */
function shared(values: (string | null)[]): string {
  const first = values[0] ?? "";
  return values.every((value) => (value ?? "") === first) ? first : "";
}

function modesOf(entries: FileEntry[]): number[] {
  return entries.flatMap((entry) => (entry.permissions === null ? [] : [entry.permissions]));
}

function kindLabel(entry: FileEntry): string {
  if (entry.kind === "symlink") {
    if (entry.linkTarget === "dir") return "Link to a folder";
    if (entry.linkTarget === "file") return "Link to a file";
    return "Broken link";
  }
  if (entry.kind === "other") return "Special file";
  if (entry.kind === "dir") return "Folder";
  const type = typeLabel(entry);
  return type === "File" ? type : `${type} file`;
}

/** For the log: `Changed permissions of 3 items, group to staff in /srv/www`. */
function describeChange(
  summary: PermissionSummary,
  owner: string | null,
  group: string | null,
  folder: string,
): string {
  const parts = [
    summary.changed > 0 && `permissions of ${pluralize(summary.changed, "item")}`,
    owner && `owner to ${owner}`,
    group && `group to ${group}`,
  ].filter(Boolean);
  const links =
    summary.skippedLinks > 0
      ? `; ${pluralize(summary.skippedLinks, "link")} kept their own permissions`
      : "";
  return `Changed ${parts.join(", ") || "nothing"} in ${folder}${links}`;
}

/** The id of the operation in progress, so closing the dialog can cancel it. */
function useOperation() {
  const [operationId, setOperationId] = useState<string | null>(null);
  const progress = useOperationStore((state) =>
    state.running.find((operation) => operation.id === operationId),
  );
  const running = useRef<string | null>(null);
  useEffect(
    () => () => {
      if (running.current) void fileOperations.cancel(running.current);
    },
    [],
  );
  const run = async <T,>(title: string, work: (operationId: string) => Promise<T>) => {
    try {
      return await track(title, (id) => {
        running.current = id;
        setOperationId(id);
        return work(id);
      });
    } finally {
      running.current = null;
      setOperationId(null);
    }
  };
  const cancel = () => {
    if (operationId) void fileOperations.cancel(operationId);
  };
  return { busy: operationId !== null, progress, run, cancel };
}

export function PropertiesDialog({
  sessionId,
  folder,
  entries,
  onClose,
  onApplied,
}: PropertiesDialogProps) {
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const single = entries.length === 1 ? entries[0] : null;
  const folders = entries.filter((entry) => entry.kind === "dir");
  const files = entries.filter((entry) => entry.kind === "file" || entry.kind === "other");
  const links = entries.length - folders.length - files.length;

  const [details, setDetails] = useState<FileDetails | null>(null);
  const [usage, setUsage] = useState<FolderUsage | null>(null);
  const [recursive, setRecursive] = useState(false);
  const [originals] = useState(() => ({
    folders: bitStates(modesOf(folders)),
    files: bitStates(modesOf(files)),
    owner: shared(entries.map((entry) => entry.owner)),
    group: shared(entries.map((entry) => entry.group)),
  }));
  const [masks, setMasks] = useState<Record<MaskKind, BitStates>>({
    folders: originals.folders,
    files: originals.files,
  });
  const [maskShown, setMaskShown] = useState<MaskKind>(folders.length > 0 ? "folders" : "files");
  const [owner, setOwner] = useState(originals.owner);
  const [group, setGroup] = useState(originals.group);
  const [error, setError] = useState<string | null>(null);
  const [failures, setFailures] = useState<OperationFailure[]>([]);
  const measuring = useOperation();
  const applying = useOperation();

  useEffect(() => {
    if (!single) return;
    let current = true;
    fileOperations
      .details(sessionId, single.path)
      .then((found) => current && setDetails(found))
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [sessionId, single]);

  const bothMasks = folders.length > 0 && (files.length > 0 || recursive);
  const mask: MaskKind = bothMasks ? maskShown : folders.length > 0 ? "folders" : "files";
  const hasPermissions = folders.length + files.length > 0;
  const changedText = (value: string, original: string) =>
    value.trim() !== original && value.trim() !== "" ? value.trim() : null;
  const newOwner = changedText(owner, originals.owner);
  const newGroup = changedText(group, originals.group);
  const modeChange = (kind: MaskKind) =>
    isUnchanged(masks[kind], originals[kind]) ? NO_CHANGE : toModeChange(masks[kind]);
  const nothingToApply =
    !newOwner &&
    !newGroup &&
    modeChange("folders") === NO_CHANGE &&
    modeChange("files") === NO_CHANGE;

  const calculate = async () => {
    setError(null);
    try {
      const counted = await measuring.run("Measuring folder size", (operationId) =>
        fileOperations.measure(
          operationId,
          sessionId,
          entries.map((entry) => entry.path),
        ),
      );
      setUsage(counted);
    } catch (caught) {
      const failure = toAppError(caught);
      if (failure.kind !== "cancelled") setError(failure.message);
    }
  };

  const apply = async (event: FormEvent) => {
    event.preventDefault();
    if (nothingToApply) {
      onClose();
      return;
    }
    setError(null);
    setFailures([]);
    const what = single ? single.name : pluralize(entries.length, "item");
    try {
      const summary = await applying.run(`Changing permissions of ${what}`, (operationId) =>
        fileOperations.setPermissions({
          operationId,
          sessionId,
          paths: entries.map((entry) => entry.path),
          files: modeChange("files"),
          folders: modeChange("folders"),
          recursive,
          owner: newOwner,
          group: newGroup,
        }),
      );
      const write = useLogStore.getState().write;
      write("info", describeChange(summary, newOwner, newGroup, folder), sessionId);
      for (const failure of summary.failures) {
        write("error", `${failure.path}: ${failure.message}`, sessionId);
      }
      onApplied();
      if (summary.failures.length > 0) setFailures(summary.failures);
      else onClose();
    } catch (caught) {
      const failure = toAppError(caught);
      onApplied();
      if (failure.kind !== "cancelled") setError(failure.message);
    }
  };

  const setMask = (states: BitStates) => setMasks((current) => ({ ...current, [mask]: states }));

  const sizeText = () => {
    if (usage) {
      const parts = [
        pluralize(usage.files, "file"),
        usage.folders > 0 && pluralize(usage.folders, "folder"),
      ].filter(Boolean);
      return `${formatSize(usage.bytes)} (${parts.join(", ")})`;
    }
    if (measuring.progress) {
      return `Counting: ${pluralize(measuring.progress.files, "file")}, ${formatSize(measuring.progress.bytes)}`;
    }
    if (folders.length + links === 0) {
      return formatSize(files.reduce((total, entry) => total + entry.size, 0));
    }
    return null;
  };
  const size = sizeText();
  const id = (value: number | null | undefined) =>
    value === null || value === undefined ? "" : ` (${value})`;

  return (
    <Dialog
      title={single ? `Properties of ${single.name}` : `Properties of ${entries.length} items`}
      onClose={onClose}
      width={500}
    >
      <form className="form properties" onSubmit={apply}>
        <dl className="properties-facts">
          {!single && (
            <>
              <dt>Items</dt>
              <dd>
                {[
                  folders.length > 0 && pluralize(folders.length, "folder"),
                  files.length > 0 && pluralize(files.length, "file"),
                  links > 0 && pluralize(links, "link"),
                ]
                  .filter(Boolean)
                  .join(", ")}
              </dd>
            </>
          )}
          {single && (
            <>
              <dt>Type</dt>
              <dd>{kindLabel(single)}</dd>
            </>
          )}
          <dt>Location</dt>
          <dd className="selectable">{folder}</dd>
          {details?.linkTarget && (
            <>
              <dt>Points to</dt>
              <dd className="selectable">{details.linkTarget}</dd>
            </>
          )}
          <dt>Size</dt>
          <dd>
            {size}
            {!usage && !measuring.busy && folders.length + links > 0 && (
              <button type="button" className="button button-small" onClick={calculate}>
                Calculate
              </button>
            )}
            {measuring.busy && (
              <button type="button" className="button button-small" onClick={measuring.cancel}>
                Stop
              </button>
            )}
          </dd>
          {single && (
            <>
              <dt>Modified</dt>
              <dd>{formatDate(single.modified, dateFormat)}</dd>
              {details?.accessed !== undefined && details.accessed !== null && (
                <>
                  <dt>Accessed</dt>
                  <dd>{formatDate(details.accessed, dateFormat)}</dd>
                </>
              )}
            </>
          )}
        </dl>

        <div className="form-row">
          <label className="field grow">
            <span>Owner{single ? id(details?.uid) : ""}</span>
            <input
              value={owner}
              placeholder={originals.owner ? undefined : "Mixed, unchanged"}
              spellCheck={false}
              onChange={(event) => setOwner(event.target.value)}
            />
          </label>
          <label className="field grow">
            <span>Group{single ? id(details?.gid) : ""}</span>
            <input
              value={group}
              placeholder={originals.group ? undefined : "Mixed, unchanged"}
              spellCheck={false}
              onChange={(event) => setGroup(event.target.value)}
            />
          </label>
        </div>

        {hasPermissions && (
          <fieldset className="permissions">
            <legend>Permissions</legend>
            {bothMasks && (
              <div className="segmented" role="tablist">
                {(["folders", "files"] as const).map((kind) => (
                  <button
                    key={kind}
                    type="button"
                    role="tab"
                    aria-selected={mask === kind}
                    className={mask === kind ? "is-selected" : ""}
                    onClick={() => setMaskShown(kind)}
                  >
                    {kind === "folders" ? "Folders" : "Files"}
                  </button>
                ))}
              </div>
            )}
            <PermissionGrid key={mask} states={masks[mask]} onChange={setMask} />
          </fieldset>
        )}

        {folders.length > 0 && (
          <label className="check">
            <input
              type="checkbox"
              checked={recursive}
              onChange={(event) => setRecursive(event.target.checked)}
            />
            Apply to everything inside the {folders.length === 1 ? "folder" : "folders"}
          </label>
        )}
        {links > 0 && hasPermissions && (
          <p className="field-hint">
            Links have no permissions of their own, so they keep theirs. Owner and group changes
            apply to the links themselves.
          </p>
        )}

        {error && <p className="form-error">{error}</p>}
        {failures.length > 0 && (
          <div className="form-error">
            <p>
              {failures.length === 1
                ? "One item could not be changed:"
                : `${failures.length} items could not be changed:`}
            </p>
            <ul className="failure-list">
              {failures.slice(0, SHOWN_FAILURES).map((failure) => (
                <li key={failure.path}>
                  {baseName(failure.path)}: {failure.message}
                </li>
              ))}
              {failures.length > SHOWN_FAILURES && (
                <li>and {failures.length - SHOWN_FAILURES} more, listed in the log</li>
              )}
            </ul>
          </div>
        )}

        <div className="dialog-actions">
          {applying.busy && (
            <span className="field-hint push-left">
              {applying.progress && applying.progress.files > 0
                ? `Changed ${pluralize(applying.progress.files, "item")}`
                : "Applying..."}
            </span>
          )}
          <button
            type="button"
            className="button"
            onClick={applying.busy ? applying.cancel : onClose}
          >
            {applying.busy ? "Stop" : failures.length > 0 ? "Close" : "Cancel"}
          </button>
          <button
            type="submit"
            data-autofocus
            className="button button-primary"
            disabled={applying.busy}
          >
            {nothingToApply ? "OK" : "Apply"}
          </button>
        </div>
      </form>
    </Dialog>
  );
}

interface PermissionGridProps {
  states: BitStates;
  onChange: (states: BitStates) => void;
}

function TriStateBox({
  state,
  label,
  onToggle,
}: {
  state: BitState;
  label: string;
  onToggle: () => void;
}) {
  return (
    <input
      type="checkbox"
      aria-label={label}
      checked={state === "on"}
      ref={(box) => {
        if (box) box.indeterminate = state === "mixed";
      }}
      onChange={onToggle}
    />
  );
}

/** Read, write and execute for owner, group and others, the special bits, and the octal. */
function PermissionGrid({ states, onChange }: PermissionGridProps) {
  const [octalDraft, setOctalDraft] = useState<string | null>(null);
  const toggle = (bit: number) => {
    setOctalDraft(null);
    onChange({ ...states, [bit]: nextState(states[bit]) });
  };
  const typeOctal = (text: string) => {
    setOctalDraft(text);
    const mode = parseOctal(text);
    if (mode !== null) onChange(statesFromMode(mode));
  };

  return (
    <div className="permission-grid">
      <table>
        <thead>
          <tr>
            <th />
            {PERMISSION_KINDS.map((kind) => (
              <th key={kind.label} scope="col">
                {kind.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {PERMISSION_CLASSES.map((permissionClass) => (
            <tr key={permissionClass.label}>
              <th scope="row">{permissionClass.label}</th>
              {PERMISSION_KINDS.map((kind) => {
                const bit = kind.value << permissionClass.shift;
                return (
                  <td key={kind.label}>
                    <TriStateBox
                      state={states[bit]}
                      label={`${permissionClass.label} ${kind.label.toLowerCase()}`}
                      onToggle={() => toggle(bit)}
                    />
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="permission-specials">
        {SPECIAL_BITS.map((special) => (
          <label key={special.bit} className="check" title={special.hint}>
            <TriStateBox
              state={states[special.bit]}
              label={special.label}
              onToggle={() => toggle(special.bit)}
            />
            {special.label}
          </label>
        ))}
      </div>
      <div className="permission-summary">
        <label className="field">
          <span>Octal</span>
          <input
            className="permission-octal"
            value={octalDraft ?? toOctal(states) ?? ""}
            placeholder="Mixed"
            inputMode="numeric"
            maxLength={4}
            spellCheck={false}
            onChange={(event) => typeOctal(event.target.value)}
            onBlur={() => setOctalDraft(null)}
          />
        </label>
        <code className="permission-symbolic" title="Question marks are left as each item has them">
          {symbolic(states)}
        </code>
      </div>
    </div>
  );
}
