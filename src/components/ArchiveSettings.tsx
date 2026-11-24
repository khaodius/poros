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
import { useT } from "../i18n";

/** The programs a server is searched for, in that order; the message puts "and" before the last. */
const SERVER_PROGRAMS = ["7-Zip", "unrar", "unar", "bsdtar", "tar", "unzip", "gzip", "bzip2", "xz"];
const LAST_SERVER_PROGRAM = "zstd";

/** Settings > Archives: the program that extracts on this computer, and what servers use. */
export function ArchiveSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
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
    { value: "", label: t("archives.theBestOneInstalledFor") },
    ...(tools ?? []).map((tool) => ({ value: tool.path, label: `${tool.name}: ${tool.path}` })),
    ...(missing ? [{ value: picked, label: t("archives.notFound", { picked }) }] : []),
    { value: CUSTOM_EXTRACTOR, label: t("archives.anotherProgram") },
  ];

  const browse = async () => {
    const windows = localSource.pathStyle === "windows";
    const chosen = await openFileDialog({
      multiple: false,
      directory: false,
      filters: windows
        ? [{ name: t("archives.programsFileType"), extensions: ["exe", "cmd", "bat"] }]
        : undefined,
    });
    if (typeof chosen === "string") set({ program: chosen });
  };

  return (
    <>
      <SettingGroup title={t("archives.onThisComputer")}>
        <SelectSetting
          label={t("archives.extractWith")}
          hint={t("archives.extractWithHint")}
          value={picked}
          options={options}
          onChange={(choice) => set({ choice })}
        />
        {picked === CUSTOM_EXTRACTOR && (
          <>
            <SettingRow label={t("archives.program")} hint={t("archives.programHint")}>
              <span className="input-with-button">
                <CommitInput
                  value={extractor.program}
                  label={t("archives.program")}
                  placeholder={
                    localSource.pathStyle === "windows"
                      ? "C:\\Program Files\\Bandizip\\bz.exe"
                      : "/usr/bin/7z"
                  }
                  onCommit={(program) => set({ program: program.trim() })}
                />
                <button type="button" className="button" onClick={() => void browse()}>
                  <FolderOpen size={14} /> {t("common.browse")}
                </button>
              </span>
            </SettingRow>
            <TextSetting
              label={t("archives.arguments")}
              hint={t("archives.argumentsHint", { archive: "{archive}", folder: "{folder}" })}
              value={extractor.arguments}
              placeholder={CUSTOM_ARGUMENTS_EXAMPLE}
              wide
              onCommit={(value) => set({ arguments: value })}
            />
          </>
        )}
        {missing && <p className="setting-note">{t("archives.theProgramPickedIsNo")}</p>}
        <SettingRow
          label={t("archives.installedPrograms")}
          hint={t("archives.installedProgramsHint")}
        >
          <button type="button" className="button" onClick={find} disabled={tools === null}>
            <RefreshCw size={14} className={tools === null && !error ? "spin" : ""} />{" "}
            {t("archives.lookAgain")}
          </button>
        </SettingRow>
        {error && <p className="setting-note">{error}</p>}
        {tools !== null && tools.length === 0 && (
          <p className="setting-note">{t("archives.noProgramThatExtractsArchives")}</p>
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
      <SettingGroup title={t("archives.onServers")}>
        <p className="setting-note">
          {t("archives.archivesOnAnSftpServer", {
            programs: SERVER_PROGRAMS.join(t("common.listSeparator")),
            lastProgram: LAST_SERVER_PROGRAM,
          })}
        </p>
      </SettingGroup>
    </>
  );
}
