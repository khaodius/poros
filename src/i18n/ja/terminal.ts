import type { TranslatedArea } from "../catalog";

export const terminal = {
  // components/TerminalPane
  couldNotReadTheClipboard:
    "クリップボードを読み取れませんでした。代わりに Shift+Insert で貼り付けてください。",
  selectAll: "すべて選択",
  couldNotOpenATerminal: "ターミナルを開けませんでした",
  closeTab: "タブを閉じる",
  restart: "再起動",
  connecting: "接続しています...",
  connected: "接続しました",
} satisfies TranslatedArea<"terminal">;
