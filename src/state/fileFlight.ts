// Starts the flying file icons when Settings > Appearance turns them on. Each file a transfer
// finishes flies from where it came from to the row it now has where it went, or to the folder
// it went into when that is what the pane shows; files moved or copied in place fly once they
// are there. Bursts too large to fly one by one go as a pile. It measures the panes and hands
// the planned paths to `components/FileFlights.tsx`.

import { create } from "zustand";
import {
  boxCenter,
  intersects,
  MAX_FLIGHT_ICONS,
  pileOf,
  planArrival,
  planFlight,
  seededRandom,
  type Box,
  type IconPath,
  type Point,
} from "../lib/fileFlight";
import { localSource, type PathStyle } from "../lib/fileSource";
import { baseName, isWithin, samePath } from "../lib/path";
import { systemIconLookup, type IconSubject } from "../lib/systemIcons";
import { isSettled, type TransferSnapshot } from "../lib/transfers";
import type { Direction, FileEntry, JobSnapshot, TransferUpdate } from "../lib/types";
import { listPanes, type PaneHandle } from "./paneRegistry";
import { useSettingsStore } from "./settingsStore";
import { iconCacheKey, requestSystemIcon } from "./systemIconStore";

export interface FlyingIcon {
  subject: IconSubject;
  path: IconPath;
}

export interface Flight {
  id: number;
  icons: FlyingIcon[];
  /** How many items the pile stands for, shown on it when more than one. */
  badge: { text: string; path: IconPath } | null;
  landing: Point;
  /** What the files land in, lit up as they arrive. */
  target: Box | null;
  /** A ring spreads where piles land, and files that land on a folder or a pane. */
  ring: boolean;
  firstLanding: number;
  total: number;
}

interface FlightState {
  flights: Flight[];
}

export const useFlightStore = create<FlightState>(() => ({ flights: [] }));

/** The flying icons' size in pixels; the file lists' icons are scaled up to it. */
export const FLIGHT_ICON_SIZE = 24;
/** The size their system icons are looked up at, the large one on Windows, so they stay sharp. */
export const FLIGHT_ICON_LOOKUP_SIZE = 32;
const MAX_FLIGHTS = 16;
/** How often waiting files are looked at, to fly the ones whose rows are on screen. */
const PUMP_MILLIS = 70;
/** How long a file waits for its row to appear after the pane reloads, before it flies anyway. */
const ROW_WAIT_MILLIS = 1500;
/** Files ready together leave one after another, this far apart. */
const STREAM_STEP_MILLIS = 70;
/** More ready at once than this, and those going to the same place fly as one pile. */
const MAX_SINGLES_AT_ONCE = 4;
const MAX_WAITING = 600;
/** A queue that settles forgets the transfers started before it, but not ones just started. */
const SETTLE_GRACE_MILLIS = 3000;
const MAX_WATCHES = 64;

let nextFlightId = 1;

export function endFlight(id: number): void {
  useFlightStore.setState(({ flights }) => ({
    flights: flights.filter((flight) => flight.id !== id),
  }));
}

export function prefersLessMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * The switch in Settings decides. Windows asks for less motion whenever its animation effects
 * are off, which many turn off for speed, so following the system would leave files still for
 * someone who turned them on.
 */
function flying(): boolean {
  return useSettingsStore.getState().settings.appearance.transferAnimation && !document.hidden;
}

function boxOf(element: Element): Box {
  const { left, top, width, height } = element.getBoundingClientRect();
  return { left, top, width, height };
}

function paneElement(tabId: string): HTMLElement | null {
  const element = document.querySelector<HTMLElement>(`[data-drop-pane="${CSS.escape(tabId)}"]`);
  return element && boxOf(element).width > 0 ? element : null;
}

function tabButton(tabId: string): HTMLElement | null {
  const element = document.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(tabId)}"]`);
  return element && boxOf(element).width > 0 ? element : null;
}

