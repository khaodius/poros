import { create } from "zustand";
import type { LogLevel, LogRecord } from "../lib/types";

const MAX_LINES = 2000;

export interface LogLine extends LogRecord {
  id: number;
}

interface LogState {
  lines: LogLine[];
  append: (record: LogRecord) => void;
  write: (level: LogLevel, message: string) => void;
  clear: () => void;
}

let nextId = 1;

export const useLogStore = create<LogState>((set, get) => ({
  lines: [],
  append: (record) =>
    set((state) => ({ lines: [...state.lines, { ...record, id: nextId++ }].slice(-MAX_LINES) })),
  write: (level, message) => get().append({ timestamp: Date.now(), level, message }),
  clear: () => set({ lines: [] }),
}));
