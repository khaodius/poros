// What the preview pane has already read or drawn, kept in memory within bounds: the least
// recently used leaves first. Nothing outlives the window.

export interface CacheBounds {
  maxEntries: number;
  /** The most all entries may weigh together, in whatever unit `set` is given. */
  maxWeight: number;
}

interface Kept<Value> {
  value: Value;
  weight: number;
}

export class BoundedCache<Value> {
  /** In order of use, the least recently used first. */
  private readonly entries = new Map<string, Kept<Value>>();
  private total = 0;

  /** `onEvict` is told of every value that leaves, to release what it holds. */
  constructor(
    private readonly bounds: CacheBounds,
    private readonly onEvict?: (value: Value) => void,
  ) {}

  get size(): number {
    return this.entries.size;
  }

  get weight(): number {
    return this.total;
  }

  has(key: string): boolean {
    return this.entries.has(key);
  }

  get(key: string): Value | undefined {
    const kept = this.entries.get(key);
    if (!kept) return undefined;
    this.entries.delete(key);
    this.entries.set(key, kept);
    return kept.value;
  }

  /** Keeps `value`, unless it alone weighs more than the cache may hold. */
  set(key: string, value: Value, weight = 1): void {
    this.delete(key);
    if (weight > this.bounds.maxWeight) {
      this.onEvict?.(value);
      return;
    }
    this.entries.set(key, { value, weight });
    this.total += weight;
    for (const oldest of this.entries.keys()) {
      const within =
        this.entries.size <= this.bounds.maxEntries && this.total <= this.bounds.maxWeight;
      if (within) break;
      this.delete(oldest);
    }
  }

  delete(key: string): void {
    const kept = this.entries.get(key);
    if (!kept) return;
    this.entries.delete(key);
    this.total -= kept.weight;
    this.onEvict?.(kept.value);
  }

  /** Drops every entry whose key starts with `prefix`. */
  deletePrefixed(prefix: string): void {
    for (const key of [...this.entries.keys()]) {
      if (key.startsWith(prefix)) this.delete(key);
    }
  }

  clear(): void {
    for (const key of [...this.entries.keys()]) this.delete(key);
  }
}

/** The parts of a listed file that change when its contents do. */
export interface ListedFile {
  path: string;
  size: number;
  modified: number | null;
}

/**
 * Names what was read of a file as its folder listed it, on one connection. A file that
 * changed has another size or time, so another key: nothing stale is ever shown for it.
 */
export function previewKey(connection: string, file: ListedFile, variant = ""): string {
  return JSON.stringify([connection, file.path, file.size, file.modified, variant]);
}

/** How every key `previewKey` makes for `connection` starts, and no key of another one does. */
export function connectionPrefix(connection: string): string {
  return `${JSON.stringify([connection]).slice(0, -1)},`;
}
