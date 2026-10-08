import { useState, type FormEvent } from "react";
import { PLACEHOLDERS, expandCommand, needsSelection, type CommandTarget } from "../lib/commands";
import { useCommandStore } from "../state/commandStore";
import { Dialog } from "./Dialog";

interface RunCommandDialogProps {
  sessionId: string;
  target: CommandTarget;
  onClose: () => void;
}

/** The command typed last, offered again the next time. */
let lastCommand = "";

export function RunCommandDialog({ sessionId, target, onClose }: RunCommandDialogProps) {
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
      title: "Run a command",
      sessionId,
      directory: target.folder,
      commands: expandCommand(trimmed, target),
      refresh,
    });
  };

  return (
    <Dialog title="Run a command on the server" onClose={onClose} width={560}>
      <form className="form" onSubmit={submit}>
        <label className="field">
          <span>
            Command to run in <code>{target.folder}</code>
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
          Refresh the server's folders when it finishes
        </label>
        {missingSelection && (
          <p className="form-error">Select files or folders to fill in the placeholders.</p>
        )}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button
            type="submit"
            className="button button-primary"
            disabled={!trimmed || missingSelection}
          >
            Run
          </button>
        </div>
      </form>
    </Dialog>
  );
}
