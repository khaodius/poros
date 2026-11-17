import { describe, expect, it } from "vitest";
import {
  MAX_FLIGHT_ICONS,
  heapOffsets,
  intersects,
  pileOf,
  planArrival,
  planFlight,
  seededRandom,
  toKeyframe,
} from "./fileFlight";
import type { IconSubject } from "./systemIcons";

const file = (name: string): IconSubject => ({ name, kind: "file" });

describe("pileOf", () => {
  it("is empty for nothing", () => {
    expect(pileOf([])).toEqual([]);
  });

  it("makes a pile of copies from a file or two", () => {
    expect(pileOf([file("a.jpg")]).map((icon) => icon.name)).toEqual([
      "a.jpg",
      "a.jpg",
      "a.jpg",
      "a.jpg",
    ]);
    expect(pileOf([file("a.jpg"), file("b.txt")]).map((icon) => icon.name)).toEqual([
      "a.jpg",
      "b.txt",
      "a.jpg",
      "b.txt",
    ]);
  });

  it("lets folders spill plain files", () => {
    const pile = pileOf([{ name: "Photos", kind: "dir" }]);
    expect(pile).toHaveLength(8);
    expect(pile[0].kind).toBe("dir");
    expect(pile.slice(1).every((icon) => icon.kind === "file")).toBe(true);
  });

  it("spreads over every entry when there are more than fit", () => {
    const entries = Array.from({ length: 240 }, (_, index) => file(`${index}.txt`));
    const pile = pileOf(entries);
    expect(pile).toHaveLength(MAX_FLIGHT_ICONS);
    expect(pile[0].name).toBe("0.txt");
    expect(pile[MAX_FLIGHT_ICONS - 1].name).toBe("230.txt");
  });
});

describe("heapOffsets", () => {
  it("piles rows that narrow toward the top", () => {
    const offsets = heapOffsets(10, () => 0.5);
    expect(offsets).toHaveLength(10);
    const rows = [...new Set(offsets.map((offset) => offset.y))];
    expect(rows).toHaveLength(4);
    const counts = rows.map((y) => offsets.filter((offset) => offset.y === y).length);
    expect(counts).toEqual([4, 3, 2, 1]);
    // The bottom row is lowest on screen, and each row is centered.
    expect(rows[0]).toBeGreaterThan(rows[rows.length - 1]);
    const bottom = offsets.slice(0, 4).map((offset) => offset.x);
    expect(bottom.reduce((sum, x) => sum + x, 0)).toBeCloseTo(0);
  });

  it("puts a single icon in the middle", () => {
    expect(heapOffsets(1, () => 0.5)).toEqual([{ x: 0, y: 0 }]);
  });
});

describe("planFlight", () => {
  const plan = planFlight({
    sources: [
      { x: 100, y: 200 },
      { x: 100, y: 224 },
    ],
    landing: { x: 900, y: 300 },
    count: 6,
    random: seededRandom(42),
  });

  it("is the same for the same seed", () => {
    const again = planFlight({
      sources: [
        { x: 100, y: 200 },
        { x: 100, y: 224 },
      ],
      landing: { x: 900, y: 300 },
      count: 6,
      random: seededRandom(42),
    });
    expect(again).toEqual(plan);
  });

  it("starts each icon on its row and leaves it near the landing", () => {
    expect(plan.icons).toHaveLength(6);
    for (const [index, icon] of plan.icons.entries()) {
      const first = icon.frames[0];
      const last = icon.frames[icon.frames.length - 1];
      expect(first).toMatchObject({ offset: 0, opacity: 0, x: 100, y: index % 2 ? 224 : 200 });
      expect(last.offset).toBe(1);
      expect(last.opacity).toBe(0);
      expect(Math.hypot(last.x - 900, last.y - 300)).toBeLessThan(40);
    }
  });

  it("keeps frames in order and arcs above both ends", () => {
    for (const icon of plan.icons) {
      const offsets = icon.frames.map((frame) => frame.offset);
      expect([...offsets].sort((left, right) => left - right)).toEqual(offsets);
      expect(Math.min(...icon.frames.map((frame) => frame.y))).toBeLessThan(200);
    }
  });

  it("staggers the icons and ends after the last one", () => {
    const delays = plan.icons.map((icon) => icon.delay);
    expect(delays[0]).toBe(0);
    expect([...delays].sort((left, right) => left - right)).toEqual(delays);
    expect(plan.total).toBeGreaterThanOrEqual(
      Math.max(...plan.icons.map((icon) => icon.delay + icon.duration)),
    );
    expect(plan.firstLanding).toBeGreaterThan(0);
    expect(plan.firstLanding).toBeLessThan(plan.total);
    expect(plan.reveal).toBeLessThan(plan.firstLanding);
    expect(plan.reveal).toBeGreaterThan(plan.firstLanding - 80);
  });

  it("launches every icon within a moment, however many there are", () => {
    const big = planFlight({
      sources: [{ x: 0, y: 0 }],
      landing: { x: 500, y: 0 },
      count: MAX_FLIGHT_ICONS,
      random: seededRandom(1),
    });
    expect(big.icons[MAX_FLIGHT_ICONS - 1].delay).toBeLessThanOrEqual(360);
  });
});

