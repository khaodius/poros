import { describe, expect, it } from "vitest";
import { archiveFormatOf, extractorOf, isSingleFile, unpackedName } from "./archives";

describe("archiveFormatOf", () => {
  it("reads compound endings before their last part", () => {
    expect(archiveFormatOf("site.tar.gz")).toBe("tarGzip");
    expect(archiveFormatOf("SITE.TGZ")).toBe("tarGzip");
    expect(archiveFormatOf("dump.sql.gz")).toBe("gzip");
    expect(archiveFormatOf("backup.tar.zst")).toBe("tarZstd");
    expect(archiveFormatOf("game.7z")).toBe("sevenZip");
    expect(archiveFormatOf("photos.part1.rar")).toBe("rar");
    expect(archiveFormatOf("app.jar")).toBe("zip");
  });

  it("leaves other files alone", () => {
    expect(archiveFormatOf("notes.md")).toBeNull();
    expect(archiveFormatOf(".zip")).toBeNull();
    expect(archiveFormatOf("zip")).toBeNull();
  });
});

describe("unpackedName", () => {
  it("names the folder or file an archive unpacks to", () => {
    expect(unpackedName("site.tar.gz")).toBe("site");
    expect(unpackedName("dump.sql.gz")).toBe("dump.sql");
    expect(unpackedName("photos.part1.rar")).toBe("photos");
    expect(unpackedName("v1.2.zip")).toBe("v1.2");
  });

  it("tells single compressed files from archives", () => {
    expect(isSingleFile("gzip")).toBe(true);
    expect(isSingleFile("tarGzip")).toBe(false);
  });
});

describe("extractorOf", () => {
  it("turns the setting into what the backend runs", () => {
    expect(extractorOf({ choice: "", program: "", arguments: "" })).toEqual({ kind: "auto" });
    expect(extractorOf({ choice: "C:\\7-Zip\\7z.exe", program: "", arguments: "" })).toEqual({
      kind: "installed",
      path: "C:\\7-Zip\\7z.exe",
    });
    expect(extractorOf({ choice: "custom", program: "bz.exe", arguments: "x {archive}" })).toEqual({
      kind: "custom",
      program: "bz.exe",
      arguments: "x {archive}",
    });
  });
});
