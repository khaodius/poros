import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, sanitizeSettings } from "./settings";

describe("sanitizeSettings", () => {
  it("fills missing sections and fields with defaults", () => {
    expect(sanitizeSettings(undefined)).toEqual(DEFAULT_SETTINGS);
    const settings = sanitizeSettings({ interface: { showHiddenFiles: true } });
    expect(settings.interface.showHiddenFiles).toBe(true);
    expect(settings.interface.doubleClickFile).toBe("transfer");
  });

  it("drops values of the wrong type and clamps ranges", () => {
    const settings = sanitizeSettings({
      appearance: { fontSize: 40, theme: 7 },
      interface: { doubleClickFile: "explode" },
      log: { maxLines: 3, levels: { warn: false, error: "yes" } },
    });
    expect(settings.appearance.fontSize).toBe(16);
    expect(settings.appearance.theme).toBe("builtin:system");
    expect(settings.interface.doubleClickFile).toBe("transfer");
    expect(settings.log.maxLines).toBe(200);
    expect(settings.log.levels).toEqual({ info: true, warn: false, error: true, server: true });
  });
});
