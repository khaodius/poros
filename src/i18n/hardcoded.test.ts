// Fails on interface text written into a component rather than into the English catalog: text
// between JSX tags, and the attributes and props people read, such as title, placeholder, label
// and hint. A text counts wherever it can end up shown: as written, in a template, or as a branch
// of a condition, `&&`, `||` or `??`. Words that are the same in every language, such as the
// product name, are on the short list below.

import ts from "typescript";
import { describe, expect, it } from "vitest";

/** Every component's source, by its path from `src/`. */
const COMPONENTS = import.meta.glob<string>(["../**/*.tsx", "!../**/*.test.tsx", "!./**"], {
  query: "?raw",
  import: "default",
  eager: true,
});
const CHECKED_ATTRIBUTES = new Set([
  "title",
  "placeholder",
  "aria-label",
  "label",
  "alt",
  "unit",
  // Props of the app's own components that are shown as they are given.
  "hint",
  "detail",
  "confirmLabel",
  "text",
  "empty",
  "firstLabel",
  "secondLabel",
  "resetLabel",
]);
/** Elements whose text is code, a key or a command, written as the program wants it. */
const CODE_ELEMENTS = new Set(["code", "kbd", "samp"]);

/** Text that reads the same in every language. */
const SAME_IN_EVERY_LANGUAGE = new Set([
  "Poros",
  "SFTP",
  "FTP",
  "WebDAV",
  "rsync",
  "SSH",
  "SSH_AUTH_SOCK",
  // Microsoft's name for the tenant every account can sign in through.
  "common",
  "KiB/s",
  "Ctrl",
  "Shift",
  "Alt",
  // Units that stay as they are, as the units of sizes do.
  "KiB",
  "MiB",
  "px",
]);

/**
 * Examples of what to type, which a translation keeps as they are: paths, addresses, commands
 * and patterns. One word is code when it has a path separator in it, or a dot before a letter
 * or digit as host and file names do, so that "Saving...", "Read-only" and "Name:" are text.
 */
function looksLikeCode(text: string): boolean {
  const oneWord = /^[^\s]+$/.test(text);
  return (
    (oneWord && (/[/\\]/.test(text) || /\.[\p{L}\d]/u.test(text))) ||
    /^(\.\/|\/|--|https?:|du |[A-Za-z]:\\)/.test(text)
  );
}

/** Sizes such as `500 KB, 2 GB`, whose units stay as they are. */
const SIZE = /\d+(?:[.,]\d+)?\s*(?:[KMGTP]i?)?B\b/g;

/** `{name}`, which a pattern such as `{name}-{n}.{ext}` is made of. */
const PLACEHOLDER = /\{[A-Za-z][A-Za-z0-9]*\}/g;

function needsTranslating(text: string): boolean {
  const trimmed = text.replace(/\s+/g, " ").trim();
  if (!/\p{L}/u.test(trimmed.replace(SIZE, "").replace(PLACEHOLDER, ""))) return false;
  return !SAME_IN_EVERY_LANGUAGE.has(trimmed) && !looksLikeCode(trimmed);
}

/** `file:line: text` for every hard-coded text in one component file. */
export function hardCodedTexts(fileName: string, source: string): string[] {
  const file = ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX,
  );
  const found: string[] = [];
  const report = (node: ts.Node, text: string) => {
    if (!needsTranslating(text)) return;
    const { line } = file.getLineAndCharacterOfPosition(node.getStart());
    found.push(`${fileName}:${line + 1}: ${text.replace(/\s+/g, " ").trim()}`);
  };
  /** The texts an expression can show: itself, a template's own words, or a branch's. */
  const shownTexts = (node: ts.Expression): string[] => {
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return [node.text];
    if (ts.isTemplateExpression(node)) {
      const words = [node.head.text, ...node.templateSpans.map((span) => span.literal.text)];
      return [
        words.join(" "),
        ...node.templateSpans.flatMap((span) => shownTexts(span.expression)),
      ];
    }
    if (ts.isParenthesizedExpression(node)) return shownTexts(node.expression);
    if (ts.isConditionalExpression(node)) {
      return [...shownTexts(node.whenTrue), ...shownTexts(node.whenFalse)];
    }
    if (ts.isBinaryExpression(node)) {
      switch (node.operatorToken.kind) {
        // The left side of `&&` is the condition.
        case ts.SyntaxKind.AmpersandAmpersandToken:
          return shownTexts(node.right);
        case ts.SyntaxKind.BarBarToken:
        case ts.SyntaxKind.QuestionQuestionToken:
          return [...shownTexts(node.left), ...shownTexts(node.right)];
      }
    }
    return [];
  };
  const visit = (node: ts.Node) => {
    if (ts.isJsxElement(node) && CODE_ELEMENTS.has(node.openingElement.tagName.getText())) return;
    if (ts.isJsxText(node)) report(node, node.text);
    if (
      ts.isJsxExpression(node) &&
      node.expression &&
      (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent))
    ) {
      for (const text of shownTexts(node.expression)) report(node, text);
    }
    if (
      ts.isJsxAttribute(node) &&
      CHECKED_ATTRIBUTES.has(node.name.getText()) &&
      node.initializer
    ) {
      const value = node.initializer;
      const texts = ts.isStringLiteral(value)
        ? [value.text]
        : ts.isJsxExpression(value) && value.expression
          ? shownTexts(value.expression)
          : [];
      for (const text of texts) report(node, text);
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return found;
}

