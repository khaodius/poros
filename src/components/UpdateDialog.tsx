import { Fragment, useMemo } from "react";
import { useLatest } from "../hooks/useLatest";
import { formatVersion } from "../lib/format";
import { parseReleaseNotes, type NoteSpan } from "../lib/releaseNotes";
import { useUiStore } from "../state/uiStore";
import { skipUpdate, startUpdate, useUpdateStore } from "../state/updateStore";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

export function UpdateDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const update = useUpdateStore((state) => state.update);
  const closeDialogs = useUiStore((state) => state.close);
  const latestOnClose = useLatest(onClose);
  const notes = useMemo(() => parseReleaseNotes(update?.body ?? ""), [update]);
  if (!update) return null;

  const skip = async () => {
    await skipUpdate();
    latestOnClose.current();
  };
  const install = () => {
    void startUpdate(update);
    // Settings too, if the update was found from there: dialogs cover the notice in the corner
    // that shows the download.
    closeDialogs();
  };

  return (
    <Dialog title={t("updates.updateAvailable")} onClose={onClose} width={560}>
      <div className="confirm-body">
        <p>
          {t.rich("updates.isAvailableYouHave", {
            version: <strong>{formatVersion(update.version)}</strong>,
            current: formatVersion(update.currentVersion),
          })}
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
        <p className="field-hint">{t("updates.porosKeepsWorkingWhileThe")}</p>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button dialog-action-start" onClick={() => void skip()}>
          {t("updates.skipThisVersion")}
        </button>
        <button type="button" className="button" onClick={onClose}>
          {t("updates.later")}
        </button>
        <button type="button" data-autofocus className="button button-primary" onClick={install}>
          {t("updates.update")}
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
