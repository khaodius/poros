import { AppWindow, FilePen, FolderOpen, Settings2, SquareArrowOutUpRight } from "lucide-react";
import { programFor, programLabel } from "../lib/externalEditor";
import {
  openFile,
  openFileWith,
  openProgramSettings,
  openWithDefault,
  type OpenTarget,
} from "../state/externalEditStore";
import { useSettingsStore } from "../state/settingsStore";
import type { MenuItem } from "./ContextMenu";
import { t } from "../i18n";

/** Open and the Open with submenu for a file: its program, the programs set up, and others. */
export function openMenuItems(target: OpenTarget, nearGroupId?: string): MenuItem[] {
  const { programs } = useSettingsStore.getState().settings.editing;
  const preferred = programFor(target.name, programs);
  return [
    {
      label: preferred
        ? t("openWith.openIn", { programLabel: programLabel(preferred) })
        : t("common.open"),
      icon: <SquareArrowOutUpRight size={14} />,
      shortcut: "Ctrl+Enter",
      onSelect: () => void openWithDefault(target),
    },
    {
      label: t("openWith.openWith"),
      icon: <AppWindow size={14} />,
      items: [
        ...programs.map((program) => ({
          label: programLabel(program),
          onSelect: () => void openFile(target, { kind: "program", id: program.id }),
        })),
        ...(programs.length > 0 ? ["separator" as const] : []),
        {
          label: t("openWith.defaultProgram"),
          onSelect: () => void openFile(target, { kind: "default" }),
        },
        {
          label: t("openWith.builtInEditor"),
          icon: <FilePen size={14} />,
          shortcut: "F4",
          onSelect: () => void openFileWith(target, { kind: "builtIn" }, { nearGroupId }),
        },
        "separator",
        {
          label: t("openWith.chooseAProgram"),
          icon: <FolderOpen size={14} />,
          onSelect: () => void openFileWith(target, "choose"),
        },
        {
          label: t("openWith.setUpPrograms"),
          icon: <Settings2 size={14} />,
          onSelect: openProgramSettings,
        },
      ],
    },
  ];
}