/** The part of a pane that lists files, where files land and leave from. */
function listBox(pane: HTMLElement): Box {
  return boxOf(pane.querySelector(".file-list-body") ?? pane);
}

function contains(box: Box, point: Point): boolean {
  return (
    point.x >= box.left &&
    point.x <= box.left + box.width &&
    point.y >= box.top &&
    point.y <= box.top + box.height
  );
}

/** Where a file is: on this computer, or on a server. */
export interface FileSide {
  kind: "local" | "remote";
  sessionId?: string;
}

function pathStyle(side: FileSide): PathStyle {
  return side.kind === "remote" ? "posix" : localSource.pathStyle;
}

function sameSide(pane: PaneHandle, side: FileSide): boolean {
  return pane.kind === side.kind && (side.kind === "local" || pane.sessionId === side.sessionId);
}

/** The panes on screen showing files of `side`, the one named first. */
function panesOn(side: FileSide, preferredTab?: string): PaneHandle[] {
  return listPanes()
    .filter((pane) => sameSide(pane, side) && pane.path() !== null)
    .sort(
      (first, second) =>
        Number(second.tabId === preferredTab) - Number(first.tabId === preferredTab) ||
        Number(second.visible) - Number(first.visible),
    );
}

interface RowSpot {
  row: Box;
  icon: Box;
  /** The row is the file's own, not a folder it went into. */
  own: boolean;
}

/** The row of `path` the pane shows on screen, else that of the folder it is in. */
function rowFor(pane: HTMLElement, path: string, style: PathStyle): RowSpot | null {
  const list = listBox(pane);
  let folder: RowSpot | null = null;
  for (const row of pane.querySelectorAll<HTMLElement>(".file-list-body [data-path]")) {
    const rowPath = row.dataset.path;
    if (!rowPath) continue;
    const own = samePath(rowPath, path, style);
    if (!own && !(row.dataset.dropFolder && isWithin(path, rowPath, style))) continue;
    const box = boxOf(row);
    if (!intersects(box, list)) continue;
    const spot = { row: box, icon: boxOf(row.querySelector(".file-icon") ?? row), own };
    if (own) return spot;
    folder ??= spot;
  }
  return folder;
}

/** Whether the pane's listing has `path`, or a folder it is in, though its row may be away. */
function listsPath(pane: PaneHandle, path: string, style: PathStyle): boolean {
  return (pane.entries?.() ?? []).some(
    (entry) => samePath(entry.path, path, style) || isWithin(path, entry.path, style),
  );
}

interface Destination {
  landing: Point;
  target: Box;
  /** The size of the icon there next to a flying one. */
  landingScale: number;
  ring: boolean;
}

interface Target extends FileSide {
  path: string;
  /** The tab the files were sent to. */
  tabId?: string;
  /** Where a drag let go of them, for files whose row cannot be found. */
  dropPoint?: Point;
}

/**
 * Where a file lands: its own row, else the row of the folder it went into, else the pane that
 * shows its folder or the tab it was sent to. "wait" while a pane shows its folder but has not
 * listed it yet, since it reloads a moment after the file is done; `waited` stops the wait.
 */
function destinationOf(target: Target, waited: boolean): Destination | "wait" | null {
  const style = pathStyle(target);
  let showing: { pane: PaneHandle; element: HTMLElement } | null = null;
  for (const pane of panesOn(target, target.tabId)) {
    const element = paneElement(pane.tabId);
    const shown = pane.path();
    if (!element || !shown) continue;
    if (!isWithin(target.path, shown, style) || samePath(target.path, shown, style)) continue;
    const spot = rowFor(element, target.path, style);
    if (spot) {
      return {
        landing: boxCenter(spot.icon),
        target: spot.row,
        landingScale: spot.icon.width / FLIGHT_ICON_SIZE,
        ring: !spot.own,
      };
    }
    if (!waited && !listsPath(pane, target.path, style)) return "wait";
    showing ??= { pane, element };
  }
  const element = showing?.element ?? (target.tabId ? paneElement(target.tabId) : null);
  if (element) {
    const list = listBox(element);
    const landing =
      target.dropPoint && contains(list, target.dropPoint)
        ? target.dropPoint
        : boxCenter(list, 0.45);
    return { landing, target: list, landingScale: 0.5, ring: true };
  }
  const tab = target.tabId ? tabButton(target.tabId) : null;
  return tab
    ? { landing: boxCenter(boxOf(tab)), target: boxOf(tab), landingScale: 0.5, ring: true }
    : null;
}

