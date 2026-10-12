import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ChevronRight } from "lucide-react";
import type { PathStyle } from "../lib/fileSource";
import { breadcrumbs } from "../lib/path";

interface PathBarProps {
  path: string | null;
  pathStyle: PathStyle;
  editRequest: number;
  onNavigate: (path: string) => Promise<boolean>;
  /** After Enter or Escape ends an edit. */
  onDone: () => void;
}

export function PathBar({ path, pathStyle, editRequest, onNavigate, onDone }: PathBarProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [handledEditRequest, setHandledEditRequest] = useState(editRequest);
  const inputRef = useRef<HTMLInputElement>(null);
  const crumbsRef = useRef<HTMLDivElement>(null);
  const [clipped, setClipped] = useState(false);

  const startEditing = () => {
    setDraft(path ?? "");
    setEditing(true);
  };

  if (editRequest !== handledEditRequest) {
    setHandledEditRequest(editRequest);
    startEditing();
  }

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  // A long path shows its end, the folder on show, and fades out where its start is cut off.
  useLayoutEffect(() => {
    const crumbs = crumbsRef.current;
    if (!crumbs) return;
    const measure = () => {
      const first = crumbs.firstElementChild;
      setClipped(
        !!first && first.getBoundingClientRect().left < crumbs.getBoundingClientRect().left - 0.5,
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(crumbs);
    return () => observer.disconnect();
  }, [editing, path]);

  if (editing) {
    return (
      <form
        className="path-bar"
        onSubmit={async (event) => {
          event.preventDefault();
          if (!(await onNavigate(draft.trim()))) return;
          setEditing(false);
          onDone();
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
            if (event.key !== "Escape") return;
            setEditing(false);
            onDone();
          }}
        />
      </form>
    );
  }

  const crumbs = path ? breadcrumbs(path, pathStyle) : [];
  // The space after the crumbs is always there, so the whole path can be picked up to copy.
  return (
    <div
      className="path-bar"
      onClick={startEditing}
      title="Click to type or copy the path (Ctrl+L)"
    >
      <div ref={crumbsRef} className={`crumbs ${clipped ? "is-clipped" : ""}`}>
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
    </div>
  );
}
