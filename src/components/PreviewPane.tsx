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
  ArrowLeft,
  Ban,
  ChevronLeft,
  ChevronRight,
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
import { PreviewKeys, usePreviewKeys, type PreviewKeyHandler } from "../hooks/usePreviewKeys";
import { useRestedLoad, type Rested } from "../hooks/useRestedLoad";
import { formatDate, formatPermissions, formatSize } from "../lib/format";
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
import { formatNumber, t, useT } from "../i18n";

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
  /**
   * The picture asked for with Preview in the list's menu, while the cursor and selection stay
   * as they were then. Among several selected pictures, it opens on its own.
   */
  request?: string | null;
  /** The user went on from the picture asked for, to another or back to them all. */
  onRequestDone?: () => void;
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
  request = null,
  onRequestDone = () => undefined,
}: PreviewPaneProps) {
  const t = useT();
  const settings = useSettingsStore((state) => state.settings.interface.preview);
  const [keyHandlers] = useState(() => new Set<PreviewKeyHandler>());
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
          requested={request === subject.entry.path}
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
          {...handlers}
        />
      );
      break;
    case "selection":
      content = (
        <SelectionPreview
          entries={subject.entries}
          box={settings.thumbnailSize}
          request={request}
          onRequestDone={onRequestDone}
          {...shared}
          {...handlers}
        />
      );
      break;
    case "special":
      content = (
        <>
          <Header icon={<FileIcon entry={subject.entry} />} title={subject.entry.name} />
          <Notice
            icon={FileQuestion}
            title={
              subject.entry.linkTarget === "broken"
                ? t("preview.brokenLink")
                : t("preview.notARegularFile")
            }
            detail={
              subject.entry.linkTarget === "broken"
                ? t("preview.theFileThisLinkLeads")
                : t("preview.thereIsNothingInIt")
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
          title={t("preview.nothingToPreview")}
          detail={t("preview.nothingToPreviewDetail")}
        />
      );
  }

  // Arrow and page keys scroll what the preview shows, wherever in it that scrolls.
  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    for (const handler of keyHandlers) {
      if (handler(event)) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }
    }
    const pane = event.currentTarget;
    const scroller =
      pane.querySelector<HTMLElement>(
        ".cm-scroller, .preview-image-frame, .preview-scroll:not([hidden])",
      ) ?? pane;
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
      aria-label={t("preview.preview")}
      onKeyDown={handleKeyDown}
      onMouseDown={keepFocus}
    >
      <PreviewKeys.Provider value={keyHandlers}>{content}</PreviewKeys.Provider>
    </aside>
  );
}

