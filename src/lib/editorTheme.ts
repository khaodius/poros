// The editor's look, drawn from the app's color and font variables so it follows the theme and
// accent. Syntax colors reuse the file icon colors, which every theme already sets.

import { HighlightStyle } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";

export const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    color: "var(--text)",
    backgroundColor: "var(--surface-sunken)",
    fontSize: "var(--font-size)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.6" },
  ".cm-content": { caretColor: "var(--accent)", padding: "6px 0" },
  ".cm-line": { padding: "0 12px 0 6px" },
  ".cm-cursor, .cm-dropCursor": { borderLeft: "2px solid var(--accent)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
    { backgroundColor: "var(--row-selected)" },
  ".cm-gutters": {
    color: "var(--text-faint)",
    backgroundColor: "var(--surface-sunken)",
    border: "none",
    borderRight: "1px solid var(--border)",
  },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 10px 0 14px" },
  ".cm-activeLine": { backgroundColor: "var(--row-hover)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--text-muted)" },
  ".cm-foldPlaceholder": {
    color: "var(--text-muted)",
    backgroundColor: "var(--surface-hover)",
    border: "none",
    borderRadius: "var(--radius-small)",
  },
  ".cm-matchingBracket, &.cm-focused .cm-matchingBracket": {
    backgroundColor: "var(--accent-soft)",
    outline: "1px solid var(--accent)",
  },
  ".cm-nonmatchingBracket": { color: "var(--danger)" },
  ".cm-selectionMatch": { backgroundColor: "var(--accent-softer)" },
  ".cm-searchMatch": {
    backgroundColor: "var(--accent-softer)",
    outline: "1px solid var(--accent-soft)",
  },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "var(--accent-soft)" },
  ".cm-specialChar": { color: "var(--danger)" },
  ".cm-panels": {
    color: "var(--text)",
    backgroundColor: "var(--surface-raised)",
    fontFamily: "var(--font-ui)",
    fontSize: "var(--font-size-small)",
  },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--border)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--border)" },
  ".cm-panel.cm-search": { padding: "6px 30px 6px 10px" },
  ".cm-panel.cm-search label": { color: "var(--text-muted)", fontSize: "var(--font-size-small)" },
  ".cm-panel.cm-search [name=close]": {
    color: "var(--text-muted)",
    fontSize: "16px",
    top: "6px",
    right: "8px",
    cursor: "pointer",
  },
  ".cm-textfield": {
    color: "var(--text)",
    backgroundColor: "var(--surface-sunken)",
    border: "1px solid var(--border-strong)",
    borderRadius: "var(--radius-small)",
    padding: "3px 6px",
    fontSize: "var(--font-size-small)",
  },
  ".cm-textfield:focus": { outline: "none", borderColor: "var(--border-focus)" },
  ".cm-button": {
    color: "var(--text)",
    backgroundImage: "none",
    backgroundColor: "var(--surface-overlay)",
    border: "1px solid var(--border-strong)",
    borderRadius: "var(--radius-small)",
    padding: "3px 9px",
    fontSize: "var(--font-size-small)",
    cursor: "pointer",
  },
  ".cm-button:hover": { backgroundColor: "var(--surface-hover)" },
  ".cm-button:active": { backgroundImage: "none", backgroundColor: "var(--surface-pressed)" },
  ".cm-tooltip": {
    color: "var(--text)",
    backgroundColor: "var(--surface-overlay)",
    border: "none",
    borderRadius: "var(--radius)",
    boxShadow: "var(--shadow-popover)",
  },
});

export const syntaxColors = HighlightStyle.define([
  { tag: [tags.keyword, tags.controlKeyword, tags.moduleKeyword], color: "var(--icon-image)" },
  { tag: [tags.operatorKeyword, tags.definitionKeyword], color: "var(--icon-image)" },
  { tag: [tags.comment, tags.meta], color: "var(--text-faint)", fontStyle: "italic" },
  { tag: [tags.string, tags.special(tags.string), tags.inserted], color: "var(--success)" },
  { tag: [tags.regexp, tags.escape, tags.url], color: "var(--icon-data)" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--icon-audio)" },
  { tag: [tags.constant(tags.name), tags.standard(tags.name)], color: "var(--icon-audio)" },
  {
    tag: [tags.function(tags.variableName), tags.function(tags.propertyName), tags.labelName],
    color: "var(--icon-code)",
  },
  { tag: [tags.propertyName, tags.attributeName], color: "var(--icon-code)" },
  {
    tag: [tags.typeName, tags.className, tags.namespace, tags.annotation, tags.self],
    color: "var(--icon-key)",
  },
  { tag: [tags.tagName, tags.deleted], color: "var(--icon-video)" },
  { tag: [tags.heading, tags.link], color: "var(--accent)" },
  { tag: tags.heading, fontWeight: "600" },
  { tag: tags.link, textDecoration: "underline" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strong, fontWeight: "600" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: [tags.changed, tags.modifier], color: "var(--warning)" },
  { tag: tags.invalid, color: "var(--danger)" },
]);
