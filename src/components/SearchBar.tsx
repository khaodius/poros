import {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import {
  CaseSensitive,
  ChevronDown,
  CircleStop,
  FolderSearch,
  History,
  LoaderCircle,
  Regex,
  SlidersHorizontal,
  TriangleAlert,
  X,
} from "lucide-react";
import { isConnectionLost, type SearchView } from "../hooks/useSearch";
import { formatCount } from "../lib/deletion";
import { baseName } from "../lib/path";
import {
  DATE_PRESETS,
  DEPTH_PRESETS,
  KIND_LABELS,
  SEARCH_LIMIT,
  SIZE_PRESETS,
  dateNarrows,
  dateRange,
  describeDate,
  describeDepth,
  describeSearch,
  describeSize,
  sizeNarrows,
  sizeRange,
  type SearchForm,
} from "../lib/search";
import type { OperationFailure, SearchKind } from "../lib/types";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

interface SearchBarProps {
  form: SearchForm;
  /** The folder a search starts in. */
  root: string;
  /** The search whose results the pane shows, if it does. */
  view: SearchView | null;
  recent: readonly SearchForm[];
  /** Why the last search could not start. */
  problem: string | null;
  /** Grows by one each time the name field should take the keyboard. */
  focusRequest: number;
  onChange: (form: SearchForm) => void;
  /** Searches for `form`, or for what the bar shows. */
  onSearch: (form?: SearchForm) => void;
  onStop: () => void;
  onClose: () => void;
  /** Down from the name field, into the results. */
  onLeave: () => void;
}

interface OpenMenu {
  x: number;
  y: number;
  items: MenuItem[];
}

type RangeEditor = "size" | "date" | null;

export function SearchBar({
  form,
  root,
  view,
  recent,
  problem,
  focusRequest,
  onChange,
  onSearch,
  onStop,
  onClose,
  onLeave,
}: SearchBarProps) {
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [editing, setEditing] = useState<RangeEditor>(null);
  const [showingProblems, setShowingProblems] = useState(false);
  const searching = view?.status === "searching";

  // Only when asked: coming back to the results by Back leaves the keyboard in the list.
  useEffect(() => {
    if (focusRequest === 0) return;
    inputRef.current?.focus({ preventScroll: true });
    inputRef.current?.select();
  }, [focusRequest]);

  /** Applies a change, searching again at once while results show. */
  const change = (next: SearchForm) => {
    onChange(next);
    if (view) onSearch(next);
  };

  const openMenu = (event: MouseEvent<HTMLElement>, items: MenuItem[]) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    setMenu({ x: bounds.left, y: bounds.bottom + 4, items });
  };

  const kindItems = (): MenuItem[] =>
    (Object.keys(KIND_LABELS) as SearchKind[]).map((kind) => ({
      label: KIND_LABELS[kind],
      checked: form.kind === kind,
      onSelect: () => change({ ...form, kind }),
    }));

  const sizeItems = (): MenuItem[] => [
    ...SIZE_PRESETS.map((preset) => ({
      label: preset.label,
      checked: form.size.preset === preset.id,
      onSelect: () => change({ ...form, size: { ...form.size, preset: preset.id } }),
    })),
    "separator",
    {
      label: t("search.customSize"),
      checked: form.size.preset === "custom",
      onSelect: () => setEditing("size"),
    },
  ];

  const dateItems = (): MenuItem[] => [
    ...DATE_PRESETS.map((preset) => ({
      label: preset.label,
      checked: form.date.preset === preset.id,
      onSelect: () => change({ ...form, date: { ...form.date, preset: preset.id } }),
    })),
    "separator",
    {
      label: t("search.customDates"),
      checked: form.date.preset === "custom",
      onSelect: () => setEditing("date"),
    },
  ];

  const depthItems = (): MenuItem[] =>
    DEPTH_PRESETS.map((preset) => ({
      label: preset.label,
      checked: form.depth === preset.depth,
      onSelect: () => change({ ...form, depth: preset.depth }),
    }));

  const optionItems = (): MenuItem[] => [
    {
      label: t("search.searchHiddenFolders"),
      checked: form.hidden,
      onSelect: () => change({ ...form, hidden: !form.hidden }),
    },
    {
      label: t("search.followLinksToFolders"),
      checked: form.followLinks,
      onSelect: () => change({ ...form, followLinks: !form.followLinks }),
    },
  ];

  const recentItems = (): MenuItem[] =>
    recent.map((earlier) => ({
      label: describeSearch(earlier),
      onSelect: () => {
        onChange(earlier);
        onSearch(earlier);
      },
    }));

  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSearch();
  };

  const optionsCount = Number(form.hidden) + Number(form.followLinks);

  return (
    <div className={`search-bar ${view ? "has-results" : ""}`} role="search">
      <form className="search-row" onSubmit={submit}>
        <label className="search-field" title={t("search.namesToFindTextOr")}>
          <FolderSearch size={14} aria-hidden />
          <input
            ref={inputRef}
            value={form.pattern}
            placeholder={`Search in ${baseName(root) || root}`}
            aria-label={t("search.nameToSearchFor")}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => onChange({ ...form, pattern: event.target.value })}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                if (searching) onStop();
                else onClose();
              } else if (event.key === "F5") {
                // Searches again, where elsewhere the pane would reload.
                event.preventDefault();
                event.stopPropagation();
                onSearch();
              } else if (event.key === "ArrowDown" && event.altKey && recent.length > 0) {
                event.preventDefault();
                const bounds = event.currentTarget.getBoundingClientRect();
                setMenu({ x: bounds.left, y: bounds.bottom + 6, items: recentItems() });
              } else if (event.key === "ArrowDown" && view) {
                event.preventDefault();
                onLeave();
              }
            }}
          />
          {recent.length > 0 && (
            <button
              type="button"
              className="icon-button"
              title={t("search.recentSearchesAltDown")}
              aria-label={t("search.recentSearches")}
              onClick={(event) => openMenu(event, recentItems())}
            >
              <History size={13} />
            </button>
          )}
          <button
            type="button"
            className={`icon-button search-switch ${form.patternKind === "regex" ? "is-pressed" : ""}`}
            title={t("search.regularExpression")}
            aria-label={t("search.regularExpression")}
            aria-pressed={form.patternKind === "regex"}
            onClick={() =>
              change({ ...form, patternKind: form.patternKind === "regex" ? "text" : "regex" })
            }
          >
            <Regex size={14} />
          </button>
          <button
            type="button"
            className={`icon-button search-switch ${form.caseSensitive ? "is-pressed" : ""}`}
            title={t("search.matchCase")}
            aria-label={t("search.matchCase")}
            aria-pressed={form.caseSensitive}
            onClick={() => change({ ...form, caseSensitive: !form.caseSensitive })}
          >
            <CaseSensitive size={15} />
          </button>
        </label>
        <div className="search-filters">
          <FilterButton
            label={KIND_LABELS[form.kind]}
            narrows={form.kind !== "any"}
            title={t("search.kind")}
            onClick={(event) => openMenu(event, kindItems())}
          />
          <FilterButton
            label={describeSize(form.size)}
            narrows={sizeNarrows(form.size)}
            title={t("common.size")}
            onClick={(event) => openMenu(event, sizeItems())}
          />
          <FilterButton
            label={describeDate(form.date)}
            narrows={dateNarrows(form.date)}
            title={t("search.dateModified")}
            onClick={(event) => openMenu(event, dateItems())}
          />
          <FilterButton
            label={describeDepth(form.depth)}
            narrows={form.depth !== null}
            title={t("search.howDeepToSearch")}
            onClick={(event) => openMenu(event, depthItems())}
          />
          <button
            type="button"
            className={`icon-button search-options ${optionsCount > 0 ? "is-pressed" : ""}`}
            title={t("search.hiddenFoldersAndLinks")}
            aria-label={t("search.hiddenFoldersAndLinks")}
            onClick={(event) => openMenu(event, optionItems())}
          >
            <SlidersHorizontal size={14} />
          </button>
        </div>
        <div className="search-actions">
          {searching ? (
            <button type="button" className="toolbar-action search-stop" onClick={onStop}>
              <CircleStop size={13} />
              {t("common.stop")}
            </button>
          ) : (
            <button type="submit" className="toolbar-action search-go">
              {t("common.search")}
            </button>
          )}
          <button
            type="button"
            className="icon-button"
            title={t("search.closeSearchEsc")}
            aria-label={t("search.closeSearch")}
            onClick={onClose}
          >
            <X size={15} />
          </button>
        </div>
      </form>
      <SearchStatus
        view={view}
        root={root}
        problem={problem}
        onShowProblems={() => setShowingProblems(true)}
      />
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />
      )}
      {editing === "size" && (
        <RangeDialog
          title={t("search.customSize2")}
          firstLabel={t("search.atLeast")}
          secondLabel={t("search.atMost")}
          placeholder="500 KB, 2 GB"
          initial={[form.size.atLeast, form.size.atMost]}
          check={([atLeast, atMost]) => {
            const range = sizeRange({ preset: "custom", atLeast, atMost });
            return "error" in range ? range.error : null;
          }}
          onApply={([atLeast, atMost]) =>
            change({ ...form, size: { preset: "custom", atLeast, atMost } })
          }
          onClose={() => setEditing(null)}
        />
      )}
      {editing === "date" && (
        <RangeDialog
          title={t("search.customDates2")}
          firstLabel={t("search.changedOnOrAfter")}
          secondLabel={t("search.changedOnOrBefore")}
          placeholder="2026-11-01"
          initial={[form.date.from, form.date.to]}
          check={([from, to]) => {
            const range = dateRange({ preset: "custom", from, to }, new Date(), 1);
            return "error" in range ? range.error : null;
          }}
          onApply={([from, to]) => change({ ...form, date: { preset: "custom", from, to } })}
          onClose={() => setEditing(null)}
        />
      )}
      {showingProblems && view && (
        <UnreadableDialog
          failures={view.unreadable}
          count={view.unreadableCount}
          onClose={() => setShowingProblems(false)}
        />
      )}
    </div>
  );
}

