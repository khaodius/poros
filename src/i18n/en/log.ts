import type { MessageArea } from "../catalog";

export const log = {
  // components/LogView
  saveLog: "Save log",
  levels: "Levels",
  session: "Session",
  allSessions: "All sessions",
  searchLog: "Search log",
  clearSearch: "Clear search",
  showTimes: "Show times",
  wrapLongLines: "Wrap long lines",
  followNewLines: "Follow new lines",
  copyShownLines: "Copy shown lines",
  saveShownLinesToA: "Save shown lines to a file",
  clearLog: "Clear log",
  // components/LogView
  info: "Info",
  warnings: "Warnings",
  errors: "Errors",
  server: "Server",
} as const satisfies MessageArea;
