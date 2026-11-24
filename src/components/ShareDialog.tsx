import { useEffect, useRef, useState, type FormEvent } from "react";
import { Check, Copy, ExternalLink, Files } from "lucide-react";
import type { PathStyle } from "../lib/fileSource";
import { clipboard, share, toAppError } from "../lib/ipc";
import { relativeFolder } from "../lib/search";
import {
  KIND_LABELS,
  LIFETIME_UNITS,
  audienceLabel,
  batchLinkLabel,
  draftLifetime,
  draftOptions,
  draftProblem,
  forEachLimited,
  initialDraft,
  isFolderLike,
  lifetimePresets,
  linkKey,
  linkStatement,
  linksText,
  mergeLinks,
  requestNow,
  sameLink,
  selectionSummary,
  takesPassword,
  withLink,
  type LifetimeChoice,
  type LifetimeUnit,
  type ShareDraft,
  type ShareRequest,
} from "../lib/shareLinks";
import type { FileEntry, ShareCapabilities, ShareLink } from "../lib/types";
import { Dialog } from "./Dialog";
import { FileIcon } from "./FileIcon";
import { useT, t } from "../i18n";

interface ShareDialogProps {
  sessionId: string;
  /** The folder the entries are in, or the one a search for them started in. */
  folder: string;
  /**
   * For entries from several folders, such as search results: the path style each row names
   * its folder in, relative to `folder`.
   */
  folderStyle?: PathStyle;
  entries: FileEntry[];
  sharing: ShareCapabilities;
  /** The service's name, such as Google Drive. */
  service: string;
  onClose: () => void;
}

/** One item of the selection and the links it has, newest first. */
interface ItemLinks {
  entry: FileEntry;
  links: ShareLink[];
  /** The links it already had are still being read. */
  loading: boolean;
  /** Why the last request for this item failed. */
  error: string | null;
}

/** Requests in flight at once, so a large selection does not flood the service. */
const PARALLEL_REQUESTS = 4;
const COPIED_FEEDBACK_MS = 1500;
/** How often the times a statement names move on. */
const CLOCK_TICK_MS = 15_000;

const UNIT_LABELS: Record<LifetimeUnit, string> = {
  get minutes() {
    return t("share.minutes2");
  },
  get hours() {
    return t("share.hours2");
  },
  get days() {
    return t("share.days2");
  },
};

/** Tells a link made for one item from the same link of another, as Drive names them alike. */
function madeKey(path: string, link: ShareLink): string {
  return `${path}\n${linkKey(link)}`;
}

function replaceAt(
  items: readonly ItemLinks[],
  index: number,
  change: (item: ItemLinks) => ItemLinks,
): ItemLinks[] {
  return items.map((item, position) => (position === index ? change(item) : item));
}

/** The time now, moving on every `tickMs`. */
function useNow(tickMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), tickMs);
    return () => window.clearInterval(timer);
  }, [tickMs]);
  return now;
}

