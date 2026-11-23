// The listings of server folders this window remembers, and everything that tells them what
// changed: the panes that list folders, the operations that change them, the sessions that
// close and the other windows of Poros. `lib/listingCache.ts` holds the listings and
// `lib/listingShare.ts` shares them between windows; nothing is kept on disk.

import { emit, emitTo, listen } from "@tauri-apps/api/event";
import { getAllWindows, getCurrentWindow } from "@tauri-apps/api/window";
import type { PaneListings } from "../hooks/usePane";
import { onRemoteChange } from "../lib/fileSource";
import { DRAG_PREVIEW_WINDOW, LISTING_CACHE_EVENT, MAIN_WINDOW } from "../lib/ipc";
import { ListingCache, listingScope, outlivesSession } from "../lib/listingCache";
import { ListingShare, parseListingMessage, type ListingMessage } from "../lib/listingShare";
import { childPath, isWithin, remoteParent, samePath } from "../lib/path";
import type { VaultInfo } from "../lib/types";
import { VAULT_DATA_FOLDER, vaultSessions } from "../lib/vaults";
import { listPanes, type PaneHandle } from "./paneRegistry";
import { serverEntry, useSessionStore, type SessionEntry } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";

/**
 * How long after a change the panes showing the folder are looked at: by then whoever made the
 * change has reloaded the panes it knows of, and only a folder nobody reloaded is listed again.
 */
const RELOAD_CHECK_MILLIS = 600;
/** How often the windows told what this one learns are checked for still being open. */
const PRUNE_MILLIS = 30_000;

function windowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    // Outside a window of Poros, as in tests, there is nobody to share with.
    return MAIN_WINDOW;
  }
}

function send(message: ListingMessage, to?: string): void {
  const sent = to ? emitTo(to, LISTING_CACHE_EVENT, message) : emit(LISTING_CACHE_EVENT, message);
  sent.catch(() => undefined);
}

function remembering(): boolean {
  return useSettingsStore.getState().settings.interface.cachedListings;
}

function scopeOf(entry: SessionEntry): string {
  return listingScope(entry.info, entry.profile);
}

function sessionEntries(): SessionEntry[] {
  return Object.values(useSessionStore.getState().sessions);
}

function scopeOfSession(sessionId: string): string | undefined {
  const entry = useSessionStore.getState().sessions[sessionId] as SessionEntry | undefined;
  return entry && scopeOf(entry);
}

function inUse(scope: string): boolean {
  return sessionEntries().some((entry) => scopeOf(entry) === scope);
}

/** The unlocked vault whose decrypted listings `scope` names, if it names one. */
function vaultBehind(scope: string): VaultInfo | undefined {
  if (outlivesSession(scope)) return undefined;
  return sessionEntries().find((entry) => entry.info.vault && scopeOf(entry) === scope)?.info.vault;
}

const label = windowLabel();

export const listingShare = new ListingShare(new ListingCache(), {
  label,
  main: label === MAIN_WINDOW,
  send,
  inUse,
  changedElsewhere: (scope, folder, entries) => reloadUnseen(scope, folder, entries),
  later: (run, millis) => setTimeout(run, millis),
  cancel: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
});

const cache = listingShare.cache;

/** The panes of this window on the connection `scope` names, with the folder each shows. */
function panesOn(scope: string): { pane: PaneHandle; path: string }[] {
  return listPanes().flatMap((pane) => {
    const path = pane.path();
    const shows = pane.kind === "remote" && pane.sessionId !== undefined && path !== null;
    return shows && scopeOfSession(pane.sessionId!) === scope ? [{ pane, path }] : [];
  });
}

/**
 * Reloads the panes left showing a folder that changed. Whoever changed it reloads the panes it
 * knows of, and every other pane on the folder takes the listing they are given; a folder no
 * pane is reloading by now is listed by one of the panes showing it, for all of them.
 */
function reloadUnseen(scope: string, folder: string, entries: readonly string[]): void {
  setTimeout(() => {
    if (!remembering()) return;
    const showing = new Map<string, PaneHandle[]>();
    for (const { pane, path } of panesOn(scope)) {
      const changed =
        samePath(path, folder, "posix") || entries.some((entry) => isWithin(path, entry, "posix"));
      if (changed) showing.set(path, [...(showing.get(path) ?? []), pane]);
    }
    for (const [path, panes] of showing) {
      const listed = cache.listedSinceChange(scope, path);
      if (listed || panes.some((pane) => pane.loading?.())) continue;
      (panes.find((pane) => pane.visible) ?? panes[0]).refresh();
    }
  }, RELOAD_CHECK_MILLIS);
}

