// Rows kept hidden while the files they show are still flying to them, so a file's row appears
// the moment its icon lands rather than when the pane reloads. A row is held from when its file
// is known to be on its way, and shows again when the file lands, or at its deadline should the
// file never fly, so no row stays hidden for good. Pure, so it is testable;
// `state/fileFlight.ts` keeps one and publishes what it holds.

import type { PathStyle } from "./fileSource";
import { normalizedPath } from "./path";

/** Names a row on one side, the same however its path is spelled. */
export function rowKey(
  kind: "local" | "remote",
  sessionId: string | undefined,
  path: string,
  style: PathStyle,
): string {
  const side = kind === "remote" ? `remote:${sessionId ?? ""}` : "local";
  return `${side}\n${normalizedPath(path, style)}`;
}

export class RowHolds {
  /** Each held row and when it shows anyway: null while its file is still being transferred. */
  private readonly deadlines = new Map<string, number | null>();

  constructor(private readonly limit: number) {}

  get size(): number {
    return this.deadlines.size;
  }

  has(key: string): boolean {
    return this.deadlines.has(key);
  }

  /** Whether the row is held only until its transfer ends, its file not yet on its way. */
  isOpen(key: string): boolean {
    return this.deadlines.get(key) === null;
  }

  /**
   * Holds the row until `until`, or until its transfer ends when null; returns whether the rows
   * held changed. Past the limit, the rows held longest show again.
   */
  hold(key: string, until: number | null): boolean {
    const added = !this.deadlines.has(key);
    this.deadlines.delete(key);
    this.deadlines.set(key, until);
    for (const oldest of this.deadlines.keys()) {
      if (this.deadlines.size <= this.limit) break;
      this.deadlines.delete(oldest);
    }
    return added;
  }

  /** Shows the rows again; returns whether any was held. */
  release(keys: Iterable<string | null>): boolean {
    let changed = false;
    for (const key of keys) {
      if (key !== null && this.deadlines.delete(key)) changed = true;
    }
    return changed;
  }

  /** Shows those of the rows whose files are not on their way, as when their transfer ends. */
  releaseOpen(keys: Iterable<string>): boolean {
    let changed = false;
    for (const key of keys) {
      if (this.isOpen(key)) changed = this.deadlines.delete(key) || changed;
    }
    return changed;
  }

  /** Shows the rows whose deadline is past. */
  expire(now: number): boolean {
    let changed = false;
    for (const [key, until] of this.deadlines) {
      if (until !== null && until <= now) changed = this.deadlines.delete(key) || changed;
    }
    return changed;
  }

  clear(): boolean {
    const changed = this.deadlines.size > 0;
    this.deadlines.clear();
    return changed;
  }

  /** The soonest deadline, null when no held row has one. */
  nextDeadline(): number | null {
    let soonest: number | null = null;
    for (const until of this.deadlines.values()) {
      if (until !== null && (soonest === null || until < soonest)) soonest = until;
    }
    return soonest;
  }

  keys(): ReadonlySet<string> {
    return new Set(this.deadlines.keys());
  }
}
