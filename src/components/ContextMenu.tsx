import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { Check, ChevronRight } from "lucide-react";
import { rememberFocus } from "../lib/focus";

interface MenuAction {
  label: string;
  icon?: ReactNode;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  /** Makes the item a checkbox showing this state. */
  checked?: boolean;
  onSelect: () => void;
}

interface MenuSubmenu {
  label: string;
  icon?: ReactNode;
  disabled?: boolean;
  items: MenuItem[];
}

export type MenuItem = MenuAction | MenuSubmenu | "separator";

interface ContextMenuProps {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

const EDGE_MARGIN = 4;
const LEVEL_ITEMS =
  ":scope > .context-menu-item:not(:disabled), :scope > .context-menu-entry > .context-menu-item:not(:disabled)";

function levelButtons(panel: HTMLElement | null): HTMLButtonElement[] {
  return Array.from(panel?.querySelectorAll<HTMLButtonElement>(LEVEL_ITEMS) ?? []);
}

export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });

  // Noted as the menu first renders, before it focuses its first item.
  const [returnFocus] = useState(rememberFocus);

  useEffect(() => {
    const menu = menuRef.current;
    return () => returnFocus(menu);
  }, [returnFocus]);

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const { width, height } = menu.getBoundingClientRect();
    setPosition({
      left: Math.max(EDGE_MARGIN, Math.min(x, window.innerWidth - width - EDGE_MARGIN)),
      top: Math.max(EDGE_MARGIN, Math.min(y, window.innerHeight - height - EDGE_MARGIN)),
    });
    levelButtons(menu)[0]?.focus();
  }, [x, y]);

  useEffect(() => {
    const closeOnOutside = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    const closeOnKey = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", closeOnOutside, true);
    window.addEventListener("keydown", closeOnKey, true);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("mousedown", closeOnOutside, true);
      window.removeEventListener("keydown", closeOnKey, true);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  return (
    <MenuPanel
      panelRef={menuRef}
      className="context-menu"
      style={position}
      items={items}
      onClose={onClose}
    />
  );
}

interface MenuPanelProps {
  panelRef: RefObject<HTMLDivElement | null>;
  className: string;
  style?: CSSProperties;
  items: MenuItem[];
  onClose: () => void;
  /** For a submenu: closes it and returns focus to the item that opened it. */
  onLeave?: () => void;
}

function MenuPanel({ panelRef, className, style, items, onClose, onLeave }: MenuPanelProps) {
  const [openSubmenu, setOpenSubmenu] = useState<{ label: string; focus: boolean } | null>(null);

  const moveFocus = (step: number) => {
    const buttons = levelButtons(panelRef.current);
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    buttons[(current + step + buttons.length) % buttons.length]?.focus();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const focusedSubmenu = (event.target as HTMLElement).dataset.submenu;
    if (event.key === "ArrowDown") moveFocus(1);
    else if (event.key === "ArrowUp") moveFocus(-1);
    else if (event.key === "ArrowRight" && focusedSubmenu) {
      setOpenSubmenu({ label: focusedSubmenu, focus: true });
    } else if (event.key === "ArrowLeft" && onLeave) onLeave();
    else return;
    event.preventDefault();
    event.stopPropagation();
  };

  return (
    <div
      ref={panelRef}
      className={className}
      role="menu"
      style={style}
      onContextMenu={(event) => event.preventDefault()}
      onKeyDown={handleKeyDown}
    >
      {items.map((item, index) => {
        if (item === "separator") {
          return <div key={`separator-${index}`} className="context-menu-separator" />;
        }
        if ("items" in item) {
          const open = openSubmenu?.label === item.label;
          return (
            <div
              key={item.label}
              className="context-menu-entry"
              onMouseEnter={() =>
                !item.disabled && setOpenSubmenu({ label: item.label, focus: false })
              }
            >
              <button
                type="button"
                role="menuitem"
                aria-haspopup="menu"
                aria-expanded={open}
                className={`context-menu-item ${open ? "is-open" : ""}`}
                disabled={item.disabled}
                data-submenu={item.label}
                onClick={() => setOpenSubmenu(open ? null : { label: item.label, focus: true })}
              >
                <span className="context-menu-icon">{item.icon}</span>
                <span className="context-menu-label">{item.label}</span>
                <ChevronRight size={14} className="context-menu-arrow" />
              </button>
              {open && (
                <Submenu
                  items={item.items}
                  focusFirst={openSubmenu.focus}
                  onClose={onClose}
                  onLeave={() => {
                    setOpenSubmenu(null);
                    levelButtons(panelRef.current)
                      .find((button) => button.dataset.submenu === item.label)
                      ?.focus();
                  }}
                />
              )}
            </div>
          );
        }
        return (
          <button
            key={item.label}
            type="button"
            role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-checked={item.checked}
            className={`context-menu-item ${item.danger ? "danger" : ""}`}
            disabled={item.disabled}
            onMouseEnter={() => setOpenSubmenu(null)}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            <span className="context-menu-icon">
              {item.checked ? <Check size={14} /> : item.icon}
            </span>
            <span className="context-menu-label">{item.label}</span>
            {item.shortcut && <kbd>{item.shortcut}</kbd>}
          </button>
        );
      })}
    </div>
  );
}

interface SubmenuProps {
  items: MenuItem[];
  focusFirst: boolean;
  onClose: () => void;
  onLeave: () => void;
}

/** Opens beside its item, on the left when there is no room on the right. */
function Submenu({ items, focusFirst, onClose, onLeave }: SubmenuProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const [style, setStyle] = useState<CSSProperties>({});

  useLayoutEffect(() => {
    const panel = panelRef.current;
    if (!panel) return;
    const bounds = panel.getBoundingClientRect();
    const overflowBottom = bounds.bottom - (window.innerHeight - EDGE_MARGIN);
    setStyle({
      ...(bounds.right > window.innerWidth - EDGE_MARGIN && {
        left: "auto",
        right: "calc(100% + 4px)",
      }),
      ...(overflowBottom > 0 && { top: `calc(-4px - ${overflowBottom}px)` }),
    });
  }, []);

  useLayoutEffect(() => {
    if (focusFirst) levelButtons(panelRef.current)[0]?.focus();
  }, [focusFirst]);

  return (
    <MenuPanel
      panelRef={panelRef}
      className="context-menu is-submenu"
      style={style}
      items={items}
      onClose={onClose}
      onLeave={onLeave}
    />
  );
}