interface Origin extends FileSide {
  path: string;
  /** The tab the files were sent from. */
  tabId?: string;
  /** Measured before they left, for files whose rows go away, as moved ones do. */
  point?: Point;
}

/** Where a file leaves from: its row, or that of the folder it was in, else the pane it left. */
function originOf(origin: Origin): Point | null {
  if (origin.point) return origin.point;
  const style = pathStyle(origin);
  const panes = panesOn(origin, origin.tabId);
  for (const pane of panes) {
    const element = paneElement(pane.tabId);
    const shown = pane.path();
    if (!element || !shown || !isWithin(origin.path, shown, style)) continue;
    const spot = rowFor(element, origin.path, style);
    if (spot) return boxCenter(spot.icon);
  }
  const sent = origin.tabId ? paneElement(origin.tabId) : null;
  if (sent) return boxCenter(listBox(sent), 0.4);
  const tab = origin.tabId ? tabButton(origin.tabId) : null;
  if (tab) return boxCenter(boxOf(tab));
  for (const pane of panes) {
    const element = paneElement(pane.tabId);
    if (element) return boxCenter(listBox(element), 0.4);
  }
  return null;
}

function addFlight(flight: Omit<Flight, "id">): void {
  const { flights } = useFlightStore.getState();
  if (flights.length >= MAX_FLIGHTS) return;
  useFlightStore.setState({ flights: [...flights, { id: nextFlightId++, ...flight }] });
}

function randomFor(): () => number {
  return seededRandom(nextFlightId * 7919 + Math.floor(performance.now()));
}

/** Plays a pile from `sources` to `landing` while the setting is off too, as previews do. */
export function playFlight(
  entries: readonly IconSubject[],
  sources: Point[],
  landing: Point,
  target: Box | null,
): void {
  if (document.hidden) return;
  const subjects = pileOf(entries);
  if (subjects.length === 0 || sources.length === 0) return;
  const plan = planFlight({ sources, landing, count: subjects.length, random: randomFor() });
  addFlight({
    icons: subjects.map((subject, index) => ({ subject, path: plan.icons[index] })),
    badge: entries.length > 1 ? { text: entries.length.toLocaleString(), path: plan.badge } : null,
    landing,
    target,
    ring: true,
    firstLanding: plan.firstLanding,
    total: plan.total,
  });
}

interface Arrival {
  subject: IconSubject;
  origin: Origin;
  target: Target;
  queuedAt: number;
}

interface ReadyArrival {
  subject: IconSubject;
  from: Point;
  destination: Destination;
}

const waiting: Arrival[] = [];
let pumpTimer: number | null = null;

/** Looks the icon up at the size it flies at while the file waits for its row. */
function prefetchIcon(subject: IconSubject): void {
  if (!useSettingsStore.getState().settings.appearance.systemIcons) return;
  const lookup = systemIconLookup(subject);
  if (!lookup) return;
  const size = Math.round(FLIGHT_ICON_LOOKUP_SIZE * (window.devicePixelRatio || 1));
  requestSystemIcon(iconCacheKey(lookup.key, size), size, lookup.request);
}

function queueArrival(arrival: Omit<Arrival, "queuedAt">): void {
  prefetchIcon(arrival.subject);
  waiting.push({ ...arrival, queuedAt: performance.now() });
  if (waiting.length > MAX_WAITING) waiting.splice(0, waiting.length - MAX_WAITING);
  pumpTimer ??= window.setTimeout(pump, PUMP_MILLIS);
}

