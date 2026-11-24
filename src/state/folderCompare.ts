// Compare folders: which panes are compared with which, what each of them shows, and what the
// comparison found. Two panes of one window share this state. A pane compared with one in
// another window gets that pane's listing in messages between the two windows, and each window
// compares for itself, so nothing is asked of a server on the way.

import { createStore } from "zustand/vanilla";
import type { PathStyle } from "../lib/fileSource";
import {
  compareEntries,
  compareListings,
  compareRules,
  deepPrefix,
  ignoresCase,
  rowMarks,
  sameEntries,
  type CompareEntry,
  type CompareRules,
  type DeepComparison,
  type ListingComparison,
  type RowCompare,
} from "../lib/folderCompare";
import { baseName, samePath } from "../lib/path";
import type { FolderCompareSettings } from "../lib/settings";
import { liveEntries } from "../lib/versions";
import type {
  CompareMode,
  DirListing,
  FileLocation,
  FolderCompareRequest,
  FolderComparison,
  FolderSummary,
  Protocol,
  SessionInfo,
  SyncProgress,
} from "../lib/types";
import { t } from "../i18n";

/** A pane by the window it is in and its tab there, with the name its header shows. */
export interface PaneRef {
  window: string;
  tabId: string;
  label: string;
}

/** What a compared pane shows, as far as comparing needs it. */
export interface PaneSnapshot {
  location: FileLocation;
  pathStyle: PathStyle;
  /** The session's protocol, for a server's pane. */
  protocol?: Protocol;
  ignoresCase: boolean;
  coarseTimes: boolean;
  /**
   * The listing is a remembered one that could not be refreshed, so it may be out of date. It
   * is compared all the same, as the pane shows it, and the bar says so.
   */
  outdated: boolean;
  /**
   * The pane shows search results in place of the folder, so comparing waits until they are
   * closed: the rows on show are not the folder's, and the other pane's marks would be about a
   * folder out of sight. `entries` stay the folder's, so nothing found below it is lost.
   */
  showingResults: boolean;
  path: string;
  parent: string | null;
  entries: readonly CompareEntry[];
}

/** What a file pane shows, as `snapshotOf` reads it. */
export interface ShownFolder {
  /** The folder's listing, as `comparedListing` gives it. */
  listing: Pick<DirListing, "path" | "parent" | "entries"> | null;
  location: FileLocation;
  pathStyle: PathStyle;
  /** The session of a server's pane. */
  info?: SessionInfo;
  /** The connection to the server, a vault's server for a vault, is lost. */
  lost: boolean;
  /** Something stands in for the rows, as a locked vault's card does. */
  covered: boolean;
  /** The listing on show is a remembered one the server could not be asked for again. */
  outdated: boolean;
  /** Search results are on show in place of the folder, whose listing stays behind them. */
  showingResults: boolean;
}

/**
 * The listing a compared pane is compared by: its folder's, which stays behind search results
 * while they are on show. The results come from many folders and are not compared.
 */
export function comparedListing(pane: {
  listing: DirListing | null;
  folder: DirListing | null;
  showingResults: boolean;
}): DirListing | null {
  return pane.showingResults ? pane.folder : pane.listing;
}

/**
 * What a pane shows, as far as comparing needs it. Null while there is nothing to compare: no
 * listing, a card over it, or the last listing of a server that cannot be reached now, which
 * says nothing about what the server holds.
 */
export function snapshotOf(shown: ShownFolder): PaneSnapshot | null {
  const { listing, info, pathStyle } = shown;
  if (!listing || shown.covered || shown.lost) return null;
  return {
    location: shown.location,
    pathStyle,
    protocol: info?.protocol,
    ignoresCase: ignoresCase(pathStyle, info?.protocol),
    coarseTimes: info?.coarseTimes === true,
    outdated: shown.outdated,
    showingResults: shown.showingResults,
    path: listing.path,
    parent: listing.parent,
    // Deleted files shown among the others are not in the folder to compare.
    entries: compareEntries(liveEntries(listing)),
  };
}

/** Two compared panes. The left one is where the comparison was turned on. */
export interface ComparePair {
  id: string;
  left: PaneRef;
  right: PaneRef;
}

/** The comparison of what is below the two folders, asked for from the compare bar. */
export type DeepState =
  | { status: "running"; requestId: string; leftEntries: number; rightEntries: number }
  /** `options` are the bar's options it was made with; it is dropped when they change. */
  | { status: "done"; comparison: DeepComparison; options: string }
  | { status: "failed"; message: string };

export interface PairComparison {
  /** The mark of each compared row, by name; empty for a pane of another window. */
  left: ReadonlyMap<string, RowCompare>;
  right: ReadonlyMap<string, RowCompare>;
}

