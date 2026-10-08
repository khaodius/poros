import {
  Ban,
  Copy,
  Download,
  File,
  Folder,
  FolderInput,
  HardDrive,
  Server,
  Sparkles,
  Upload,
} from "lucide-react";
import { baseName } from "../lib/path";
import { isDirLike } from "../lib/sort";
import { useDragStore, type DragPayload, type DropTarget } from "../state/dragStore";
import { dropAction } from "../state/fileOperations";

const POINTER_OFFSET = { x: 14, y: 12 };

type Hint = { icon: typeof Upload; text: string; blocked?: boolean };

function hintFor(payload: DragPayload, target: DropTarget | null, copy: boolean): Hint | null {
  if (!target || payload.kind !== "files") return null;
  const action = dropAction(payload.sourceTabId, payload.entries, target, copy);
  if (!action) return null;
  switch (action.kind) {
    case "blocked":
      return { icon: Ban, text: action.reason, blocked: true };
    case "transfer":
      return action.direction === "upload"
        ? { icon: Upload, text: `Upload to ${baseName(action.folder)}` }
        : { icon: Download, text: `Download to ${baseName(action.folder)}` };
    case "place":
      return action.mode === "copy"
        ? { icon: Copy, text: `Copy to ${baseName(action.folder)}` }
        : { icon: FolderInput, text: `Move to ${baseName(action.folder)}` };
  }
}

/**
 * Follows the pointer during a drag inside the window, saying what a drop would do, and names
 * a tab another window drags over this one.
 */
export function DragGhost() {
  const payload = useDragStore((state) => state.payload);
  const target = useDragStore((state) => state.target);
  const pointer = useDragStore((state) => state.pointer);
  const copy = useDragStore((state) => state.copy);
  const incoming = useDragStore((state) => state.incoming);
  const style = {
    transform: `translate(${pointer.x + POINTER_OFFSET.x}px, ${pointer.y + POINTER_OFFSET.y}px)`,
  };
  if (incoming) {
    const TabIcon =
      incoming.kind === "local" ? HardDrive : incoming.kind === "remote" ? Server : Sparkles;
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

  const hint = hintFor(payload, target, copy);
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
