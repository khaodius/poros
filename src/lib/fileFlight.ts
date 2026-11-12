// Plans the pile of file icons that flies from one pane to another when files are sent, moved or
// copied: each icon pops out of its row, joins a heap beside the files, arcs across the window
// and lands on a heap where the files go, then sinks into it. Pure, so the paths are testable;
// `components/FileFlights.tsx` plays them.

import type { IconSubject } from "./systemIcons";

export interface Point {
  x: number;
  y: number;
}

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** One moment of an icon's flight; positions are the icon's center in window coordinates. */
export interface FlightFrame {
  /** From 0 to 1 through the icon's own duration. */
  offset: number;
  x: number;
  y: number;
  rotate: number;
  scaleX: number;
  scaleY: number;
  opacity: number;
}

export interface IconPath {
  frames: FlightFrame[];
  /** Milliseconds after the flight starts. */
  delay: number;
  duration: number;
}

export const MAX_FLIGHT_ICONS = 24;
/** A file or two still makes a pile, of copies of their icons. */
const MIN_PILE = 4;
/** Folders spill some of what they hold, as plain file icons. */
const FOLDER_PILE = 8;
/** All icons have left within this, however many there are. */
const MAX_STAGGER_MILLIS = 360;
const MAX_STAGGER_STEP_MILLIS = 34;
const SPACING = { across: 13, up: 8 };
/** The heap that forms beside the files before they leave is tighter than the one they land in. */
const GATHER_SCALE = 0.7;
const ARC_STEPS = 10;

/** Where each phase ends, through an icon's duration. */
const PHASES = {
  popped: 0.1,
  gathered: 0.24,
  landed: 0.72,
  squashed: 0.78,
  rebound: 0.84,
  settled: 0.9,
};

const SPILLED_FILE: IconSubject = { name: "file", kind: "file" };

/** A small, seeded generator, so a flight's scatter can be reproduced. */
export function seededRandom(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let mixed = state;
    mixed = Math.imul(mixed ^ (mixed >>> 15), mixed | 1);
    mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), mixed | 61);
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * The icons the pile is made of: the entries' own, spread evenly over them when there are more
 * than fit. A pile of fewer is filled out with copies, and folders add plain files.
 */
export function pileOf(entries: readonly IconSubject[]): IconSubject[] {
  if (entries.length === 0) return [];
  const picked =
    entries.length <= MAX_FLIGHT_ICONS
      ? [...entries]
      : Array.from(
          { length: MAX_FLIGHT_ICONS },
          (_, index) => entries[Math.floor((index * entries.length) / MAX_FLIGHT_ICONS)],
        );
  const holdsFolders = picked.some(
    (entry) => entry.kind === "dir" || (entry.kind === "symlink" && entry.linkTarget === "dir"),
  );
  const wanted = holdsFolders ? FOLDER_PILE : MIN_PILE;
  const pile = [...picked];
  for (let index = 0; pile.length < wanted; index++) {
    pile.push(holdsFolders ? SPILLED_FILE : picked[index % picked.length]);
  }
  return pile;
}

/**
 * Offsets of a heap of `count` icons from its center, bottom row first: each row holds one fewer
 * than the row under it, like a pile of paper.
 */
export function heapOffsets(count: number, random: () => number): Point[] {
  let base = 1;
  while ((base * (base + 1)) / 2 < count) base++;
  const rowSizes: number[] = [];
  for (let left = count; left > 0; left -= rowSizes[rowSizes.length - 1]) {
    rowSizes.push(Math.min(Math.max(1, base - rowSizes.length), left));
  }
  const offsets: Point[] = [];
  rowSizes.forEach((inRow, row) => {
    for (let slot = 0; slot < inRow; slot++) {
      offsets.push({
        x: (slot - (inRow - 1) / 2) * SPACING.across + (random() - 0.5) * 6,
        y: ((rowSizes.length - 1) / 2 - row) * SPACING.up + (random() - 0.5) * 4,
      });
    }
  });
  return offsets;
}

function easeInOut(progress: number): number {
  return progress < 0.5 ? 4 * progress ** 3 : 1 - (-2 * progress + 2) ** 3 / 2;
}

function quadratic(from: Point, control: Point, to: Point, progress: number): Point {
  const rest = 1 - progress;
  return {
    x: rest * rest * from.x + 2 * rest * progress * control.x + progress * progress * to.x,
    y: rest * rest * from.y + 2 * rest * progress * control.y + progress * progress * to.y,
  };
}

function centerOf(points: readonly Point[]): Point {
  const sum = points.reduce((total, point) => ({ x: total.x + point.x, y: total.y + point.y }), {
    x: 0,
    y: 0,
  });
  return { x: sum.x / points.length, y: sum.y / points.length };
}

export interface FlightPlanInput {
  /** Where each icon starts, such as its file's row; fewer than the icons are reused in turn. */
  sources: readonly Point[];
  landing: Point;
  count: number;
  random: () => number;
}

