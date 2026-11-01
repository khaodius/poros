import { useEffect, useMemo, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FileSearch, KeyRound, UserRound } from "lucide-react";
import {
  IMPORT_SOURCES,
  defaultSelection,
  folderPrefix,
  plannedImports,
} from "../lib/connectionImport";
import { connectionDetail } from "../lib/connectDraft";
import { connectionImport, toAppError } from "../lib/ipc";
import type { ImportScan, ImportSource, ImportSourceInfo } from "../lib/types";
import { useToastStore } from "../state/toastStore";
import { ConnectionIcon } from "./ConnectionIcon";
import { Dialog } from "./Dialog";

/** What is being read or saved, to show on its button. */
type Busy = { kind: "scan"; source: ImportSource | null } | { kind: "import" } | null;

export function ImportConnectionsDialog({ onClose }: { onClose: () => void }) {
  const showToast = useToastStore((state) => state.show);
  const [sources, setSources] = useState<ImportSourceInfo[] | null>(null);
  const [scan, setScan] = useState<ImportScan | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [savePasswords, setSavePasswords] = useState(true);
  const [folderNames, setFolderNames] = useState(true);
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<string | null>(null);
  // The scan the backend still holds passwords for, to forget when the dialog closes.
  const pendingScan = useRef<string | null>(null);

  useEffect(() => {
    connectionImport
      .sources()
      .then(setSources)
      .catch((caught) => setError(toAppError(caught).message));
    return () => {
      if (pendingScan.current) void connectionImport.discard(pendingScan.current).catch(() => {});
    };
  }, []);

  const forgetScan = () => {
    if (pendingScan.current) void connectionImport.discard(pendingScan.current).catch(() => {});
    pendingScan.current = null;
  };

  const read = async (source: ImportSource | null, paths: string[]) => {
    if (busy) return;
    setBusy({ kind: "scan", source });
    setError(null);
    try {
      const found = await connectionImport.scan(source, paths);
      pendingScan.current = found.scanId;
      if (found.entries.length === 0 && found.skipped.length === 0) {
        setError(`There are no saved sites in ${found.origins.join(", ") || "that file"}.`);
      } else {
        setScan(found);
        setSelected(defaultSelection(found.entries));
      }
    } catch (caught) {
      setError(toAppError(caught).message);
    }
    setBusy(null);
  };

  const chooseFiles = async (source: ImportSource | null) => {
    const chosen = await openFileDialog({
      title: source ? `Import from ${IMPORT_SOURCES[source].label}` : "Import connections",
      multiple: true,
      directory: false,
    });
    const paths = chosen === null ? [] : Array.isArray(chosen) ? chosen : [chosen];
    if (paths.length > 0) await read(source, paths);
  };

  const back = () => {
    forgetScan();
    setScan(null);
    setError(null);
  };

  const planned = useMemo(
    () => (scan ? plannedImports(scan.entries, selected) : new Map<number, number | null>()),
    [scan, selected],
  );

  const runImport = async () => {
    if (!scan || busy || planned.size === 0) return;
    setBusy({ kind: "import" });
    setError(null);
    try {
      const outcome = await connectionImport.run({
        scanId: scan.scanId,
        selected: [...selected],
        savePasswords,
        folderNames,
      });
      pendingScan.current = null;
      const count = `${outcome.imported} connection${outcome.imported === 1 ? "" : "s"}`;
      const passwords =
        outcome.withoutPasswords > 0
          ? ` The system keychain is not available, so ${outcome.withoutPasswords} password${
              outcome.withoutPasswords === 1 ? " was" : "s were"
            } left out; Poros asks for them when you connect.`
          : "";
      if (outcome.failures.length === 0) {
        showToast(
          "info",
          `Imported ${count} from ${IMPORT_SOURCES[scan.source].label}.${passwords}`,
        );
        onClose();
        return;
      }
      setScan(null);
      setError(
        `Imported ${count}.${passwords} These could not be saved: ${outcome.failures.join("; ")}`,
      );
    } catch (caught) {
      setError(toAppError(caught).message);
    }
    setBusy(null);
  };

  return (
    <Dialog title="Import connections" onClose={onClose} width={680}>
      {scan ? (
        <ImportReview
          scan={scan}
          selected={selected}
          planned={planned}
          savePasswords={savePasswords}
          folderNames={folderNames}
          onSelect={setSelected}
          onSavePasswords={setSavePasswords}
          onFolderNames={setFolderNames}
        />
      ) : (
        <SourceList
          sources={sources}
          busy={busy}
          onRead={(source) => void read(source, [])}
          onChoose={(source) => void chooseFiles(source)}
        />
      )}

      {error && <p className="form-error">{error}</p>}

      <div className="dialog-actions">
        {scan ? (
          <button
            type="button"
            className="button push-left"
            disabled={busy !== null}
            onClick={back}
          >
            Back
          </button>
        ) : (
          <button
            type="button"
            className="button push-left"
            disabled={busy !== null}
            title="Poros works out which app wrote the file"
            onClick={() => void chooseFiles(null)}
          >
            <FileSearch size={14} />
            {busy?.kind === "scan" && busy.source === null ? "Reading..." : "Other file..."}
          </button>
        )}
        <button type="button" className="button" onClick={onClose}>
          Cancel
        </button>
        {scan && (
          <button
            type="button"
            className="button button-primary"
            disabled={busy !== null || planned.size === 0}
            onClick={() => void runImport()}
          >
            {busy?.kind === "import"
              ? "Importing..."
              : `Import ${planned.size} connection${planned.size === 1 ? "" : "s"}`}
          </button>
        )}
      </div>
    </Dialog>
  );
}

