import { describe, expect, it } from "vitest";
import { ListingLoads } from "./listingLoads";

describe("ListingLoads", () => {
  it("keeps a navigation on its way when a refresh comes in", () => {
    const loads = new ListingLoads();
    const navigation = loads.start("navigation")!;
    expect(loads.start("refresh")).toBeNull();
    expect(loads.isCurrent(navigation)).toBe(true);
  });

  it("lets a navigation replace a refresh on its way", () => {
    const loads = new ListingLoads();
    const refresh = loads.start("refresh")!;
    const navigation = loads.start("navigation")!;
    expect(loads.isCurrent(refresh)).toBe(false);
    expect(loads.isCurrent(navigation)).toBe(true);
  });

  it("shows the newest navigation and refreshes once it is done", () => {
    const loads = new ListingLoads();
    const first = loads.start("navigation")!;
    const second = loads.start("navigation")!;
    expect(loads.isCurrent(first)).toBe(false);
    loads.finish(first);
    expect(loads.start("refresh")).toBeNull();
    loads.finish(second);
    const refresh = loads.start("refresh")!;
    expect(loads.isCurrent(refresh)).toBe(true);
    expect(loads.isCurrent(second)).toBe(false);
  });

  it("refreshes again after a navigation that failed", () => {
    const loads = new ListingLoads();
    loads.finish(loads.start("navigation")!);
    expect(loads.start("refresh")).not.toBeNull();
  });

  it("shows only the newest of several refreshes", () => {
    const loads = new ListingLoads();
    const first = loads.start("refresh")!;
    const second = loads.start("refresh")!;
    expect(loads.isCurrent(first)).toBe(false);
    expect(loads.isCurrent(second)).toBe(true);
  });
});
