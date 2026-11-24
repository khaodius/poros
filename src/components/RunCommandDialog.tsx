import { useState, type FormEvent } from "react";
import { PLACEHOLDERS, expandCommand, needsSelection, type CommandTarget } from "../lib/commands";
import { useCommandStore } from "../state/commandStore";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

interface RunCommandDialogProps {
  sessionId: string;
  target: CommandTarget;
  onClose: () => void;
}

/** The command typed last, offered again the next time. */
let lastCommand = "";

export function RunCommandDialog({ sessionId, target, onClose }: RunCommandDialogProps) {
  const t = useT();
  const [command, setCommand] = useState(lastCommand);
  const [refresh, setRefresh] = useState(true);
  const trimmed = command.trim();
  const missingSelection = needsSelection(trimmed) && target.items.length === 0;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!trimmed || missingSelection) return;
    lastCommand = trimmed;
    onClose();
    useCommandStore.getState().show({
      title: t("automation.runACommand"),
      sessionId,
      directory: target.folder,
      commands: expandCommand(trimmed, target),
      refresh,
    });
  };

  return (
    <Dialog title={t("automation.runACommandOnThe")} onClose={onClose} width={560}>
      <form className="form" onSubmit={submit}>
        <label className="field">
          <span>
            {t.rich("automation.commandToRunIn", { folder: <code>{target.folder}</code> })}
          </span>
          <input
            data-autofocus
            className="mono"
            value={command}
            spellCheck={false}
            autoCapitalize="off"
            placeholder="du -sh {paths}"
            onChange={(event) => setCommand(event.target.value)}
          />
        </label>
        <dl className="placeholder-list">
          {PLACEHOLDERS.map(({ placeholder, meaning }) => (
            <div key={placeholder}>
              <dt>
                <code>{placeholder}</code>
              </dt>
              <dd>{meaning}</dd>
            </div>
          ))}
        </dl>
        <label className="check">
          <input
            type="checkbox"
            checked={refresh}
            onChange={(event) => setRefresh(event.target.checked)}
          />
          {t("automation.refreshTheServersFoldersWhen")}
        </label>
        {missingSelection && <p className="form-error">{t("automation.selectFilesOrFoldersTo")}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="submit"
            className="button button-primary"
            disabled={!trimmed || missingSelection}
          >
            {t("automation.run")}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
