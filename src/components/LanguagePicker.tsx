import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { Check, ChevronDown } from "lucide-react";
import { resolveLanguage, SYSTEM_LANGUAGE, TRANSLATIONS, useT, type LanguageChoice } from "../i18n";

const OPTION_HEIGHT = 30;
const LIST_PADDING = 8;
const POPOVER_WIDTH = 240;
const EDGE_MARGIN = 8;
const GAP = 4;

interface LanguageOption {
  choice: LanguageChoice;
  /** The tag the option's text is written in, so screen readers and fonts read it right. */
  lang: string | undefined;
  label: string;
}

interface LanguagePickerProps {
  label: string;
  value: LanguageChoice;
  onChange: (choice: LanguageChoice) => void;
}

/** System, then every shipped language, each named in itself. */
export function LanguagePicker({ label, value, onChange }: LanguagePickerProps) {
  const t = useT();
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const system = resolveLanguage(SYSTEM_LANGUAGE);
  const options: LanguageOption[] = [
    {
      choice: SYSTEM_LANGUAGE,
      lang: undefined,
      label: t("settings.languageSystem", { language: system.name }),
    },
    ...TRANSLATIONS.map((translation) => ({
      choice: translation.locale,
      lang: translation.locale,
      label: translation.name,
    })),
  ];
  const shown = options.find((option) => option.choice === value) ?? options[0];

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="font-picker-button"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={anchor !== null}
        onClick={() =>
          setAnchor((current) =>
            current ? null : (buttonRef.current?.getBoundingClientRect() ?? null),
          )
        }
      >
        <span lang={shown.lang}>{shown.label}</span>
        <ChevronDown size={14} />
      </button>
      {anchor && (
        <LanguageList
          anchor={anchor}
          label={label}
          value={value}
          options={options}
          onPick={(choice) => {
            setAnchor(null);
            buttonRef.current?.focus();
            if (choice !== value) onChange(choice);
          }}
          onDismiss={(refocus) => {
            setAnchor(null);
            if (refocus) buttonRef.current?.focus();
          }}
          isOwnButton={(target) => buttonRef.current?.contains(target) ?? false}
        />
      )}
    </>
  );
}

interface LanguageListProps {
  anchor: DOMRect;
  label: string;
  value: LanguageChoice;
  options: LanguageOption[];
  onPick: (choice: LanguageChoice) => void;
  onDismiss: (refocus: boolean) => void;
  isOwnButton: (target: Node) => boolean;
}

function LanguageList({
  anchor,
  label,
  value,
  options,
  onPick,
  onDismiss,
  isOwnButton,
}: LanguageListProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(() =>
    Math.max(
      0,
      options.findIndex((option) => option.choice === value),
    ),
  );

  useLayoutEffect(() => {
    // Plain focusing would scroll the settings page, which closes the list.
    listRef.current?.focus({ preventScroll: true });
  }, []);

  useEffect(() => {
    const dismissOnOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!listRef.current?.contains(target) && !isOwnButton(target)) onDismiss(false);
    };
    const dismiss = () => onDismiss(false);
    window.addEventListener("mousedown", dismissOnOutside, true);
    window.addEventListener("scroll", dismiss, true);
    window.addEventListener("resize", dismiss);
    return () => {
      window.removeEventListener("mousedown", dismissOnOutside, true);
      window.removeEventListener("scroll", dismiss, true);
      window.removeEventListener("resize", dismiss);
    };
  }, [onDismiss, isOwnButton]);

  const height = options.length * OPTION_HEIGHT + LIST_PADDING;
  const roomBelow = window.innerHeight - anchor.bottom - GAP - EDGE_MARGIN;
  const below = roomBelow >= height || roomBelow >= anchor.top;
  const style: CSSProperties = {
    left: Math.max(
      EDGE_MARGIN,
      Math.min(anchor.right - POPOVER_WIDTH, window.innerWidth - POPOVER_WIDTH - EDGE_MARGIN),
    ),
    top: below ? anchor.bottom + GAP : anchor.top - GAP - height,
    width: POPOVER_WIDTH,
  };
  const activeId = `language-option-${active}`;

  return (
    <div
      ref={listRef}
      className="font-picker language-list"
      style={style}
      role="listbox"
      aria-label={label}
      aria-activedescendant={activeId}
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown") setActive(Math.min(options.length - 1, active + 1));
        else if (event.key === "ArrowUp") setActive(Math.max(0, active - 1));
        else if (event.key === "Home") setActive(0);
        else if (event.key === "End") setActive(options.length - 1);
        else if (event.key === "Enter" || event.key === " ") onPick(options[active].choice);
        else if (event.key === "Escape" || event.key === "Tab") {
          onDismiss(event.key === "Escape");
          if (event.key === "Tab") return;
        } else return;
        // Also keeps Escape from closing the settings dialog.
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      {options.map((option, index) => {
        const selected = option.choice === value;
        return (
          <div
            key={option.choice}
            id={`language-option-${index}`}
            role="option"
            lang={option.lang}
            aria-selected={selected}
            className={`language-option ${index === active ? "is-active" : ""}`}
            onMouseMove={() => index !== active && setActive(index)}
            onClick={() => onPick(option.choice)}
          >
            <span>{option.label}</span>
            {selected && <Check size={14} />}
          </div>
        );
      })}
    </div>
  );
}
