import { useEffect, useLayoutEffect, useRef, useState, type MouseEvent } from "react";
import { ClipboardPaste, Copy, Eraser, RotateCw, SquareTerminal, TextSelect } from "lucide-react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal, type ITheme } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { terminal as terminals, toAppError } from "../lib/ipc";
import { findTab, type TerminalTab } from "../lib/layout";
import { terminalTheme } from "../lib/terminalTheme";
import type { TerminalEvent } from "../lib/types";
import { useLayoutStore } from "../state/layoutStore";
import { useSettingsStore } from "../state/settingsStore";
import { closeTerminalTab } from "../state/terminalActions";
import { useToastStore } from "../state/toastStore";
import { ContextMenu, type MenuItem } from "./ContextMenu";

interface TerminalPaneProps {
  tab: TerminalTab;
  visible: boolean;
  active: boolean;
}

type Phase = "connecting" | "running" | "ended" | "failed";

const SCROLLBACK_LINES = 5000;
// Windows terminals copy with Ctrl+C when text is selected and paste with Ctrl+V; elsewhere
// those keys belong to the shell.
const WINDOWS_KEYS = navigator.userAgent.includes("Windows");

/** The terminal colors for the theme on screen now. */
function currentTheme(): ITheme {
  const probe = document.createElement("span");
  probe.style.display = "none";
  document.body.append(probe);
  const style = getComputedStyle(probe);
  const theme = terminalTheme((variable) => {
    probe.style.color = `var(${variable})`;
    return style.color || null;
  });
  probe.remove();
  return theme;
}

function monoFont(): string {
  const font = getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim();
  return font || "monospace";
}

/** Fits the terminal to its pane, unless the pane is hidden and has no size to fit. */
function fitShown(host: HTMLElement, fit: FitAddon): void {
  if (host.clientWidth > 0 && host.clientHeight > 0) fit.fit();
}

