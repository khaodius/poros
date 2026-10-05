import { create } from "zustand";
import type { LogLevel, LogRecord } from "../lib/types";
import { useSettingsStore } from "./settingsStore";

const FLUSH_DELAY_MILLIS = 50;

export interface LogLine extends LogRecord {
  id: number;
}

interface LogState {
  lines: LogLine[];
  append: (record: LogRecord) => void;
  write: (level: LogLevel, message: string, sessionId?: string) => void;
  clear: () => void;
}

let nextId = 1;
let pending: LogLine[] = [];
let flushTimer = 0;

export const useLogStore = create<LogState>((set, get) => {
  // Lines arrive in bursts while transfers run; adding them in batches keeps the panel smooth.
  const flush = () => {
    flushTimer = 0;
    const added = pending;
    pending = [];
    const limit = useSettingsStore.getState().settings.log.maxLines;
    set((state) => ({ lines: [...state.lines, ...added].slice(-limit) }));
  };
  return {
    lines: [],
    append: (record) => {
      pending.push({ ...record, id: nextId++ });
      if (!flushTimer) flushTimer = window.setTimeout(flush, FLUSH_DELAY_MILLIS);
    },
    write: (level, message, sessionId) =>
      get().append({ timestamp: Date.now(), level, message, sessionId }),
    clear: () => {
      pending = [];
      set({ lines: [] });
    },
  };
});
