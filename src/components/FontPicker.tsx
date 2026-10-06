import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Check, ChevronDown, Search } from "lucide-react";
import { recommendedFonts, type FontKind, type FontOption } from "../lib/fonts";
import { fontStack } from "../state/themeStore";

type Row = { kind: "heading"; label: string } | { kind: "font"; name: string };

const ROW_HEIGHT = 30;
const POPOVER_WIDTH = 320;
const POPOVER_HEIGHT = 360;
const MIN_HEIGHT = 220;
const EDGE_MARGIN = 8;
const GAP = 4;

interface FontPickerProps {
  kind: FontKind;
  label: string;
  /** The picked family; empty for the theme's font or the default. */
  value: string;
  fonts: FontOption[] | null;
  onChange: (family: string) => void;
}

function buildRows(fonts: FontOption[], kind: FontKind, query: string): Row[] {
  const needle = query.trim().toLowerCase();
  const matches = (option: FontOption) => option.name.toLowerCase().includes(needle);
  const recommended = recommendedFonts(fonts, kind).filter(matches);
  const listed = new Set(recommended.map((option) => option.name));
  const rest = fonts.filter((option) => !listed.has(option.name) && matches(option));
  const sections: [string, FontOption[]][] =
    kind === "mono"
      ? [
          ["Recommended", recommended],
          ["Fixed width", rest.filter((option) => option.monospaced)],
          ["Other fonts", rest.filter((option) => !option.monospaced)],
        ]
      : [
          ["Recommended", recommended],
          ["All fonts", rest],
        ];
  return [
    ...(needle ? [] : [{ kind: "font", name: "" } as const]),
    ...sections.flatMap(([label, options]): Row[] =>
      options.length === 0
        ? []
        : [
            { kind: "heading", label },
            ...options.map((option) => ({ kind: "font", name: option.name }) as const),
          ],
    ),
  ];
}

/** A searchable list of installed fonts, each name drawn in its own face. */
export function FontPicker({ kind, label, value, fonts, onChange }: FontPickerProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [anchor, setAnchor] = useState<DOMRect | null>(null);

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="font-picker-button"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={anchor !== null}
        style={{ fontFamily: value ? fontStack(value, kind) : `var(--font-${kind})` }}
        onClick={() =>
          setAnchor((current) =>
            current ? null : (buttonRef.current?.getBoundingClientRect() ?? null),
          )
        }
      >
        <span>{value || "Default"}</span>
        <ChevronDown size={14} />
      </button>
      {anchor && (
        <FontList
          anchor={anchor}
          kind={kind}
          label={label}
          value={value}
          fonts={fonts}
          onPick={(family) => {
            setAnchor(null);
            buttonRef.current?.focus();
            if (family !== value) onChange(family);
          }}
          onDismiss={(refocus) => {
            setAnchor(null);
            if (refocus) buttonRef.current?.focus();
          }}
          isOwnButton={(target) => buttonRef.current?.contains(target) ?? false}
          onAnchorMoved={() => setAnchor(buttonRef.current?.getBoundingClientRect() ?? null)}
        />
      )}
    </>
  );
}

interface FontListProps {
  anchor: DOMRect;
  kind: FontKind;
  label: string;
  value: string;
  fonts: FontOption[] | null;
  onPick: (family: string) => void;
  onDismiss: (refocus: boolean) => void;
  isOwnButton: (target: Node) => boolean;
  /** The page scrolled, so the list follows its button. */
  onAnchorMoved: () => void;
}

