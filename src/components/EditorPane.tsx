import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent,
  type ReactNode,
} from "react";
import { FileText, RotateCw, Save, Search, TriangleAlert, WrapText, X } from "lucide-react";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language";
import { highlightSelectionMatches, openSearchPanel, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension, type Text } from "@codemirror/state";
import {
  EditorView,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { languageFor } from "../lib/editorLanguage";
import { editorTheme, syntaxColors } from "../lib/editorTheme";
import { editor, toAppError } from "../lib/ipc";
import type { EditorTab } from "../lib/layout";
import type { FileStamp, TextLineEnding, TextEncoding } from "../lib/types";
import { registerEditor, useUnsavedStore } from "../state/editorRegistry";
import { useLayoutStore } from "../state/layoutStore";
import { ConfirmDialog } from "./ConfirmDialog";
import { ContextMenu, type MenuItem } from "./ContextMenu";

interface EditorPaneProps {
  tab: EditorTab;
  visible: boolean;
  active: boolean;
}

interface Format {
  encoding: TextEncoding;
  lineEnding: TextLineEnding;
}

/** The file as last read or saved; `doc` is null when this window never saw it saved. */
interface Baseline extends Format {
  doc: Text | null;
}

const ENCODING_LABELS: Record<TextEncoding, string> = {
  utf8: "UTF-8",
  utf8Bom: "UTF-8 with BOM",
  utf16Le: "UTF-16 LE",
  utf16Be: "UTF-16 BE",
  latin1: "Latin-1",
};
const LINE_ENDING_LABELS: Record<TextLineEnding, string> = { lf: "LF", crlf: "CRLF" };
const WRAP_STORAGE_KEY = "poros.editor.wrap";

function rememberedWrap(): boolean {
  try {
    return localStorage.getItem(WRAP_STORAGE_KEY) === "on";
  } catch {
    return false;
  }
}

function rememberWrap(wrap: boolean): void {
  try {
    localStorage.setItem(WRAP_STORAGE_KEY, wrap ? "on" : "off");
  } catch {
    // Storage can be unavailable; wrapping then starts off next time.
  }
}

function hasChanges(view: EditorView | null, baseline: Baseline | null, format: Format): boolean {
  if (!view || !baseline) return false;
  return (
    !baseline.doc ||
    !view.state.doc.eq(baseline.doc) ||
    format.encoding !== baseline.encoding ||
    format.lineEnding !== baseline.lineEnding
  );
}

interface Cursor {
  line: number;
  column: number;
  selected: number;
}

function cursorOf(state: EditorState): Cursor {
  const head = state.selection.main.head;
  const line = state.doc.lineAt(head);
  const selected = state.selection.ranges.reduce(
    (total, range) => total + range.to - range.from,
    0,
  );
  return { line: line.number, column: head - line.from + 1, selected };
}

const DEFAULT_FORMAT: Format = { encoding: "utf8", lineEnding: "lf" };

/** A text editor for one local or server file. Ctrl+S saves; server files upload on save. */
export default function EditorPane({ tab, visible, active }: EditorPaneProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const baselineRef = useRef<Baseline | null>(null);
  const stampRef = useRef<FileStamp | null>(null);
  const formatRef = useRef<Format>(DEFAULT_FORMAT);
  const savingRef = useRef(false);
  const [language] = useState(() => new Compartment());
  const [wrapping] = useState(() => new Compartment());
  const tabRef = useRef(tab);
  useLayoutEffect(() => {
    tabRef.current = tab;
  });

  const [phase, setPhase] = useState<"loading" | "ready" | "failed">("loading");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloads, setReloads] = useState(0);
  const [format, setFormat] = useState<Format>(DEFAULT_FORMAT);
  const [unsaved, setUnsaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<{ removed: boolean } | null>(null);
  const [cursor, setCursor] = useState<Cursor>({ line: 1, column: 1, selected: 0 });
  const [languageName, setLanguageName] = useState("Plain text");
  const [wrap, setWrap] = useState(rememberedWrap);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [confirmReload, setConfirmReload] = useState(false);
  const markUnsaved = useUnsavedStore((state) => state.mark);
  const where = tab.sessionId ? "on the server" : "on disk";

  const refreshUnsaved = useCallback(() => {
    setUnsaved(hasChanges(viewRef.current, baselineRef.current, formatRef.current));
  }, []);

  useEffect(() => markUnsaved(tab.id, unsaved), [tab.id, unsaved, markUnsaved]);

  const save = async (overwrite: boolean): Promise<boolean> => {
    const view = viewRef.current;
    if (!view || savingRef.current) return false;
    savingRef.current = true;
    setSaving(true);
    setSaveError(null);
    const doc = view.state.doc;
    const savedFormat = formatRef.current;
    try {
      const outcome = await editor.save({
        documentId: tabRef.current.documentId,
        text: doc.toString(),
        ...savedFormat,
        expected: overwrite ? null : stampRef.current,
      });
      if (outcome.status === "changed") {
        setConflict({ removed: outcome.current === null });
        return false;
      }
      stampRef.current = outcome.stamp;
      baselineRef.current = { doc, ...savedFormat };
      setConflict(null);
      refreshUnsaved();
      return true;
    } catch (caught) {
      setSaveError(toAppError(caught).message);
      return false;
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };
  const saveRef = useRef(save);
  useLayoutEffect(() => {
    saveRef.current = save;
  });

  useEffect(
    () =>
      registerEditor(tab.id, {
        save: () => saveRef.current(false),
        draft: () => {
          const view = viewRef.current;
          if (!view || !hasChanges(view, baselineRef.current, formatRef.current)) return null;
          return { text: view.state.doc.toString(), ...formatRef.current, stamp: stampRef.current };
        },
      }),
    [tab.id],
  );

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let cancelled = false;
    const { documentId, name, draft } = tabRef.current;
    // A draft comes along when the tab moved here from another window; a reload drops it.
    const fromDraft = draft && reloads === 0 ? draft : null;
    const opening = fromDraft
      ? editor.adopt(documentId).then(() => fromDraft)
      : editor.load(documentId);
    setPhase("loading");
    setLoadError(null);

    opening
      .then((document) => {
        if (cancelled) return;
        const extensions: Extension[] = [
          lineNumbers(),
          foldGutter(),
          highlightActiveLineGutter(),
          highlightSpecialChars(),
          history(),
          drawSelection(),
          dropCursor(),
          EditorState.allowMultipleSelections.of(true),
          indentOnInput(),
          syntaxHighlighting(syntaxColors),
          bracketMatching(),
          rectangularSelection(),
          crosshairCursor(),
          highlightActiveLine(),
          highlightSelectionMatches(),
          keymap.of([
            {
              key: "Mod-s",
              preventDefault: true,
              run: () => {
                void saveRef.current(false);
                return true;
              },
            },
            ...defaultKeymap,
            ...searchKeymap,
            ...historyKeymap,
            ...foldKeymap,
            indentWithTab,
          ]),
          language.of([]),
          wrapping.of(rememberedWrap() ? EditorView.lineWrapping : []),
          editorTheme,
          EditorView.updateListener.of((update) => {
            if (update.docChanged) refreshUnsaved();
            if (update.docChanged || update.selectionSet) setCursor(cursorOf(update.state));
          }),
        ];
        const view = new EditorView({
          parent: host,
          state: EditorState.create({ doc: document.text, extensions }),
        });
        viewRef.current = view;
        formatRef.current = { encoding: document.encoding, lineEnding: document.lineEnding };
        stampRef.current = document.stamp;
        baselineRef.current = { doc: fromDraft ? null : view.state.doc, ...formatRef.current };
        setFormat(formatRef.current);
        setCursor(cursorOf(view.state));
        setConflict(null);
        setSaveError(null);
        refreshUnsaved();
        setPhase("ready");

        const description = languageFor(name);
        setLanguageName(description?.name ?? "Plain text");
        void description
          ?.load()
          .then((support) => {
            if (viewRef.current === view) view.dispatch({ effects: language.reconfigure(support) });
          })
          .catch(() => undefined);

        if (fromDraft) {
          const withoutDraft = { ...tabRef.current };
          delete withoutDraft.draft;
          useLayoutStore.getState().replaceTab(withoutDraft.id, withoutDraft);
        }
      })
      .catch((caught) => {
        if (cancelled) return;
        setLoadError(toAppError(caught).message);
        setPhase("failed");
      });

    return () => {
      cancelled = true;
      viewRef.current?.destroy();
      viewRef.current = null;
    };
  }, [reloads, language, wrapping, refreshUnsaved]);

  useEffect(() => {
    if (visible && active && phase === "ready") viewRef.current?.focus();
  }, [visible, active, phase]);

  const changeFormat = (change: Partial<Format>) => {
    formatRef.current = { ...formatRef.current, ...change };
    setFormat(formatRef.current);
    refreshUnsaved();
  };

  const toggleWrap = () => {
    const next = !wrap;
    setWrap(next);
    rememberWrap(next);
    viewRef.current?.dispatch({
      effects: wrapping.reconfigure(next ? EditorView.lineWrapping : []),
    });
  };

  const reload = () => {
    if (hasChanges(viewRef.current, baselineRef.current, formatRef.current)) setConfirmReload(true);
    else setReloads((count) => count + 1);
  };

  const showMenu = (event: MouseEvent<HTMLButtonElement>, items: MenuItem[]) => {
    const anchor = event.currentTarget.getBoundingClientRect();
    setMenu({ x: anchor.left, y: anchor.top, items });
  };

  const encodingMenu = () =>
    (Object.keys(ENCODING_LABELS) as TextEncoding[]).map((encoding): MenuItem => ({
      label: ENCODING_LABELS[encoding],
      checked: format.encoding === encoding,
      onSelect: () => changeFormat({ encoding }),
    }));
  const lineEndingMenu = () =>
    (Object.keys(LINE_ENDING_LABELS) as TextLineEnding[]).map((lineEnding): MenuItem => ({
      label: lineEnding === "lf" ? "LF (Linux, macOS)" : "CRLF (Windows)",
      checked: format.lineEnding === lineEnding,
      onSelect: () => changeFormat({ lineEnding }),
    }));

  const ready = phase === "ready";
  return (
    <section className={["pane", "editor-pane", active && "is-active"].filter(Boolean).join(" ")}>
      <header className="pane-header">
        <div className="pane-title">
          <FileText size={15} />
          <span title={tab.path}>{tab.name}</span>
          {unsaved && <span className="editor-unsaved-mark" title="Unsaved changes" />}
        </div>
        <div className="pane-toolbar">
          <EditorButton
            label="Find and replace (Ctrl+F)"
            disabled={!ready}
            onClick={() => viewRef.current && openSearchPanel(viewRef.current)}
          >
            <Search size={15} />
          </EditorButton>
          <EditorButton
            label={`Reload from ${tab.sessionId ? "the server" : "disk"}`}
            disabled={phase === "loading"}
            onClick={reload}
          >
            <RotateCw size={15} />
          </EditorButton>
          <EditorButton
            label="Save (Ctrl+S)"
            disabled={!ready || saving}
            onClick={() => void save(false)}
          >
            <Save size={15} />
          </EditorButton>
        </div>
      </header>

      <div className="editor-location selectable" title={tab.path}>
        <span className="editor-origin">{tab.origin}</span>
        <span className="editor-path">{tab.path}</span>
      </div>

      {conflict && (
        <div className="editor-banner is-warning" role="alert">
          <TriangleAlert size={14} />
          <span>
            {conflict.removed
              ? `${tab.name} was deleted ${where} after you opened it.`
              : `${tab.name} changed ${where} since you opened it.`}
          </span>
          <button type="button" className="button" onClick={() => void save(true)}>
            {conflict.removed ? "Save it again" : "Overwrite"}
          </button>
          {!conflict.removed && (
            <button
              type="button"
              className="button"
              onClick={() => setReloads((count) => count + 1)}
            >
              Reload theirs
            </button>
          )}
          <button
            type="button"
            className="icon-button"
            aria-label="Dismiss"
            onClick={() => setConflict(null)}
          >
            <X size={13} />
          </button>
        </div>
      )}
      {saveError && (
        <div className="editor-banner is-error" role="alert">
          <TriangleAlert size={14} />
          <span className="selectable">Could not save: {saveError}</span>
          <button type="button" className="button" onClick={() => void save(false)}>
            Try again
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label="Dismiss"
            onClick={() => setSaveError(null)}
          >
            <X size={13} />
          </button>
        </div>
      )}

      <div className="editor-body">
        <div className="editor-host" ref={hostRef} />
        {phase === "loading" && <p className="editor-placeholder">Opening {tab.name}...</p>}
        {phase === "failed" && (
          <div className="pane-empty editor-placeholder">
            <FileText size={36} className="pane-empty-icon" />
            <p className="pane-empty-title">Could not open {tab.name}</p>
            <p className="pane-empty-detail selectable">{loadError}</p>
            <button
              type="button"
              className="button"
              onClick={() => setReloads((count) => count + 1)}
            >
              Try again
            </button>
          </div>
        )}
      </div>

      <footer className="pane-footer editor-footer">
        <span className={unsaved ? "editor-state is-unsaved" : "editor-state"}>
          {!ready ? "" : saving ? "Saving..." : unsaved ? "Unsaved changes" : "Saved"}
        </span>
        {ready && (
          <div className="editor-status">
            <span>
              Ln {cursor.line}, Col {cursor.column}
              {cursor.selected > 0 && ` (${cursor.selected} selected)`}
            </span>
            <span>{languageName}</span>
            <button
              type="button"
              className="editor-status-button"
              title="Encoding used when saving"
              onClick={(event) => showMenu(event, encodingMenu())}
            >
              {ENCODING_LABELS[format.encoding]}
            </button>
            <button
              type="button"
              className="editor-status-button"
              title="Line endings used when saving"
              onClick={(event) => showMenu(event, lineEndingMenu())}
            >
              {LINE_ENDING_LABELS[format.lineEnding]}
            </button>
            <button
              type="button"
              className={`editor-status-button ${wrap ? "is-on" : ""}`}
              title={wrap ? "Stop wrapping long lines" : "Wrap long lines"}
              aria-pressed={wrap}
              onClick={toggleWrap}
            >
              <WrapText size={13} />
            </button>
          </div>
        )}
      </footer>

      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />
      )}
      {confirmReload && (
        <ConfirmDialog
          title={`Reload ${tab.name}?`}
          confirmLabel="Discard and reload"
          danger
          onConfirm={async () => setReloads((count) => count + 1)}
          onClose={() => setConfirmReload(false)}
        >
          <p>Your unsaved changes will be lost.</p>
        </ConfirmDialog>
      )}
    </section>
  );
}

interface EditorButtonProps {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}

function EditorButton({ label, disabled, onClick, children }: EditorButtonProps) {
  return (
    <button
      type="button"
      className="icon-button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
