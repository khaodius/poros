import { describe, expect, it } from "vitest";
import { cornersFor } from "./windowCorners";

describe("window corners", () => {
  it("follows the corner radius setting", () => {
    expect(cornersFor(0)).toBe("square");
    expect(cornersFor(2)).toBe("small");
    expect(cornersFor(5)).toBe("small");
    expect(cornersFor(6)).toBe("round");
    expect(cornersFor(16)).toBe("round");
  });
});