export interface FolderCompareState {
  pairs: Record<string, ComparePair>;
  /** The pair each compared pane of this window is in, by tab. */
  pairOf: Record<string, string>;
  /** By pane key; null while a pane has no folder on show. */
  snapshots: Record<string, PaneSnapshot | null>;
  /** By pair; null until both panes show a folder. */
  comparisons: Record<string, PairComparison | null>;
  deep: Record<string, DeepState>;
  /** By pair: why the panes stopped browsing together. */
  notes: Record<string, string>;
}

/** What browsing together passes on: into the folder of a name, or up. */
export type BrowseMove = { kind: "enter"; name: string } | { kind: "up" };

/** A pane as its partner moves it. */
export interface PaneFollower {
  /**
   * Starts the same move in this pane. Gives the folder it is going to and whether it got there,
   * or null when the pane has no such folder.
   */
  follow: (
    move: BrowseMove,
    ignoreCase: boolean,
  ) => { path: string; done: Promise<boolean> } | null;
}

/** A pane another could be compared with. */
export interface ComparablePane {
  tabId: string;
  label: string;
  visible: boolean;
  activatedAt: number;
  /** It shows a folder's files. */
  ready: boolean;
}

interface Candidate extends PaneRef {
  visible: boolean;
  activatedAt: number;
}

type DeepWire =
  | { status: "running"; requestId: string }
  | {
      status: "done";
      leftRoot: string;
      rightRoot: string;
      ignoreCase: boolean;
      folders: FolderSummary[];
      truncated: boolean;
      options: string;
    }
  | { status: "failed"; message: string };

/** What the windows of two compared panes tell each other. */
export type CompareMessage =
  | { type: "seek"; from: string; seekId: string }
  | { type: "candidate"; from: string; seekId: string; candidate: Candidate | null }
  | { type: "link"; from: string; pair: ComparePair }
  | { type: "unlink"; from: string; pairId: string }
  | {
      type: "snapshot";
      from: string;
      pairId: string;
      tabId: string;
      snapshot: PaneSnapshot | null;
    }
  | { type: "follow"; from: string; pairId: string; move: BrowseMove }
  /** The partner could not make a move: how many folders the pane that moved is now ahead. */
  | { type: "followed"; from: string; pairId: string; depth: number }
  | { type: "note"; from: string; pairId: string; note: string | null }
  | { type: "deep"; from: string; pairId: string; deep: DeepWire | null };

const MESSAGE_TYPES = [
  "seek",
  "candidate",
  "link",
  "unlink",
  "snapshot",
  "follow",
  "followed",
  "note",
  "deep",
];

/** Whether what arrived from another window has the shape of a message, before it is read. */
export function isCompareMessage(value: unknown): value is CompareMessage {
  if (!value || typeof value !== "object") return false;
  const message = value as Record<string, unknown>;
  if (typeof message.type !== "string" || !MESSAGE_TYPES.includes(message.type)) return false;
  if (typeof message.from !== "string") return false;
  if (message.type === "seek" || message.type === "candidate") {
    return typeof message.seekId === "string";
  }
  if (message.type === "link") {
    const pair = message.pair as Partial<ComparePair> | null | undefined;
    return !!pair && typeof pair.id === "string" && isPaneRef(pair.left) && isPaneRef(pair.right);
  }
  return typeof message.pairId === "string";
}

function isPaneRef(value: unknown): value is PaneRef {
  if (!value || typeof value !== "object") return false;
  const ref = value as Record<string, unknown>;
  return (
    typeof ref.window === "string" && typeof ref.tabId === "string" && typeof ref.label === "string"
  );
}

/** What the comparison reaches the rest of the app through; tests stand in for it. */
export interface CompareEnvironment {
  /** This window's label. */
  window: string;
  /** Sends to one window, or to every other window when `to` is null. */
  send: (to: string | null, message: CompareMessage) => void;
  otherWindows: () => Promise<string[]>;
  /** The file panes of this window. */
  panes: () => ComparablePane[];
  options: () => FolderCompareSettings;
  /** The comparison defaults in Settings > Sync. */
  syncDefaults: () => { compare: CompareMode; timeToleranceSecs: number };
  compareFolders: (request: FolderCompareRequest) => Promise<FolderComparison>;
  cancel: (requestId: string) => Promise<void>;
  newId: () => string;
  /** How long other windows get to name their panes. */
  seekMillis: number;
}

export const paneKey = (pane: { window: string; tabId: string }) => `${pane.window}/${pane.tabId}`;

function partnerOf(pair: ComparePair, pane: PaneRef): PaneRef {
  return paneKey(pane) === paneKey(pair.left) ? pair.right : pair.left;
}

