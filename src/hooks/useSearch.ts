import { useCallback, useEffect, useRef, useState } from "react";
import { firstDayOfWeek } from "../lib/dateGroups";
import { search, toAppError } from "../lib/ipc";
import { applyRecheck, buildSearchRequest, mergeReports, type SearchForm } from "../lib/search";
import type {
  AppError,
  FileEntry,
  FileLocation,
  OperationFailure,
  SearchEngine,
  SearchReport,
} from "../lib/types";

const WEEK_STARTS_ON = firstDayOfWeek(navigator.language);
/** Finds arriving faster than this are shown together, so the list does not sort for each. */
const SHOW_INTERVAL_MILLIS = 150;

export type SearchStatus = "searching" | "finished" | "stopped" | "capped" | "failed";

export interface SearchView {
  id: string;
  /** The folder the search started in. */
  root: string;
  form: SearchForm;
  entries: FileEntry[];
  /** Folders looked at so far. */
  folders: number;
  /** The folder being looked at while the search runs. */
  current: string;
  /** Folders that could not be read, as many as the search listed. */
  unreadable: OperationFailure[];
  /** How many folders could not be read, also those not listed. */
  unreadableCount: number;
  status: SearchStatus;
  engine: SearchEngine | null;
  /** Why the search ended before it was done, when it failed. */
  error: AppError | null;
}

export interface SearchController {
  view: SearchView | null;
  /** Searches below `root`, in place of the results shown. The reason when it cannot. */
  start: (root: string, form: SearchForm) => string | null;
  stop: () => void;
  /** Stops the search and forgets its results. */
  discard: () => void;
  /** Looks at `paths` again: results changed, gone, or new under a new name. */
  recheck: (paths: readonly string[]) => Promise<void>;
}

const CONNECTION_LOST = new Set(["disconnected", "timeout", "connection", "ssh"]);

export function isConnectionLost(error: AppError | null): boolean {
  return error !== null && CONNECTION_LOST.has(error.kind);
}

/** Runs one search at a time for a pane and holds its results until they are discarded. */
export function useSearch(location: FileLocation): SearchController {
  const [view, setView] = useState<SearchView | null>(null);
  const running = useRef<string | null>(null);
  // Finds and progress waiting to be shown, so batches that come close together show at once.
  const pending = useRef<SearchReport | null>(null);
  const showTimer = useRef(0);
  const locationRef = useRef(location);
  useEffect(() => {
    locationRef.current = location;
  }, [location]);

  const showPending = useCallback((searchId: string) => {
    window.clearTimeout(showTimer.current);
    showTimer.current = 0;
    const report = pending.current;
    pending.current = null;
    if (!report) return;
    setView((current) => {
      if (!current || current.id !== searchId) return current;
      const { entries, unreadable } = report;
      return {
        ...current,
        // Results are below the start folder as its listing names it.
        root: report.start ?? current.root,
        entries: entries.length > 0 ? [...current.entries, ...entries] : current.entries,
        folders: report.folders,
        current: report.current,
        unreadable:
          unreadable.length > 0 ? [...current.unreadable, ...unreadable] : current.unreadable,
        unreadableCount: current.unreadableCount + unreadable.length,
      };
    });
  }, []);

  const cancelRunning = useCallback(() => {
    const searchId = running.current;
    running.current = null;
    window.clearTimeout(showTimer.current);
    showTimer.current = 0;
    pending.current = null;
    if (searchId) void search.cancel(searchId).catch(() => undefined);
  }, []);

  const start = useCallback(
    (root: string, form: SearchForm): string | null => {
      const searchId = crypto.randomUUID();
      const request = buildSearchRequest(form, {
        searchId,
        location: locationRef.current,
        folder: root,
        now: new Date(),
        weekStartsOn: WEEK_STARTS_ON,
      });
      if ("error" in request) return request.error;
      cancelRunning();
      running.current = searchId;
      setView({
        id: searchId,
        root,
        form,
        entries: [],
        folders: 0,
        current: root,
        unreadable: [],
        unreadableCount: 0,
        status: "searching",
        engine: null,
        error: null,
      });
      const onReport = (report: SearchReport) => {
        if (running.current !== searchId) return;
        pending.current = mergeReports(pending.current, report);
        if (!showTimer.current) {
          showTimer.current = window.setTimeout(() => showPending(searchId), SHOW_INTERVAL_MILLIS);
        }
      };
      search
        .start(request, onReport)
        .then((summary) => {
          if (running.current !== searchId) return;
          showPending(searchId);
          running.current = null;
          setView((current) =>
            current?.id === searchId
              ? {
                  ...current,
                  status: summary.outcome,
                  engine: summary.engine,
                  folders: summary.folders,
                  current: "",
                  unreadableCount: summary.unreadable,
                }
              : current,
          );
        })
        .catch((caught: unknown) => {
          if (running.current !== searchId) return;
          showPending(searchId);
          running.current = null;
          setView((current) =>
            current?.id === searchId
              ? { ...current, status: "failed", current: "", error: toAppError(caught) }
              : current,
          );
        });
      return null;
    },
    [cancelRunning, showPending],
  );

  const stop = useCallback(() => {
    const searchId = running.current;
    if (searchId) void search.cancel(searchId).catch(() => undefined);
  }, []);

  const discard = useCallback(() => {
    cancelRunning();
    setView(null);
  }, [cancelRunning]);

  const viewRef = useRef(view);
  useEffect(() => {
    viewRef.current = view;
  }, [view]);

  const recheck = useCallback(async (paths: readonly string[]) => {
    const searchId = viewRef.current?.id;
    if (!searchId || paths.length === 0) return;
    const asked = [...new Set(paths)];
    try {
      const answer = await search.recheck(locationRef.current, asked);
      setView((current) =>
        current?.id === searchId
          ? { ...current, entries: applyRecheck(current.entries, answer, asked) }
          : current,
      );
    } catch {
      // The results stay as they are; acting on one that is gone says so.
    }
  }, []);

  // A pane that closes stops its search.
  useEffect(() => cancelRunning, [cancelRunning]);

  return { view, start, stop, discard, recheck };
}