describe("interface text", () => {
  it("comes from the catalog, not from the components", () => {
    const paths = Object.keys(COMPONENTS);
    expect(paths).toContain("../components/FilePane.tsx");
    const found = paths.flatMap((path) =>
      hardCodedTexts(path.replace(/^\.\.\//, ""), COMPONENTS[path]),
    );
    expect(found).toEqual([]);
  });

  it("finds text between tags and in the attributes people read", () => {
    const component = `
      export function Example({ count }: { count: number }) {
        return (
          <section title="Shown on hover" aria-label={"Read aloud"}>
            <h2>Plain words</h2>
            <input placeholder={\`Search \${count} files\`} />
            <Field label="A label" />
            {"Also text"}
          </section>
        );
      }`;
    expect(hardCodedTexts("Example.tsx", component)).toEqual([
      "Example.tsx:4: Shown on hover",
      "Example.tsx:4: Read aloud",
      "Example.tsx:5: Plain words",
      "Example.tsx:6: Search files",
      "Example.tsx:7: A label",
      "Example.tsx:8: Also text",
    ]);
  });

  it("finds text in conditions, templates and branches", () => {
    const component = `
      export function Example({ busy, failed, open, name }: Props) {
        return (
          <>
            <button>{busy ? "Saving..." : "Save"}</button>
            <span>{failed && "Failed"}</span>
            <span title={open ? "Hide" : "Show"} />
            <span>{name || "Untitled"}</span>
            <span>{name ?? ("Nameless")}</span>
            <span title={\`\${busy ? "Busy" : "Idle"} now\`} />
          </>
        );
      }`;
    expect(hardCodedTexts("Example.tsx", component)).toEqual([
      "Example.tsx:5: Saving...",
      "Example.tsx:5: Save",
      "Example.tsx:6: Failed",
      "Example.tsx:7: Hide",
      "Example.tsx:7: Show",
      "Example.tsx:8: Untitled",
      "Example.tsx:9: Nameless",
      "Example.tsx:10: now",
      "Example.tsx:10: Busy",
      "Example.tsx:10: Idle",
    ]);
  });

  it("checks the props the app's own components show", () => {
    const component = `
      export function Example() {
        return (
          <>
            <SettingRow hint="A hint" detail="Some detail" />
            <ConfirmDialog confirmLabel="Go ahead" text="Body text" empty="Nothing here" />
            <Choice firstLabel="First" secondLabel="Second" resetLabel="Reset all" />
            <img alt="A picture" />
            <NumberSetting unit="min" />
          </>
        );
      }`;
    expect(hardCodedTexts("Example.tsx", component)).toEqual([
      "Example.tsx:5: A hint",
      "Example.tsx:5: Some detail",
      "Example.tsx:6: Go ahead",
      "Example.tsx:6: Body text",
      "Example.tsx:6: Nothing here",
      "Example.tsx:7: First",
      "Example.tsx:7: Second",
      "Example.tsx:7: Reset all",
      "Example.tsx:8: A picture",
      "Example.tsx:9: min",
    ]);
  });

  it("takes one word for code only when it looks like a path or a dotted name", () => {
    const component = `
      export function Example() {
        return (
          <div>
            <span>Saving...</span>
            <span title="Read-only" />
            <span>Name:</span>
            <input placeholder="C:\\Program Files\\Tool\\tool.exe" />
            <input placeholder="~/Poros/server" />
            <input placeholder="{name}-{n}.{ext}" />
            <input placeholder="report.pdf" />
            <input placeholder="v1.2" />
          </div>
        );
      }`;
    expect(hardCodedTexts("Example.tsx", component)).toEqual([
      "Example.tsx:5: Saving...",
      "Example.tsx:6: Read-only",
      "Example.tsx:7: Name:",
    ]);
  });

  it("leaves alone what reads the same in every language", () => {
    const component = `
      export function Example({ t }: { t: (key: string) => string }) {
        return (
          <div title={t("common.close")} className="Not checked">
            <span>Poros</span> <span>· 42 %</span> <code>offline_access</code>
            <input placeholder="500 KB, 2 GB" />
            <input placeholder="/var/backups" />
            <input placeholder="proxy.example.com" />
            <input placeholder="--delete-after --exclude=*.log" />
          </div>
        );
      }`;
    expect(hardCodedTexts("Example.tsx", component)).toEqual([]);
  });
});
