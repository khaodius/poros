import { describe, expect, it } from "vitest";
import {
  bitStates,
  isUnchanged,
  nextState,
  parseOctal,
  statesFromMode,
  symbolic,
  toModeChange,
  toOctal,
} from "./permissions";

describe("permission bits", () => {
  it("reads one mode as on and off bits", () => {
    const states = statesFromMode(0o755);
    expect(toOctal(states)).toBe("755");
    expect(symbolic(states)).toBe("rwxr-xr-x");
    expect(toModeChange(states)).toEqual({ set: 0o755, clear: 0o7022 });
  });

  it("marks bits the items disagree on as mixed", () => {
    const states = bitStates([0o644, 0o664]);
    expect(states[0o020]).toBe("mixed");
    expect(states[0o400]).toBe("on");
    expect(states[0o001]).toBe("off");
    expect(toOctal(states)).toBeNull();
    expect(symbolic(states)).toBe("rw-r?-r--");
    expect(toModeChange(states)).toEqual({ set: 0o644, clear: 0o7113 });
  });

  it("leaves everything mixed without items", () => {
    expect(toModeChange(bitStates([]))).toEqual({ set: 0, clear: 0 });
  });

  it("shows special bits the way ls does", () => {
    expect(symbolic(statesFromMode(0o4755))).toBe("rwsr-xr-x");
    expect(symbolic(statesFromMode(0o2644))).toBe("rw-r-Sr--");
    expect(symbolic(statesFromMode(0o1777))).toBe("rwxrwxrwt");
    expect(toOctal(statesFromMode(0o4755))).toBe("4755");
    expect(toOctal(statesFromMode(0o044))).toBe("044");
  });

  it("parses three or four octal digits only", () => {
    expect(parseOctal("644")).toBe(0o644);
    expect(parseOctal(" 2775 ")).toBe(0o2775);
    expect(parseOctal("648")).toBeNull();
    expect(parseOctal("64")).toBeNull();
    expect(parseOctal("07777")).toBeNull();
  });

  it("knows when nothing would change", () => {
    const original = bitStates([0o644, 0o664]);
    expect(isUnchanged(original, original)).toBe(true);
    expect(isUnchanged({ ...original, [0o020]: "on" }, original)).toBe(false);
    expect(nextState("mixed")).toBe("on");
    expect(nextState("on")).toBe("off");
  });
});