function pump(): void {
  pumpTimer = null;
  if (!flying()) {
    waiting.length = 0;
    return;
  }
  const now = performance.now();
  const ready: ReadyArrival[] = [];
  for (let index = 0; index < waiting.length;) {
    const arrival = waiting[index];
    const destination = destinationOf(arrival.target, now - arrival.queuedAt >= ROW_WAIT_MILLIS);
    if (destination === "wait") {
      index++;
      continue;
    }
    waiting.splice(index, 1);
    const from = destination && originOf(arrival.origin);
    if (destination && from) ready.push({ subject: arrival.subject, from, destination });
  }
  launch(ready);
  if (waiting.length > 0) pumpTimer = window.setTimeout(pump, PUMP_MILLIS);
}

function launchOne({ subject, from, destination }: ReadyArrival, delay: number): void {
  const plan = planArrival({
    source: from,
    landing: destination.landing,
    landingScale: destination.landingScale,
    delay,
    random: randomFor(),
  });
  addFlight({
    icons: [{ subject, path: plan.path }],
    badge: null,
    landing: destination.landing,
    target: destination.target,
    ring: destination.ring,
    firstLanding: plan.firstLanding,
    total: plan.total,
  });
}

function boxKey(box: Box): string {
  return [box.left, box.top, box.width, box.height].map(Math.round).join(",");
}

/** Files ready together leave in turn; too many, and those headed to one place go as a pile. */
function launch(ready: ReadyArrival[]): void {
  if (ready.length === 0) return;
  if (ready.length <= MAX_SINGLES_AT_ONCE) {
    ready.forEach((arrival, index) => launchOne(arrival, index * STREAM_STEP_MILLIS));
    return;
  }
  const groups = new Map<string, ReadyArrival[]>();
  for (const arrival of ready) {
    const key = boxKey(arrival.destination.target);
    groups.set(key, [...(groups.get(key) ?? []), arrival]);
  }
  for (const group of groups.values()) {
    const [first] = group;
    if (group.length === 1) {
      launchOne(first, 0);
      continue;
    }
    const subjects = pileOf(group.map((arrival) => arrival.subject));
    const sources = group.slice(0, MAX_FLIGHT_ICONS).map((arrival) => arrival.from);
    const { landing, target } = first.destination;
    const plan = planFlight({ sources, landing, count: subjects.length, random: randomFor() });
    addFlight({
      icons: subjects.map((subject, index) => ({ subject, path: plan.icons[index] })),
      badge: { text: group.length.toLocaleString(), path: plan.badge },
      landing,
      target,
      ring: true,
      firstLanding: plan.firstLanding,
      total: plan.total,
    });
  }
}

/** Files queued from one pane to another, whose finished files fly as the queue reports them. */
export interface TransferWatch {
  direction: Direction;
  sessionId: string;
  sourceSessionId?: string;
  source: { tabId: string };
  target: { tabId: string; folder: string; dropPoint?: Point };
  /** What was queued, each a file or a folder whose files belong to it too. */
  items: { path: string; isDir: boolean }[];
}

const watches: (TransferWatch & { since: number })[] = [];

/** Watches a transfer about to be queued; the returned function forgets it if queuing fails. */
export function watchTransfer(watch: TransferWatch): () => void {
  if (!useSettingsStore.getState().settings.appearance.transferAnimation) return () => undefined;
  const watched = { ...watch, since: performance.now() };
  watches.push(watched);
  if (watches.length > MAX_WATCHES) watches.splice(0, watches.length - MAX_WATCHES);
  return () => {
    const index = watches.indexOf(watched);
    if (index >= 0) watches.splice(index, 1);
  };
}

