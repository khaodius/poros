import { useEffect, useRef } from "react";
import { CircleAlert, CircleCheck, LoaderCircle } from "lucide-react";
import { useCommandStore } from "../state/commandStore";
import { Dialog } from "./Dialog";

/** Shows what a server command prints while it runs, with a way to stop it. */
export function CommandOutputDialog() {
  const run = useCommandStore((state) => state.run);
  const output = useCommandStore((state) => state.output);
  const truncated = useCommandStore((state) => state.truncated);
  const running = useCommandStore((state) => state.running);
  const summary = useCommandStore((state) => state.summary);
  const stop = useCommandStore((state) => state.stop);
  const dismiss = useCommandStore((state) => state.dismiss);
  const outputRef = useRef<HTMLPreElement>(null);
  const followRef = useRef(true);

  useEffect(() => {
    const element = outputRef.current;
    if (element && followRef.current) element.scrollTop = element.scrollHeight;
  }, [output]);

  if (!run) return null;
  const single = run.commands.length === 1 ? run.commands[0] : null;

  return (
    <Dialog title={run.title} onClose={dismiss} width={720}>
      <div className="command-output-dialog">
        <p className="command-output-where">
          {single ? <code>{single}</code> : `${run.commands.length} commands`} in{" "}
          <code>{run.directory || "the login folder"}</code>
        </p>
        <pre
          ref={outputRef}
          className="command-output selectable"
          tabIndex={0}
          onScroll={(event) => {
            const element = event.currentTarget;
            followRef.current =
              element.scrollHeight - element.scrollTop - element.clientHeight < 24;
          }}
        >
          {truncated && (
            <span className="command-output-note">Earlier output was dropped.{"\n"}</span>
          )}
          {output.map((part) => (
            <span key={part.id} className={`command-output-${part.stream}`}>
              {part.text}
            </span>
          ))}
          {!running && output.length === 0 && (
            <span className="command-output-note">The command printed nothing.</span>
          )}
        </pre>
        <div className="dialog-actions">
          <span
            className={`command-output-status ${summary?.succeeded === false ? "is-failed" : ""}`}
          >
            {running ? (
              <>
                <LoaderCircle size={14} className="spin" /> Running
              </>
            ) : summary?.succeeded ? (
              <>
                <CircleCheck size={14} /> {summary.text}
              </>
            ) : (
              <>
                <CircleAlert size={14} /> {summary?.text}
              </>
            )}
          </span>
          {running ? (
            <button type="button" className="button button-danger" onClick={stop}>
              Stop
            </button>
          ) : (
            <button type="button" className="button button-primary" onClick={dismiss}>
              Close
            </button>
          )}
        </div>
      </div>
    </Dialog>
  );
}