function asMove(value: unknown): BrowseMove | null {
  const move = (value && typeof value === "object" ? value : {}) as Record<string, unknown>;
  if (move.kind === "up") return { kind: "up" };
  return move.kind === "enter" && typeof move.name === "string"
    ? { kind: "enter", name: move.name }
    : null;
}

const asRecord = (value: unknown) =>
  value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;

function asLocation(value: unknown): FileLocation | null {
  const location = asRecord(value);
  if (location?.kind === "local") return { kind: "local" };
  return location?.kind === "remote" && typeof location.sessionId === "string"
    ? { kind: "remote", sessionId: location.sessionId }
    : null;
}

const SHAPES: readonly string[] = ["folder", "file", "other"] satisfies CompareEntry["shape"][];

function asEntry(value: unknown): CompareEntry | null {
  const entry = asRecord(value);
  if (!entry || typeof entry.name !== "string" || !SHAPES.includes(entry.shape as string)) {
    return null;
  }
  const size = entry.size;
  const modified = entry.modified;
  if (typeof size !== "number" || !Number.isFinite(size)) return null;
  if (modified !== null && (typeof modified !== "number" || !Number.isFinite(modified))) {
    return null;
  }
  return {
    name: entry.name,
    shape: entry.shape as CompareEntry["shape"],
    size,
    modified,
    hidden: entry.hidden === true,
  };
}

/**
 * A listing as another window sent it, with what is misshapen in it left out, or null when it
 * is not one at all.
 */
function asSnapshot(value: unknown): PaneSnapshot | null {
  const snapshot = asRecord(value);
  if (!snapshot) return null;
  const location = asLocation(snapshot.location);
  const { path, pathStyle, entries } = snapshot;
  if (!location || typeof path !== "string" || !Array.isArray(entries)) return null;
  if (pathStyle !== "posix" && pathStyle !== "windows") return null;
  return {
    location,
    pathStyle,
    protocol: typeof snapshot.protocol === "string" ? (snapshot.protocol as Protocol) : undefined,
    ignoresCase: snapshot.ignoresCase === true,
    coarseTimes: snapshot.coarseTimes === true,
    outdated: snapshot.outdated === true,
    showingResults: snapshot.showingResults === true,
    path,
    parent: typeof snapshot.parent === "string" ? snapshot.parent : null,
    entries: entries.map(asEntry).filter((entry) => entry !== null),
  };
}

const COUNTS = [
  "identical",
  "newerLeft",
  "newerRight",
  "onlyLeft",
  "onlyRight",
  "different",
  "notCompared",
] as const satisfies readonly (keyof FolderSummary)[];

function asSummary(value: unknown): FolderSummary | null {
  const summary = asRecord(value);
  if (!summary || typeof summary.path !== "string") return null;
  const counted = (key: (typeof COUNTS)[number]) => {
    const count = summary[key];
    return typeof count === "number" && Number.isFinite(count) ? count : 0;
  };
  return {
    path: summary.path,
    ...(Object.fromEntries(COUNTS.map((key) => [key, counted(key)])) as Omit<
      FolderSummary,
      "path"
    >),
  };
}

/** The pane the user looked at last: one on screen before one in a hidden tab. */
function better(first: Candidate | null, second: Candidate): Candidate {
  if (!first) return second;
  if (first.visible !== second.visible) return first.visible ? first : second;
  return second.activatedAt > first.activatedAt ? second : first;
}

/** The move that takes a pane from one folder to the next, when it is into a folder or up. */
export function moveBetween(previous: PaneSnapshot, next: PaneSnapshot): BrowseMove | null {
  const style = next.pathStyle;
  if (next.parent !== null && samePath(next.parent, previous.path, style)) {
    return { kind: "enter", name: baseName(next.path) };
  }
  if (previous.parent !== null && samePath(previous.parent, next.path, style)) {
    return { kind: "up" };
  }
  return null;
}

const NO_MARKS: ReadonlyMap<string, RowCompare> = new Map();

function failure(caught: unknown): { cancelled: boolean; message: string } {
  const error = (caught && typeof caught === "object" ? caught : {}) as Record<string, unknown>;
  const message = typeof error.message === "string" ? error.message : String(caught);
  return { cancelled: error.kind === "cancelled", message };
}

function without<T>(record: Record<string, T>, keys: readonly string[]): Record<string, T> {
  const copy = { ...record };
  for (const key of keys) delete copy[key];
  return copy;
}

