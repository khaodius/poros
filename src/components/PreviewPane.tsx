import {
  Suspense,
  lazy,
  useMemo,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import {
  Ban,
  CircleSlash,
  Eye,
  FileCheck,
  FilePen,
  FileQuestion,
  FileX,
  Files,
  FolderOpen,
  Hourglass,
  Loader,
  LockKeyhole,
  Scaling,
  Unplug,
  type LucideIcon,
} from "lucide-react";
import { useRestedLoad, type Rested } from "../hooks/useRestedLoad";
import { formatDate, formatPermissions, formatSize, pluralize } from "../lib/format";
import type { FileView } from "../lib/previewLoad";
import {
  HEX_BYTES,
  MAX_SHOWN_PIXELS,
  capsOf,
  firstStep,
  listsOwnSize,
  selectionKey,
  summarize,
  type Caps,
  type Problem,
  type Subject,
  type Summary,
} from "../lib/previewPlan";
import type { PreviewLoad } from "../lib/previewSettings";
import { IMAGE_FORMAT_LABELS } from "../lib/previewSniff";
import { TEXT_ENCODING_LABELS, hexRows } from "../lib/previewText";
import type { DateFormat } from "../lib/settings";
import { typeLabel } from "../lib/sort";
import type { DirListing, FileEntry } from "../lib/types";
import { vaultSign } from "../lib/vaults";
import {
  cachedFileView,
  cachedFolder,
  listFolder,
  readFileView,
  type PreviewPlace,
} from "../state/previewReads";
import { useSettingsStore } from "../state/settingsStore";
import { FileIcon } from "./FileIcon";
import { PreviewImage, type PictureSize } from "./PreviewImage";
import { PreviewThumbnails } from "./PreviewThumbnails";

// Loaded on first use, with the editor's highlighting.
const PreviewText = lazy(() => import("./PreviewText"));

/** As many thumbnails as one preview shows; a folder or selection with more says how many. */
const MAX_TILES = 120;
const NOTICE_ICON_SIZE = 32;
/** How far an arrow key scrolls the preview. */
const LINE_SCROLL = 40;

const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

interface PreviewPaneProps {
  place: PreviewPlace;
  subject: Subject;
  dateFormat: DateFormat;
  onOpenInEditor: (entry: FileEntry) => void;
  /** Lists the pane's folder again, as after a file turned out to have changed. */
  onRefresh: () => void;
  /** Tab moves on to the next pane. */
  onSwitchPane: (from: HTMLElement) => void;
  /** Escape and Shift+Tab go back to the file list. */
  onLeave: () => void;
}

interface Handlers {
  onOpenInEditor: (entry: FileEntry) => void;
  onRefresh: () => void;
}

/**
 * Shows the item under the cursor of a file list without opening it: a picture, the start of
 * a text, what a folder holds, or the details the list has for anything else.
 */
export function PreviewPane({
  place,
  subject,
  dateFormat,
  onOpenInEditor,
  onRefresh,
  onSwitchPane,
  onLeave,
}: PreviewPaneProps) {
  const settings = useSettingsStore((state) => state.settings.interface.preview);
  const caps = useMemo(() => capsOf(settings), [settings]);
  const shared = { place, caps, mode: settings.load[place.storage], dateFormat };
  const handlers = { onOpenInEditor, onRefresh };

  let content: ReactNode;
  switch (subject.kind) {
    case "file":
      // Another version of the file starts afresh: "Load anyway" and its size were for this one.
      content = (
        <FilePreview
          key={JSON.stringify([subject.entry.path, subject.entry.size, subject.entry.modified])}
          entry={subject.entry}
          {...shared}
          {...handlers}
        />
      );
      break;
    case "folder":
      content = (
        <FolderPreview
          key={subject.entry.path}
          entry={subject.entry}
          box={settings.thumbnailSize}
          {...shared}
        />
      );
      break;
    case "selection":
      content = (
        <SelectionPreview entries={subject.entries} box={settings.thumbnailSize} {...shared} />
      );
      break;
    case "special":
      content = (
        <>
          <Header icon={<FileIcon entry={subject.entry} />} title={subject.entry.name} />
          <Notice
            icon={FileQuestion}
            title={subject.entry.linkTarget === "broken" ? "Broken link" : "Not a regular file"}
            detail={
              subject.entry.linkTarget === "broken"
                ? "The file this link leads to is not there."
                : "There is nothing in it to preview."
            }
          />
          <Details rows={entryRows(subject.entry, dateFormat)} />
        </>
      );
      break;
    case "nothing":
      content = (
        <Notice
          icon={Eye}
          title="Nothing to preview"
          detail="Move the cursor to a file or folder to see it here."
        />
      );
  }

  // Arrow and page keys scroll what the preview shows, wherever in it that scrolls.
  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const pane = event.currentTarget;
    const scroller =
      pane.querySelector<HTMLElement>(".cm-scroller, .preview-image, .preview-scroll") ?? pane;
    const page = Math.max(LINE_SCROLL, scroller.clientHeight - LINE_SCROLL);
    const scrolls: Record<string, () => void> = {
      ArrowDown: () => scroller.scrollBy({ top: LINE_SCROLL }),
      ArrowUp: () => scroller.scrollBy({ top: -LINE_SCROLL }),
      ArrowRight: () => scroller.scrollBy({ left: LINE_SCROLL }),
      ArrowLeft: () => scroller.scrollBy({ left: -LINE_SCROLL }),
      PageDown: () => scroller.scrollBy({ top: page }),
      PageUp: () => scroller.scrollBy({ top: -page }),
      Home: () => scroller.scrollTo({ top: 0 }),
      End: () => scroller.scrollTo({ top: scroller.scrollHeight }),
    };
    if (event.key === "Tab") {
      if (event.shiftKey) onLeave();
      else onSwitchPane(pane);
    } else if (event.key === "Escape") {
      onLeave();
    } else if (event.key === "Enter" || event.key === " ") {
      pane.querySelector<HTMLElement>("[data-preview-primary]")?.click();
    } else if (event.key in scrolls && !event.shiftKey) {
      scrolls[event.key]();
    } else {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
  };

  // A click in the preview leaves the keyboard with the file list, except where there is text
  // to select.
  const keepFocus = (event: MouseEvent<HTMLElement>) => {
    if (!(event.target as Element).closest(".preview-selectable")) event.preventDefault();
  };

  return (
    <aside
      className="preview"
      // Reached from the file list with Ctrl+Shift+P, never by itself.
      tabIndex={0}
      aria-label="Preview"
      onKeyDown={handleKeyDown}
      onMouseDown={keepFocus}
    >
      {content}
    </aside>
  );
}

function Header({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
}) {
  return (
    <header className="preview-header">
      {icon}
      <span className="preview-name" title={title}>
        {title}
      </span>
      {children}
    </header>
  );
}

interface NoticeProps {
  icon: LucideIcon;
  title: string;
  detail?: string;
  /** A button under the text, which Enter presses while the preview has the keyboard. */
  action?: { label: string; onSelect: () => void };
}

function Notice({ icon: Icon, title, detail, action }: NoticeProps) {
  return (
    <div className="preview-notice">
      <Icon size={NOTICE_ICON_SIZE} className="preview-notice-icon" aria-hidden />
      <p className="preview-notice-title">{title}</p>
      {detail && <p className="preview-notice-detail">{detail}</p>}
      {action && (
        <button type="button" className="button" data-preview-primary onClick={action.onSelect}>
          {action.label}
        </button>
      )}
    </div>
  );
}

/** Appears only once a load has taken a moment, so quick ones show nothing in between. */
function Loading() {
  return (
    <div className="preview-loading" role="status">
      <Loader size={16} className="spin" aria-hidden />
      Loading...
    </div>
  );
}

type Row = [label: string, value: string];

function Details({ rows }: { rows: Row[] }) {
  return (
    <dl className="preview-details">
      {rows.map(([label, value]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd className="preview-selectable selectable" title={value}>
            {value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** A size inside a sentence, which never breaks between the number and its unit. */
function sizeInText(bytes: number): string {
  return formatSize(bytes).replace(" ", "\u00a0");
}

function dimensionsText({ width, height }: { width: number; height: number }): string {
  return `${width} × ${height}`;
}

/** Why a picture is not shown: its file's size, or the pixels its header says it has. */
function tooLargeText(view: Extract<FileView, { view: "tooLarge" }>, caps: Caps): string {
  if (view.dimensions) {
    const pixels = `${dimensionsText(view.dimensions).replace(/ /g, "\u00a0")}\u00a0pixels`;
    return view.canLoad
      ? `This picture is ${pixels}. Pictures up to ${MAX_SHOWN_PIXELS / 1_000_000}\u00a0megapixels show by themselves.`
      : `This picture is ${pixels}, more than a preview can show.`;
  }
  return view.canLoad
    ? `This picture is ${sizeInText(view.size)}. Pictures up to ${sizeInText(caps.imageBytes)} load by themselves.`
    : `This picture is ${sizeInText(view.size)}, more than a preview can load.`;
}

function sizeText(bytes: number): string {
  const size = formatSize(bytes);
  return bytes < 1024 ? size : `${size} (${bytes.toLocaleString()} bytes)`;
}

/** The details the file list has for an entry. `type` stands in for the list's own. */
function entryRows(entry: FileEntry, dateFormat: DateFormat, type?: string): Row[] {
  const isFolder = entry.kind === "dir" || entry.linkTarget === "dir";
  const rows: Row[] = [["Type", type ?? typeLabel(entry)]];
  if (entry.kind === "symlink") rows.push(["Link", isFolder ? "To a folder" : "To a file"]);
  if (entry.kind === "file") rows.push(["Size", sizeText(entry.size)]);
  if (entry.modified !== null) rows.push(["Modified", formatDate(entry.modified, dateFormat)]);
  if (entry.permissions !== null) rows.push(["Permissions", formatPermissions(entry)]);
  if (entry.owner !== null || entry.group !== null) {
    rows.push(["Owner", [entry.owner, entry.group].filter(Boolean).join(":")]);
  }
  return rows;
}

function contentsText({ folders, files, bytes }: Summary): string {
  if (folders + files === 0) return "Nothing";
  const parts = [
    folders > 0 ? pluralize(folders, "folder") : "",
    files > 0 ? pluralize(files, "file") : "",
  ].filter(Boolean);
  return bytes > 0 ? `${parts.join(", ")} (${formatSize(bytes)})` : parts.join(", ");
}

/** What to say, and what to offer, when something could not be read. */
function Failure({
  problem,
  message,
  retrying,
  what,
  onRetry,
  onRefresh,
}: {
  problem: Problem;
  message: string;
  /** It is tried again by itself shortly. */
  retrying: boolean;
  what: "file" | "folder";
  onRetry: () => void;
  onRefresh?: () => void;
}) {
  const retry = { label: "Try again", onSelect: onRetry };
  switch (problem) {
    case "denied":
      return (
        <Notice
          icon={Ban}
          title={`No permission to read this ${what}`}
          detail="The account signed in here is not allowed to open it."
        />
      );
    case "gone":
      return (
        <Notice
          icon={FileX}
          title={`This ${what} is no longer there`}
          detail="It was deleted, moved or renamed after the folder was listed."
          action={onRefresh && { label: "Refresh", onSelect: onRefresh }}
        />
      );
    case "lost":
      return <Notice icon={Unplug} title="Connection lost" detail={message} action={retry} />;
    case "locked":
      return (
        <Notice
          icon={LockKeyhole}
          title="The vault is locked"
          detail="Unlock it again to preview its files."
        />
      );
    case "busy":
      return (
        <Notice
          icon={Hourglass}
          title={message}
          detail={
            retrying
              ? "The preview tries again as soon as the connection is free."
              : "It stayed busy for a while. Try again in a moment."
          }
          action={retrying ? undefined : retry}
        />
      );
    case "lent":
      return (
        <Notice
          icon={Hourglass}
          title={message}
          detail="The preview waits rather than slow the transfer down."
          action={retry}
        />
      );
    case "other":
      return (
        <Notice
          icon={CircleSlash}
          title="Could not load the preview"
          detail={message}
          action={retry}
        />
      );
  }
}

interface SharedProps {
  place: PreviewPlace;
  caps: Caps;
  mode: PreviewLoad;
  dateFormat: DateFormat;
}

/** What a file's type row says once its bytes have told what it is. */
function kindOf(view: FileView | null): string | undefined {
  if (view?.view === "image") return `${IMAGE_FORMAT_LABELS[view.format]} image`;
  if (view?.view === "text") return `Text, ${TEXT_ENCODING_LABELS[view.encoding]}`;
  if (view?.view === "binary" && view.description) return view.description;
  return undefined;
}

function FilePreview({
  entry,
  place,
  caps,
  mode,
  dateFormat,
  onOpenInEditor,
  onRefresh,
}: SharedProps & Handlers & { entry: FileEntry }) {
  /** The user asked for a picture over the cap. */
  const [anyway, setAnyway] = useState(false);
  const [pictureSize, setPictureSize] = useState<PictureSize | null>(null);
  /** The picture the web view could not decode. */
  const [broken, setBroken] = useState<Uint8Array | null>(null);
  const key = JSON.stringify([
    place.connection,
    entry.path,
    entry.size,
    entry.modified,
    caps.imageBytes,
    caps.textBytes,
    anyway,
  ]);
  /** Listed by a server as 0 bytes, which may be a size it does not know: read when asked. */
  const unsized = place.sizes === "listed" && listsOwnSize(entry) && entry.size === 0;
  const { state, show } = useRestedLoad<FileView>({
    key,
    cached: cachedFileView(place, entry, caps, anyway),
    mode: unsized ? "click" : mode,
    storage: place.storage,
    inVault: place.inVault,
    // What needs no read, as an empty file or a picture over the cap, shows without asking.
    eager: anyway || firstStep(entry, caps, anyway, place.sizes).step === "show",
    load: (signal) => readFileView(place, entry, caps, anyway, signal),
  });
  const view = state.status === "ready" ? state.result : null;
  const changed = view !== null && "changed" in view && view.changed;

  const rows = entryRows(entry, dateFormat, kindOf(view));
  if (view?.view === "image" && pictureSize && pictureSize.width > 0 && broken !== view.bytes) {
    rows.splice(1, 0, ["Dimensions", dimensionsText(pictureSize)]);
  } else if (view?.view === "tooLarge" && view.dimensions) {
    rows.splice(1, 0, ["Dimensions", dimensionsText(view.dimensions)]);
  }

  return (
    <>
      <Header icon={<FileIcon entry={entry} />} title={entry.name}>
        {view?.view === "text" && (
          <button
            type="button"
            className="icon-button"
            title="Open in editor (F4)"
            aria-label="Open in editor"
            onClick={() => onOpenInEditor(entry)}
          >
            <FilePen size={15} />
          </button>
        )}
      </Header>
      {changed && (
        <p className="preview-note">
          <span>This file changed after the folder was listed.</span>
          <button type="button" className="link-button" onClick={onRefresh}>
            Refresh
          </button>
        </p>
      )}
      <FileBody
        state={state}
        entry={entry}
        caps={caps}
        unsized={unsized}
        onServer={place.storage !== "local"}
        broken={broken}
        onShow={show}
        onLoadAnyway={() => setAnyway(true)}
        onRefresh={onRefresh}
        onPictureSize={setPictureSize}
        onBroken={setBroken}
      />
      {view?.view === "text" && view.partial && (
        <p className="preview-note">
          <span>
            Only the beginning is shown: {sizeInText(caps.textBytes)} of {sizeInText(view.size)}.
          </span>
          <button type="button" className="link-button" onClick={() => onOpenInEditor(entry)}>
            Open in editor
          </button>
        </p>
      )}
      <Details rows={rows} />
    </>
  );
}

interface FileBodyProps {
  state: Rested<FileView>;
  entry: FileEntry;
  caps: Caps;
  /** Listed as 0 bytes by a server, which may not know its size. */
  unsized: boolean;
  onServer: boolean;
  broken: Uint8Array | null;
  onShow: () => void;
  onLoadAnyway: () => void;
  onRefresh: () => void;
  onPictureSize: (size: PictureSize) => void;
  onBroken: (bytes: Uint8Array) => void;
}

function FileBody({
  state,
  entry,
  caps,
  unsized,
  onServer,
  broken,
  onShow,
  onLoadAnyway,
  onRefresh,
  onPictureSize,
  onBroken,
}: FileBodyProps) {
  switch (state.status) {
    case "resting":
      return <div className="preview-body" />;
    case "loading":
      return <Loading />;
    case "manual":
      if (unsized) {
        return (
          <Notice
            icon={FileQuestion}
            title="Listed as 0 bytes"
            detail="Some servers list no size for a file they make themselves, so it may not be empty. Reading it shows what is in it."
            action={{ label: "Show preview", onSelect: onShow }}
          />
        );
      }
      return (
        <Notice
          icon={Eye}
          title="Preview not loaded"
          detail="Reading from this storage may be charged for, so the file is read only when asked."
          action={{ label: "Show preview", onSelect: onShow }}
        />
      );
    case "failed":
      return <Failure {...state} what="file" onRetry={onShow} onRefresh={onRefresh} />;
  }
  const view = state.result;
  switch (view.view) {
    case "empty":
      return (
        <Notice
          icon={FileCheck}
          title="Empty file"
          detail={onServer ? "The server says there is nothing in it." : "There is nothing in it."}
        />
      );
    case "tooLarge":
      return (
        <Notice
          icon={Scaling}
          title="Too large to preview"
          detail={tooLargeText(view, caps)}
          action={view.canLoad ? { label: "Load anyway", onSelect: onLoadAnyway } : undefined}
        />
      );
    case "image":
      if (broken === view.bytes) {
        return (
          <Notice
            icon={FileX}
            title="This picture cannot be shown"
            detail="It may be damaged, or of a kind this system does not decode."
          />
        );
      }
      return (
        <PreviewImage
          bytes={view.bytes}
          format={view.format}
          onSize={onPictureSize}
          onBroken={() => onBroken(view.bytes)}
        />
      );
    case "text":
      return (
        <div className="preview-body">
          <Suspense fallback={null}>
            <PreviewText text={view.text} name={entry.name} />
          </Suspense>
        </div>
      );
    case "binary":
      return (
        <div className="preview-body preview-scroll">
          <div className="preview-binary">
            <FileIcon entry={entry} size={NOTICE_ICON_SIZE} />
            <p className="preview-notice-title">{view.description ?? "Not text or a picture"}</p>
            <p className="preview-notice-detail">
              The first {pluralize(Math.min(HEX_BYTES, view.head.length), "byte")}:
            </p>
          </div>
          <HexView bytes={view.head} />
        </div>
      );
  }
}

function HexView({ bytes }: { bytes: Uint8Array }) {
  const rows = useMemo(() => hexRows(bytes, HEX_BYTES), [bytes]);
  return (
    <div className="preview-hex preview-selectable selectable">
      {rows.map((row) => (
        <div key={row.offset} className="preview-hex-row">
          <span className="preview-hex-offset">{row.offset}</span>
          <span>{row.bytes.join(" ")}</span>
          <span className="preview-hex-characters">{row.characters}</span>
        </div>
      ))}
    </div>
  );
}

function FolderPreview({
  entry,
  place,
  caps,
  mode,
  dateFormat,
  box,
}: SharedProps & { entry: FileEntry; box: number }) {
  const { state, show } = useRestedLoad<DirListing>({
    key: JSON.stringify([place.connection, entry.path, entry.modified]),
    cached: cachedFolder(place, entry),
    mode,
    storage: place.storage,
    inVault: place.inVault,
    load: () => listFolder(place, entry),
  });
  const listing = state.status === "ready" ? state.result : null;
  const summary = useMemo(
    () => (listing ? summarize(listing.entries, caps) : null),
    [listing, caps],
  );
  // A server shows a vault's folder as the encrypted files it is kept in.
  const isVault = listing !== null && !place.inVault && vaultSign(listing.entries) !== "none";
  const rows = entryRows(entry, dateFormat);
  if (summary && !isVault) rows.splice(1, 0, ["Contains", contentsText(summary)]);

  let body: ReactNode;
  if (state.status === "resting") {
    body = <div className="preview-body" />;
  } else if (state.status === "loading") {
    body = <Loading />;
  } else if (state.status === "manual") {
    body = (
      <Notice
        icon={FolderOpen}
        title="Folder not listed"
        detail="Listing on this storage may be charged for, so the folder is listed only when asked."
        action={{ label: "Show preview", onSelect: show }}
      />
    );
  } else if (state.status === "failed") {
    body = <Failure {...state} what="folder" onRetry={show} />;
  } else if (isVault) {
    body = (
      <Notice
        icon={LockKeyhole}
        title="Cryptomator vault"
        detail="Open the folder to unlock it and see its files."
      />
    );
  } else if (summary && summary.pictures.length > 0) {
    body = <Pictures place={place} pictures={summary.pictures} box={box} caps={caps} />;
  } else {
    body = (
      <Notice
        icon={FolderOpen}
        title={summary && summary.folders + summary.files > 0 ? "Folder" : "Empty folder"}
        detail={summary ? contentsText(summary) : undefined}
      />
    );
  }

  return (
    <>
      <Header icon={<FileIcon entry={entry} />} title={entry.name} />
      {body}
      <Details rows={rows} />
    </>
  );
}

function Pictures({
  place,
  pictures,
  box,
  caps,
}: {
  place: PreviewPlace;
  pictures: FileEntry[];
  box: number;
  caps: Caps;
}) {
  const shown = useMemo(
    () =>
      [...pictures]
        .sort((left, right) => byName.compare(left.name, right.name))
        .slice(0, MAX_TILES),
    [pictures],
  );
  return (
    <div className="preview-body preview-scroll">
      <PreviewThumbnails
        place={place}
        entries={shown}
        total={pictures.length}
        box={box}
        caps={caps}
      />
    </div>
  );
}

function SelectionPreview({
  entries,
  place,
  caps,
  mode,
  box,
}: SharedProps & { entries: FileEntry[]; box: number }) {
  const summary = useMemo(() => summarize(entries, caps), [entries, caps]);
  const selection = useMemo(() => selectionKey(entries), [entries]);
  /** The selection whose thumbnails the user asked for, where they do not load by themselves. */
  const [asked, setAsked] = useState("");
  const waitsForClick = mode === "click" && asked !== selection;
  const rows: Row[] = [["Selected", contentsText({ ...summary, bytes: 0 })]];
  if (summary.files > 0) rows.push(["Total size", sizeText(summary.bytes)]);
  if (summary.folders > 0) rows.push(["Folders", "Not counted in the size"]);

  let body: ReactNode;
  if (summary.pictures.length === 0) {
    body = (
      <Notice
        icon={Files}
        title={pluralize(entries.length, "item")}
        detail={summary.files > 0 ? formatSize(summary.bytes) : undefined}
      />
    );
  } else if (waitsForClick) {
    body = (
      <Notice
        icon={Eye}
        title="Thumbnails not loaded"
        detail="Reading from this storage may be charged for, so pictures are read only when asked."
        action={{ label: "Show preview", onSelect: () => setAsked(selection) }}
      />
    );
  } else {
    body = <Pictures place={place} pictures={summary.pictures} box={box} caps={caps} />;
  }

  return (
    <>
      <Header icon={<Files size={16} aria-hidden />} title={`${entries.length} items selected`} />
      {body}
      <Details rows={rows} />
    </>
  );
}
