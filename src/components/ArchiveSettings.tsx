import { useEffect, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, RefreshCw } from "lucide-react";
import {
  CUSTOM_ARGUMENTS_EXAMPLE,
  CUSTOM_EXTRACTOR,
  FORMAT_NAMES,
  type ExtractorSettings,
} from "../lib/archives";
import { localSource } from "../lib/fileSource";
import { archives, toAppError } from "../lib/ipc";
import type { Settings } from "../lib/settings";
import type { InstalledArchiveTool } from "../lib/types";
import { saveSettingsSection } from "../state/settingsStore";
import {
  CommitInput,
  SelectSetting,
  SettingGroup,
  SettingRow,
  TextSetting,
} from "./settingsFields";

const SERVER_PROGRAMS = "7-Zip, unrar, unar, bsdtar, tar, unzip, gzip, bzip2, xz and zstd";

/** Settings > Archives: the program that extracts on this computer, and what servers use. */
export function ArchiveSettingsPage({ settings }: { settings: Settings }) {
  const extractor = settings.interface.extractor;
  const [tools, setTools] = useState<InstalledArchiveTool[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const set = (change: Partial<ExtractorSettings>) =>
    void saveSettingsSection("interface", { extractor: { ...extractor, ...change } });

  const [search, setSearch] = useState(0);
  useEffect(() => {
    let current = true;
    archives
      .installed()
      .then((found) => current && setTools(found))
      .catch((caught: unknown) => current && setError(toAppError(caught).message));
    return () => {
      current = false;
    };
  }, [search]);
  const find = () => {
    setTools(null);
    setError(null);
    setSearch((count) => count + 1);
  };

  const picked = extractor.choice;
  const missing =
    tools !== null &&
    picked !== "" &&
    picked !== CUSTOM_EXTRACTOR &&
    !tools.some((tool) => tool.path === picked);
  const options = [
    { value: "", label: "The best one installed for each format" },
    ...(tools ?? []).map((tool) => ({ value: tool.path, label: `${tool.name}: ${tool.path}` })),
    ...(missing ? [{ value: picked, label: `${picked} (not found)` }] : []),
    { value: CUSTOM_EXTRACTOR, label: "Another program..." },
  ];

  const browse = async () => {
    const windows = localSource.pathStyle === "windows";
    const chosen = await openFileDialog({
      multiple: false,
      directory: false,
      filters: windows ? [{ name: "Programs", extensions: ["exe", "cmd", "bat"] }] : undefined,
    });
    if (typeof chosen === "string") set({ program: chosen });
  };

  return (
    <>
      <SettingGroup title="On this computer">
        <SelectSetting
          label="Extract with"
          hint="An installed program extracts every format it opens; the rest go to the best program installed for them."
          value={picked}
          options={options}
          onChange={(choice) => set({ choice })}
        />
        {picked === CUSTOM_EXTRACTOR && (
          <>
            <SettingRow label="Program" hint="The program file to run.">
              <span className="input-with-button">
                <CommitInput
                  value={extractor.program}
                  label="Program"
                  placeholder={
                    localSource.pathStyle === "windows"
                      ? "C:\\Program Files\\Bandizip\\bz.exe"
                      : "/usr/bin/7z"
                  }
                  onCommit={(program) => set({ program: program.trim() })}
                />
                <button type="button" className="button" onClick={() => void browse()}>
                  <FolderOpen size={14} /> Browse
                </button>
              </span>
            </SettingRow>
            <TextSetting
              label="Arguments"
              hint="{archive} is the archive and {folder} the folder it goes into. Quote them, as paths can hold spaces."
              value={extractor.arguments}
              placeholder={CUSTOM_ARGUMENTS_EXAMPLE}
              wide
              onCommit={(value) => set({ arguments: value })}
            />
          </>
        )}
        {missing && (
          <p className="setting-note">
            The program picked is no longer there, so archives go to the best one installed until
            you pick another.
          </p>
        )}
        <SettingRow
          label="Installed programs"
          hint="Found on the PATH and where 7-Zip, WinRAR and PeaZip install themselves."
        >
          <button type="button" className="button" onClick={find} disabled={tools === null}>
            <RefreshCw size={14} className={tools === null && !error ? "spin" : ""} /> Look again
          </button>
        </SettingRow>
        {error && <p className="setting-note">{error}</p>}
        {tools !== null && tools.length === 0 && (
          <p className="setting-note">
            No program that extracts archives was found. Install 7-Zip, or name a program above.
          </p>
        )}
        {tools !== null && tools.length > 0 && (
          <ul className="archive-tools">
            {tools.map((tool) => (
              <li key={tool.path}>
                <span className="archive-tool-name">{tool.name}</span>
                <span className="archive-tool-path" title={tool.path}>
                  <bdi>{tool.path}</bdi>
                </span>
                <span className="archive-tool-formats">
                  {[...new Set(tool.formats.map((format) => FORMAT_NAMES[format]))].join(", ")}
                </span>
              </li>
            ))}
          </ul>
        )}
      </SettingGroup>
      <SettingGroup title="On servers">
        <p className="setting-note">
          Archives on an SFTP server are extracted on the server, with the programs it has. Poros
          looks for {SERVER_PROGRAMS} the first time, and uses the one that opens each format best.
          Servers that allow SFTP alone cannot extract; download the archive to extract it here.
        </p>
      </SettingGroup>
    </>
  );
}
