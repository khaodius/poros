import { create } from "zustand";
import type { LogLevel, LogRecord } from "../lib/types";

const MAX_RECORDS = 2000;

interface LogState {
  records: LogRecord[];
  append: (record: LogRecord) => void;
  write: (level: LogLevel, message: string) => void;
  clear: () => void;
}

export const useLogStore = create<LogState>((set, get) => ({
  records: [],
  append: (record) =>
    set((state) => ({ records: [...state.records, record].slice(-MAX_RECORDS) })),
  write: (level, message) => get().append({ timestamp: Date.now(), level, message }),
  clear: () => set({ records: [] }),
}));
