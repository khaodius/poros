import { FileText, HardDrive, Server, Sparkles, SquareTerminal } from "lucide-react";
import type { PaneTab } from "../lib/layout";

/** The icon each kind of tab shows on its tab and while it is dragged. */
export const TAB_ICONS = {
  local: HardDrive,
  remote: Server,
  welcome: Sparkles,
  editor: FileText,
  terminal: SquareTerminal,
} satisfies Record<PaneTab["kind"], unknown>;
