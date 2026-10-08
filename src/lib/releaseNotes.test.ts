import { describe, expect, it } from "vitest";
import { parseInline, parseReleaseNotes } from "./releaseNotes";

describe("parseInline", () => {
  it("reads bold, code and links", () => {
    expect(
      parseInline("**Themes** as `.json` files, see [the docs](https://example.com)."),
    ).toEqual([
      { kind: "strong", text: "Themes" },
      { kind: "text", text: " as " },
      { kind: "code", text: ".json" },
      { kind: "text", text: " files, see the docs." },
    ]);
  });

  it("leaves unmatched markers as text", () => {
    expect(parseInline("2 ** 3 and a `tick")).toEqual([
      { kind: "text", text: "2 ** 3 and a `tick" },
    ]);
  });
});

describe("parseReleaseNotes", () => {
  it("joins wrapped lines of paragraphs and list items", () => {
    const notes = [
      "The first published release of Poros, a dual-pane SFTP and rsync client",
      "for Linux and Windows.",
      "",
      "- **Tabs and docking**: local folders and server sessions are tabs. Drag a tab onto another",
      "  pane to merge it in.",
      "- **Saved connections**.",
    ].join("\n");
    expect(parseReleaseNotes(notes)).toEqual([
      {
        kind: "paragraph",
        spans: [
          {
            kind: "text",
            text: "The first published release of Poros, a dual-pane SFTP and rsync client for Linux and Windows.",
          },
        ],
      },
      {
        kind: "list",
        items: [
          [
            { kind: "strong", text: "Tabs and docking" },
            {
              kind: "text",
              text: ": local folders and server sessions are tabs. Drag a tab onto another pane to merge it in.",
            },
          ],
          [
            { kind: "strong", text: "Saved connections" },
            { kind: "text", text: "." },
          ],
        ],
      },
    ]);
  });

  it("keeps a list together across blank lines and ends it at a paragraph", () => {
    const notes = "### Fixed\r\n\r\n- One\r\n\r\n- Two\r\n\r\nThanks.";
    expect(parseReleaseNotes(notes)).toEqual([
      { kind: "heading", spans: [{ kind: "text", text: "Fixed" }] },
      {
        kind: "list",
        items: [[{ kind: "text", text: "One" }], [{ kind: "text", text: "Two" }]],
      },
      { kind: "paragraph", spans: [{ kind: "text", text: "Thanks." }] },
    ]);
  });

  it("returns nothing for empty notes", () => {
    expect(parseReleaseNotes("")).toEqual([]);
    expect(parseReleaseNotes("\n\n")).toEqual([]);
  });
});
