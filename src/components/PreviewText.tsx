import { useEffect, useRef } from "react";
import { syntaxHighlighting } from "@codemirror/language";
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView, highlightSpecialChars, lineNumbers } from "@codemirror/view";
import { languageFor } from "../lib/editorLanguage";
import { editorTheme, syntaxColors } from "../lib/editorTheme";

/**
 * How long a text waits before its highlighted view is built, so that holding an arrow key
 * down in the file list builds one view where the cursor stops, not one for every file passed.
 */
const BUILD_DELAY_MILLIS = 60;

interface PreviewTextProps {
  text: string;
  /** The file's name, which picks the highlighting. */
  name: string;
}

/**
 * Text shown the way the editor shows it, in its theme, font and highlighting, but read-only.
 * It is highlighted as source whatever it is: nothing in it is rendered.
 */
export default function PreviewText({ text, name }: PreviewTextProps) {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let view: EditorView | null = null;
    let current = true;
    const timer = window.setTimeout(() => {
      const language = new Compartment();
      view = new EditorView({
        parent: host,
        state: EditorState.create({
          doc: text,
          extensions: [
            lineNumbers(),
            highlightSpecialChars(),
            syntaxHighlighting(syntaxColors),
            language.of([]),
            EditorState.readOnly.of(true),
            EditorView.editable.of(false),
            editorTheme,
          ],
        }),
      });
      void languageFor(name)
        ?.load()
        .then((support) => {
          if (current) view?.dispatch({ effects: language.reconfigure(support) });
        })
        .catch(() => undefined);
    }, BUILD_DELAY_MILLIS);
    return () => {
      current = false;
      window.clearTimeout(timer);
      view?.destroy();
    };
  }, [text, name]);

  return <div ref={hostRef} className="preview-text preview-selectable selectable" />;
}
