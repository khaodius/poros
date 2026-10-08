import { Ban, Download, File, Folder, Upload } from "lucide-react";
import { isDirLike } from "../lib/sort";
import { useDragStore, type DragPayload, type DropTarget } from "../state/dragStore";
import { getPane } from "../state/paneRegistry";
import { TAB_ICONS } from "./tabIcons";

const POINTER_OFFSET = { x: 14, y: 12 };

type Hint = { icon: typeof Upload; text: string; blocked?: boolean };

function hintFor(payload: DragPayload, target: DropTarget | null): Hint | null {
  if (!target) return null;
  if (payload.kind !== "files" || target.kind !== "pane") return null;
  const source = getPane(payload.sourceTabId);
  const destination = getPane(target.tabId);
  if (!source || !destination) return null;
  const folder = target.folder ?? destination.path() ?? "";
  const folderName = folder.split(/[\\/]/).filter(Boolean).pop() ?? folder;
  if (source.kind === "local" && destination.kind === "remote") {
    return { icon: Upload, text: `Upload to ${folderName}` };
  }
  if (source.kind === "remote" && destination.kind === "local") {
    return { icon: Download, text: `Download to ${folderName}` };
  }
  if (source.tabId === destination.tabId && target.folder === null) return null;
  return { icon: Ban, text: "Only between local and server tabs", blocked: true };
}

/**
 * Follows the pointer during a drag inside the window, saying what a drop would do, and names
 * a tab another window drags over this one.
 */
export function DragGhost() {
  const payload = useDragStore((state) => state.payload);
  const target = useDragStore((state) => state.target);
  const pointer = useDragStore((state) => state.pointer);
  const incoming = useDragStore((state) => state.incoming);
  const style = {
    transform: `translate(${pointer.x + POINTER_OFFSET.x}px, ${pointer.y + POINTER_OFFSET.y}px)`,
  };
  if (incoming) {
    const TabIcon = TAB_ICONS[incoming.kind];
    return (
      <div className="drag-ghost" style={style}>
        <span className="drag-ghost-label">
          <TabIcon size={14} />
          <span>{incoming.label}</span>
        </span>
      </div>
    );
  }
  // Files from the operating system already show the system's drag image, and a dragged tab
  // carries its whole pane.
  if (!payload || payload.kind !== "files") return null;

  const hint = hintFor(payload, target);
  let label: string;
  let Icon = File;
  if (payload.entries.length === 1) {
    label = payload.entries[0].name;
    if (isDirLike(payload.entries[0])) Icon = Folder;
  } else {
    label = `${payload.entries.length} items`;
  }

  return (
    <div className="drag-ghost" style={style}>
      <span className="drag-ghost-label">
        <Icon size={14} />
        <span>{label}</span>
      </span>
      {hint && (
        <span className={`drag-ghost-hint ${hint.blocked ? "is-blocked" : ""}`}>
          <hint.icon size={12} />
          {hint.text}
        </span>
      )}
    </div>
  );
}
