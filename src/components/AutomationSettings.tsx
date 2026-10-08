import { useState, type FormEvent } from "react";
import { CalendarClock, Plus, Trash2 } from "lucide-react";
import { WHEN_DONE_LABELS } from "../lib/automation";
import { PLACEHOLDERS } from "../lib/commands";
import {
  WHEN_DONE_ACTIONS,
  type AutomationSettings,
  type ServerCommand,
  type Settings,
} from "../lib/settings";
import { saveSettingsSection } from "../state/settingsStore";
import { useUiStore } from "../state/uiStore";
import {
  CommitInput,
  SelectSetting,
  SettingGroup,
  SettingRow,
  SwitchSetting,
  TextSetting,
} from "./settingsFields";

export function AutomationSettingsPage({ settings }: { settings: Settings }) {
  const automation = settings.automation;
  const set = (change: Partial<AutomationSettings>) =>
    void saveSettingsSection("automation", change);
  const openDialog = useUiStore((state) => state.open);

  return (
    <>
      <SettingGroup title="When the transfer queue finishes">
        <SelectSetting
          label="Then"
          hint="Also in the queue's toolbar. Everything but a command counts down for 30 seconds first, so you can stop it."
          value={automation.whenDone}
          options={WHEN_DONE_ACTIONS.map((action) => ({
            value: action,
            label: WHEN_DONE_LABELS[action],
          }))}
          onChange={(whenDone) => set({ whenDone })}
        />
        <SwitchSetting
          label="Only the next time"
          hint="Goes back to Do nothing once it has happened."
          checked={automation.whenDoneOnce}
          onChange={(whenDoneOnce) => set({ whenDoneOnce })}
        />
        <TextSetting
          label="Command"
          hint="For Run a command; runs on this computer. POROS_FILES_DONE, POROS_FILES_FAILED, POROS_FILES_SKIPPED and POROS_BYTES hold the totals."
          value={automation.whenDoneCommand}
          placeholder="A command or script"
          disabled={automation.whenDone !== "runCommand"}
          onCommit={(whenDoneCommand) => set({ whenDoneCommand })}
        />
        <SwitchSetting
          label="Show a notification"
          checked={automation.notify}
          onChange={(notify) => set({ notify })}
        />
        <SwitchSetting
          label="Only when Poros is in the background"
          checked={automation.notifyOnlyInBackground}
          disabled={!automation.notify}
          onChange={(notifyOnlyInBackground) => set({ notifyOnlyInBackground })}
        />
        <SwitchSetting
          label="Play a sound"
          checked={automation.sound}
          onChange={(sound) => set({ sound })}
        />
      </SettingGroup>

      <SettingGroup title="Server commands">
        <p className="setting-note">
          Listed under Commands when you right-click in a server pane. They run through the server's
          shell in the folder shown, with these filled in:
        </p>
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
        <CommandList commands={automation.commands} onChange={(commands) => set({ commands })} />
      </SettingGroup>

      <SettingGroup title="Scheduled tasks">
        <SettingRow
          label="Synchronize folders or run commands at set times"
          hint="Tasks run while Poros is open, in the background, through saved connections."
        >
          <button
            type="button"
            className="button"
            onClick={() => openDialog({ kind: "schedules" })}
          >
            <CalendarClock size={14} /> Scheduled tasks
          </button>
        </SettingRow>
      </SettingGroup>
    </>
  );
}

interface CommandListProps {
  commands: ServerCommand[];
  onChange: (commands: ServerCommand[]) => void;
}

function CommandList({ commands, onChange }: CommandListProps) {
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");

  const update = (id: string, change: Partial<ServerCommand>) => {
    if (change.command !== undefined && change.command.trim() === "") return;
    onChange(
      commands.map((existing) => (existing.id === id ? { ...existing, ...change } : existing)),
    );
  };

  const add = (event: FormEvent) => {
    event.preventDefault();
    if (!command.trim()) return;
    onChange([
      ...commands,
      {
        id: crypto.randomUUID(),
        name: name.trim(),
        command: command.trim(),
        showOutput: true,
        refresh: false,
      },
    ]);
    setName("");
    setCommand("");
  };

  return (
    <div className="command-list">
      {commands.map((saved) => (
        <div key={saved.id} className="command-row">
          <CommitInput
            className="command-name"
            label="Name"
            value={saved.name}
            placeholder="Name"
            onCommit={(value) => update(saved.id, { name: value })}
          />
          <CommitInput
            className="command-text"
            label="Command"
            value={saved.command}
            onCommit={(value) => update(saved.id, { command: value })}
          />
          <button
            type="button"
            className={`toggle ${saved.showOutput ? "is-on" : ""}`}
            aria-pressed={saved.showOutput}
            title="Open a window with what the command prints; off runs it quietly"
            onClick={() => update(saved.id, { showOutput: !saved.showOutput })}
          >
            Output
          </button>
          <button
            type="button"
            className={`toggle ${saved.refresh ? "is-on" : ""}`}
            aria-pressed={saved.refresh}
            title="Reload the server's folders when the command finishes"
            onClick={() => update(saved.id, { refresh: !saved.refresh })}
          >
            Refresh
          </button>
          <button
            type="button"
            className="icon-button"
            title="Delete command"
            aria-label="Delete command"
            onClick={() => onChange(commands.filter((existing) => existing.id !== saved.id))}
          >
            <Trash2 size={14} />
          </button>
        </div>
      ))}
      <form className="command-row command-row-new" onSubmit={add}>
        <input
          className="command-name"
          aria-label="Name of the new command"
          value={name}
          placeholder="Name"
          spellCheck={false}
          onChange={(event) => setName(event.target.value)}
        />
        <input
          className="command-text"
          aria-label="New command"
          value={command}
          placeholder="du -sh {paths}"
          spellCheck={false}
          onChange={(event) => setCommand(event.target.value)}
        />
        <button type="submit" className="button" disabled={!command.trim()}>
          <Plus size={14} /> Add
        </button>
      </form>
    </div>
  );
}
