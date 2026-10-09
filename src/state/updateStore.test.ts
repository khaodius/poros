import { describe, expect, it } from "vitest";
import { describeCheckError } from "./updateStore";

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
