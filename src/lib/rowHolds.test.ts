import { describe, expect, it } from "vitest";
import { RowHolds, rowKey } from "./rowHolds";

describe("rowKey", () => {
  it("is one key however a path is spelled on its side", () => {
    expect(rowKey("local", undefined, "C:\\Data\\A.txt", "windows")).toBe(
      rowKey("local", undefined, "c:/data/a.txt", "windows"),
    );
    expect(rowKey("remote", "one", "/srv/a.txt/", "posix")).toBe(
      rowKey("remote", "one", "/srv/a.txt", "posix"),
    );
  });

  it("tells sides and servers apart, and case on POSIX", () => {
    const remote = rowKey("remote", "one", "/srv/a.txt", "posix");
    expect(rowKey("remote", "two", "/srv/a.txt", "posix")).not.toBe(remote);
    expect(rowKey("local", undefined, "/srv/a.txt", "posix")).not.toBe(remote);
    expect(rowKey("remote", "one", "/srv/A.txt", "posix")).not.toBe(remote);
  });
});

describe("RowHolds", () => {
  it("reports a change only when the rows held change", () => {
    const holds = new RowHolds(10);
    expect(holds.hold("a", null)).toBe(true);
    expect(holds.hold("a", 500)).toBe(false);
    expect(holds.has("a")).toBe(true);
    expect(holds.release(["b", null])).toBe(false);
    expect(holds.release(["a"])).toBe(true);
    expect(holds.size).toBe(0);
  });

  it("keeps rows waiting on their transfer past any deadline", () => {
    const holds = new RowHolds(10);
    holds.hold("waiting", null);
    holds.hold("flying", 1000);
    expect(holds.nextDeadline()).toBe(1000);
    expect(holds.expire(999)).toBe(false);
    expect(holds.expire(1000)).toBe(true);
    expect([...holds.keys()]).toEqual(["waiting"]);
    expect(holds.nextDeadline()).toBeNull();
  });

  it("gives a waiting row a deadline once its file is on its way", () => {
    const holds = new RowHolds(10);
    holds.hold("a", null);
    expect(holds.isOpen("a")).toBe(true);
    holds.hold("a", 2000);
    expect(holds.isOpen("a")).toBe(false);
    expect(holds.expire(2000)).toBe(true);
  });

  it("shows waiting rows when their transfer ends, but not rows whose files fly", () => {
    const holds = new RowHolds(10);
    holds.hold("never came", null);
    holds.hold("flying", 1000);
    expect(holds.releaseOpen(["never came", "flying", "unknown"])).toBe(true);
    expect([...holds.keys()]).toEqual(["flying"]);
    expect(holds.releaseOpen(["flying"])).toBe(false);
  });

  it("shows the rows held longest past its limit", () => {
    const holds = new RowHolds(2);
    holds.hold("first", null);
    holds.hold("second", null);
    holds.hold("first", 100);
    holds.hold("third", null);
    expect([...holds.keys()]).toEqual(["first", "third"]);
  });

  it("clears", () => {
    const holds = new RowHolds(4);
    expect(holds.clear()).toBe(false);
    holds.hold("a", 10);
    expect(holds.clear()).toBe(true);
    expect(holds.size).toBe(0);
  });
});
