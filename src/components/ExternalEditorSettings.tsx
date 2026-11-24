import { useEffect, useRef, useState, type FormEvent } from "react";
import { FolderOpen, Plus, Trash2 } from "lucide-react";
import {
  FILE_PLACEHOLDER,
  formatPatterns,
  parsePatterns,
  programLabel,
} from "../lib/externalEditor";
import { localSource } from "../lib/fileSource";
import {
  MAX_KEEP_WORKING_COPY_MINUTES,
  type EditingSettings,
  type ExternalProgram,
  type Settings,
} from "../lib/settings";
import { pickProgram, takeProgramSettingsAsk } from "../state/externalEditStore";
import { saveSettingsSection } from "../state/settingsStore";
import { CommitInput, NumberSetting, SettingGroup, SwitchSetting } from "./settingsFields";
import { useT } from "../i18n";

const COMMAND_EXAMPLE =
  localSource.pathStyle === "windows"
    ? "C:\\Program Files\\Notepad++\\notepad++.exe"
    : "/usr/bin/gimp";

/** Settings > Interface > Open with: the programs files open in, and what becomes of saves. */
export function ExternalEditorSettings({ settings }: { settings: Settings }) {
  const t = useT();
  const editing = settings.editing;
  const set = (change: Partial<EditingSettings>) => void saveSettingsSection("editing", change);
  const noteRef = useRef<HTMLParagraphElement>(null);
  useEffect(() => {
    if (!takeProgramSettingsAsk()) return;
    // The settings dialog shows itself after its contents mount, so the group scrolls then.
    requestAnimationFrame(() => {
      noteRef.current?.closest(".setting-group")?.scrollIntoView({ block: "start" });
    });
  }, []);
  return (
    <SettingGroup title={t("openWith.openWith")}>
      <p ref={noteRef} className="setting-note">
        {t("openWith.openCtrlEnterUsesThe")}
      </p>
      <ProgramList programs={editing.programs} onChange={(programs) => set({ programs })} />
      <SwitchSetting
        label={t("openWith.uploadEachSaveWithoutAsking")}
        hint={t("openWith.uploadEachSaveWithoutAskingHint")}
        checked={editing.uploadOnSave}
        onChange={(uploadOnSave) => set({ uploadOnSave })}
      />
      <NumberSetting
        label={t("openWith.keepCopiesAfterTheProgram")}
        hint={t("openWith.keepCopiesAfterTheProgramHint")}
        value={editing.keepMinutes}
        min={0}
        max={MAX_KEEP_WORKING_COPY_MINUTES}
        unit="min"
        onChange={(keepMinutes) => set({ keepMinutes })}
      />
    </SettingGroup>
  );
}

interface ProgramListProps {
  programs: ExternalProgram[];
  onChange: (programs: ExternalProgram[]) => void;
}

function ProgramList({ programs, onChange }: ProgramListProps) {
  const t = useT();
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");

  const update = (id: string, change: Partial<ExternalProgram>) => {
    if (change.command !== undefined && change.command.trim() === "") return;
    onChange(programs.map((program) => (program.id === id ? { ...program, ...change } : program)));
  };

  const add = (event: FormEvent) => {
    event.preventDefault();
    if (!command.trim()) return;
    onChange([
      ...programs,
      {
        id: crypto.randomUUID(),
        name: name.trim(),
        command: command.trim(),
        arguments: FILE_PLACEHOLDER,
        patterns: [],
      },
    ]);
    setName("");
    setCommand("");
  };

  const browse = async (apply: (command: string) => void) => {
    const chosen = await pickProgram();
    if (chosen) apply(chosen);
  };

  return (
    <div className="program-list">
      {programs.map((program) => (
        <div key={program.id} className="program-card">
          <div className="program-card-header">
            <CommitInput
              className="program-name"
              label={t("common.name")}
              value={program.name}
              placeholder={programLabel(program)}
              onCommit={(value) => update(program.id, { name: value.trim() })}
            />
            <button
              type="button"
              className="icon-button"
              title={t("openWith.removeProgram")}
              aria-label={t("openWith.remove", { programLabel: programLabel(program) })}
              onClick={() => onChange(programs.filter((existing) => existing.id !== program.id))}
            >
              <Trash2 size={14} />
            </button>
          </div>
          <div className="program-field">
            <span>{t("openWith.program")}</span>
            <span className="input-with-button">
              <CommitInput
                label={t("openWith.program")}
                value={program.command}
                placeholder={COMMAND_EXAMPLE}
                onCommit={(value) => update(program.id, { command: value.trim() })}
              />
              <button
                type="button"
                className="button"
                onClick={() => void browse((chosen) => update(program.id, { command: chosen }))}
              >
                <FolderOpen size={14} /> {t("common.browse")}
              </button>
            </span>
          </div>
          <div className="program-field">
            <span>{t("openWith.arguments")}</span>
            <CommitInput
              label={t("openWith.arguments")}
              value={program.arguments}
              placeholder={FILE_PLACEHOLDER}
              onCommit={(value) => update(program.id, { arguments: value })}
            />
          </div>
          <div className="program-field">
            <span>{t("openWith.defaultFor")}</span>
            <CommitInput
              label={t("openWith.filesThisProgramOpensBy")}
              value={formatPatterns(program.patterns)}
              placeholder={t("openWith.psdXcf")}
              onCommit={(value) => update(program.id, { patterns: parsePatterns(value) })}
            />
          </div>
        </div>
      ))}
      <form className="program-new" onSubmit={add}>
        <input
          className="program-name"
          aria-label={t("openWith.nameOfTheNewProgram")}
          value={name}
          placeholder={t("common.name")}
          spellCheck={false}
          onChange={(event) => setName(event.target.value)}
        />
        <span className="input-with-button">
          <input
            aria-label={t("openWith.newProgram")}
            value={command}
            placeholder={COMMAND_EXAMPLE}
            spellCheck={false}
            onChange={(event) => setCommand(event.target.value)}
          />
          <button type="button" className="button" onClick={() => void browse(setCommand)}>
            <FolderOpen size={14} /> {t("common.browse")}
          </button>
        </span>
        <button type="submit" className="button" disabled={!command.trim()}>
          <Plus size={14} /> {t("common.add")}
        </button>
      </form>
      <p className="setting-hint program-hint">
        {t("openWith.inTheArgumentsStandsFor", { placeholder: FILE_PLACEHOLDER })}
      </p>
    </div>
  );
}
