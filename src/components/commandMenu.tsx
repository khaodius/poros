import { Settings2, SquareTerminal, Terminal } from "lucide-react";
import { needsSelection, type CommandTarget } from "../lib/commands";
import { runServerCommand } from "../state/commandStore";
import { useSettingsStore } from "../state/settingsStore";
import { useUiStore } from "../state/uiStore";
import type { MenuItem } from "./ContextMenu";

/** The Commands submenu of a server pane: saved commands, then one-off and editing entries. */
export function serverCommandsMenu(sessionId: string, target: CommandTarget): MenuItem {
  const { commands } = useSettingsStore.getState().settings.automation;
  const { open } = useUiStore.getState();
  const labels = new Set<string>();
  const saved: MenuItem[] = commands.map((command) => {
    const name = command.name.trim() || command.command.trim();
    let label = name;
    for (let copy = 2; labels.has(label); copy++) label = `${name} (${copy})`;
    labels.add(label);
    return {
      label,
      disabled: needsSelection(command.command) && target.items.length === 0,
      onSelect: () => runServerCommand(command, sessionId, target),
    };
  });
  return {
    label: "Commands",
    icon: <SquareTerminal size={14} />,
    items: [
      ...saved,
      ...(saved.length > 0 ? ["separator" as const] : []),
      {
        label: "Run a command...",
        icon: <Terminal size={14} />,
        onSelect: () => open({ kind: "runCommand", sessionId, target }),
      },
      {
        label: "Edit commands...",
        icon: <Settings2 size={14} />,
        onSelect: () => open({ kind: "settings", section: "automation" }),
      },
    ],
  };
}
