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
import { useT } from "../i18n";

export function AutomationSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
  const automation = settings.automation;
  const set = (change: Partial<AutomationSettings>) =>
    void saveSettingsSection("automation", change);
  const openDialog = useUiStore((state) => state.open);

  return (
    <>
      <SettingGroup title={t("automation.whenTheTransferQueueFinishes")}>
        <SelectSetting
          label={t("automation.then")}
          hint={t("automation.thenHint")}
          value={automation.whenDone}
          options={WHEN_DONE_ACTIONS.map((action) => ({
            value: action,
            label: WHEN_DONE_LABELS[action],
          }))}
          onChange={(whenDone) => set({ whenDone })}
        />
        <SwitchSetting
          label={t("automation.onlyTheNextTime")}
          hint={t("automation.onlyTheNextTimeHint")}
          checked={automation.whenDoneOnce}
          onChange={(whenDoneOnce) => set({ whenDoneOnce })}
        />
        <TextSetting
          label={t("automation.command")}
          hint={t("automation.commandHint", {
            done: "POROS_FILES_DONE",
            failed: "POROS_FILES_FAILED",
            skipped: "POROS_FILES_SKIPPED",
            bytes: "POROS_BYTES",
          })}
          value={automation.whenDoneCommand}
          placeholder={t("automation.aCommandOrScript")}
          disabled={automation.whenDone !== "runCommand"}
          onCommit={(whenDoneCommand) => set({ whenDoneCommand })}
        />
        <SwitchSetting
          label={t("automation.showANotification")}
          checked={automation.notify}
          onChange={(notify) => set({ notify })}
        />
        <SwitchSetting
          label={t("automation.onlyWhenPorosIsIn")}
          checked={automation.notifyOnlyInBackground}
          disabled={!automation.notify}
          onChange={(notifyOnlyInBackground) => set({ notifyOnlyInBackground })}
        />
        <SwitchSetting
          label={t("automation.playASound")}
          checked={automation.sound}
          onChange={(sound) => set({ sound })}
        />
      </SettingGroup>

      <SettingGroup title={t("automation.serverCommands")}>
        <p className="setting-note">{t("automation.listedUnderCommandsWhenYou")}</p>
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

      <SettingGroup title={t("automation.scheduledTasks")}>
        <SettingRow
          label={t("automation.synchronizeFoldersOrRunCommands")}
          hint={t("automation.synchronizeFoldersOrRunCommandsHint")}
        >
          <button
            type="button"
            className="button"
            onClick={() => openDialog({ kind: "schedules" })}
          >
            <CalendarClock size={14} /> {t("automation.scheduledTasks")}
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
  const t = useT();
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
            label={t("common.name")}
            value={saved.name}
            placeholder={t("common.name")}
            onCommit={(value) => update(saved.id, { name: value })}
          />
          <CommitInput
            className="command-text"
            label={t("automation.command")}
            value={saved.command}
            onCommit={(value) => update(saved.id, { command: value })}
          />
          <button
            type="button"
            className={`toggle ${saved.showOutput ? "is-on" : ""}`}
            aria-pressed={saved.showOutput}
            title={t("automation.openAWindowWithWhat")}
            onClick={() => update(saved.id, { showOutput: !saved.showOutput })}
          >
            {t("automation.output")}
          </button>
          <button
            type="button"
            className={`toggle ${saved.refresh ? "is-on" : ""}`}
            aria-pressed={saved.refresh}
            title={t("automation.reloadTheServersFoldersWhen")}
            onClick={() => update(saved.id, { refresh: !saved.refresh })}
          >
            {t("common.refresh")}
          </button>
          <button
            type="button"
            className="icon-button"
            title={t("automation.deleteCommand")}
            aria-label={t("automation.deleteCommand")}
            onClick={() => onChange(commands.filter((existing) => existing.id !== saved.id))}
          >
            <Trash2 size={14} />
          </button>
        </div>
      ))}
      <form className="command-row command-row-new" onSubmit={add}>
        <input
          className="command-name"
          aria-label={t("automation.nameOfTheNewCommand")}
          value={name}
          placeholder={t("common.name")}
          spellCheck={false}
          onChange={(event) => setName(event.target.value)}
        />
        <input
          className="command-text"
          aria-label={t("automation.newCommand")}
          value={command}
          placeholder="du -sh {paths}"
          spellCheck={false}
          onChange={(event) => setCommand(event.target.value)}
        />
        <button type="submit" className="button" disabled={!command.trim()}>
          <Plus size={14} /> {t("common.add")}
        </button>
      </form>
    </div>
  );
}