/** A shell on the server, in a terminal that follows the theme and the monospace font. */
export default function TerminalPane({ tab, visible, active }: TerminalPaneProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const pasteRef = useRef<() => Promise<void>>(async () => undefined);
  const copyRef = useRef<() => void>(() => undefined);
  const terminalIdRef = useRef(tab.terminalId);
  const tabRef = useRef(tab);
  useLayoutEffect(() => {
    tabRef.current = tab;
  });
  const fontSize = useSettingsStore((state) => state.settings.appearance.fontSize);
  const fontSizeRef = useRef(fontSize);
  const [phase, setPhase] = useState<Phase>("connecting");
  const [message, setMessage] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [restarting, setRestarting] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const showToast = useToastStore((state) => state.show);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;
    const xterm = new Terminal({
      cursorBlink: true,
      fontFamily: monoFont(),
      fontSize: fontSizeRef.current,
      scrollback: SCROLLBACK_LINES,
      macOptionIsMeta: true,
      theme: currentTheme(),
    });
    const fit = new FitAddon();
    xterm.loadAddon(fit);
    xterm.open(host);
    xtermRef.current = xterm;
    fitRef.current = fit;
    fitShown(host, fit);
    setPhase("connecting");
    setMessage(null);

    const send = (data: string) => {
      const id = terminalIdRef.current;
      if (id) void terminals.write(id, data).catch(() => undefined);
    };
    const onEvent = (event: TerminalEvent) => {
      if (disposed) return;
      if (event.type === "output") {
        xterm.write(event.data);
      } else {
        setPhase("ended");
        setMessage(event.message);
      }
    };

    const connect = async () => {
      try {
        const known = terminalIdRef.current;
        if (known) {
          await terminals.attach(known, onEvent);
          await terminals.resize(known, xterm.cols, xterm.rows);
        } else {
          const { sessionId } = tabRef.current;
          const info = await terminals.open(sessionId, xterm.cols, xterm.rows, onEvent);
          const layout = useLayoutStore.getState();
          if (disposed || !findTab(layout.root, tabRef.current.id)) {
            void terminals.close(info.id).catch(() => undefined);
            return;
          }
          terminalIdRef.current = info.id;
          layout.replaceTab(tabRef.current.id, { ...tabRef.current, terminalId: info.id });
        }
        if (!disposed) setPhase((current) => (current === "connecting" ? "running" : current));
      } catch (caught) {
        if (disposed) return;
        setPhase("failed");
        setMessage(toAppError(caught).message);
      }
    };
    void connect();

    const copySelection = () => {
      const selection = xterm.getSelection();
      if (selection) void navigator.clipboard.writeText(selection);
    };
    const pasteFromClipboard = async () => {
      try {
        xterm.paste(await navigator.clipboard.readText());
      } catch {
        showToast("info", "Could not read the clipboard. Paste with Shift+Insert instead.");
      }
    };
    pasteRef.current = pasteFromClipboard;
    copyRef.current = copySelection;
    const subscriptions = [
      xterm.onData(send),
      xterm.onResize(({ cols, rows }) => {
        const id = terminalIdRef.current;
        if (id) void terminals.resize(id, cols, rows).catch(() => undefined);
      }),
    ];
    xterm.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      const key = event.key.toLowerCase();
      const control = event.ctrlKey && !event.altKey && !event.metaKey;
      if (control && key === "c" && (event.shiftKey || (WINDOWS_KEYS && xterm.hasSelection()))) {
        copySelection();
        if (!event.shiftKey) xterm.clearSelection();
        event.preventDefault();
        return false;
      }
      // Left to the web view, which pastes into the terminal.
      if ((event.shiftKey && key === "insert") || (control && WINDOWS_KEYS && key === "v")) {
        return false;
      }
      if (control && event.shiftKey && key === "v") {
        event.preventDefault();
        void pasteFromClipboard();
        return false;
      }
      return true;
    });

    const resizing = new ResizeObserver(() => fitShown(host, fit));
    resizing.observe(host);
    const restyle = () => {
      xterm.options.theme = currentTheme();
      xterm.options.fontFamily = monoFont();
    };
    // Themes apply by changing the variables on the root element.
    const themeChanges = new MutationObserver(() => requestAnimationFrame(restyle));
    themeChanges.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["style", "class", "data-theme"],
    });
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    scheme.addEventListener("change", restyle);

    return () => {
      disposed = true;
      scheme.removeEventListener("change", restyle);
      themeChanges.disconnect();
      resizing.disconnect();
      for (const subscription of subscriptions) subscription.dispose();
      xterm.dispose();
      xtermRef.current = null;
      fitRef.current = null;
    };
  }, [attempt, showToast]);

  useEffect(() => {
    fontSizeRef.current = fontSize;
    const xterm = xtermRef.current;
    const fit = fitRef.current;
    const host = hostRef.current;
    if (!xterm || !fit || !host || xterm.options.fontSize === fontSize) return;
    xterm.options.fontSize = fontSize;
    fitShown(host, fit);
  }, [fontSize]);

  useEffect(() => {
    if (visible && active) xtermRef.current?.focus();
  }, [visible, active, phase]);

  const restart = async () => {
    const id = terminalIdRef.current;
    if (!id) return;
    setRestarting(true);
    try {
      await terminals.restart(id);
      xtermRef.current?.write("\r\n");
      setPhase("running");
      setMessage(null);
      xtermRef.current?.focus();
    } catch (caught) {
      setMessage(toAppError(caught).message);
    } finally {
      setRestarting(false);
    }
  };

  const showMenu = (event: MouseEvent) => {
    event.preventDefault();
    const xterm = xtermRef.current;
    const items: MenuItem[] = [
      {
        label: "Copy",
        icon: <Copy size={14} />,
        shortcut: "Ctrl+Shift+C",
        disabled: !xterm?.hasSelection(),
        onSelect: () => copyRef.current(),
      },
      {
        label: "Paste",
        icon: <ClipboardPaste size={14} />,
        shortcut: "Ctrl+Shift+V",
        disabled: phase !== "running",
        onSelect: () => void pasteRef.current(),
      },
      {
        label: "Select all",
        icon: <TextSelect size={14} />,
        onSelect: () => xterm?.selectAll(),
      },
      "separator",
      {
        label: "Clear",
        icon: <Eraser size={14} />,
        onSelect: () => xterm?.clear(),
      },
    ];
    setMenu({ x: event.clientX, y: event.clientY, items });
  };

  return (
    <section className={["pane", "terminal-pane", active && "is-active"].filter(Boolean).join(" ")}>
      <header className="pane-header">
        <div className="pane-title">
          <SquareTerminal size={15} />
          <span title={tab.label}>{tab.label}</span>
        </div>
        <span className="terminal-state">
          {phase === "connecting" ? "Connecting..." : phase === "running" ? "Connected" : ""}
        </span>
      </header>
      <div className="terminal-body" onContextMenu={showMenu}>
        <div className="terminal-host" ref={hostRef} />
        {phase === "failed" && (
          <div className="pane-overlay">
            <SquareTerminal size={36} className="pane-empty-icon" />
            <p className="pane-empty-title">Could not open a terminal</p>
            <p className="pane-empty-detail selectable">{message}</p>
            <div className="dialog-actions">
              <button type="button" className="button" onClick={() => closeTerminalTab(tab.id)}>
                Close tab
              </button>
              <button
                type="button"
                className="button button-primary"
                onClick={() => setAttempt((count) => count + 1)}
              >
                Try again
              </button>
            </div>
          </div>
        )}
      </div>
      {phase === "ended" && (
        <div className="terminal-ended" role="status">
          <span>{message}</span>
          <button type="button" className="button" onClick={() => closeTerminalTab(tab.id)}>
            Close tab
          </button>
          <button
            type="button"
            className="button button-primary"
            disabled={restarting}
            onClick={() => void restart()}
          >
            <RotateCw size={13} className={restarting ? "spin" : ""} />
            Restart
          </button>
        </div>
      )}
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />
      )}
    </section>
  );
}
