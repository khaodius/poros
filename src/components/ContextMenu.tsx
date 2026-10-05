import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

export type MenuItem =
  | {
      label: string;
      icon?: ReactNode;
      shortcut?: string;
      disabled?: boolean;
      danger?: boolean;
      onSelect: () => void;
    }
  | "separator";

interface ContextMenuProps {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const { width, height } = menu.getBoundingClientRect();
    setPosition({
      left: Math.max(4, Math.min(x, window.innerWidth - width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - height - 4)),
    });
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [x, y]);

  useEffect(() => {
    const closeOnOutside = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    const closeOnKey = (event: KeyboardEvent) => {
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

  const moveFocus = (step: number) => {
    const buttons = Array.from(
      menuRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [],
    );
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    buttons[(current + step + buttons.length) % buttons.length]?.focus();
  };

  return (
    <div
      ref={menuRef}
      className="context-menu"
      role="menu"
      style={position}
      onContextMenu={(event) => event.preventDefault()}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown") moveFocus(1);
        else if (event.key === "ArrowUp") moveFocus(-1);
        else return;
        event.preventDefault();
      }}
    >
      {items.map((item, index) =>
        item === "separator" ? (
          <div key={`separator-${index}`} className="context-menu-separator" />
        ) : (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            className={`context-menu-item ${item.danger ? "danger" : ""}`}
            disabled={item.disabled}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            <span className="context-menu-icon">{item.icon}</span>
            <span className="context-menu-label">{item.label}</span>
            {item.shortcut && <kbd>{item.shortcut}</kbd>}
          </button>
        ),
      )}
    </div>
  );
}
