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
    expect(settings.appearance.fontSize).toBe(20);
    expect(settings.appearance.theme).toBe("builtin:system");
    expect(settings.interface.doubleClickFile).toBe("transfer");
    expect(settings.log.maxLines).toBe(200);
    expect(settings.log.levels).toEqual({ info: true, warn: false, error: true, server: true });
  });

  it("keeps the cloud apps and the FXP switch", () => {
    const settings = sanitizeSettings({
      transfers: { fxp: false },
      cloud: { googleClientId: "id.apps.googleusercontent.com", microsoftTenant: 5 },
    });
    expect(settings.transfers.fxp).toBe(false);
    expect(settings.cloud).toEqual({
      ...DEFAULT_SETTINGS.cloud,
      googleClientId: "id.apps.googleusercontent.com",
    });
  });

  it("turns the old compact rows switch into a row height", () => {
    expect(sanitizeSettings({ appearance: { compactRows: true } }).appearance.rowHeight).toBe(21);
    expect(sanitizeSettings({ appearance: { compactRows: false } }).appearance.rowHeight).toBe(24);
    expect(
      sanitizeSettings({ appearance: { compactRows: true, rowHeight: 30 } }).appearance.rowHeight,
    ).toBe(30);
  });

  it("keeps a corner radius only when it is a number", () => {
    expect(sanitizeSettings({ appearance: { radius: 40 } }).appearance.radius).toBe(16);
    expect(sanitizeSettings({ appearance: { radius: 3.4 } }).appearance.radius).toBe(3);
    expect(sanitizeSettings({ appearance: { radius: "round" } }).appearance.radius).toBeNull();
    expect(sanitizeSettings({ appearance: { radius: { size: 4 } } }).appearance.radius).toBeNull();
  });

  it("checks the file list options", () => {
    const options = sanitizeSettings({
      interface: {
        hiddenColumns: ["owner", "bogus", 4, "type"],
        sort: { key: "colour", direction: 1 },
        dateFormat: "julian",
      },
    }).interface;
    expect(options.hiddenColumns).toEqual(["type", "owner"]);
    expect(options.sort).toEqual({ key: "name", direction: 1 });
    expect(options.dateFormat).toBe("minutes");
    const sorted = sanitizeSettings({ interface: { sort: { key: "type", direction: -1 } } });
    expect(sorted.interface.sort).toEqual({ key: "type", direction: -1 });
  });

  it("checks the synchronization defaults", () => {
    const sync = sanitizeSettings({
      sync: { direction: "sideways", compare: "checksum", timeToleranceSecs: -5, excludes: 3 },
    }).sync;
    expect(sync.direction).toBe("upload");
    expect(sync.compare).toBe("checksum");
    expect(sync.timeToleranceSecs).toBe(0);
    expect(sync.excludes).toBe("");
    expect(sanitizeSettings({ sync: { timeToleranceSecs: 1e9 } }).sync.timeToleranceSecs).toBe(
      86400,
    );
  });

  it("checks for updates at start-up unless turned off", () => {
    expect(sanitizeSettings({}).updates).toEqual({ checkOnStart: true, skippedVersion: "" });
    const updates = sanitizeSettings({
      updates: { checkOnStart: "no", skippedVersion: " 0.3.0 " },
    }).updates;
    expect(updates.checkOnStart).toBe(true);
    expect(updates.skippedVersion).toBe("0.3.0");
    expect(sanitizeSettings({ updates: { checkOnStart: false } }).updates.checkOnStart).toBe(false);
  });
});
