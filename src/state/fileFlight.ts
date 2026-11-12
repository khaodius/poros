// Starts the flying pile of file icons for files sent, moved or copied between panes, when
// Settings > Appearance turns it on. It measures where the files are and where they go, and
// hands the planned paths to `components/FileFlights.tsx`.

import { create } from "zustand";
import {
  boxCenter,
  intersects,
  MAX_FLIGHT_ICONS,
  pileOf,
  planFlight,
  seededRandom,
  type Box,
  type IconPath,
  type Point,
} from "../lib/fileFlight";
import type { IconSubject } from "../lib/systemIcons";
import type { FileEntry } from "../lib/types";
import { getPane, panesShowing } from "./paneRegistry";
import { useSettingsStore } from "./settingsStore";

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
  firstLanding: number;
  total: number;
}

interface FlightState {
  flights: Flight[];
}

export const useFlightStore = create<FlightState>(() => ({ flights: [] }));

const MAX_FLIGHTS = 3;
let nextFlightId = 1;

export function endFlight(id: number): void {
  useFlightStore.setState(({ flights }) => ({
    flights: flights.filter((flight) => flight.id !== id),
  }));
}

export function prefersLessMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
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

/** The part of a pane that lists files, where piles land and leave from. */
function listBox(pane: HTMLElement): Box {
  return boxOf(pane.querySelector(".file-list-body") ?? pane);
}

/** The icon of a row the pane shows on screen, else nothing. */
function rowIcon(pane: HTMLElement, selector: string): Box | null {
  const row = pane.querySelector(selector);
  if (!row) return null;
  const box = boxOf(row.querySelector(".file-icon") ?? row);
  return intersects(box, listBox(pane)) ? box : null;
}

/** Where the files start: their rows' icons, or the middle of their pane or its tab. */
function sourcePoints(tabId: string, entries: readonly FileEntry[]): Point[] | null {
  const pane = paneElement(tabId);
  if (!pane) {
    const tab = tabButton(tabId);
    return tab ? [boxCenter(boxOf(tab))] : null;
  }
  const points: Point[] = [];
  for (const entry of entries.slice(0, MAX_FLIGHT_ICONS)) {
    const icon = rowIcon(pane, `[data-path="${CSS.escape(entry.path)}"]`);
    if (icon) points.push(boxCenter(icon));
  }
  return points.length > 0 ? points : [boxCenter(listBox(pane), 0.4)];
}

export interface FlightRequest {
  /** The pane the files are in: a tab, or for the clipboard the folder they were cut from. */
  source: { tabId: string; kind: "local" | "remote"; sessionId?: string; folder: string | null };
  target: { tabId: string; folder: string };
  entries: readonly FileEntry[];
  /** Where a drag let go of them. */
  landing?: Point;
}

function sourceTab(source: FlightRequest["source"]): string | null {
  if (getPane(source.tabId)) return source.tabId;
  if (!source.folder) return null;
  const showing = panesShowing(source.kind, source.folder, source.sessionId);
  return (showing.find((pane) => pane.visible) ?? showing[0])?.tabId ?? null;
}

/** Where the pile lands and what lights up: the folder's row, the pane, or the pane's tab. */
function landingFor(request: FlightRequest): { landing: Point; target: Box } | null {
  const { tabId, folder } = request.target;
  const pane = paneElement(tabId);
  if (pane) {
    const shown = getPane(tabId)?.path();
    const row =
      folder !== shown
        ? pane.querySelector<HTMLElement>(`[data-drop-folder="${CSS.escape(folder)}"]`)
        : null;
    const rowBox = row ? boxOf(row) : null;
    const onRow = rowBox && intersects(rowBox, listBox(pane)) ? rowBox : null;
    const icon = onRow && row ? boxOf(row.querySelector(".file-icon") ?? row) : null;
    return {
      landing: request.landing ?? (icon ? boxCenter(icon) : boxCenter(listBox(pane), 0.45)),
      target: onRow ?? listBox(pane),
    };
  }
  const tab = tabButton(tabId);
  return tab ? { landing: boxCenter(boxOf(tab)), target: boxOf(tab) } : null;
}

/** Plays a flight from `sources` to `landing`, whatever the settings say, as previews do. */
export function playFlight(
  entries: readonly IconSubject[],
  sources: Point[],
  landing: Point,
  target: Box | null,
): void {
  const { flights } = useFlightStore.getState();
  if (flights.length >= MAX_FLIGHTS || document.hidden || prefersLessMotion()) return;
  const subjects = pileOf(entries);
  if (subjects.length === 0) return;
  const id = nextFlightId++;
  const plan = planFlight({
    sources,
    landing,
    count: subjects.length,
    random: seededRandom(id * 7919 + Math.floor(performance.now())),
  });
  const flight: Flight = {
    id,
    icons: subjects.map((subject, index) => ({ subject, path: plan.icons[index] })),
    badge: entries.length > 1 ? { text: entries.length.toLocaleString(), path: plan.badge } : null,
    landing,
    target,
    firstLanding: plan.firstLanding,
    total: plan.total,
  };
  useFlightStore.setState({ flights: [...flights, flight] });
}

/** Sends the pile across for files a transfer, move or copy has just taken on. */
export function launchFileFlight(request: FlightRequest): void {
  if (!useSettingsStore.getState().settings.appearance.transferAnimation) return;
  if (request.entries.length === 0) return;
  const tabId = sourceTab(request.source);
  const sources = tabId ? sourcePoints(tabId, request.entries) : null;
  const destination = landingFor(request);
  if (!sources || !destination) return;
  playFlight(request.entries, sources, destination.landing, destination.target);
}
