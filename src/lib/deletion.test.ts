import { describe, expect, it } from "vitest";
import { deleteFraction, deleteProgressText } from "./deletion";

describe("delete progress", () => {
  it("counts first, then shows how much of the total is gone", () => {
    expect(deleteProgressText({ files: 0 })).toBe("Counting what to delete...");
    expect(deleteProgressText({ files: 12, total: 4000, counting: true })).toBe(
      "12 deleted, 4,000 found so far",
    );
    expect(deleteProgressText({ files: 1204, total: 5000 })).toBe("1,204 of 5,000 deleted");
    expect(deleteProgressText({ files: 3 })).toBe("3 deleted");
  });

  it("fills the bar once the total is known, failures included", () => {
    expect(deleteFraction({ files: 10, total: 100, counting: true })).toBeNull();
    expect(deleteFraction({ files: 0 })).toBeNull();
    expect(deleteFraction({ files: 40, total: 100, failed: 10 })).toBe(0.5);
    expect(deleteFraction({ files: 120, total: 100 })).toBe(1);
  });
});