export function createFolderCompare(env: CompareEnvironment) {
  const store = createStore<FolderCompareState>(() => ({
    pairs: {},
    pairOf: {},
    snapshots: {},
    comparisons: {},
    deep: {},
    notes: {},
  }));
  const followers = new Map<string, PaneFollower>();
  /** Where a pane is going after its partner, so its arrival is not passed on as its own move. */
  const following = new Map<string, string>();
  /** Moves a pane's partner made while the pane was still on its way after an earlier one. */
  const queued = new Map<string, BrowseMove[]>();
  /** Pairs with one pane moved and the other on its way after it; they wait to be compared. */
  const settling = new Set<string>();
  /**
   * Pairs whose one pane went where the other could not follow: that pane, and how many folders
   * further down it is. The other stays put until the first is back where they parted.
   */
  const apart = new Map<string, { pane: string; depth: number }>();
  /** The last listing comparison of each pair, kept while neither listing nor the rules change. */
  const listings = new Map<
    string,
    {
      left: readonly CompareEntry[];
      right: readonly CompareEntry[];
      rules: string;
      found: ListingComparison;
    }
  >();
  const seeks = new Map<string, (from: string, candidate: Candidate | null) => void>();
  let optionsSeen = "";

  const mine = (pane: PaneRef) => pane.window === env.window;
  const tell = (pane: PaneRef, message: CompareMessage) => {
    if (!mine(pane)) env.send(pane.window, message);
  };

  function rulesFor(left: PaneSnapshot, right: PaneSnapshot): CompareRules {
    return compareRules(env.options(), env.syncDefaults(), {
      ignoreCase: left.ignoresCase || right.ignoresCase,
      leftCoarse: left.coarseTimes,
      rightCoarse: right.coarseTimes,
    });
  }

  /** The options a deep comparison is made with, to tell when one no longer holds. */
  function optionsKey(): string {
    const rules = compareRules(env.options(), env.syncDefaults(), {
      ignoreCase: false,
      leftCoarse: false,
      rightCoarse: false,
    });
    return JSON.stringify([
      rules.sizeOnly,
      rules.toleranceSecs,
      rules.ignoreHourOffsets,
      rules.includeHidden,
    ]);
  }

  function sidesOf(pairId: string) {
    const state = store.getState();
    const pair = state.pairs[pairId];
    if (!pair) return null;
    return {
      pair,
      left: state.snapshots[paneKey(pair.left)] ?? null,
      right: state.snapshots[paneKey(pair.right)] ?? null,
    };
  }

  function setComparison(pairId: string, comparison: PairComparison | null) {
    const { comparisons } = store.getState();
    if (comparison === null && comparisons[pairId] === null) return;
    store.setState({ comparisons: { ...comparisons, [pairId]: comparison } });
  }

  function recompute(pairId: string) {
    const sides = sidesOf(pairId);
    if (!sides) return;
    const { pair, left, right } = sides;
    if (!left || !right || settling.has(pairId) || left.showingResults || right.showingResults) {
      setComparison(pairId, null);
      return;
    }
    const rules = rulesFor(left, right);
    const rulesKey = JSON.stringify(rules);
    let listing = listings.get(pairId);
    if (
      !listing ||
      listing.left !== left.entries ||
      listing.right !== right.entries ||
      listing.rules !== rulesKey
    ) {
      listing = {
        left: left.entries,
        right: right.entries,
        rules: rulesKey,
        found: compareListings(left.entries, right.entries, rules),
      };
      listings.set(pairId, listing);
    }
    const deep = store.getState().deep[pairId];
    const prefix = deep?.status === "done" ? deepPrefix(deep.comparison, left, right) : null;
    const covered =
      deep?.status === "done" && prefix !== null ? { comparison: deep.comparison, prefix } : null;
    setComparison(pairId, {
      left: mine(pair.left) ? rowMarks(listing.found.left, "left", covered) : NO_MARKS,
      right: mine(pair.right) ? rowMarks(listing.found.right, "right", covered) : NO_MARKS,
    });
  }

  function setDeep(pairId: string, deep: DeepState | null) {
    const state = store.getState();
    store.setState({
      deep: deep ? { ...state.deep, [pairId]: deep } : without(state.deep, [pairId]),
    });
  }

  function setNote(pairId: string, note: string | null) {
    const { notes } = store.getState();
    if (note === null && !(pairId in notes)) return;
    store.setState({ notes: note ? { ...notes, [pairId]: note } : without(notes, [pairId]) });
  }

  function forget(tabId: string) {
    following.delete(tabId);
    queued.delete(tabId);
  }

  /** Ends a comparison here, and in the other pane's window unless it said so itself. */
  function unlink(pairId: string, announce = true) {
    const state = store.getState();
    const pair = state.pairs[pairId];
    if (!pair) return;
    const deep = state.deep[pairId];
    if (deep?.status === "running") void env.cancel(deep.requestId).catch(() => undefined);
    const tabs = [pair.left, pair.right].filter(mine).map((pane) => pane.tabId);
    for (const tabId of tabs) forget(tabId);
    settling.delete(pairId);
    apart.delete(pairId);
    listings.delete(pairId);
    store.setState({
      pairs: without(state.pairs, [pairId]),
      pairOf: without(
        state.pairOf,
        tabs.filter((tabId) => state.pairOf[tabId] === pairId),
      ),
      snapshots: without(state.snapshots, [paneKey(pair.left), paneKey(pair.right)]),
      comparisons: without(state.comparisons, [pairId]),
      deep: without(state.deep, [pairId]),
      notes: without(state.notes, [pairId]),
    });
    if (!announce) return;
    for (const pane of [pair.left, pair.right]) {
      tell(pane, { type: "unlink", from: env.window, pairId });
    }
  }

  /** Starts a comparison here. A pane that was compared with another leaves that one. */
  function link(pair: ComparePair) {
    const tabs = [pair.left, pair.right].filter(mine).map((pane) => pane.tabId);
    for (const tabId of tabs) {
      const earlier = store.getState().pairOf[tabId];
      if (earlier) unlink(earlier);
    }
    const state = store.getState();
    store.setState({
      pairs: { ...state.pairs, [pair.id]: pair },
      pairOf: { ...state.pairOf, ...Object.fromEntries(tabs.map((tabId) => [tabId, pair.id])) },
      comparisons: { ...state.comparisons, [pair.id]: null },
    });
  }

  function candidates(except?: string): Candidate[] {
    return env
      .panes()
      .filter((pane) => pane.ready && pane.tabId !== except)
      .map(({ tabId, label, visible, activatedAt }) => ({
        window: env.window,
        tabId,
        label,
        visible,
        activatedAt,
      }));
  }

  /** Asks the other windows for the pane each of them would offer. */
  async function seekElsewhere(): Promise<Candidate[]> {
    const windows = await env.otherWindows().catch(() => [] as string[]);
    if (windows.length === 0) return [];
    const seekId = env.newId();
    return new Promise((resolve) => {
      const found: Candidate[] = [];
      const waiting = new Set(windows);
      const finish = () => {
        clearTimeout(timer);
        seeks.delete(seekId);
        resolve(found);
      };
      const timer = setTimeout(finish, env.seekMillis);
      seeks.set(seekId, (from, candidate) => {
        if (candidate) found.push(candidate);
        waiting.delete(from);
        if (waiting.size === 0) finish();
      });
      env.send(null, { type: "seek", from: env.window, seekId });
    });
  }

  /**
   * Compares the pane with the other pane the user looked at last, in this window or another.
   * Gives the reason when there is none to compare with.
   */
  async function turnOn(tabId: string): Promise<string | null> {
    const own = env.panes().find((pane) => pane.tabId === tabId);
    if (!own) return t("compare.noFolderToCompare");
    const elsewhere = await seekElsewhere();
    if (store.getState().pairOf[tabId]) return null;
    const partner = [...candidates(tabId), ...elsewhere].reduce<Candidate | null>(better, null);
    if (!partner) return t("compare.openASecondPane");
    const pair: ComparePair = {
      id: env.newId(),
      left: { window: env.window, tabId, label: own.label },
      right: { window: partner.window, tabId: partner.tabId, label: partner.label },
    };
    link(pair);
    tell(pair.right, { type: "link", from: env.window, pair });
    return null;
  }

  /** Turns the comparison of a pane on, or off for it and its partner. */
  async function toggle(tabId: string): Promise<string | null> {
    const pairId = store.getState().pairOf[tabId];
    if (!pairId) return turnOn(tabId);
    unlink(pairId);
    return null;
  }

  function stopSettling(pairId: string) {
    if (settling.delete(pairId)) recompute(pairId);
  }

  /** Shows a note in the bars of both panes, or takes it away. */
  function shareNote(pair: ComparePair, note: string | null) {
    if ((store.getState().notes[pair.id] ?? null) === note) return;
    setNote(pair.id, note);
    const message: CompareMessage = { type: "note", from: env.window, pairId: pair.id, note };
    for (const pane of [pair.left, pair.right]) tell(pane, message);
  }

  const depthOf = (moves: readonly BrowseMove[]) =>
    moves.reduce((depth, move) => depth + (move.kind === "enter" ? 1 : -1), 0);

  /** Notes that a pane of this window is `depth` folders ahead of its partner, alone. */
  function setApart(pair: ComparePair, pane: PaneRef, depth: number) {
    if (depth === 0) {
      apart.delete(pair.id);
      shareNote(pair, null);
    } else {
      apart.set(pair.id, { pane: paneKey(pane), depth });
    }
  }

  /**
   * The pane could not make its partner's move, nor the ones waiting after it: it stays where it
   * is and the bars say so.
   */
  function followFailed(pair: ComparePair, pane: PaneRef, move: BrowseMove, note: string) {
    const depth = depthOf([move, ...(queued.get(pane.tabId) ?? [])]);
    forget(pane.tabId);
    shareNote(pair, note);
    stopSettling(pair.id);
    const mover = partnerOf(pair, pane);
    if (mine(mover)) setApart(pair, mover, depth);
    else env.send(mover.window, { type: "followed", from: env.window, pairId: pair.id, depth });
  }

  function startFollow(pair: ComparePair, pane: PaneRef, move: BrowseMove) {
    // Going elsewhere would close the results, so the pane stays with them.
    if (store.getState().snapshots[paneKey(pane)]?.showingResults) {
      followFailed(pair, pane, move, t("compare.paneShowsResults", { pane: pane.label }));
      return;
    }
    const sides = sidesOf(pair.id);
    const ignoreCase = Boolean(sides?.left?.ignoresCase || sides?.right?.ignoresCase);
    const going = followers.get(pane.tabId)?.follow(move, ignoreCase) ?? null;
    if (!going) {
      const note =
        move.kind === "enter"
          ? t("compare.noFolderNamed", { name: move.name, pane: pane.label })
          : t("compare.noFolderAbove", { pane: pane.label });
      followFailed(pair, pane, move, note);
      return;
    }
    following.set(pane.tabId, going.path);
    void going.done.then((listed) => {
      if (listed || following.get(pane.tabId) !== going.path) return;
      const current = store.getState().pairs[pair.id];
      if (current) {
        followFailed(
          current,
          pane,
          move,
          t("compare.paneCouldNotOpen", { pane: pane.label, path: going.path }),
        );
      }
    });
  }

  /** Makes a pane of this window do what its partner just did. */
  function follow(pair: ComparePair, pane: PaneRef, move: BrowseMove) {
    settling.add(pair.id);
    if (following.has(pane.tabId)) {
      queued.set(pane.tabId, [...(queued.get(pane.tabId) ?? []), move]);
      return;
    }
    startFollow(pair, pane, move);
  }

  /** A pane of this window went somewhere by itself: its partner goes along where it can. */
  function ownMove(pair: ComparePair, pane: PaneRef, move: BrowseMove | null) {
    const alone = apart.get(pair.id);
    if (alone && move && alone.pane === paneKey(pane)) {
      setApart(pair, pane, alone.depth + depthOf([move]));
      return;
    }
    apart.delete(pair.id);
    shareNote(pair, null);
    if (!move || !env.options().browseTogether) return;
    const partner = partnerOf(pair, pane);
    if (mine(partner)) {
      follow(pair, partner, move);
    } else {
      settling.add(pair.id);
      env.send(partner.window, { type: "follow", from: env.window, pairId: pair.id, move });
    }
  }

  /** Works out what a pane's new listing means for its pair, then compares again. */
  function snapshotChanged(
    pair: ComparePair,
    pane: PaneRef,
    previous: PaneSnapshot | null,
    next: PaneSnapshot | null,
  ) {
    const moved = !!previous && !!next && !samePath(previous.path, next.path, next.pathStyle);
    if (previous && next && !moved && !sameEntries(previous.entries, next.entries)) {
      // What was found below this folder may no longer hold.
      if (store.getState().deep[pair.id]?.status === "done") setDeep(pair.id, null);
    }
    if (previous && !next) {
      // Nothing is on show any more, as when a vault locks or a connection is lost: what was
      // found below the folder goes too, decrypted names included.
      const deep = store.getState().deep[pair.id];
      if (deep?.status === "running") void env.cancel(deep.requestId).catch(() => undefined);
      if (deep) setDeep(pair.id, null);
    }
    if (moved && !mine(pane)) {
      // The partner in another window got where it was going, or went somewhere by itself.
      settling.delete(pair.id);
    } else if (moved && previous && next) {
      const expected = following.get(pane.tabId);
      following.delete(pane.tabId);
      if (expected !== undefined && samePath(expected, next.path, next.pathStyle)) {
        const waiting = queued.get(pane.tabId)?.shift();
        if (waiting) startFollow(pair, pane, waiting);
        else settling.delete(pair.id);
      } else {
        queued.delete(pane.tabId);
        settling.delete(pair.id);
        ownMove(pair, pane, moveBetween(previous, next));
      }
    }
    recompute(pair.id);
  }

  /** Called by a compared pane with what it shows, each time that changes. */
  function publish(tabId: string, snapshot: PaneSnapshot | null) {
    const state = store.getState();
    const pair = state.pairs[state.pairOf[tabId] ?? ""];
    if (!pair) return;
    const pane = mine(pair.left) && pair.left.tabId === tabId ? pair.left : pair.right;
    const partner = partnerOf(pair, pane);
    const key = paneKey(pane);
    const previous = state.snapshots[key] ?? null;
    store.setState({ snapshots: { ...state.snapshots, [key]: snapshot } });
    tell(partner, { type: "snapshot", from: env.window, pairId: pair.id, tabId, snapshot });
    snapshotChanged(pair, pane, previous, snapshot);
  }

  /** Called when a compared pane goes away: its tab closed, or moved to another window. */
  function leave(tabId: string) {
    followers.delete(tabId);
    const pairId = store.getState().pairOf[tabId];
    if (pairId) unlink(pairId);
  }

  function attach(tabId: string, follower: PaneFollower): () => void {
    followers.set(tabId, follower);
    return () => {
      if (followers.get(tabId) === follower) followers.delete(tabId);
    };
  }

  function wireOf(deep: DeepState | null): DeepWire | null {
    if (!deep || deep.status === "failed") return deep;
    if (deep.status === "running") return { status: "running", requestId: deep.requestId };
    const { leftRoot, rightRoot, ignoreCase, folders, truncated } = deep.comparison;
    return {
      status: "done",
      leftRoot,
      rightRoot,
      ignoreCase,
      folders: [...folders.values()],
      truncated,
      options: deep.options,
    };
  }

  /** A deep comparison's state as another window sent it, or null when it is misshapen. */
  function fromWire(value: unknown): DeepState | null {
    const deep = asRecord(value);
    if (deep?.status === "failed") {
      return { status: "failed", message: typeof deep.message === "string" ? deep.message : "" };
    }
    if (deep?.status === "running") {
      return typeof deep.requestId === "string"
        ? { status: "running", requestId: deep.requestId, leftEntries: 0, rightEntries: 0 }
        : null;
    }
    const { leftRoot, rightRoot, folders, options } = deep ?? {};
    if (deep?.status !== "done" || !Array.isArray(folders)) return null;
    if (typeof leftRoot !== "string" || typeof rightRoot !== "string") return null;
    const summaries = folders.map(asSummary);
    if (summaries.some((summary) => summary === null)) return null;
    return {
      status: "done",
      comparison: {
        leftRoot,
        rightRoot,
        ignoreCase: deep.ignoreCase === true,
        folders: new Map(
          summaries.map((summary) => [(summary as FolderSummary).path, summary as FolderSummary]),
        ),
        truncated: deep.truncated === true,
      },
      options: typeof options === "string" ? options : "",
    };
  }

  /** Shows a deep comparison's new state here and in the other pane's window. */
  function shareDeep(pair: ComparePair, deep: DeepState | null) {
    setDeep(pair.id, deep);
    const message: CompareMessage = {
      type: "deep",
      from: env.window,
      pairId: pair.id,
      deep: wireOf(deep),
    };
    for (const pane of [pair.left, pair.right]) tell(pane, message);
    recompute(pair.id);
  }

  /** Compares what is below the two folders on show, in the background. */
  async function startDeep(tabId: string): Promise<void> {
    const pairId = store.getState().pairOf[tabId];
    const sides = pairId ? sidesOf(pairId) : null;
    if (!sides?.left || !sides.right) return;
    const { pair, left, right } = sides;
    if (left.showingResults || right.showingResults) return;
    if (store.getState().deep[pair.id]?.status === "running") return;
    const rules = rulesFor(left, right);
    const options = optionsKey();
    const requestId = env.newId();
    const current = () => {
      const deep = store.getState().deep[pair.id];
      return deep?.status === "running" && deep.requestId === requestId;
    };
    shareDeep(pair, { status: "running", requestId, leftEntries: 0, rightEntries: 0 });
    try {
      const found = await env.compareFolders({
        requestId,
        left: { location: left.location, path: left.path },
        right: { location: right.location, path: right.path },
        sizeOnly: rules.sizeOnly,
        timeToleranceSecs: rules.toleranceSecs,
        ignoreHourOffsets: rules.ignoreHourOffsets,
        includeHidden: rules.includeHidden,
        ignoreCase: rules.ignoreCase,
      });
      if (!current()) return;
      // Options changed while it ran: what it found is by the old ones.
      if (options !== optionsKey()) {
        shareDeep(pair, null);
        return;
      }
      shareDeep(pair, {
        status: "done",
        comparison: {
          leftRoot: left.path,
          rightRoot: right.path,
          ignoreCase: rules.ignoreCase,
          folders: new Map(found.folders.map((folder) => [folder.path, folder])),
          truncated: found.truncated,
        },
        options,
      });
    } catch (caught) {
      if (!current()) return;
      const { cancelled, message } = failure(caught);
      shareDeep(pair, cancelled ? null : { status: "failed", message });
    }
  }

  function stopDeep(tabId: string) {
    const deep = store.getState().deep[store.getState().pairOf[tabId] ?? ""];
    if (deep?.status === "running") void env.cancel(deep.requestId).catch(() => undefined);
  }

  /** Forgets a deep comparison that failed, once its message has been read. */
  function dismissDeep(tabId: string) {
    const state = store.getState();
    const pair = state.pairs[state.pairOf[tabId] ?? ""];
    if (pair && state.deep[pair.id]?.status === "failed") shareDeep(pair, null);
  }

  /** Called with each progress event of a synchronization or a deep comparison. */
  function progress(update: SyncProgress) {
    const { deep } = store.getState();
    for (const [pairId, state] of Object.entries(deep)) {
      if (state.status !== "running" || state.requestId !== update.requestId) continue;
      setDeep(pairId, {
        ...state,
        leftEntries: update.localEntries,
        rightEntries: update.remoteEntries,
      });
    }
  }

  /** Called when the bar's options or the defaults in Settings > Sync may have changed. */
  function rulesChanged() {
    const options = optionsKey();
    const browse = String(env.options().browseTogether);
    if (optionsSeen === options + browse) return;
    optionsSeen = options + browse;
    const state = store.getState();
    for (const pairId of Object.keys(state.pairs)) {
      const deep = state.deep[pairId];
      if (deep?.status === "done" && deep.options !== options) setDeep(pairId, null);
      recompute(pairId);
    }
  }

  /** Ends comparisons with panes of windows that are no longer there. */
  async function dropClosedWindows(): Promise<void> {
    const pairs = Object.values(store.getState().pairs).filter(
      (pair) => !mine(pair.left) || !mine(pair.right),
    );
    if (pairs.length === 0) return;
    const open = await env.otherWindows().catch(() => null);
    if (!open) return;
    for (const pair of pairs) {
      const elsewhere = mine(pair.left) ? pair.right : pair.left;
      if (!open.includes(elsewhere.window)) unlink(pair.id, false);
    }
  }

  /** Ends every comparison with a pane of another window, as this window closes. */
  function leaveAll() {
    for (const pairId of Object.keys(store.getState().pairs)) unlink(pairId);
  }

  /** Takes a message from another window. */
  function receive(message: unknown) {
    if (!isCompareMessage(message) || message.from === env.window) return;
    if (message.type === "seek") {
      const candidate = candidates().reduce<Candidate | null>(better, null);
      env.send(message.from, {
        type: "candidate",
        from: env.window,
        seekId: message.seekId,
        candidate,
      });
      return;
    }
    if (message.type === "candidate") {
      const offered = message.candidate;
      const candidate = isPaneRef(offered)
        ? {
            window: message.from,
            tabId: offered.tabId,
            label: offered.label,
            visible: offered.visible === true,
            activatedAt: Number(offered.activatedAt) || 0,
          }
        : null;
      seeks.get(message.seekId)?.(message.from, candidate);
      return;
    }
    if (message.type === "link") {
      const { pair } = message;
      const here = pair.right.window === env.window && pair.left.window === message.from;
      const known = env.panes().some((pane) => pane.tabId === pair.right.tabId);
      if (here && known) link(pair);
      else env.send(message.from, { type: "unlink", from: env.window, pairId: pair.id });
      return;
    }
    const pair = store.getState().pairs[message.pairId];
    if (!pair) return;
    const theirs = mine(pair.left) ? pair.right : pair.left;
    const ours = partnerOf(pair, theirs);
    if (theirs.window !== message.from || !mine(ours)) return;
    switch (message.type) {
      case "unlink":
        unlink(pair.id, false);
        return;
      case "snapshot": {
        if (message.tabId !== theirs.tabId) return;
        const state = store.getState();
        const key = paneKey(theirs);
        const previous = state.snapshots[key] ?? null;
        const snapshot = asSnapshot(message.snapshot);
        store.setState({ snapshots: { ...state.snapshots, [key]: snapshot } });
        snapshotChanged(pair, theirs, previous, snapshot);
        return;
      }
      case "follow": {
        const move = asMove(message.move);
        if (move) follow(pair, ours, move);
        return;
      }
      case "followed":
        setApart(pair, ours, Math.trunc(Number(message.depth)) || 0);
        stopSettling(pair.id);
        return;
      case "note":
        setNote(pair.id, typeof message.note === "string" ? message.note : null);
        return;
      case "deep":
        setDeep(pair.id, fromWire(message.deep));
        recompute(pair.id);
        return;
    }
  }

  return {
    store,
    toggle,
    publish,
    leave,
    attach,
    startDeep,
    stopDeep,
    dismissDeep,
    progress,
    rulesChanged,
    dropClosedWindows,
    leaveAll,
    receive,
  };
}

export type FolderCompare = ReturnType<typeof createFolderCompare>;