function Header({
  icon,
  title,
  before,
  children,
}: {
  icon: ReactNode;
  title: string;
  /** Ahead of the icon, as the way back to the pictures a picture was opened from. */
  before?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <header className={before ? "preview-header has-back" : "preview-header"}>
      {before}
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
  const t = useT();
  return (
    <div className="preview-loading" role="status">
      <Loader size={16} className="spin" aria-hidden />
      {t("common.loading")}
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
  return `${formatNumber(width)} × ${formatNumber(height)}`;
}

/** Why a picture is not shown: its file's size, or the pixels its header says it has. */
function tooLargeText(view: Extract<FileView, { view: "tooLarge" }>, caps: Caps): string {
  if (view.dimensions) {
    const dimensions = dimensionsText(view.dimensions).replace(/ /g, "\u00a0");
    return view.canLoad
      ? t("preview.pictureHasPixelsShowsUpTo", {
          dimensions,
          megapixels: MAX_SHOWN_PIXELS / 1_000_000,
        })
      : t("preview.pictureHasPixelsMoreThanShown", { dimensions });
  }
  return view.canLoad
    ? t("preview.pictureIsSizeLoadsUpTo", {
        size: sizeInText(view.size),
        limit: sizeInText(caps.imageBytes),
      })
    : t("preview.pictureIsSizeMoreThanLoaded", { size: sizeInText(view.size) });
}

function sizeText(bytes: number): string {
  const size = formatSize(bytes);
  return bytes < 1024 ? size : t("preview.sizeWithBytes", { size, count: bytes });
}

/** The details the file list has for an entry. `type` stands in for the list's own. */
function entryRows(entry: FileEntry, dateFormat: DateFormat, type?: string): Row[] {
  const isFolder = entry.kind === "dir" || entry.linkTarget === "dir";
  const rows: Row[] = [[t("preview.type"), type ?? typeLabel(entry)]];
  if (entry.kind === "symlink") {
    rows.push([t("preview.link"), isFolder ? t("preview.toAFolder") : t("preview.toAFile")]);
  }
  if (entry.kind === "file") rows.push([t("common.size"), sizeText(entry.size)]);
  if (entry.modified !== null) {
    rows.push([t("preview.modified"), formatDate(entry.modified, dateFormat)]);
  }
  if (entry.permissions !== null) rows.push([t("preview.permissions"), formatPermissions(entry)]);
  if (entry.owner !== null || entry.group !== null) {
    rows.push([t("preview.owner"), [entry.owner, entry.group].filter(Boolean).join(":")]);
  }
  return rows;
}

function contentsText({ folders, files, bytes }: Summary): string {
  if (folders + files === 0) return t("common.nothing");
  const parts = [
    folders > 0 ? t("preview.folders", { count: folders }) : "",
    files > 0 ? t("preview.files", { count: files }) : "",
  ].filter(Boolean);
  const counted = parts.join(t("common.listSeparator"));
  return bytes > 0
    ? t("preview.contentsWithSize", { contents: counted, size: formatSize(bytes) })
    : counted;
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
  const t = useT();
  const retry = { label: t("common.tryAgain"), onSelect: onRetry };
  switch (problem) {
    case "denied":
      return (
        <Notice
          icon={Ban}
          title={
            what === "file"
              ? t("preview.noPermissionToReadFile")
              : t("preview.noPermissionToReadFolder")
          }
          detail={t("preview.theAccountSignedInHere")}
        />
      );
    case "gone":
      return (
        <Notice
          icon={FileX}
          title={
            what === "file" ? t("preview.fileNoLongerThere") : t("preview.folderNoLongerThere")
          }
          detail={t("preview.itWasDeletedMovedOr")}
          action={onRefresh && { label: t("common.refresh"), onSelect: onRefresh }}
        />
      );
    case "lost":
      return (
        <Notice icon={Unplug} title={t("preview.connectionLost")} detail={message} action={retry} />
      );
    case "locked":
      return (
        <Notice
          icon={LockKeyhole}
          title={t("preview.theVaultIsLocked")}
          detail={t("preview.theVaultIsLockedDetail")}
        />
      );
    case "busy":
      return (
        <Notice
          icon={Hourglass}
          title={message}
          detail={retrying ? t("preview.thePreviewTriesAgainAs") : t("preview.itStayedBusyForA")}
          action={retrying ? undefined : retry}
        />
      );
    case "lent":
      return (
        <Notice
          icon={Hourglass}
          title={message}
          detail={t("preview.thePreviewWaitsRatherThan")}
          action={retry}
        />
      );
    case "other":
      return (
        <Notice
          icon={CircleSlash}
          title={t("preview.couldNotLoadThePreview")}
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
  if (view?.view === "image") {
    return t("preview.imageOfFormat", { format: IMAGE_FORMAT_LABELS[view.format] });
  }
  if (view?.view === "text") {
    return t("preview.textInEncoding", { encoding: TEXT_ENCODING_LABELS[view.encoding] });
  }
  if (view?.view === "binary" && view.description) return view.description;
  return undefined;
}

interface FilePreviewProps extends SharedProps, Handlers {
  entry: FileEntry;
  /** The user asked for this file, so it loads without waiting for a rest or a click. */
  requested?: boolean;
  /** Goes back to the pictures this one was opened from. */
  onBack?: () => void;
  /** In the header after the name, as the way to the pictures beside this one. */
  navigation?: ReactNode;
}

function FilePreview({
  entry,
  place,
  caps,
  mode,
  dateFormat,
  onOpenInEditor,
  onRefresh,
  requested = false,
  onBack,
  navigation,
}: FilePreviewProps) {
  const t = useT();
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
    eager: anyway || requested || firstStep(entry, caps, anyway, place.sizes).step === "show",
    load: (signal) => readFileView(place, entry, caps, anyway, signal),
  });
  const view = state.status === "ready" ? state.result : null;
  const changed = view !== null && "changed" in view && view.changed;

  const rows = entryRows(entry, dateFormat, kindOf(view));
  if (view?.view === "image" && pictureSize && pictureSize.width > 0 && broken !== view.bytes) {
    rows.splice(1, 0, [t("preview.dimensions"), dimensionsText(pictureSize)]);
  } else if (view?.view === "tooLarge" && view.dimensions) {
    rows.splice(1, 0, [t("preview.dimensions"), dimensionsText(view.dimensions)]);
  }

  return (
    <>
      <Header
        icon={<FileIcon entry={entry} />}
        title={entry.name}
        before={
          onBack && (
            <button
              type="button"
              className="icon-button"
              title={t("preview.backToAllPictures")}
              aria-label={t("preview.backToAllPictures")}
              onClick={onBack}
            >
              <ArrowLeft size={15} />
            </button>
          )
        }
      >
        {navigation}
        {view?.view === "text" && (
          <button
            type="button"
            className="icon-button"
            title={t("preview.openInEditorF4")}
            aria-label={t("preview.openInEditor")}
            onClick={() => onOpenInEditor(entry)}
          >
            <FilePen size={15} />
          </button>
        )}
      </Header>
      {changed && (
        <p className="preview-note">
          <span>{t("preview.thisFileChangedAfterIt")}</span>
          <button type="button" className="link-button" onClick={onRefresh}>
            {t("common.refresh")}
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
            {t("preview.onlyTheBeginningIsShown", {
              shown: sizeInText(caps.textBytes),
              size: sizeInText(view.size),
            })}
          </span>
          <button type="button" className="link-button" onClick={() => onOpenInEditor(entry)}>
            {t("preview.openInEditor")}
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
  const t = useT();
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
            title={t("preview.listedAs0Bytes")}
            detail={t("preview.listedAs0BytesDetail")}
            action={{ label: t("preview.showPreview"), onSelect: onShow }}
          />
        );
      }
      return (
        <Notice
          icon={Eye}
          title={t("preview.previewNotLoaded")}
          detail={t("preview.previewNotLoadedDetail")}
          action={{ label: t("preview.showPreview"), onSelect: onShow }}
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
          title={t("preview.emptyFile")}
          detail={onServer ? t("preview.theServerSaysThereIs") : t("preview.thereIsNothingInIt2")}
        />
      );
    case "tooLarge":
      return (
        <Notice
          icon={Scaling}
          title={t("preview.tooLargeToPreview")}
          detail={tooLargeText(view, caps)}
          action={
            view.canLoad ? { label: t("preview.loadAnyway"), onSelect: onLoadAnyway } : undefined
          }
        />
      );
    case "image":
      if (broken === view.bytes) {
        return (
          <Notice
            icon={FileX}
            title={t("preview.thisPictureCannotBeShown")}
            detail={t("preview.thisPictureCannotBeShownDetail")}
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
            <p className="preview-notice-title">
              {view.description ?? t("preview.notTextOrAPicture")}
            </p>
            <p className="preview-notice-detail">
              {t("preview.theFirstBytes", { count: Math.min(HEX_BYTES, view.head.length) })}
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
  box,
  ...shared
}: SharedProps & Handlers & { entry: FileEntry; box: number }) {
  const { place, caps, mode, dateFormat } = shared;
  const t = useT();
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
  if (summary && !isVault) rows.splice(1, 0, [t("preview.contains"), contentsText(summary)]);

  let body: ReactNode;
  if (state.status === "resting") {
    body = <div className="preview-body" />;
  } else if (state.status === "loading") {
    body = <Loading />;
  } else if (state.status === "manual") {
    body = (
      <Notice
        icon={FolderOpen}
        title={t("preview.folderNotListed")}
        detail={t("preview.folderNotListedDetail")}
        action={{ label: t("preview.showPreview"), onSelect: show }}
      />
    );
  } else if (state.status === "failed") {
    body = <Failure {...state} what="folder" onRetry={show} />;
  } else if (isVault) {
    body = (
      <Notice
        icon={LockKeyhole}
        title={t("preview.cryptomatorVault")}
        detail={t("preview.cryptomatorVaultDetail")}
      />
    );
  } else if (summary && summary.pictures.length > 0) {
    return (
      <Gallery
        pictures={summary.pictures}
        box={box}
        header={<Header icon={<FileIcon entry={entry} />} title={entry.name} />}
        details={<Details rows={rows} />}
        {...shared}
      />
    );
  } else {
    body = (
      <Notice
        icon={FolderOpen}
        title={
          summary && summary.folders + summary.files > 0
            ? t("common.folder")
            : t("preview.emptyFolder")
        }
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

interface GalleryProps extends SharedProps, Handlers {
  pictures: FileEntry[];
  box: number;
  /** What the preview shows above and below the thumbnails, for the folder or selection. */
  header: ReactNode;
  details: ReactNode;
  /** A picture among them to open, as asked for with Preview in the list's menu. */
  request?: string | null;
  onRequestDone?: () => void;
}

/**
 * Thumbnails of pictures, each of which opens on its own in the preview, to zoom in on and to
 * step through the others from. The thumbnails stay where they were scrolled to meanwhile.
 */
function Gallery({
  pictures,
  box,
  header,
  details,
  request = null,
  onRequestDone,
  ...shared
}: GalleryProps) {
  const t = useT();
  const shown = useMemo(
    () =>
      [...pictures]
        .sort((left, right) => byName.compare(left.name, right.name))
        .slice(0, MAX_TILES),
    [pictures],
  );
  /** The picture opened from the thumbnails, or from one to the next. */
  const [chosen, setChosen] = useState<string | null>(null);
  const asked = request !== null && shown.some((picture) => picture.path === request);
  const index = shown.findIndex((picture) => picture.path === (asked ? request : chosen));
  const viewing = index >= 0 ? shown[index] : null;

  const open = (path: string | null) => {
    setChosen(path);
    if (asked) onRequestDone?.();
  };
  const step = (by: 1 | -1) => {
    const next = shown[index + by];
    if (viewing && next) open(next.path);
  };

  usePreviewKeys((event) => {
    if (!viewing) return false;
    if (event.key === "Escape" || event.key === "Backspace") {
      open(null);
      return true;
    }
    if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && !event.shiftKey) {
      // A picture zoomed wider than the pane scrolls with them instead.
      const frame = event.currentTarget.querySelector(".preview-image-frame");
      if (frame && frame.scrollWidth > frame.clientWidth) return false;
      step(event.key === "ArrowRight" ? 1 : -1);
      return true;
    }
    return false;
  });

  const navigation = viewing && shown.length > 1 && (
    <div className="preview-steps">
      <button
        type="button"
        className="icon-button"
        title={t("preview.previousPicture")}
        aria-label={t("preview.previousPicture")}
        disabled={index === 0}
        onClick={() => step(-1)}
      >
        <ChevronLeft size={15} />
      </button>
      <span className="preview-steps-count">
        {t("preview.pictureOfCount", { index: index + 1, count: shown.length })}
      </span>
      <button
        type="button"
        className="icon-button"
        title={t("preview.nextPicture")}
        aria-label={t("preview.nextPicture")}
        disabled={index === shown.length - 1}
        onClick={() => step(1)}
      >
        <ChevronRight size={15} />
      </button>
    </div>
  );

  return (
    <>
      {viewing ? (
        <FilePreview
          key={JSON.stringify([viewing.path, viewing.size, viewing.modified])}
          entry={viewing}
          requested
          onBack={() => open(null)}
          navigation={navigation}
          {...shared}
        />
      ) : (
        header
      )}
      <div className="preview-body preview-scroll" hidden={viewing !== null}>
        <PreviewThumbnails
          place={shared.place}
          entries={shown}
          total={pictures.length}
          box={box}
          caps={shared.caps}
          onOpen={(picture) => open(picture.path)}
        />
      </div>
      {!viewing && details}
    </>
  );
}

function SelectionPreview({
  entries,
  box,
  request = null,
  onRequestDone,
  ...shared
}: SharedProps &
  Handlers & {
    entries: FileEntry[];
    box: number;
    request?: string | null;
    onRequestDone?: () => void;
  }) {
  const t = useT();
  const { caps, mode } = shared;
  const summary = useMemo(() => summarize(entries, caps), [entries, caps]);
  const selection = useMemo(() => selectionKey(entries), [entries]);
  /** The selection whose thumbnails the user asked for, where they do not load by themselves. */
  const [asked, setAsked] = useState("");
  // A picture asked for with Preview in the list's menu is as good as a click.
  const requested = request !== null && summary.pictures.some((entry) => entry.path === request);
  const waitsForClick = mode === "click" && asked !== selection && !requested;
  const rows: Row[] = [[t("preview.selected"), contentsText({ ...summary, bytes: 0 })]];
  if (summary.files > 0) rows.push([t("preview.totalSize"), sizeText(summary.bytes)]);
  if (summary.folders > 0) rows.push([t("preview.foldersRow"), t("preview.notCountedInTheSize")]);

  const header = (
    <Header
      icon={<Files size={16} aria-hidden />}
      title={t("preview.itemsSelected", { count: entries.length })}
    />
  );
  if (summary.pictures.length > 0 && !waitsForClick) {
    return (
      <Gallery
        pictures={summary.pictures}
        box={box}
        header={header}
        details={<Details rows={rows} />}
        request={request}
        onRequestDone={onRequestDone}
        {...shared}
      />
    );
  }

  return (
    <>
      {header}
      {summary.pictures.length === 0 ? (
        <Notice
          icon={Files}
          title={t("preview.items", { count: entries.length })}
          detail={summary.files > 0 ? formatSize(summary.bytes) : undefined}
        />
      ) : (
        <Notice
          icon={Eye}
          title={t("preview.thumbnailsNotLoaded")}
          detail={t("preview.thumbnailsNotLoadedDetail")}
          action={{ label: t("preview.showPreview"), onSelect: () => setAsked(selection) }}
        />
      )}
      <Details rows={rows} />
    </>
  );
}