function changed(scope: string, folder: string, entries: readonly string[]): void {
  if (!remembering()) return;
  listingShare.forget(scope, folder, entries);
  reloadUnseen(scope, folder, entries);
  // What changes in an unlocked vault changes the encrypted files it is kept in on the server.
  const vault = vaultBehind(scope);
  const server = vault && scopeOfSession(vault.parentSessionId);
  if (!vault || !server) return;
  const encrypted = childPath(vault.root, VAULT_DATA_FOLDER, "posix");
  listingShare.forget(server, encrypted, [encrypted]);
}

/**
 * Poros changed `folder` on a session's server: its listing is forgotten in every window, and so
 * are those of the folders among `entries`, which were moved, deleted or written over, with all
 * inside them. Panes showing any of them that nobody reloads are reloaded.
 */
export function forgetListing(
  sessionId: string,
  folder: string,
  entries: readonly string[] = [],
): void {
  const scope = scopeOfSession(sessionId);
  if (scope) changed(scope, folder, entries);
}

/** Forgets `folder` on a session's server with every folder inside it, as after a sync into it. */
export function forgetListingsWithin(sessionId: string, folder: string): void {
  forgetListing(sessionId, folder, [folder]);
}

/** Forgets the listing of the folder the file at `path` is in, as after the file is saved. */
export function forgetListingOf(sessionId: string, path: string): void {
  const folder = remoteParent(path);
  if (folder !== null) forgetListing(sessionId, folder);
}

/**
 * Anything on a session's server may have changed, as after a command run there: every listing
 * of the connection is forgotten, and those of the vaults unlocked on it.
 */
export function forgetServerListings(sessionId: string): void {
  const { sessions } = useSessionStore.getState();
  const server = serverEntry(sessions, sessionId);
  if (!server) return;
  listingShare.forgetAll(scopeOf(server));
  for (const vault of vaultSessions(sessions, server.info.id)) {
    cache.dropScope(scopeOf(sessions[vault.id]));
  }
}

/**
 * Sessions left this window. What is kept for one session only, as a vault's decrypted listings
 * are, goes at once. When they `closed` for good rather than moved to another window, the
 * listings of a connection no session is on any more go too: at once, or after a while for a
 * saved connection, which is likely to be opened again.
 */
export function listingsLeft(entries: readonly SessionEntry[], closed: boolean): void {
  for (const entry of entries) {
    const scope = scopeOf(entry);
    if (!outlivesSession(scope)) {
      cache.dropScope(scope);
      continue;
    }
    if (!closed) continue;
    const saved = Boolean(entry.info.savedConnectionId ?? entry.profile?.savedConnectionId);
    listingShare.closed(scope, saved);
  }
}

/** The listings the panes of this window remember, where the setting has them remembered. */
export const paneListings: PaneListings = {
  recall: (source, path) => {
    const scope = source.listingScope;
    return scope && remembering() ? (cache.get(scope, path) ?? null) : null;
  },
  asking: (source, path, by) => {
    const scope = source.listingScope;
    if (!scope || !remembering()) return () => undefined;
    const ticket = cache.begin(scope, path);
    return (answer) => {
      if (answer) listingShare.store(scope, answer, ticket, by);
      else cache.cancel(ticket);
    };
  },
  forget: (source, path) => {
    if (source.listingScope) cache.invalidateSubtree(source.listingScope, path);
  },
  watch: (listener) => cache.watch(listener),
};

onRemoteChange(({ scope, folder, entries }) => changed(scope, folder, entries));

/**
 * Starts sharing listings with the other windows of Poros, and forgets them all when the setting
 * that has them remembered is turned off. The function returned stops it.
 */
export function startSharingListings(): () => void {
  let stopped = false;
  const listening = listen<unknown>(LISTING_CACHE_EVENT, (event) => {
    const message = parseListingMessage(event.payload);
    if (message && remembering()) listingShare.receive(message);
  });
  listening
    .then(() => {
      if (!stopped) listingShare.join();
    })
    .catch(() => undefined);
  const unsubscribe = useSettingsStore.subscribe((state, previous) => {
    const { cachedListings } = state.settings.interface;
    if (!cachedListings && previous.settings.interface.cachedListings) cache.clear();
  });
  // A window that closes says nothing, so the ones told are checked against the ones open.
  const pruning = setInterval(() => {
    if (listingShare.audience.length === 0) return;
    void getAllWindows()
      .then((open) => {
        const labels = open.map((entry) => entry.label);
        listingShare.prune(labels.filter((other) => other !== DRAG_PREVIEW_WINDOW));
      })
      .catch(() => undefined);
  }, PRUNE_MILLIS);
  return () => {
    stopped = true;
    void listening.then((unlisten) => unlisten()).catch(() => undefined);
    unsubscribe();
    clearInterval(pruning);
  };
}
