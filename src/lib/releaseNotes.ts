// Release notes arrive as the Markdown of the version's CHANGELOG.md section. This reads the
// few constructs the changelog uses (paragraphs, headings, lists with wrapped lines, bold, code
// and links) so the update dialog can show them without a Markdown library.

export interface NoteSpan {
  kind: "text" | "strong" | "code";
  text: string;
}

export type NoteBlock =
  | { kind: "heading"; spans: NoteSpan[] }
  | { kind: "paragraph"; spans: NoteSpan[] }
  | { kind: "list"; items: NoteSpan[][] };

const HEADING = /^#{1,6}\s+/;
const LIST_MARKER = /^[-*+]\s+/;
/** Bold, inline code, or a link whose text is kept and whose address is dropped. */
const INLINE = /\*\*(.+?)\*\*|`([^`]+)`|\[([^\]]+)\]\([^)]*\)/g;

export function parseInline(text: string): NoteSpan[] {
  const spans: NoteSpan[] = [];
  const addText = (value: string) => {
    if (!value) return;
    const previous = spans[spans.length - 1];
    if (previous?.kind === "text") previous.text += value;
    else spans.push({ kind: "text", text: value });
  };
  let consumed = 0;
  for (const match of text.matchAll(INLINE)) {
    addText(text.slice(consumed, match.index));
    const [whole, strong, code, linkText] = match;
    if (strong !== undefined) spans.push({ kind: "strong", text: strong });
    else if (code !== undefined) spans.push({ kind: "code", text: code });
    else addText(linkText);
    consumed = match.index + whole.length;
  }
  addText(text.slice(consumed));
  return spans;
}

export function parseReleaseNotes(markdown: string): NoteBlock[] {
  const blocks: NoteBlock[] = [];
  let paragraph: string[] = [];
  let items: string[] | null = null;
  let blankSinceItem = false;

  const endParagraph = () => {
    if (paragraph.length === 0) return;
    blocks.push({ kind: "paragraph", spans: parseInline(paragraph.join(" ")) });
    paragraph = [];
  };
  const endList = () => {
    if (items) blocks.push({ kind: "list", items: items.map(parseInline) });
    items = null;
  };

  for (const rawLine of markdown.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) {
      endParagraph();
      blankSinceItem = items !== null;
      continue;
    }
    if (HEADING.test(line)) {
      endParagraph();
      endList();
      blocks.push({ kind: "heading", spans: parseInline(line.replace(HEADING, "")) });
    } else if (LIST_MARKER.test(line)) {
      endParagraph();
      items ??= [];
      items.push(line.replace(LIST_MARKER, ""));
      blankSinceItem = false;
    } else if (items && !blankSinceItem) {
      // A wrapped line of the item above.
      items[items.length - 1] += ` ${line}`;
    } else {
      endList();
      paragraph.push(line);
    }
  }
  endParagraph();
  endList();
  return blocks;
}