describe("planArrival", () => {
  const arrival = planArrival({
    source: { x: 120, y: 300 },
    landing: { x: 860, y: 180 },
    landingScale: 16 / 24,
    delay: 140,
    random: seededRandom(7),
  });
  const { frames } = arrival.path;
  const last = frames[frames.length - 1];

  it("leaves from the file's row and settles into the icon of the row it lands on", () => {
    expect(frames[0]).toMatchObject({ offset: 0, x: 120, y: 300, opacity: 0 });
    expect(last).toMatchObject({ offset: 1, x: 860, y: 180, opacity: 0 });
    expect(last.scaleX).toBeCloseTo(16 / 24);
    expect(last.scaleY).toBeCloseTo(16 / 24);
  });

  it("turns once and lands upright, without turning back", () => {
    const landed = frames.filter((frame) => frame.x === 860 && frame.y === 180);
    for (const frame of landed) expect(Math.abs(frame.rotate % 360)).toBeCloseTo(0);
    expect(new Set(landed.map((frame) => frame.rotate)).size).toBe(1);
  });

  it("waits its turn and is quicker than a pile", () => {
    expect(arrival.path.delay).toBe(140);
    expect(arrival.total).toBe(140 + arrival.path.duration);
    expect(arrival.path.duration).toBeLessThanOrEqual(900);
    expect(arrival.firstLanding).toBeGreaterThan(140);
    expect(arrival.firstLanding).toBeLessThan(arrival.total);
  });

  it("shows its row as it touches down, all but there and not yet landed", () => {
    const offset = (arrival.reveal - arrival.path.delay) / arrival.path.duration;
    const after = frames.findIndex((frame) => frame.offset >= offset);
    const [from, to] = [frames[after - 1], frames[after]];
    const through = (offset - from.offset) / (to.offset - from.offset);
    const x = from.x + (to.x - from.x) * through;
    const y = from.y + (to.y - from.y) * through;
    expect(arrival.reveal).toBeLessThan(arrival.firstLanding);
    expect(Math.hypot(860 - x, 180 - y)).toBeLessThan(6);
    expect(Math.hypot(860 - x, 180 - y)).toBeGreaterThan(0);
  });
});

describe("toKeyframe", () => {
  it("moves the icon's center to the frame's point", () => {
    expect(
      toKeyframe(
        { offset: 0.5, x: 30, y: 40, rotate: 12.345, scaleX: 1.3, scaleY: 0.7, opacity: 1 },
        20,
      ),
    ).toEqual({
      offset: 0.5,
      opacity: 1,
      transform: "translate(20px, 30px) rotate(12.35deg) scale(1.3, 0.7)",
    });
  });
});

describe("intersects", () => {
  const list = { left: 0, top: 100, width: 300, height: 400 };

  it("tells rows on screen from rows scrolled away", () => {
    expect(intersects({ left: 10, top: 120, width: 20, height: 20 }, list)).toBe(true);
    expect(intersects({ left: 10, top: 520, width: 20, height: 20 }, list)).toBe(false);
    expect(intersects({ left: 10, top: 120, width: 0, height: 0 }, list)).toBe(false);
  });
});
