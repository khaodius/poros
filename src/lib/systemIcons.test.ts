import { describe, expect, it } from "vitest";
import { systemIconLookup } from "./systemIcons";

describe("systemIconLookup", () => {
  it("asks about files by extension, whatever their name and case", () => {
    const report = systemIconLookup({ name: "Report.PDF", kind: "file" });
    const notes = systemIconLookup({ name: "notes.pdf", kind: "file" });
    expect(report).toEqual({
      key: "file:.pdf",
      request: { name: "file.pdf", folder: false, link: false },
    });
    expect(notes?.key).toBe(report?.key);
  });

  it("asks about files without an extension by name", () => {
    expect(systemIconLookup({ name: "Makefile", kind: "file" })).toEqual({
      key: "file:makefile",
      request: { name: "Makefile", folder: false, link: false },
    });
    expect(systemIconLookup({ name: ".bashrc", kind: "file" })?.key).toBe("file:.bashrc");
  });

  it("shares one icon between folders, and one between links to folders", () => {
    expect(systemIconLookup({ name: "src", kind: "dir" })).toEqual({
      key: "folder",
      request: { name: "folder", folder: true, link: false },
    });
    expect(systemIconLookup({ name: "home", kind: "symlink", linkTarget: "dir" })).toEqual({
      key: "folder-link",
      request: { name: "folder", folder: true, link: true },
    });
  });

  it("draws links to files with the link overlay, and leaves broken links alone", () => {
    expect(systemIconLookup({ name: "app.log", kind: "symlink", linkTarget: "file" })).toEqual({
      key: "file-link:.log",
      request: { name: "file.log", folder: false, link: true },
    });
    expect(systemIconLookup({ name: "gone", kind: "symlink", linkTarget: "broken" })).toBeNull();
  });
});
