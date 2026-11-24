import type { TranslatedArea } from "../catalog";

export const log = {
  // components/LogView
  saveLog: "ログを保存",
  levels: "レベル",
  session: "セッション",
  allSessions: "すべてのセッション",
  searchLog: "ログを検索",
  clearSearch: "検索をクリア",
  showTimes: "時刻を表示",
  wrapLongLines: "長い行を折り返す",
  followNewLines: "新しい行に追従",
  copyShownLines: "表示中の行をコピー",
  saveShownLinesToA: "表示中の行をファイルに保存",
  clearLog: "ログをクリア",
  info: "情報",
  warnings: "警告",
  errors: "エラー",
  server: "サーバー",
  closedSession: "終了したセッション {id}",
  connectionAndTransferActivityAppears: "接続と転送の動作がここに表示されます。",
  noLinesMatchTheFilters: "フィルターに一致する行はありません。",
  logFiles: "ログファイル",
} satisfies TranslatedArea<"log">;
