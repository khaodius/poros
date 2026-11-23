import {
  Cloud,
  FileText,
  FolderLock,
  HardDrive,
  Server,
  Sparkles,
  SquareTerminal,
  type LucideIcon,
} from "lucide-react";
import type { PaneTab } from "../lib/layout";
import { isCloud, isVault } from "../lib/protocols";
import type { Protocol } from "../lib/types";

/** The icon each kind of tab shows on its tab and while it is dragged. */
export const TAB_ICONS = {
  local: HardDrive,
  remote: Server,
  welcome: Sparkles,
  editor: FileText,
  terminal: SquareTerminal,
} satisfies Record<PaneTab["kind"], unknown>;

/** What a session is connected to, as its icon tells it. */
export type ConnectionKind = "server" | "cloud" | "vault";

export const CONNECTION_ICONS = {
  server: Server,
  cloud: Cloud,
  vault: FolderLock,
} satisfies Record<ConnectionKind, LucideIcon>;

export function connectionKind(protocol: Protocol): ConnectionKind {
  if (isVault(protocol)) return "vault";
  return isCloud(protocol) ? "cloud" : "server";
}
