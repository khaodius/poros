import { Fragment, useMemo } from "react";
import { formatVersion } from "../lib/format";
import { parseReleaseNotes, type NoteSpan } from "../lib/releaseNotes";
import { useUiStore } from "../state/uiStore";
import { skipUpdate, startUpdate, useUpdateStore } from "../state/updateStore";
import { Dialog } from "./Dialog";

export function UpdateDialog({ onClose }: { onClose: () => void }) {
  const update = useUpdateStore((state) => state.update);
  const closeDialogs = useUiStore((state) => state.close);
  const notes = useMemo(() => parseReleaseNotes(update?.body ?? ""), [update]);
  if (!update) return null;

  const skip = async () => {
    await skipUpdate();
    onClose();
  };
  const install = () => {
    void startUpdate();
    // Settings too, if the update was found from there: dialogs cover the notice in the corner
    // that shows the download.
    closeDialogs();
  };

  return (
    <Dialog title="Update available" onClose={onClose} width={560}>
      <div className="confirm-body">
        <p>
          Poros <strong>{formatVersion(update.version)}</strong> is available. You have{" "}
          {formatVersion(update.currentVersion)}.
        </p>
        {notes.length > 0 && (
          <div className="release-notes selectable">
            {notes.map((block, index) => {
              switch (block.kind) {
                case "heading":
                  return (
                    <h4 key={index}>
                      <Spans spans={block.spans} />
                    </h4>
                  );
                case "paragraph":
                  return (
                    <p key={index}>
                      <Spans spans={block.spans} />
                    </p>
                  );
                case "list":
                  return (
                    <ul key={index}>
                      {block.items.map((item, itemIndex) => (
                        <li key={itemIndex}>
                          <Spans spans={item} />
                        </li>
                      ))}
                    </ul>
                  );
              }
            })}
          </div>
        )}
        <p className="field-hint">
          Poros keeps working while the update downloads, and asks before it restarts.
        </p>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button dialog-action-start" onClick={() => void skip()}>
          Skip this version
        </button>
        <button type="button" className="button" onClick={onClose}>
          Later
        </button>
        <button type="button" data-autofocus className="button button-primary" onClick={install}>
          Update
        </button>
      </div>
    </Dialog>
  );
}

function Spans({ spans }: { spans: NoteSpan[] }) {
  return spans.map((span, index) => (
    <Fragment key={index}>
      {span.kind === "strong" ? (
        <strong>{span.text}</strong>
      ) : span.kind === "code" ? (
        <code>{span.text}</code>
      ) : (
        span.text
      )}
    </Fragment>
  ));
}