function sidesOf(job: JobSnapshot): { from: FileSide; to: FileSide } {
  switch (job.direction) {
    case "upload":
      return { from: { kind: "local" }, to: { kind: "remote", sessionId: job.sessionId } };
    case "download":
      return { from: { kind: "remote", sessionId: job.sessionId }, to: { kind: "local" } };
    case "relay":
      return {
        from: { kind: "remote", sessionId: job.sourceSessionId },
        to: { kind: "remote", sessionId: job.sessionId },
      };
  }
}

function watchOf(job: JobSnapshot, from: FileSide, to: FileSide): TransferWatch | undefined {
  const sourceStyle = pathStyle(from);
  return watches.find(
    (watch) =>
      watch.direction === job.direction &&
      watch.sessionId === job.sessionId &&
      (job.direction !== "relay" || watch.sourceSessionId === job.sourceSessionId) &&
      isWithin(job.target, watch.target.folder, pathStyle(to)) &&
      watch.items.some((item) =>
        item.isDir
          ? isWithin(job.source, item.path, sourceStyle)
          : samePath(job.source, item.path, sourceStyle),
      ),
  );
}

/**
 * Flies each file the update reports done, of the transfers watched; `previous` is the queue as
 * it was before the update.
 */
export function noteTransferUpdate(previous: TransferSnapshot, update: TransferUpdate): void {
  if (watches.length === 0) return;
  if (flying()) {
    for (const job of update.jobs) {
      if (job.kind !== "file" || job.state !== "done") continue;
      if (previous.jobs.get(job.id)?.state === "done") continue;
      const { from, to } = sidesOf(job);
      const watch = watchOf(job, from, to);
      if (!watch) continue;
      queueArrival({
        subject: { name: job.name, kind: "file" },
        origin: { ...from, path: job.source, tabId: watch.source.tabId },
        target: {
          ...to,
          path: job.target,
          tabId: watch.target.tabId,
          dropPoint: watch.target.dropPoint,
        },
      });
    }
  }
  if (isSettled(update.stats)) {
    const now = performance.now();
    for (let index = watches.length - 1; index >= 0; index--) {
      if (now - watches[index].since > SETTLE_GRACE_MILLIS) watches.splice(index, 1);
    }
  }
}

/** Where each of `entries` is on screen now, for files whose rows go away before they fly. */
export function measureRows(
  tabId: string | undefined,
  side: FileSide,
  entries: readonly FileEntry[],
): Map<string, Point> {
  const points = new Map<string, Point>();
  if (!useSettingsStore.getState().settings.appearance.transferAnimation) return points;
  const style = pathStyle(side);
  const panes = panesOn(side, tabId);
  for (const entry of entries) {
    for (const pane of panes) {
      const element = paneElement(pane.tabId);
      const spot = element && rowFor(element, entry.path, style);
      if (spot?.own) {
        points.set(entry.path, boxCenter(spot.icon));
        break;
      }
    }
  }
  return points;
}

export interface PlacedFlight {
  side: FileSide;
  /** The pane the entries came from, and where their rows were before they moved. */
  source: { tabId?: string; folder: string; points: Map<string, Point> };
  target: { tabId: string; dropPoint?: Point };
  entries: readonly FileEntry[];
  /** Where each entry moved or copied in place is now. */
  placed: readonly string[];
}

/** Flies entries moved or copied in place to the rows they have now. */
export function flyPlaced({ side, source, target, entries, placed }: PlacedFlight): void {
  if (!flying()) return;
  const style = pathStyle(side);
  const unmatched = [...entries];
  for (const path of placed) {
    const name = baseName(path);
    const index = unmatched.findIndex((entry) => entry.name === name);
    const [entry] = index >= 0 ? unmatched.splice(index, 1) : unmatched.splice(0, 1);
    const origin = entry?.path ?? (isWithin(path, source.folder, style) ? path : source.folder);
    queueArrival({
      subject: entry ? { ...entry, name } : { name, kind: "file" },
      origin: { ...side, path: origin, tabId: source.tabId, point: source.points.get(origin) },
      target: { ...side, path, tabId: target.tabId, dropPoint: target.dropPoint },
    });
  }
}
