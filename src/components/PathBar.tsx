import { useEffect, useRef, useState } from "react";
import { ChevronRight } from "lucide-react";
import type { PathStyle } from "../lib/fileSource";
import { breadcrumbs } from "../lib/path";

interface PathBarProps {
  path: string | null;
  pathStyle: PathStyle;
  editRequest: number;
  onNavigate: (path: string) => Promise<boolean>;
}

export function PathBar({ path, pathStyle, editRequest, onNavigate }: PathBarProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const startEditing = () => {
    setDraft(path ?? "");
    setEditing(true);
  };

  useEffect(() => {
    if (editRequest > 0) startEditing();
    // Only a new edit request should open the editor.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editRequest]);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  if (editing) {
    return (
      <form
        className="path-bar"
        onSubmit={async (event) => {
          event.preventDefault();
          if (await onNavigate(draft.trim())) setEditing(false);
        }}
      >
        <input
          ref={inputRef}
          className="path-input"
          value={draft}
          spellCheck={false}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={() => setEditing(false)}
          onKeyDown={(event) => {
            if (event.key === "Escape") setEditing(false);
          }}
        />
      </form>
    );
  }

  const crumbs = path ? breadcrumbs(path, pathStyle) : [];
  return (
    <div className="path-bar" onClick={startEditing} title="Click to type a path (Ctrl+L)">
      {crumbs.map((crumb, index) => (
        <span key={crumb.path} className="crumb-group">
          {index > 0 && <ChevronRight size={12} className="crumb-separator" />}
          <button
            type="button"
            className="crumb"
            onClick={(event) => {
              event.stopPropagation();
              void onNavigate(crumb.path);
            }}
          >
            {crumb.label}
          </button>
        </span>
      ))}
    </div>
  );
}