interface SourceListProps {
  sources: ImportSourceInfo[] | null;
  busy: Busy;
  onRead: (source: ImportSource) => void;
  onChoose: (source: ImportSource) => void;
}

function SourceList({ sources, busy, onRead, onChoose }: SourceListProps) {
  const firstImport = useRef<HTMLButtonElement>(null);
  const loaded = sources !== null;
  // The list arrives after the dialog has opened and focused what was there.
  useEffect(() => {
    if (loaded) firstImport.current?.focus();
  }, [loaded]);
  if (!sources) return <p className="field-hint">Looking for other apps...</p>;
  const firstFound = sources.find((info) => info.found.length > 0)?.source;
  return (
    <>
      <p className="field-hint import-intro">
        Poros copies the saved sites of these apps into its own saved connections. Nothing changes
        in the other app.
      </p>
      <ul className="import-sources">
        {sources.map((info) => {
          const text = IMPORT_SOURCES[info.source];
          const found = info.found.length > 0;
          const reading = busy?.kind === "scan" && busy.source === info.source;
          const where = found
            ? `Found ${info.found.join(", ")}`
            : info.places.length > 0
              ? `Not found in ${info.places.join(" or ")}. Choose ${text.files}`
              : `Choose ${text.files}`;
          return (
            <li key={info.source} className="import-source">
              <span className="import-source-text">
                <span className="import-source-name">{text.label}</span>
                <span className="import-source-detail" title={where}>
                  {where}
                </span>
                {info.note && <span className="import-source-note">{info.note}</span>}
              </span>
              <button
                type="button"
                className="button"
                disabled={busy !== null}
                onClick={() => onChoose(info.source)}
              >
                Choose file...
              </button>
              {found && (
                <button
                  type="button"
                  className="button button-primary"
                  ref={info.source === firstFound ? firstImport : undefined}
                  disabled={busy !== null}
                  onClick={() => onRead(info.source)}
                >
                  {reading ? "Reading..." : "Import"}
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </>
  );
}

interface ImportReviewProps {
  scan: ImportScan;
  selected: Set<number>;
  planned: Map<number, number | null>;
  savePasswords: boolean;
  folderNames: boolean;
  onSelect: (selected: Set<number>) => void;
  onSavePasswords: (save: boolean) => void;
  onFolderNames: (folderNames: boolean) => void;
}

function ImportReview({
  scan,
  selected,
  planned,
  savePasswords,
  folderNames,
  onSelect,
  onSavePasswords,
  onFolderNames,
}: ImportReviewProps) {
  const entries = scan.entries;
  const allSelected = entries.length > 0 && selected.size === entries.length;
  const someSelected = selected.size > 0 && !allSelected;
  const plannedEntries = [...planned.keys()].map((index) => entries[index]);
  const passwords = plannedEntries.filter((entry) => entry.hasPassword).length;
  const inFolders = plannedEntries.some((entry) => entry.folder.length > 0);

  const toggle = (index: number) => {
    const next = new Set(selected);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    onSelect(next);
  };

  return (
    <div className="import-review">
      <div className="import-review-header">
        <label className="check">
          <input
            type="checkbox"
            checked={allSelected}
            ref={(input) => {
              if (input) input.indeterminate = someSelected;
            }}
            onChange={() =>
              onSelect(allSelected ? new Set() : new Set(entries.map((_, index) => index)))
            }
          />
          {entries.length} site{entries.length === 1 ? "" : "s"} from{" "}
          {IMPORT_SOURCES[scan.source].label}
        </label>
        <span className="import-origin" title={scan.origins.join("\n")}>
          {scan.origins.join(", ")}
        </span>
      </div>

      {entries.length > 0 && (
        <ul className="import-entries">
          {entries.map((entry, index) => {
            const neededBy = planned.get(index);
            const broughtAlong = !selected.has(index) && neededBy != null;
            const jump = entry.jump != null ? entries[entry.jump] : undefined;
            const detail = [
              connectionDetail(entry.connection),
              jump && `through ${jump.connection.name}`,
              broughtAlong && `needed by ${entries[neededBy].connection.name}`,
            ]
              .filter(Boolean)
              .join(", ");
            return (
              <li key={index}>
                <label className={`import-entry ${broughtAlong ? "is-brought-along" : ""}`}>
                  <input
                    type="checkbox"
                    checked={planned.has(index)}
                    disabled={broughtAlong}
                    onChange={() => toggle(index)}
                  />
                  <ConnectionIcon
                    protocol={entry.connection.protocol}
                    size={14}
                    className="import-entry-icon"
                  />
                  <span className="import-entry-text">
                    <span className="import-entry-name">
                      {entry.folder.length > 0 && (
                        <span className="import-entry-folder">{folderPrefix(entry)}</span>
                      )}
                      {entry.connection.name}
                    </span>
                    <span className="import-entry-detail">{detail}</span>
                    {entry.notes.map((note) => (
                      <span key={note} className="import-entry-note">
                        {note}
                      </span>
                    ))}
                  </span>
                  <span className="import-entry-tags">
                    {entry.existingId && <span className="import-tag">Already saved</span>}
                    {entry.hasPassword && (
                      <span className="import-tag">
                        <KeyRound size={11} /> Password
                      </span>
                    )}
                    {entry.connection.authType === "publicKey" && (
                      <span className="import-tag" title={entry.connection.keyPath ?? undefined}>
                        <KeyRound size={11} /> Key file
                      </span>
                    )}
                    {entry.connection.authType === "agent" && (
                      <span className="import-tag">
                        <UserRound size={11} /> SSH agent
                      </span>
                    )}
                  </span>
                </label>
              </li>
            );
          })}
        </ul>
      )}

      {scan.skipped.length > 0 && (
        <details className="import-skipped">
          <summary>
            {scan.skipped.length} left out, since Poros cannot connect to them as they are
          </summary>
          <ul>
            {scan.skipped.map((skipped, index) => (
              <li key={index}>
                <strong>{skipped.name || "Unnamed"}</strong>: {skipped.reason}
              </li>
            ))}
          </ul>
        </details>
      )}

      {scan.warnings.map((warning) => (
        <p key={warning} className="field-hint">
          {warning}
        </p>
      ))}

      {(passwords > 0 || inFolders) && (
        <div className="checks">
          {passwords > 0 && (
            <label className="check">
              <input
                type="checkbox"
                checked={savePasswords}
                onChange={(event) => onSavePasswords(event.target.checked)}
              />
              Keep the {passwords} password{passwords === 1 ? "" : "s"} in the system keychain
            </label>
          )}
          {inFolders && (
            <label className="check">
              <input
                type="checkbox"
                checked={folderNames}
                onChange={(event) => onFolderNames(event.target.checked)}
              />
              Put folder names in front of connection names
            </label>
          )}
        </div>
      )}
    </div>
  );
}
