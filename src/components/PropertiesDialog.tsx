import { useEffect, useRef, useState, type FormEvent } from "react";
import { useLatest } from "../hooks/useLatest";
import { englishCount, formatDate, formatSize } from "../lib/format";
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
import { extensionLabel } from "../lib/sort";
import type {
  FileDetails,
  FileEntry,
  FolderUsage,
  ModeChange,
  OperationFailure,
  PermissionSummary,
} from "../lib/types";
import { forgetListing } from "../state/listings";
import { useLogStore } from "../state/logStore";
import { track, useOperationStore } from "../state/operationStore";
import { useSettingsStore } from "../state/settingsStore";
import { Dialog } from "./Dialog";
import { midSentence, t, useT } from "../i18n";

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
    if (entry.linkTarget === "dir") return t("dialogs.linkToAFolder");
    if (entry.linkTarget === "file") return t("dialogs.linkToAFile");
    return t("fileList.brokenLink");
  }
  if (entry.kind === "other") return t("dialogs.specialFile");
  if (entry.kind === "dir") return t("fileList.folder");
  const extension = extensionLabel(entry);
  return extension ? t("dialogs.extensionFile", { extension }) : t("fileList.file");
}

/** For the log: `Changed permissions of 3 items, group to staff in /srv/www`. */
function describeChange(
  summary: PermissionSummary,
  owner: string | null,
  group: string | null,
  folder: string,
): string {
  const parts = [
    summary.changed > 0 && `permissions of ${englishCount(summary.changed, "item")}`,
    owner && `owner to ${owner}`,
    group && `group to ${group}`,
  ].filter(Boolean);
  const links =
    summary.skippedLinks > 0
      ? `; ${englishCount(summary.skippedLinks, "link")} kept their own permissions`
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
  const t = useT();
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const latestOnClose = useLatest(onClose);
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
      const counted = await measuring.run(t("dialogs.measuringFolderSize"), (operationId) =>
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

  /** What the folder's listing says of the entries changed, and of all inside them if asked. */
  const changed = () => {
    forgetListing(sessionId, folder, recursive ? entries.map((entry) => entry.path) : []);
    onApplied();
  };

  const apply = async (event: FormEvent) => {
    event.preventDefault();
    if (nothingToApply) {
      onClose();
      return;
    }
    setError(null);
    setFailures([]);
    const what = single ? single.name : t("dialogs.items", { count: entries.length });
    try {
      const summary = await applying.run(
        t("dialogs.changingPermissionsOf", { what }),
        (operationId) =>
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
      changed();
      if (summary.failures.length > 0) setFailures(summary.failures);
      else latestOnClose.current();
    } catch (caught) {
      const failure = toAppError(caught);
      changed();
      if (failure.kind !== "cancelled") setError(failure.message);
    }
  };

  const setMask = (states: BitStates) => setMasks((current) => ({ ...current, [mask]: states }));

  const sizeText = () => {
    if (usage) {
      const parts = [
        t("dialogs.files", { count: usage.files }),
        usage.folders > 0 && t("dialogs.folders", { count: usage.folders }),
      ].filter(Boolean);
      return `${formatSize(usage.bytes)} (${parts.join(", ")})`;
    }
    if (measuring.progress) {
      return t("dialogs.countingFiles", {
        count: measuring.progress.files,
        size: formatSize(measuring.progress.bytes),
      });
    }
    if (folders.length + links === 0) {
      return formatSize(files.reduce((total, entry) => total + entry.size, 0));
    }
    return null;
  };
  const size = sizeText();
  /** A field's name, with the user or group number of a single item after it. */
  const withId = (name: string, value: number | null | undefined) =>
    !single || value === null || value === undefined
      ? name
      : t("dialogs.nameWithId", { name, id: String(value) });

  return (
    <Dialog
      title={
        single
          ? t("dialogs.propertiesOf", { name: single.name })
          : t("dialogs.propertiesOfItems", { count: entries.length })
      }
      onClose={onClose}
      width={500}
    >
      <form className="form properties" onSubmit={apply}>
        <dl className="properties-facts">
          {!single && (
            <>
              <dt>{t("dialogs.items2")}</dt>
              <dd>
                {[
                  folders.length > 0 && t("dialogs.folders", { count: folders.length }),
                  files.length > 0 && t("dialogs.files", { count: files.length }),
                  links > 0 && t("dialogs.links", { count: links }),
                ]
                  .filter(Boolean)
                  .join(", ")}
              </dd>
            </>
          )}
          {single && (
            <>
              <dt>{t("dialogs.type")}</dt>
              <dd>{kindLabel(single)}</dd>
            </>
          )}
          <dt>{t("dialogs.location")}</dt>
          <dd className="selectable">{folder}</dd>
          {details?.linkTarget && (
            <>
              <dt>{t("dialogs.pointsTo")}</dt>
              <dd className="selectable">{details.linkTarget}</dd>
            </>
          )}
          <dt>{t("common.size")}</dt>
          <dd>
            {size}
            {!usage && !measuring.busy && folders.length + links > 0 && (
              <button type="button" className="button button-small" onClick={calculate}>
                {t("dialogs.calculate")}
              </button>
            )}
            {measuring.busy && (
              <button type="button" className="button button-small" onClick={measuring.cancel}>
                {t("common.stop")}
              </button>
            )}
          </dd>
          {single && (
            <>
              <dt>{t("dialogs.modified")}</dt>
              <dd>{formatDate(single.modified, dateFormat)}</dd>
              {details?.accessed !== undefined && details.accessed !== null && (
                <>
                  <dt>{t("dialogs.accessed")}</dt>
                  <dd>{formatDate(details.accessed, dateFormat)}</dd>
                </>
              )}
            </>
          )}
        </dl>

        <div className="form-row">
          <label className="field grow">
            <span>{withId(t("dialogs.owner"), details?.uid)}</span>
            <input
              value={owner}
              placeholder={originals.owner ? undefined : t("dialogs.mixedUnchanged")}
              spellCheck={false}
              onChange={(event) => setOwner(event.target.value)}
            />
          </label>
          <label className="field grow">
            <span>{withId(t("dialogs.group"), details?.gid)}</span>
            <input
              value={group}
              placeholder={originals.group ? undefined : t("dialogs.mixedUnchanged")}
              spellCheck={false}
              onChange={(event) => setGroup(event.target.value)}
            />
          </label>
        </div>

        {hasPermissions && (
          <fieldset className="permissions">
            <legend>{t("dialogs.permissions")}</legend>
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
                    {kind === "folders" ? t("dialogs.folders2") : t("dialogs.files2")}
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
            {t("dialogs.applyToEverythingInside", { count: folders.length })}
          </label>
        )}
        {links > 0 && hasPermissions && (
          <p className="field-hint">{t("dialogs.linksHaveNoPermissionsOf")}</p>
        )}

        {error && <p className="form-error">{error}</p>}
        {failures.length > 0 && (
          <div className="form-error">
            <p>
              {failures.length === 1
                ? t("dialogs.oneItemCouldNotBe")
                : t("dialogs.itemsCouldNotBeChanged", { count: failures.length })}
            </p>
            <ul className="failure-list">
              {failures.slice(0, SHOWN_FAILURES).map((failure) => (
                <li key={failure.path}>
                  {baseName(failure.path)}: {failure.message}
                </li>
              ))}
              {failures.length > SHOWN_FAILURES && (
                <li>
                  {t("dialogs.andMoreListedInThe", { count: failures.length - SHOWN_FAILURES })}
                </li>
              )}
            </ul>
          </div>
        )}

        <div className="dialog-actions">
          {applying.busy && (
            <span className="field-hint push-left">
              {applying.progress && applying.progress.files > 0
                ? t("dialogs.changedItems", { count: applying.progress.files })
                : t("dialogs.applying")}
            </span>
          )}
          <button
            type="button"
            className="button"
            onClick={applying.busy ? applying.cancel : onClose}
          >
            {applying.busy
              ? t("common.stop")
              : failures.length > 0
                ? t("common.close")
                : t("common.cancel")}
          </button>
          <button
            type="submit"
            data-autofocus
            className="button button-primary"
            disabled={applying.busy}
          >
            {nothingToApply ? "OK" : t("common.apply")}
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
  const t = useT();
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
                      label={t("dialogs.permissionOfClass", {
                        who: permissionClass.label,
                        permission: midSentence(kind.label),
                      })}
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
          <span>{t("dialogs.octal")}</span>
          <input
            className="permission-octal"
            value={octalDraft ?? toOctal(states) ?? ""}
            placeholder={t("dialogs.mixed")}
            inputMode="numeric"
            maxLength={4}
            spellCheck={false}
            onChange={(event) => typeOctal(event.target.value)}
            onBlur={() => setOctalDraft(null)}
          />
        </label>
        <code className="permission-symbolic" title={t("dialogs.questionMarksAreLeftAs")}>
          {symbolic(states)}
        </code>
      </div>
    </div>
  );
}