function FontList({
  anchor,
  kind,
  label,
  value,
  fonts,
  onPick,
  onDismiss,
  isOwnButton,
  onAnchorMoved,
}: FontListProps) {
  const popoverRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const rows = useMemo(() => buildRows(fonts ?? [], kind, query), [fonts, kind, query]);
  const fontIndexes = useMemo(
    () => rows.flatMap((row, index) => (row.kind === "font" ? [index] : [])),
    [rows],
  );
  const [activeIndex, setActiveIndex] = useState(() =>
    Math.max(
      0,
      rows.findIndex((row) => row.kind === "font" && row.name === value),
    ),
  );
  const active = fontIndexes.includes(activeIndex) ? activeIndex : (fontIndexes[0] ?? -1);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  useLayoutEffect(() => {
    // Plain focusing would scroll the settings page, which closes the list.
    searchRef.current?.focus({ preventScroll: true });
  }, []);

  useLayoutEffect(() => {
    if (active >= 0) virtualizer.scrollToIndex(active, { align: "auto" });
  }, [active, virtualizer]);

  useEffect(() => {
    const dismissOnOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!popoverRef.current?.contains(target) && !isOwnButton(target)) onDismiss(false);
    };
    const followScroll = (event: Event) => {
      if (!popoverRef.current?.contains(event.target as Node)) onAnchorMoved();
    };
    const dismiss = () => onDismiss(false);
    window.addEventListener("mousedown", dismissOnOutside, true);
    window.addEventListener("scroll", followScroll, true);
    window.addEventListener("resize", dismiss);
    return () => {
      window.removeEventListener("mousedown", dismissOnOutside, true);
      window.removeEventListener("scroll", followScroll, true);
      window.removeEventListener("resize", dismiss);
    };
  }, [onDismiss, isOwnButton, onAnchorMoved]);

  const roomBelow = window.innerHeight - anchor.bottom - GAP - EDGE_MARGIN;
  const roomAbove = anchor.top - GAP - EDGE_MARGIN;
  const below = roomBelow >= Math.min(POPOVER_HEIGHT, MIN_HEIGHT) || roomBelow >= roomAbove;
  const height = Math.min(POPOVER_HEIGHT, below ? roomBelow : roomAbove);
  const style: CSSProperties = {
    left: Math.max(
      EDGE_MARGIN,
      Math.min(anchor.right - POPOVER_WIDTH, window.innerWidth - POPOVER_WIDTH - EDGE_MARGIN),
    ),
    top: below ? anchor.bottom + GAP : anchor.top - GAP - height,
    width: POPOVER_WIDTH,
    height,
  };

  const moveActive = (step: number) => {
    if (fontIndexes.length === 0) return;
    const position = fontIndexes.indexOf(active);
    const next = Math.min(fontIndexes.length - 1, Math.max(0, position + step));
    setActiveIndex(fontIndexes[next]);
  };

  return (
    <div
      ref={popoverRef}
      className="font-picker"
      style={style}
      onKeyDown={(event) => {
        const pageSize = Math.floor(height / ROW_HEIGHT) - 2;
        const moves: Record<string, number> = {
          ArrowDown: 1,
          ArrowUp: -1,
          PageDown: pageSize,
          PageUp: -pageSize,
        };
        if (event.key in moves) moveActive(moves[event.key]);
        else if (event.key === "Enter") {
          const row = rows[active];
          if (row?.kind === "font") onPick(row.name);
        } else if (event.key === "Escape" || event.key === "Tab") {
          onDismiss(event.key === "Escape");
          if (event.key === "Tab") return;
        } else return;
        // Also keeps Escape from closing the settings dialog.
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <label className="font-picker-search">
        <Search size={13} />
        <input
          ref={searchRef}
          value={query}
          placeholder={fonts ? `Search ${fonts.length} fonts` : "Search fonts"}
          spellCheck={false}
          aria-label={`Search ${label.toLowerCase()}`}
          aria-controls="font-picker-options"
          onChange={(event) => setQuery(event.target.value)}
        />
      </label>
      <div
        ref={listRef}
        id="font-picker-options"
        className="font-picker-list"
        role="listbox"
        aria-label={label}
      >
        {fonts === null && <p className="font-picker-note">Reading installed fonts...</p>}
        {fonts !== null && rows.length === 0 && (
          <p className="font-picker-note">No installed font matches</p>
        )}
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index];
            const position = { transform: `translateY(${item.start}px)`, height: ROW_HEIGHT };
            if (row.kind === "heading") {
              return (
                <div key={`heading-${row.label}`} className="font-picker-heading" style={position}>
                  {row.label}
                </div>
              );
            }
            const selected = row.name === value;
            return (
              <div
                key={row.name || "default"}
                role="option"
                aria-selected={selected}
                className={`font-picker-option ${item.index === active ? "is-active" : ""}`}
                style={{
                  ...position,
                  fontFamily: row.name ? fontStack(row.name, kind) : `var(--font-${kind}-default)`,
                }}
                onMouseMove={() => item.index !== active && setActiveIndex(item.index)}
                onClick={() => onPick(row.name)}
              >
                <span>{row.name || "Default"}</span>
                {selected && <Check size={14} />}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
