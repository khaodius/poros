import { describe, expect, it } from "vitest";
import { BoundedCache, connectionPrefix, previewKey } from "./previewCache";

describe("BoundedCache", () => {
  it("keeps no more entries than its bound, letting the least recently used go", () => {
    const evicted: string[] = [];
    const cache = new BoundedCache<string>({ maxEntries: 3, maxWeight: 100 }, (value) =>
      evicted.push(value),
    );
    cache.set("a", "first");
    cache.set("b", "second");
    cache.set("c", "third");
    // Reading one makes it the most recently used.
    expect(cache.get("a")).toBe("first");
    cache.set("d", "fourth");
    expect(evicted).toEqual(["second"]);
    expect(cache.has("b")).toBe(false);
    expect(cache.size).toBe(3);
    cache.set("e", "fifth");
    expect(evicted).toEqual(["second", "third"]);
    expect(cache.get("a")).toBe("first");
  });

  it("keeps no more weight than its bound", () => {
    const evicted: string[] = [];
    const cache = new BoundedCache<string>({ maxEntries: 100, maxWeight: 10 }, (value) =>
      evicted.push(value),
    );
    cache.set("a", "four", 4);
    cache.set("b", "four more", 4);
    expect(cache.weight).toBe(8);
    cache.set("c", "five", 5);
    expect(evicted).toEqual(["four"]);
    expect(cache.weight).toBe(9);
    // One heavy entry pushes out as many as it takes.
    cache.set("d", "ten", 10);
    expect(evicted).toEqual(["four", "four more", "five"]);
    expect(cache.size).toBe(1);
    expect(cache.weight).toBe(10);
  });

  it("does not keep what alone weighs more than the bound, and releases it", () => {
    const evicted: string[] = [];
    const cache = new BoundedCache<string>({ maxEntries: 5, maxWeight: 10 }, (value) =>
      evicted.push(value),
    );
    cache.set("small", "kept", 3);
    cache.set("huge", "too heavy", 11);
    expect(cache.has("huge")).toBe(false);
    expect(evicted).toEqual(["too heavy"]);
    expect(cache.get("small")).toBe("kept");
    expect(cache.weight).toBe(3);
  });

  it("releases a value when its key is set again, deleted or the cache is cleared", () => {
    const evicted: string[] = [];
    const cache = new BoundedCache<string>({ maxEntries: 5, maxWeight: 50 }, (value) =>
      evicted.push(value),
    );
    cache.set("a", "old", 5);
    cache.set("a", "new", 7);
    expect(evicted).toEqual(["old"]);
    expect(cache.weight).toBe(7);
    cache.delete("a");
    cache.delete("a");
    expect(evicted).toEqual(["old", "new"]);
    cache.set("b", "one");
    cache.set("c", "two");
    cache.clear();
    expect(evicted).toEqual(["old", "new", "one", "two"]);
    expect(cache.size).toBe(0);
    expect(cache.weight).toBe(0);
    expect(cache.get("b")).toBeUndefined();
  });

  it("drops the entries of one connection only, releasing each", () => {
    const evicted: string[] = [];
    const cache = new BoundedCache<string>({ maxEntries: 10, maxWeight: 100 }, (value) =>
      evicted.push(value),
    );
    const file = { path: "/vault/diary.txt", size: 10, modified: 1 };
    cache.set(previewKey("remote:vault", file), "cleartext", 4);
    cache.set(previewKey("remote:vault", file, "thumbnail:96"), "thumbnail", 2);
    // One session's id may start with another's.
    cache.set(previewKey("remote:vault2", file), "other session", 3);
    cache.set(previewKey("local", file), "this computer", 1);
    cache.deletePrefixed(connectionPrefix("remote:vault"));
    expect(evicted).toEqual(["cleartext", "thumbnail"]);
    expect(cache.size).toBe(2);
    expect(cache.weight).toBe(4);
    expect(cache.get(previewKey("remote:vault2", file))).toBe("other session");
  });
});

describe("previewKey", () => {
  const file = { path: "/srv/www/logo.png", size: 2048, modified: 1_700_000_000 };

  it("is another key once the file's size or time changes, so nothing stale is found", () => {
    const key = previewKey("remote:one", file);
    expect(previewKey("remote:one", { ...file })).toBe(key);
    expect(previewKey("remote:one", { ...file, size: 2049 })).not.toBe(key);
    expect(previewKey("remote:one", { ...file, modified: 1_700_000_001 })).not.toBe(key);
    expect(previewKey("remote:one", { ...file, modified: null })).not.toBe(key);
  });

  it("tells connections, paths and variants apart", () => {
    const key = previewKey("remote:one", file);
    expect(previewKey("remote:two", file)).not.toBe(key);
    expect(previewKey("local", file)).not.toBe(key);
    expect(previewKey("remote:one", { ...file, path: "/srv/www/Logo.png" })).not.toBe(key);
    expect(previewKey("remote:one", file, "thumbnail:96")).not.toBe(key);
    // Names may hold anything, line breaks included, without two files sharing a key.
    const first = previewKey("local", { path: "/a\n1", size: 2, modified: 3 });
    const second = previewKey("local", { path: "/a", size: 1, modified: 2 }, "3");
    expect(first).not.toBe(second);
  });

  it("starts with its connection's prefix, which no other connection's keys start with", () => {
    for (const connection of ["local", "remote:a", 'remote:"quoted"', "remote:a\\b"]) {
      expect(previewKey(connection, file).startsWith(connectionPrefix(connection))).toBe(true);
      expect(previewKey(`${connection}x`, file).startsWith(connectionPrefix(connection))).toBe(
        false,
      );
    }
  });
});
