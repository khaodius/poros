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

  it("refreshes a folder that is on show from a remembered listing", () => {
    const loads = new ListingLoads();
    const navigation = loads.start("navigation")!;
    // Its folder is up already, so the load still on its way only confirms it.
    loads.landed(navigation);
    const refresh = loads.start("refresh")!;
    expect(refresh).not.toBeNull();
    expect(loads.isCurrent(refresh)).toBe(true);
    expect(loads.isCurrent(navigation)).toBe(false);
  });

  it("keeps the newest navigation on its way when an older one lands", () => {
    const loads = new ListingLoads();
    const first = loads.start("navigation")!;
    loads.start("navigation");
    loads.landed(first);
    expect(loads.start("refresh")).toBeNull();
  });

  it("is busy until every load has finished", () => {
    const loads = new ListingLoads();
    expect(loads.busy).toBe(false);
    const navigation = loads.start("navigation")!;
    loads.landed(navigation);
    const refresh = loads.start("refresh")!;
    expect(loads.busy).toBe(true);
    loads.finish(navigation);
    expect(loads.busy).toBe(true);
    loads.finish(refresh);
    expect(loads.busy).toBe(false);
  });

  it("shows only the newest of several refreshes", () => {
    const loads = new ListingLoads();
    const first = loads.start("refresh")!;
    const second = loads.start("refresh")!;
    expect(loads.isCurrent(first)).toBe(false);
    expect(loads.isCurrent(second)).toBe(true);
  });
});
