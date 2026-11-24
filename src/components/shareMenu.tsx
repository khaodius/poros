import { Link } from "lucide-react";
import { shareMenuEntry } from "../lib/shareLinks";
import type { FileEntry, ShareCapabilities } from "../lib/types";
import type { MenuItem } from "./ContextMenu";

/**
 * "Share link..." in a server pane's menu for `targets`, or `null` with nothing selected. Where
 * the selection cannot be shared, the entry stays, disabled, and says why.
 */
export function shareMenuItem(
  targets: FileEntry[],
  sharing: ShareCapabilities | undefined,
  inVault: boolean,
  onShare: () => void,
): MenuItem | null {
  const entry = shareMenuEntry(targets, sharing, inVault);
  if (!entry) return null;
  return {
    label: entry.label,
    icon: <Link size={14} />,
    disabled: entry.disabled,
    onSelect: entry.disabled ? () => undefined : onShare,
  };
}
