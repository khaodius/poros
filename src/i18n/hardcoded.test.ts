// Fails on interface text written into a component rather than into the English catalog: text
// between JSX tags, and the title, placeholder, aria-label and label attributes. Words that are
// the same in every language, such as the product name, are on the short list below.

import ts from "typescript";
import { describe, expect, it } from "vitest";

/** Every component's source, by its path from `src/`. */
const COMPONENTS = import.meta.glob<string>(["../**/*.tsx", "!../**/*.test.tsx", "!./**"], {
  query: "?raw",
  import: "default",
  eager: true,
});
const CHECKED_ATTRIBUTES = new Set(["title", "placeholder", "aria-label", "label"]);
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
]);

/**
 * Examples of what to type, which a translation keeps as they are: paths, addresses, commands
 * and patterns, told apart by having no space between words or by starting as code does.
 */
function looksLikeCode(text: string): boolean {
  return (
    (/^[^\s]+$/.test(text) && /[./:\\{}=@-]/.test(text)) || /^(\.\/|\/|--|https?:|du )/.test(text)
  );
}

/** Sizes such as `500 KB, 2 GB`, whose units stay as they are. */
const SIZE = /\d+(?:[.,]\d+)?\s*(?:[KMGTP]i?)?B\b/g;

function needsTranslating(text: string): boolean {
  const trimmed = text.replace(/\s+/g, " ").trim();
  if (!/\p{L}/u.test(trimmed.replace(SIZE, ""))) return false;
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
  const literalText = (node: ts.Expression): string | null => {
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return node.text;
    if (ts.isTemplateExpression(node)) {
      return [node.head.text, ...node.templateSpans.map((span) => span.literal.text)].join(" ");
    }
    return null;
  };
  const visit = (node: ts.Node) => {
    if (ts.isJsxElement(node) && CODE_ELEMENTS.has(node.openingElement.tagName.getText())) return;
    if (ts.isJsxText(node)) report(node, node.text);
    if (ts.isJsxExpression(node) && node.expression && ts.isJsxElement(node.parent)) {
      const text = literalText(node.expression);
      if (text !== null) report(node, text);
    }
    if (
      ts.isJsxAttribute(node) &&
      CHECKED_ATTRIBUTES.has(node.name.getText()) &&
      node.initializer
    ) {
      const value = node.initializer;
      const text = ts.isStringLiteral(value)
        ? value.text
        : ts.isJsxExpression(value) && value.expression
          ? literalText(value.expression)
          : null;
      if (text !== null) report(node, text);
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
