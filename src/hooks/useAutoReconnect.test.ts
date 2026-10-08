import { describe, expect, it } from "vitest";
import { canRetryAlone, retryDelaySecs } from "./useAutoReconnect";

describe("retryDelaySecs", () => {
  it("starts quickly, grows, then settles at half a minute", () => {
    expect(retryDelaySecs(0)).toBe(1);
    for (let failures = 1; failures < 6; failures += 1) {
      expect(retryDelaySecs(failures)).toBeGreaterThan(retryDelaySecs(failures - 1));
    }
    expect(retryDelaySecs(5)).toBe(30);
    expect(retryDelaySecs(100)).toBe(30);
  });
});

describe("canRetryAlone", () => {
  it("keeps trying through network trouble but not through a refused login", () => {
    expect(canRetryAlone({ kind: "connection", message: "refused" })).toBe(true);
    expect(canRetryAlone({ kind: "timeout", message: "timed out" })).toBe(true);
    expect(canRetryAlone({ kind: "authFailed", message: "wrong password" })).toBe(false);
    expect(canRetryAlone({ kind: "hostKeyChanged", message: "changed" })).toBe(false);
  });
});