function FilterButton({
  label,
  title,
  narrows,
  onClick,
}: {
  label: string;
  title: string;
  narrows: boolean;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      type="button"
      className={`search-filter ${narrows ? "is-narrowing" : ""}`}
      title={`${title}: ${label}`}
      aria-haspopup="menu"
      onClick={onClick}
    >
      <span>{label}</span>
      <ChevronDown size={12} aria-hidden />
    </button>
  );
}

function SearchStatus({
  view,
  root,
  problem,
  onShowProblems,
}: {
  view: SearchView | null;
  root: string;
  problem: string | null;
  onShowProblems: () => void;
}) {
  const t = useT();
  let tone: "plain" | "warning" | "danger" = "plain";
  let icon: ReactNode = null;
  let text: ReactNode;
  let detail: string | null = null;
  if (problem) {
    tone = "danger";
    text = problem;
  } else if (!view) {
    text = `Press Enter to search ${baseName(root) || root} and the folders inside it`;
  } else {
    const found = t("search.results", { count: view.entries.length });
    const folders = t("search.folders", { count: view.folders });
    switch (view.status) {
      case "searching":
        icon = <LoaderCircle size={13} className="spin" aria-hidden />;
        text = `Searching: ${formatCount(view.entries.length)} found in ${folders}`;
        detail = view.current;
        break;
      case "finished":
        text = `${found} in ${folders} searched`;
        if (view.engine === "find") detail = "Searched by the server";
        break;
      case "stopped":
        tone = "warning";
        text = `Stopped after ${folders}: ${found} so far`;
        break;
      case "capped":
        tone = "warning";
        text = `First ${formatCount(SEARCH_LIMIT)} results shown. Narrow the search to see the rest.`;
        break;
      case "failed":
        tone = "danger";
        text = isConnectionLost(view.error)
          ? `The connection was lost during the search. ${found} found before that.`
          : (view.error?.message ?? "The search failed");
        break;
    }
  }
  const unreadable = view?.unreadableCount ?? 0;
  return (
    <div className={`search-status tone-${tone}`} aria-live="polite">
      {icon}
      <span className="search-status-text">{text}</span>
      {detail && (
        <span className="search-status-detail" title={detail}>
          {/* Shown from its end, which is cut from the left; the mark keeps a leading slash
              where it belongs. */}
          {`\u200e${detail}`}
        </span>
      )}
      {unreadable > 0 && (
        <button type="button" className="link-button search-unreadable" onClick={onShowProblems}>
          <TriangleAlert size={12} aria-hidden />
          {t("search.couldNotBeRead", { t: t("search.folders", { count: unreadable }) })}
        </button>
      )}
    </div>
  );
}

