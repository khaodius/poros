import { useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { File, FilePlus, Folder, FolderOpen, FolderPlus, Plus, X } from "lucide-react";
import {
  PATTERN_PLACEHOLDERS,
  TASK_EXISTS_ACTIONS,
  expandFolderPattern,
  withDirection,
  withTaskItems,
  type TransferTaskAction,
} from "../lib/schedule";
import type { Direction, ExistsAction, SavedConnection } from "../lib/types";
import { t, useT } from "../i18n";

const DIRECTIONS: { value: Direction; label: string; hint: string }[] = [
  {
    value: "upload",
    get label() {
      return t("common.upload");
    },
    get hint() {
      return t("workspace.copiesFilesAndFoldersFrom");
    },
  },
  {
    value: "download",
    get label() {
      return t("common.download");
    },
    get hint() {
      return t("workspace.copiesFilesAndFoldersFromThe");
    },
  },
  {
    value: "relay",
    get label() {
      return t("workspace.betweenServers");
    },
    get hint() {
      return t("workspace.copiesFilesAndFoldersFromOne");
    },
  },
];

interface TransferTaskFieldsProps {
  action: TransferTaskAction;
  /** Every saved connection, in the order the list shows them. */
  connections: SavedConnection[];
  onChange: (change: Partial<TransferTaskAction>) => void;
}

export function TransferTaskFields({ action, connections, onChange }: TransferTaskFieldsProps) {
  const t = useT();
  const [typedPath, setTypedPath] = useState("");
  const direction = DIRECTIONS.find((option) => option.value === action.direction);
  // The items are on this computer for an upload, on a server otherwise.
  const local = action.direction === "upload";
  const pattern = expandFolderPattern(action.subfolder, new Date());
  const sourceKnown = connections.some((connection) => connection.id === action.sourceConnectionId);

  const pick = async (folders: boolean) => {
    const selected = await openFileDialog({
      title: folders ? t("workspace.addFolders") : t("workspace.addFiles"),
      directory: folders,
      multiple: true,
    });
    const paths = selected === null ? [] : Array.isArray(selected) ? selected : [selected];
    onChange({
      items: withTaskItems(
        action.items,
        paths.map((path) => ({ path, isDir: folders })),
      ),
    });
  };

  const addTyped = () => {
    const path = typedPath.trim();
    if (!path) return;
    onChange({
      items: withTaskItems(action.items, [
        { path: path.length > 1 ? path.replace(/\/+$/, "") : path, isDir: path.endsWith("/") },
      ]),
    });
    setTypedPath("");
  };

  const browseDestination = async () => {
    const selected = await openFileDialog({
      title: t("workspace.chooseTheFolderToDownload"),
      directory: true,
      multiple: false,
      defaultPath: action.destination || undefined,
    });
    if (typeof selected === "string") onChange({ destination: selected });
  };

  return (
    <>
      <div className="field">
        <span>{t("workspace.direction")}</span>
        <div className="segmented" role="radiogroup">
          {DIRECTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={action.direction === option.value}
              className={action.direction === option.value ? "is-selected" : ""}
              onClick={() => onChange(withDirection(action, option.value))}
            >
              {option.label}
            </button>
          ))}
        </div>
        {direction && <small className="field-hint">{direction.hint}</small>}
      </div>

      {action.direction === "relay" && (
        <label className="field">
          <span>{t("workspace.copyFrom")}</span>
          <select
            value={action.sourceConnectionId ?? ""}
            onChange={(event) => onChange({ sourceConnectionId: event.target.value || null })}
          >
            <option value="">{t("workspace.chooseAConnection")}</option>
            {action.sourceConnectionId && !sourceKnown && (
              <option value={action.sourceConnectionId}>{t("workspace.aDeletedConnection")}</option>
            )}
            {connections.map((connection) => (
              <option key={connection.id} value={connection.id}>
                {connection.name}
              </option>
            ))}
          </select>
        </label>
      )}

      <div className="field">
        <span>
          {local ? "Files and folders on this computer" : "Files and folders on the server"}
        </span>
        <div className="task-items">
          {action.items.length === 0 ? (
            <p className="task-items-empty">
              {local
                ? "Add the files and folders the task uploads. A folder goes with everything in it."
                : "Add the server paths the task copies, such as /var/www/site. A folder goes with everything in it."}
            </p>
          ) : (
            <ul aria-label={t("workspace.itemsToCopy")}>
              {action.items.map((item) => (
                <li key={item.path} className="task-item">
                  {item.isDir ? (
                    <Folder size={14} className="file-icon file-icon-folder" />
                  ) : (
                    <File size={14} className="file-icon" />
                  )}
                  <span className="task-item-path" title={item.path}>
                    {item.path}
                  </span>
                  <button
                    type="button"
                    className="icon-button"
                    title={t("workspace.removeFromTheTask")}
                    aria-label={t("workspace.remove", { path: item.path })}
                    onClick={() =>
                      onChange({
                        items: action.items.filter((existing) => existing.path !== item.path),
                      })
                    }
                  >
                    <X size={13} />
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="task-items-actions">
            {local ? (
              <>
                <button type="button" className="button" onClick={() => void pick(false)}>
                  <FilePlus size={14} /> {t("workspace.addFiles")}
                </button>
                <button type="button" className="button" onClick={() => void pick(true)}>
                  <FolderPlus size={14} /> {t("workspace.addFolders")}
                </button>
              </>
            ) : (
              <div className="input-with-button grow">
                <input
                  value={typedPath}
                  placeholder={t("workspace.aFullPathOnThe")}
                  spellCheck={false}
                  aria-label={t("workspace.serverPathToAdd")}
                  onChange={(event) => setTypedPath(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      addTyped();
                    }
                  }}
                />
                <button
                  type="button"
                  className="button"
                  disabled={!typedPath.trim()}
                  onClick={addTyped}
                >
                  <Plus size={14} /> {t("common.add")}
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="form-row">
        {action.direction === "download" ? (
          <div className="field grow">
            <span>{t("workspace.intoTheFolderOnThis")}</span>
            <div className="input-with-button">
              <input
                value={action.destination}
                placeholder={t("workspace.theFolderToDownloadInto")}
                spellCheck={false}
                onChange={(event) => onChange({ destination: event.target.value })}
              />
              <button type="button" className="button" onClick={() => void browseDestination()}>
                <FolderOpen size={14} /> {t("common.browse")}
              </button>
            </div>
          </div>
        ) : (
          <label className="field grow">
            <span>{t("workspace.intoTheFolderOnThe")}</span>
            <input
              value={action.destination}
              placeholder={t("workspace.varBackups")}
              spellCheck={false}
              onChange={(event) => onChange({ destination: event.target.value })}
            />
          </label>
        )}
        <label className="field grow">
          <span>{t("workspace.inAFolderOfIts")}</span>
          <input
            className="mono"
            value={action.subfolder}
            placeholder={t("workspace.backup")}
            spellCheck={false}
            autoCapitalize="off"
            aria-invalid={"error" in pattern}
            onChange={(event) => onChange({ subfolder: event.target.value })}
          />
        </label>
      </div>
      <small className={`field-hint task-pattern-hint ${"error" in pattern ? "is-error" : ""}`}>
        {"error" in pattern
          ? pattern.error
          : pattern.folder
            ? `A run now goes into ${pattern.folder}.`
            : `Each run can get a folder named from its time: ${PATTERN_PLACEHOLDERS.map((name) => `{${name}}`).join(" ")}.`}
      </small>

      <label className="field">
        <span>{t("workspace.whenAFileIsAlready")}</span>
        <select
          value={action.exists}
          onChange={(event) => onChange({ exists: event.target.value as ExistsAction })}
        >
          {TASK_EXISTS_ACTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
    </>
  );
}
