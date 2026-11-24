import { useEffect, useRef } from "react";
import { CircleAlert, CircleCheck, LoaderCircle } from "lucide-react";
import { useCommandStore } from "../state/commandStore";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

/** Shows what a server command prints while it runs, with a way to stop it. */
export function CommandOutputDialog() {
  const t = useT();
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
            <span className="command-output-note">{t("automation.earlierOutputWasDropped")}</span>
          )}
          {output.map((part) => (
            <span key={part.id} className={`command-output-${part.stream}`}>
              {part.text}
            </span>
          ))}
          {!running && output.length === 0 && (
            <span className="command-output-note">{t("automation.theCommandPrintedNothing")}</span>
          )}
        </pre>
        <div className="dialog-actions">
          <span
            className={`command-output-status ${summary?.succeeded === false ? "is-failed" : ""}`}
          >
            {running ? (
              <>
                <LoaderCircle size={14} className="spin" /> {t("automation.running")}
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
              {t("common.stop")}
            </button>
          ) : (
            <button type="button" className="button button-primary" onClick={dismiss}>
              {t("common.close")}
            </button>
          )}
        </div>
      </div>
    </Dialog>
  );
}
