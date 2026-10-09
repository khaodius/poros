import { windows } from "./ipc";
import type { WindowCorners } from "./types";

/** Windows 11 rounds corners by 4 or 8 pixels; this picks the nearer to the app's radius. */
export function cornersFor(radius: number): WindowCorners {
  if (radius <= 0) return "square";
  return radius < 6 ? "small" : "round";
}

let applied: WindowCorners | null = null;

/** Rounds this window's corners like the app's own, where the system draws them. */
export function matchWindowCorners(): void {
  const radius = parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue("--radius"),
  );
  const corners = cornersFor(Number.isFinite(radius) ? radius : 6);
  if (corners === applied) return;
  applied = corners;
  void windows.setCorners(corners).catch(() => undefined);
}