/** Lets people choose what a link does and who it is for, makes it, and shows the links. */
export function ShareDialog({
  sessionId,
  folder,
  folderStyle,
  entries,
  sharing,
  service,
  onClose,
}: ShareDialogProps) {
  const t = useT();
  const [draft, setDraft] = useState<ShareDraft>(() => initialDraft(sharing));
  const [items, setItems] = useState<ItemLinks[]>(() =>
    entries.map((entry) => ({ entry, links: [], loading: true, error: null })),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** The links the last Create made, by `madeKey`, which are marked as new. */
  const [fresh, setFresh] = useState<ReadonlySet<string>>(new Set());
  /** What the last Create asked for, which the links it made are checked against. */
  const [asked, setAsked] = useState<ShareRequest | null>(null);
  /** Counts the Creates that made a link, each of which moves the focus to Copy. */
  const [created, setCreated] = useState(0);
  const bodyRef = useRef<HTMLDivElement>(null);
  const single = entries.length === 1 ? entries[0] : null;
  const nowMillis = useNow(CLOCK_TICK_MS);

  // Relative to the folder the subject names, and nothing for an entry in it.
  const placeOf = (entry: FileEntry) => {
    const place = folderStyle ? relativeFolder(folder, entry, folderStyle) : undefined;
    return place && place !== "." ? place : undefined;
  };

  const update = (index: number, change: (item: ItemLinks) => ItemLinks) =>
    setItems((current) => replaceAt(current, index, change));

  // The links made earlier, on the services that keep them, so they can be copied or removed.
  useEffect(() => {
    let current = true;
    void forEachLimited(entries, PARALLEL_REQUESTS, async (entry, index) => {
      try {
        const read = await share.links(sessionId, entry.path);
        if (!current) return;
        setItems((items) =>
          replaceAt(items, index, (item) => ({
            ...item,
            links: mergeLinks(item.links, read),
            loading: false,
          })),
        );
      } catch (caught) {
        const message = `Could not read the links it has: ${toAppError(caught).message}`;
        if (!current) return;
        setItems((items) =>
          replaceAt(items, index, (item) => ({ ...item, loading: false, error: message })),
        );
      }
    });
    return () => {
      current = false;
    };
  }, [sessionId, entries]);

  const change = (patch: Partial<ShareDraft>) => setDraft((current) => ({ ...current, ...patch }));
  const options = draftOptions(draft, sharing);
  const problem = draftProblem(draft, sharing);
  const lifetime = draftLifetime(draft, sharing);
  const folderAmong = entries.some(isFolderLike);
  const presets = lifetimePresets(sharing);
  const withPassword = takesPassword(draft, sharing);

  // Copy is what comes next once a link is made.
  const copySelector = single ? ".share-link.is-new .share-copy" : ".share-copy-all";
  useEffect(() => {
    if (created > 0) bodyRef.current?.querySelector<HTMLElement>(copySelector)?.focus();
  }, [created, copySelector]);

  const create = async (event: FormEvent) => {
    event.preventDefault();
    if (!options || busy) return;
    setBusy(true);
    setError(null);
    const request = requestNow(options);
    const made = new Set<string>();
    await forEachLimited(entries, PARALLEL_REQUESTS, async (entry, index) => {
      try {
        const link = await share.create(sessionId, entry.path, options);
        made.add(madeKey(entry.path, link));
        update(index, (item) => ({ ...item, links: withLink(item.links, link), error: null }));
      } catch (caught) {
        const message = toAppError(caught).message;
        if (single) setError(message);
        else update(index, (item) => ({ ...item, error: message }));
      }
    });
    setAsked(request);
    setFresh(made);
    setBusy(false);
    if (made.size > 0) setCreated((count) => count + 1);
  };

  const remove = async (index: number, link: ShareLink) => {
    setError(null);
    await share.remove(sessionId, items[index].entry.path, link.id);
    update(index, (item) => ({
      ...item,
      links: item.links.filter((other) => !sameLink(other, link)),
    }));
    bodyRef.current?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
  };

  const statementFor = (link: ShareLink, entry: FileEntry) =>
    linkStatement({
      kind: link.kind,
      audience: link.audience,
      expiresAt: link.expiresAt,
      revocable: link.revocable,
      organization: sharing.organization,
      count: 1,
      folder: isFolderLike(entry),
      hasPassword: link.hasPassword,
      nowMillis,
    });

  const draftStatement =
    options === null
      ? null
      : linkStatement({
          kind: options.kind,
          audience: options.audience,
          expiresAt:
            typeof lifetime === "number" ? Math.floor(nowMillis / 1000) + lifetime : undefined,
          revocable: sharing.revocable,
          organization: sharing.organization,
          count: entries.length,
          folder: folderAmong,
          hasPassword: Boolean(options.password),
          nowMillis,
        });

  const isFresh = (item: ItemLinks, link: ShareLink) => fresh.has(madeKey(item.entry.path, link));
  // Only what the last Create made: the rows say what any other link does.
  const made = items.flatMap((item) => item.links.filter((link) => isFresh(item, link)));
  const singleItem = single ? items[0] : null;
  // A service with one link per item changes the link an item has, for everyone who has it.
  const changesLink = sharing.singleLink && items.some((item) => item.links.length > 0);
  const createLabel = single
    ? changesLink
      ? "Change link"
      : "Create link"
    : `Create ${entries.length} links`;

  return (
    <Dialog
      title={
        single
          ? t("share.share", { name: single.name })
          : t("share.share2", { selectionSummary: selectionSummary(entries) })
      }
      onClose={onClose}
      width={560}
      className="share-dialog"
    >
      <form className="form" onSubmit={create}>
        <div ref={bodyRef} className="share-body">
          <div className="share-subject">
            {single ? <FileIcon entry={single} size={22} /> : <Files size={22} />}
            <div className="share-subject-text">
              <strong title={single?.name}>
                {single ? single.name : selectionSummary(entries)}
              </strong>
              <span title={folder}>{t("share.onIn", { service, folder })}</span>
            </div>
          </div>

          {singleItem && singleItem.links.length > 0 && (
            <section className="share-section" aria-label={t("share.links")}>
              <h3 className="share-section-title">
                {singleItem.links.length === 1
                  ? t("share.link")
                  : t("share.links2", { count: singleItem.links.length })}
              </h3>
              <ul className="share-links">
                {singleItem.links.map((link) => (
                  <LinkRow
                    key={linkKey(link)}
                    link={link}
                    statement={statementFor(link, singleItem.entry)}
                    fresh={isFresh(singleItem, link)}
                    onRemove={() => remove(0, link)}
                    onError={setError}
                  />
                ))}
              </ul>
            </section>
          )}
          {singleItem?.loading && sharing.revocable && (
            <p className="field-hint">{t("share.lookingForLinksItAlready")}</p>
          )}
          {singleItem?.error && <p className="field-hint is-error">{singleItem.error}</p>}

          <section
            className="share-section share-options"
            aria-label={single && changesLink ? t("share.changeLink") : t("share.newLink")}
          >
            {singleItem && singleItem.links.length > 0 && (
              <h3 className="share-section-title">
                {changesLink ? t("share.changeLink") : t("share.newLink")}
              </h3>
            )}
            {sharing.kinds.length > 1 && (
              <div className="field">
                <span>{t("share.letsPeople")}</span>
                <div className="segmented" role="radiogroup" aria-label={t("share.letsPeople")}>
                  {sharing.kinds.map((kind) => (
                    <button
                      key={kind}
                      type="button"
                      role="radio"
                      aria-checked={draft.kind === kind}
                      className={draft.kind === kind ? "is-selected" : ""}
                      onClick={() => change({ kind })}
                    >
                      {KIND_LABELS[kind]}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {sharing.audiences.length > 1 && (
              <label className="field">
                <span>{t("share.whoCanOpenIt")}</span>
                <select
                  value={draft.audience}
                  onChange={(event) =>
                    change({ audience: event.target.value as ShareDraft["audience"] })
                  }
                >
                  {sharing.audiences.map((audience) => (
                    <option key={audience} value={audience}>
                      {audienceLabel(audience, sharing.organization)}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {sharing.expiry !== "never" && (
              <div className="field">
                <span>{t("share.worksFor")}</span>
                <div className="share-lifetime">
                  <div className="segmented" role="radiogroup" aria-label={t("share.worksFor")}>
                    {sharing.expiry === "optional" && (
                      <LifetimeButton
                        value="never"
                        label={t("share.noEnd")}
                        draft={draft}
                        onChoose={(choice) => change({ lifetime: choice })}
                      />
                    )}
                    {presets.map((preset) => (
                      <LifetimeButton
                        key={preset.seconds}
                        value={preset.seconds}
                        label={preset.label}
                        draft={draft}
                        onChoose={(choice) => change({ lifetime: choice })}
                      />
                    ))}
                    <LifetimeButton
                      value="custom"
                      label={t("share.custom")}
                      draft={draft}
                      onChoose={(choice) => change({ lifetime: choice })}
                    />
                  </div>
                  {draft.lifetime === "custom" && (
                    <div className="share-custom-lifetime">
                      <input
                        aria-label={t("share.howMany")}
                        inputMode="numeric"
                        className={problem ? "has-error" : ""}
                        value={draft.customAmount}
                        onChange={(event) => change({ customAmount: event.target.value })}
                      />
                      <select
                        aria-label={t("share.unit")}
                        value={draft.customUnit}
                        onChange={(event) =>
                          change({ customUnit: event.target.value as LifetimeUnit })
                        }
                      >
                        {LIFETIME_UNITS.map((unit) => (
                          <option key={unit} value={unit}>
                            {UNIT_LABELS[unit]}
                          </option>
                        ))}
                      </select>
                    </div>
                  )}
                </div>
                {problem && <small className="field-hint is-error">{problem}</small>}
              </div>
            )}
            {withPassword && (
              <label className="field">
                <span>{t("share.passwordOptional")}</span>
                <input
                  type="password"
                  value={draft.password}
                  autoComplete="new-password"
                  placeholder={t("share.anyoneWithTheLinkCan")}
                  onChange={(event) => change({ password: event.target.value })}
                />
              </label>
            )}
            {draftStatement && <p className="share-statement">{draftStatement}</p>}
            {changesLink && (
              <p className="field-hint">
                {single
                  ? t("share.keepsOneLinkPerItem", { service })
                  : t("share.keepsOneLinkPerItemSo", { service })}
              </p>
            )}
          </section>

          {!single && (
            <ul className="share-batch" aria-label={t("share.links")}>
              {items.map((item, index) => {
                const link = item.links[0];
                const isNew = link !== undefined && isFresh(item, link);
                return (
                  <BatchRow
                    key={item.entry.path}
                    item={item}
                    place={placeOf(item.entry)}
                    busy={busy}
                    fresh={isNew}
                    label={
                      link === undefined
                        ? null
                        : batchLinkLabel(
                            link,
                            isNew ? asked : null,
                            options,
                            statementFor(link, item.entry),
                          )
                    }
                    onError={(message) =>
                      update(index, (current) => ({ ...current, error: message }))
                    }
                  />
                );
              })}
            </ul>
          )}

          {error && <p className="form-error">{error}</p>}

          <div className="dialog-actions">
            {!single && (
              <CopyButton
                className="share-copy-all push-left"
                text={linksText(made)}
                label={made.length === 1 ? t("share.copyNewLink") : t("share.copyNewLinks")}
                disabled={made.length === 0}
                onError={setError}
              />
            )}
            <button type="button" className="button" onClick={onClose}>
              {t("common.close")}
            </button>
            <button
              type="submit"
              data-autofocus
              className="button button-primary"
              disabled={busy || options === null}
            >
              {busy
                ? single && changesLink
                  ? t("share.changing")
                  : t("share.creating")
                : createLabel}
            </button>
          </div>
        </div>
      </form>
    </Dialog>
  );
}

function LifetimeButton({
  value,
  label,
  draft,
  onChoose,
}: {
  value: LifetimeChoice;
  label: string;
  draft: ShareDraft;
  onChoose: (choice: LifetimeChoice) => void;
}) {
  const chosen = draft.lifetime === value;
  return (
    <button
      type="button"
      role="radio"
      aria-checked={chosen}
      className={chosen ? "is-selected" : ""}
      onClick={() => onChoose(value)}
    >
      {label}
    </button>
  );
}

interface CopyButtonProps {
  text: string;
  label?: string;
  className?: string;
  disabled?: boolean;
  /** Only the icon, for a row of a list. */
  compact?: boolean;
  onError: (message: string) => void;
}

/** Copies through the app's own clipboard command, and says so for a moment. */
function CopyButton({
  text,
  label = "Copy",
  className = "",
  disabled,
  compact,
  onError,
}: CopyButtonProps) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copy = async () => {
    try {
      await clipboard.writeText(text);
      setCopied(true);
    } catch (caught) {
      onError(toAppError(caught).message);
    }
  };
  const icon = copied ? <Check size={14} /> : <Copy size={14} />;
  if (compact) {
    const title = copied ? "Copied" : "Copy link";
    return (
      <button
        type="button"
        className={`icon-button share-copy ${copied ? "is-copied" : ""} ${className}`}
        title={title}
        aria-label={title}
        disabled={disabled}
        onClick={() => void copy()}
      >
        {icon}
      </button>
    );
  }
  return (
    <button
      type="button"
      className={`button share-copy ${copied ? "is-copied" : ""} ${className}`}
      disabled={disabled}
      onClick={() => void copy()}
    >
      {icon}
      {copied ? t("share.copied") : label}
    </button>
  );
}

function LinkAddress({ url }: { url: string }) {
  const t = useT();
  return (
    <input
      readOnly
      className="share-address"
      aria-label={t("share.linkAddress")}
      spellCheck={false}
      value={url}
      onFocus={(event) => event.currentTarget.select()}
      // Enter in a field submits the form, which here would make another link.
      onKeyDown={(event) => {
        if (event.key === "Enter") event.preventDefault();
      }}
    />
  );
}

interface LinkRowProps {
  link: ShareLink;
  statement: string;
  /** Made by the last Create. */
  fresh: boolean;
  onRemove: () => Promise<void>;
  onError: (message: string) => void;
}

/** A link of the one item shared: its address, what it allows, and how to stop it. */
function LinkRow({ link, statement, fresh, onRemove, onError }: LinkRowProps) {
  const t = useT();
  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);

  const open = async () => {
    try {
      await share.openInBrowser(link.url);
    } catch (caught) {
      onError(toAppError(caught).message);
    }
  };

  const stop = async () => {
    setRemoving(true);
    try {
      await onRemove();
    } catch (caught) {
      onError(toAppError(caught).message);
      setRemoving(false);
      setConfirming(false);
    }
  };

  return (
    <li className={`share-link ${fresh ? "is-new" : ""}`}>
      <div className="share-link-field">
        <LinkAddress url={link.url} />
        <CopyButton text={link.url} onError={onError} />
        <button
          type="button"
          className="icon-button"
          title={t("share.openInBrowser")}
          aria-label={t("share.openInBrowser")}
          onClick={() => void open()}
        >
          <ExternalLink size={15} />
        </button>
      </div>
      <div className="share-link-facts">
        <p className="share-link-statement">{statement}</p>
        {link.revocable && !confirming && (
          <button type="button" className="button button-small" onClick={() => setConfirming(true)}>
            {t("share.stopSharing")}
          </button>
        )}
      </div>
      {link.notes.map((note) => (
        <p key={note} className="share-link-note">
          {note}
        </p>
      ))}
      {confirming && (
        <div className="share-stop" role="group" aria-label={t("share.stopSharing")}>
          <span>{t("share.peopleWhoHaveThisLink")}</span>
          <button
            type="button"
            className="button button-small"
            autoFocus
            disabled={removing}
            onClick={() => setConfirming(false)}
          >
            {t("share.keepSharing")}
          </button>
          <button
            type="button"
            className="button button-small button-danger"
            disabled={removing}
            onClick={() => void stop()}
          >
            {removing ? t("share.stopping") : t("share.stopSharing")}
          </button>
        </div>
      )}
    </li>
  );
}

interface BatchRowProps {
  item: ItemLinks;
  /** The folder the item is in, among items from several folders. */
  place?: string;
  busy: boolean;
  /** Made by the last Create. */
  fresh: boolean;
  /** What the link does, where the statement above does not say it. */
  label: string | null;
  onError: (message: string) => void;
}

/**
 * One item of several shared at once: its name, and its newest link or why it has none, with
 * what that link does when it is not what the statement above says, and the service's notes.
 */
function BatchRow({ item, busy, fresh, label, place, onError }: BatchRowProps) {
  const link = item.links[0];
  const status = item.loading ? "Checking..." : busy ? "Making the link..." : "No link yet";
  return (
    <li className={`share-batch-row ${fresh ? "is-new" : ""}`}>
      <span className="share-batch-name" title={item.entry.path}>
        <FileIcon entry={item.entry} />
        {place ? (
          <span className="share-batch-where">
            <span>{item.entry.name}</span>
            <span className="share-batch-place">{place}</span>
          </span>
        ) : (
          <span>{item.entry.name}</span>
        )}
      </span>
      {link ? (
        <span className="share-batch-link">
          <LinkAddress url={link.url} />
          <CopyButton compact text={link.url} onError={onError} />
        </span>
      ) : (
        !item.error && <span className="share-batch-status">{status}</span>
      )}
      {link && label && <span className="share-batch-label">{label}</span>}
      {link?.notes.map((note) => (
        <span key={note} className="share-batch-note">
          {note}
        </span>
      ))}
      {item.error && <span className="share-batch-error">{item.error}</span>}
    </li>
  );
}
