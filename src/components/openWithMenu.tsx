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

/** Open and the Open with submenu for a file: its program, the programs set up, and others. */
export function openMenuItems(target: OpenTarget, nearGroupId?: string): MenuItem[] {
  const { programs } = useSettingsStore.getState().settings.editing;
  const preferred = programFor(target.name, programs);
  return [
    {
      label: preferred ? `Open in ${programLabel(preferred)}` : "Open",
      icon: <SquareArrowOutUpRight size={14} />,
      shortcut: "Ctrl+Enter",
      onSelect: () => void openWithDefault(target),
    },
    {
      label: "Open with",
      icon: <AppWindow size={14} />,
      items: [
        ...programs.map((program) => ({
          label: programLabel(program),
          onSelect: () => void openFile(target, { kind: "program", id: program.id }),
        })),
        ...(programs.length > 0 ? ["separator" as const] : []),
        {
          label: "Default program",
          onSelect: () => void openFile(target, { kind: "default" }),
        },
        {
          label: "Built-in editor",
          icon: <FilePen size={14} />,
          shortcut: "F4",
          onSelect: () => void openFileWith(target, { kind: "builtIn" }, { nearGroupId }),
        },
        "separator",
        {
          label: "Choose a program...",
          icon: <FolderOpen size={14} />,
          onSelect: () => void openFileWith(target, "choose"),
        },
        {
          label: "Set up programs...",
          icon: <Settings2 size={14} />,
          onSelect: openProgramSettings,
        },
      ],
    },
  ];
}
