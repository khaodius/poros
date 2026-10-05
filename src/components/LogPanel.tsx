import { useEffect, useRef } from "react";
import { Trash2 } from "lucide-react";
import { formatTime } from "../lib/format";
import { useLogStore } from "../state/logStore";

const STICK_TO_BOTTOM_THRESHOLD = 24;

export function LogPanel() {
  const lines = useLogStore((state) => state.lines);
  const clear = useLogStore((state) => state.clear);
  const scrollRef = useRef<HTMLDivElement>(null);
  const followTail = useRef(true);

  useEffect(() => {
    const scroller = scrollRef.current;
    if (scroller && followTail.current) scroller.scrollTop = scroller.scrollHeight;
  }, [lines]);

  return (
    <section className="bottom-panel">
      <header className="bottom-panel-header">
        <div className="tabs" role="tablist">
          <button type="button" role="tab" aria-selected className="tab is-selected">
            Log
          </button>
        </div>
        <button
          type="button"
          className="icon-button"
          title="Clear log"
          aria-label="Clear log"
          onClick={clear}
          disabled={lines.length === 0}
        >
          <Trash2 size={14} />
        </button>
      </header>
      <div
        ref={scrollRef}
        className="log selectable"
        onScroll={(event) => {
          const target = event.currentTarget;
          followTail.current =
            target.scrollHeight - target.scrollTop - target.clientHeight <
            STICK_TO_BOTTOM_THRESHOLD;
        }}
      >
        {lines.length === 0 && (
          <p className="log-empty">Connection and file activity appears here.</p>
        )}
        {lines.map((line) => (
          <div key={line.id} className={`log-line log-${line.level}`}>
            <time>{formatTime(line.timestamp)}</time>
            <span className="log-message">{line.message}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
