import { memo, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowDownToLine,
  ClipboardCopy,
  Clock,
  Save,
  Search,
  Trash2,
  WrapText,
  X,
} from "lucide-react";
import { formatTime } from "../lib/format";
import { files, toAppError } from "../lib/ipc";
import type { LogLevel } from "../lib/types";
import { useLogStore, type LogLine } from "../state/logStore";
import { useSessionStore } from "../state/sessionStore";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { useToastStore } from "../state/toastStore";

const STICK_TO_BOTTOM_THRESHOLD = 24;
const ALL_SESSIONS = "";

const LEVELS: { level: LogLevel; label: string }[] = [
  { level: "info", label: "Info" },
  { level: "warn", label: "Warnings" },
  { level: "error", label: "Errors" },
  { level: "server", label: "Server" },
];

function plainText(line: LogLine): string {
  return `${formatTime(line.timestamp)} ${line.level.toUpperCase().padEnd(6)} ${line.message}`;
}

export function LogView() {
  const lines = useLogStore((state) => state.lines);
  const clear = useLogStore((state) => state.clear);
  const options = useSettingsStore((state) => state.settings.log);
  const sessions = useSessionStore((state) => state.sessions);
  const showToast = useToastStore((state) => state.show);
  const [query, setQuery] = useState("");
  const [sessionFilter, setSessionFilter] = useState(ALL_SESSIONS);
  const [follow, setFollow] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return lines.filter(
      (line) =>
        options.levels[line.level] &&
        (sessionFilter === ALL_SESSIONS || line.sessionId === sessionFilter) &&
        (needle === "" || line.message.toLowerCase().includes(needle)),
    );
  }, [lines, options.levels, sessionFilter, query]);

  const loggedSessions = useMemo(() => {
    const ids = new Set<string>();
    for (const line of lines) if (line.sessionId) ids.add(line.sessionId);
    return [...ids];
  }, [lines]);

  useEffect(() => {
    const scroller = scrollRef.current;
    if (scroller && follow) scroller.scrollTop = scroller.scrollHeight;
  }, [visible, follow]);

  const toggleLevel = (level: LogLevel) =>
    void saveSettingsSection("log", {
      levels: { ...options.levels, [level]: !options.levels[level] },
    });

  const copyVisible = () => void navigator.clipboard.writeText(visible.map(plainText).join("\n"));

  const saveVisible = async () => {
    const path = await saveFileDialog({
      title: "Save log",
      defaultPath: "poros.log",
      filters: [{ name: "Log files", extensions: ["log", "txt"] }],
    });
    if (!path) return;
    try {
      await files.saveText(path, `${visible.map(plainText).join("\n")}\n`);
    } catch (caught) {
      showToast("error", toAppError(caught).message);
    }
  };

  return (
    <div className="log-view">
      <div className="log-toolbar">
        <div className="toggle-group" role="group" aria-label="Levels">
          {LEVELS.map(({ level, label }) => (
            <button
              key={level}
              type="button"
              className={`toggle log-toggle-${level} ${options.levels[level] ? "is-on" : ""}`}
              aria-pressed={options.levels[level]}
              onClick={() => toggleLevel(level)}
            >
              {label}
            </button>
          ))}
        </div>
        <select
          className="log-session"
          value={sessionFilter}
          aria-label="Session"
          onChange={(event) => setSessionFilter(event.target.value)}
        >
          <option value={ALL_SESSIONS}>All sessions</option>
          {loggedSessions.map((id) => (
            <option key={id} value={id}>
              {sessions[id]?.info.label ?? `Closed session ${id.slice(0, 8)}`}
            </option>
          ))}
        </select>
        <label className="filter log-search">
          <Search size={13} />
          <input
            value={query}
            placeholder="Search log"
            spellCheck={false}
            onChange={(event) => setQuery(event.target.value)}
          />
          {query && (
            <button
              type="button"
              className="icon-button"
              aria-label="Clear search"
              onClick={() => setQuery("")}
            >
              <X size={12} />
            </button>
          )}
        </label>
        <span className="toolbar-spacer" />
        <IconToggle
          label="Show times"
          on={options.timestamps}
          onToggle={() => void saveSettingsSection("log", { timestamps: !options.timestamps })}
        >
          <Clock size={14} />
        </IconToggle>
        <IconToggle
          label="Wrap long lines"
          on={options.wrapLines}
          onToggle={() => void saveSettingsSection("log", { wrapLines: !options.wrapLines })}
        >
          <WrapText size={14} />
        </IconToggle>
        <IconToggle label="Follow new lines" on={follow} onToggle={() => setFollow(!follow)}>
          <ArrowDownToLine size={14} />
        </IconToggle>
        <span className="toolbar-divider" />
        <button
          type="button"
          className="icon-button"
          title="Copy shown lines"
          aria-label="Copy shown lines"
          disabled={visible.length === 0}
          onClick={copyVisible}
        >
          <ClipboardCopy size={14} />
        </button>
        <button
          type="button"
          className="icon-button"
          title="Save shown lines to a file"
          aria-label="Save shown lines to a file"
          disabled={visible.length === 0}
          onClick={() => void saveVisible()}
        >
          <Save size={14} />
        </button>
        <button
          type="button"
          className="icon-button"
          title="Clear log"
          aria-label="Clear log"
          disabled={lines.length === 0}
          onClick={clear}
        >
          <Trash2 size={14} />
        </button>
      </div>
      <div
        ref={scrollRef}
        className={`log selectable ${options.wrapLines ? "" : "no-wrap"}`}
        onScroll={(event) => {
          const target = event.currentTarget;
          const atBottom =
            target.scrollHeight - target.scrollTop - target.clientHeight <
            STICK_TO_BOTTOM_THRESHOLD;
          if (atBottom !== follow) setFollow(atBottom);
        }}
      >
        {visible.length === 0 && (
          <p className="log-empty">
            {lines.length === 0
              ? "Connection and transfer activity appears here."
              : "No lines match the filters."}
          </p>
        )}
        {visible.map((line) => (
          <LogLineView key={line.id} line={line} timestamps={options.timestamps} />
        ))}
      </div>
    </div>
  );
}

const LogLineView = memo(function LogLineView({
  line,
  timestamps,
}: {
  line: LogLine;
  timestamps: boolean;
}) {
  return (
    <div className={`log-line log-${line.level}`}>
      {timestamps && <time>{formatTime(line.timestamp)}</time>}
      <span className="log-message">{line.message}</span>
    </div>
  );
});

interface IconToggleProps {
  label: string;
  on: boolean;
  onToggle: () => void;
  children: ReactNode;
}

function IconToggle({ label, on, onToggle, children }: IconToggleProps) {
  return (
    <button
      type="button"
      className={`icon-button ${on ? "is-pressed" : ""}`}
      title={label}
      aria-label={label}
      aria-pressed={on}
      onClick={onToggle}
    >
      {children}
    </button>
  );
}
