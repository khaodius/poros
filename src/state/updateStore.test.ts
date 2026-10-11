import { describe, expect, it } from "vitest";
import { describeCheckError, percentDownloaded } from "./updateStore";

describe("describeCheckError", () => {
  it("explains a release without update information", () => {
    expect(describeCheckError("Could not fetch a valid release JSON from the remote")).toBe(
      "The latest release on GitHub does not offer in-app updates yet",
    );
  });

  it("passes other failures through", () => {
    expect(describeCheckError("error sending request")).toBe(
      "Could not check for updates: error sending request",
    );
  });
});

describe("percentDownloaded", () => {
  it("rounds down, so 100 means the whole download", () => {
    expect(percentDownloaded(0, 2000)).toBe(0);
    expect(percentDownloaded(1999, 2000)).toBe(99);
    expect(percentDownloaded(2000, 2000)).toBe(100);
  });

  it("is unknown without a size", () => {
    expect(percentDownloaded(500, null)).toBeNull();
  });

  it("stops at 100 when the server understated the size", () => {
    expect(percentDownloaded(3000, 2000)).toBe(100);
  });
});
