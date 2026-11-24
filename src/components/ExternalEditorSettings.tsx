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

const COMMAND_EXAMPLE =
  localSource.pathStyle === "windows"
    ? "C:\\Program Files\\Notepad++\\notepad++.exe"
    : "/usr/bin/gimp";

/** Settings > Interface > Open with: the programs files open in, and what becomes of saves. */
export function ExternalEditorSettings({ settings }: { settings: Settings }) {
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
    <SettingGroup title="Open with">
      <p ref={noteRef} className="setting-note">
        Open (Ctrl+Enter) uses the program set up for a file's kind, else the system's own. A server
        file opens from a copy on this computer, which is uploaded each time the program saves it.
      </p>
      <ProgramList programs={editing.programs} onChange={(programs) => set({ programs })} />
      <SwitchSetting
        label="Upload each save without asking"
        hint="Off: asks before every upload."
        checked={editing.uploadOnSave}
        onChange={(uploadOnSave) => set({ uploadOnSave })}
      />
      <NumberSetting
        label="Keep copies after the program closes"
        hint="Counted from the last save or upload. Copies of files in vaults are wiped at once."
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
              label="Name"
              value={program.name}
              placeholder={programLabel(program)}
              onCommit={(value) => update(program.id, { name: value.trim() })}
            />
            <button
              type="button"
              className="icon-button"
              title="Remove program"
              aria-label={`Remove ${programLabel(program)}`}
              onClick={() => onChange(programs.filter((existing) => existing.id !== program.id))}
            >
              <Trash2 size={14} />
            </button>
          </div>
          <div className="program-field">
            <span>Program</span>
            <span className="input-with-button">
              <CommitInput
                label="Program"
                value={program.command}
                placeholder={COMMAND_EXAMPLE}
                onCommit={(value) => update(program.id, { command: value.trim() })}
              />
              <button
                type="button"
                className="button"
                onClick={() => void browse((chosen) => update(program.id, { command: chosen }))}
              >
                <FolderOpen size={14} /> Browse
              </button>
            </span>
          </div>
          <div className="program-field">
            <span>Arguments</span>
            <CommitInput
              label="Arguments"
              value={program.arguments}
              placeholder={FILE_PLACEHOLDER}
              onCommit={(value) => update(program.id, { arguments: value })}
            />
          </div>
          <div className="program-field">
            <span>Default for</span>
            <CommitInput
              label="Files this program opens by default"
              value={formatPatterns(program.patterns)}
              placeholder="*.psd, *.xcf"
              onCommit={(value) => update(program.id, { patterns: parsePatterns(value) })}
            />
          </div>
        </div>
      ))}
      <form className="program-new" onSubmit={add}>
        <input
          className="program-name"
          aria-label="Name of the new program"
          value={name}
          placeholder="Name"
          spellCheck={false}
          onChange={(event) => setName(event.target.value)}
        />
        <span className="input-with-button">
          <input
            aria-label="New program"
            value={command}
            placeholder={COMMAND_EXAMPLE}
            spellCheck={false}
            onChange={(event) => setCommand(event.target.value)}
          />
          <button type="button" className="button" onClick={() => void browse(setCommand)}>
            <FolderOpen size={14} /> Browse
          </button>
        </span>
        <button type="submit" className="button" disabled={!command.trim()}>
          <Plus size={14} /> Add
        </button>
      </form>
      <p className="setting-hint program-hint">
        {FILE_PLACEHOLDER} in the arguments stands for the file, and the file goes last when it is
        left out. Quotes keep spaces in one argument. Files matching a pattern under Default for
        open in that program.
      </p>
    </div>
  );
}