function RangeDialog({
  title,
  firstLabel,
  secondLabel,
  placeholder,
  initial,
  check,
  onApply,
  onClose,
}: {
  title: string;
  firstLabel: string;
  secondLabel: string;
  placeholder: string;
  initial: [string, string];
  check: (values: [string, string]) => string | null;
  onApply: (values: [string, string]) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [values, setValues] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const trimmed: [string, string] = [values[0].trim(), values[1].trim()];
    const problem = check(trimmed);
    if (problem) {
      setError(problem);
      return;
    }
    onApply(trimmed);
    onClose();
  };
  return (
    <Dialog title={title} onClose={onClose} width={380}>
      <form className="form" onSubmit={submit}>
        <div className="search-range">
          {[firstLabel, secondLabel].map((label, index) => (
            <label key={label} className="field">
              <span>{label}</span>
              <input
                data-autofocus={index === 0 ? true : undefined}
                value={values[index]}
                placeholder={placeholder}
                spellCheck={false}
                onChange={(event) => {
                  const next: [string, string] = [...values];
                  next[index] = event.target.value;
                  setValues(next);
                  setError(null);
                }}
              />
            </label>
          ))}
        </div>
        <p className="field-hint">{t("search.leaveABoxEmptyTo")}</p>
        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="button button-primary">
            {t("common.apply")}
          </button>
        </div>
      </form>
    </Dialog>
  );
}

function UnreadableDialog({
  failures,
  count,
  onClose,
}: {
  failures: OperationFailure[];
  count: number;
  onClose: () => void;
}) {
  const t = useT();
  const unlisted = count - failures.length;
  return (
    <Dialog title={t("search.foldersThatCouldNotBe")} onClose={onClose} width={520}>
      <p>{t("search.theSearchWentPastIt", { t: t("search.folders", { count }) })}</p>
      <ul className="search-failures">
        {/* find can name one folder twice, so the place in the list tells them apart. */}
        {failures.map((failure, index) => (
          <li key={index}>
            <span className="search-failure-path" title={failure.path}>
              {failure.path}
            </span>
            <span className="search-failure-message">{failure.message}</span>
          </li>
        ))}
      </ul>
      {unlisted > 0 && <p className="field-hint">{t("search.andMore", { count: unlisted })}</p>}
      <div className="dialog-actions">
        <button type="button" className="button button-primary" data-autofocus onClick={onClose}>
          {t("common.close")}
        </button>
      </div>
    </Dialog>
  );
}