/** Every icon's path, plus one for the count badge riding at the heap's middle. */
export function planFlight({ sources, landing, count, random }: FlightPlanInput): {
  icons: IconPath[];
  badge: IconPath;
  /** When the first icon lands, from the start. */
  firstLanding: number;
  total: number;
} {
  const origin = centerOf(sources);
  const distance = Math.hypot(landing.x - origin.x, landing.y - origin.y);
  const lift = Math.min(220, Math.max(60, distance * 0.35));
  const baseDuration = Math.min(1300, Math.max(900, 900 + (distance / 2000) * 400));
  const step = Math.min(MAX_STAGGER_STEP_MILLIS, MAX_STAGGER_MILLIS / Math.max(1, count));
  const landingHeap = heapOffsets(count, random);
  // The heap leans the way the files travel.
  const lean = Math.sign(landing.x - origin.x) || 1;

  const pathFor = (
    start: Point,
    gathered: Point,
    landed: Point,
    spin: number,
    tilt: number,
  ): FlightFrame[] => {
    const control = {
      x: (gathered.x + landed.x) / 2,
      y: Math.min(gathered.y, landed.y) - lift,
    };
    const frames: FlightFrame[] = [
      { offset: 0, ...start, rotate: 0, scaleX: 0.4, scaleY: 0.4, opacity: 0 },
      { offset: PHASES.popped, ...start, rotate: tilt / 3, scaleX: 1.1, scaleY: 1.1, opacity: 1 },
      { offset: PHASES.gathered, ...gathered, rotate: tilt, scaleX: 1, scaleY: 1, opacity: 1 },
    ];
    for (let index = 1; index <= ARC_STEPS; index++) {
      const progress = easeInOut(index / ARC_STEPS);
      const swell = Math.sin(progress * Math.PI) * 0.3;
      frames.push({
        offset: PHASES.gathered + ((PHASES.landed - PHASES.gathered) * index) / ARC_STEPS,
        ...quadratic(gathered, control, landed, progress),
        rotate: tilt + spin * progress,
        scaleX: 1 + swell,
        scaleY: 1 + swell,
        opacity: 1,
      });
    }
    const resting = tilt + spin;
    frames.push(
      { offset: PHASES.squashed, ...landed, rotate: resting, scaleX: 1.3, scaleY: 0.7, opacity: 1 },
      {
        offset: PHASES.rebound,
        x: landed.x,
        y: landed.y - 3,
        rotate: resting,
        scaleX: 0.92,
        scaleY: 1.12,
        opacity: 1,
      },
      { offset: PHASES.settled, ...landed, rotate: resting, scaleX: 1, scaleY: 1, opacity: 1 },
      {
        offset: 1,
        x: landing.x + (landed.x - landing.x) * 0.3,
        y: landing.y + (landed.y - landing.y) * 0.3 + 4,
        rotate: resting,
        scaleX: 0.25,
        scaleY: 0.25,
        opacity: 0,
      },
    );
    return frames;
  };

  const icons = landingHeap.map((offset, index): IconPath => {
    const start = sources[index % sources.length];
    const gathered = {
      x: origin.x + offset.x * GATHER_SCALE,
      y: origin.y + offset.y * GATHER_SCALE - 14,
    };
    const landed = { x: landing.x + offset.x, y: landing.y + offset.y };
    const tilt = (random() - 0.5) * 50;
    const spin = lean * (180 + random() * 180) * (random() < 0.25 ? -1 : 1);
    return {
      frames: pathFor(start, gathered, landed, spin, tilt),
      delay: Math.round(index * step),
      duration: Math.round(baseDuration + (random() - 0.5) * 120),
    };
  });

  const top = Math.min(0, ...landingHeap.map((offset) => offset.y)) - 18;
  const badge: IconPath = {
    frames: pathFor(
      origin,
      { x: origin.x, y: origin.y + top },
      { x: landing.x, y: landing.y + top },
      0,
      0,
    ),
    delay: Math.round(((count - 1) * step) / 2),
    duration: Math.round(baseDuration),
  };

  const firstLanding = icons.length > 0 ? icons[0].delay + icons[0].duration * PHASES.landed : 0;
  const total = Math.max(
    badge.delay + badge.duration,
    ...icons.map((icon) => icon.delay + icon.duration),
  );
  return { icons, badge, firstLanding: Math.round(firstLanding), total };
}

/** A frame as a Web Animations keyframe; `size` is the icon's, to move its center. */
export function toKeyframe(frame: FlightFrame, size: number): Keyframe {
  const half = size / 2;
  const round = (value: number) => Math.round(value * 100) / 100;
  return {
    offset: frame.offset,
    opacity: frame.opacity,
    transform:
      `translate(${round(frame.x - half)}px, ${round(frame.y - half)}px) ` +
      `rotate(${round(frame.rotate)}deg) scale(${round(frame.scaleX)}, ${round(frame.scaleY)})`,
  };
}

/** The middle of a box, nudged up a little where a pile looks settled. */
export function boxCenter(box: Box, height = 0.5): Point {
  return { x: box.left + box.width / 2, y: box.top + box.height * height };
}

export function intersects(inner: Box, outer: Box): boolean {
  return (
    inner.width > 0 &&
    inner.height > 0 &&
    inner.left < outer.left + outer.width &&
    inner.left + inner.width > outer.left &&
    inner.top < outer.top + outer.height &&
    inner.top + inner.height > outer.top
  );
}
