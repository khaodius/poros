import type { TranslatedArea } from "../catalog";

export const rename = {
  // components/BulkRenameDialog
  nothingToRename: "名前を変更する項目はありません",
  renameItems: { other: "{count} 個の項目の名前を変更" },
  appliedInThisOrderEach: "この順序で、それぞれ前の結果に適用します。",
  findAndReplace: "検索と置換",
  find: "検索",
  regularExpression: "正規表現",
  recentPatterns: "最近使ったパターン",
  recent: "最近",
  replaceWith: "置換後の文字列",
  replaceWith1OrName: "置換後の文字列 (グループは $1 または $<name>)",
  patternIs: "パターンの種類",
  matchCase: "大文字と小文字を区別",
  everyMatch: "すべての一致箇所",
  lookIn: "対象",
  template: "テンプレート",
  templateForTheNewName: "新しい名前のテンプレート",
  insertAToken: "トークンを挿入",
  dateAs: "日付の形式",
  timeAs: "時刻の形式",
  theDateAndTimeAre:
    "日付と時刻は、項目の最終更新日時です。{ext} がない場合、項目の拡張子はそのまま残ります。",
  counter: "連番",
  step: "増分",
  digits: "桁数",
  countsIn: "番号の順序",
  listOrder: "一覧の順",
  nameOrder: "名前の順",
  theTemplatePlacesTheCounter: "テンプレートでは {n} の位置に連番が入ります。",
  addToEachName: "各名前に追加",
  separator: "区切り文字",
  case: "大文字と小文字",
  extension: "拡張子",
  newExtension: "新しい拡張子",
  newExtensionSuchAsJpg: "新しい拡張子 (jpg など)",
  trimAndTidy: "空白の整理",
  trimSpacesAtTheEnds: "両端の空白を削除",
  makeRepeatedSpacesOne: "連続する空白を 1 つにする",
  replaceSpacesWith: "空白を置き換える文字",
  characterThatReplacesSpaces: "空白の代わりに使う文字",
  rowsShown: "表示する行",
  oldAndNewNames: "変更前と変更後の名前",
  noNamesChangeYet: "まだ変わる名前はありません。",
  couldNotRefreshThisFolder:
    "このフォルダーを更新できませんでした。この一覧は最新でない可能性があります。",
  resetRules: "ルールをリセット",
  ofItemsWillBeRenamed: {
    other: "{count} 個の項目のうち {ready} 個の名前を変更します。",
  },
  ofItemsWillBeRenamedSomeUnchanged: {
    other: "{count} 個の項目のうち {ready} 個の名前を変更し、{unchanged} 個は変更しません。",
  },
  text: "テキスト",
  regex: "正規表現",
  both: "両方",
  unchanged: "変更しない",
  titleCase: "単語の先頭を大文字",
  sentenceCase: "先頭だけ大文字",
  beforeTheName: "名前の前",
  afterTheName: "名前の後",
  all: "すべて",
  changed: "変更あり",
  problems: "問題あり",
  checkingTheFolder: "フォルダーを確認しています...",
  theseItemsAreNoLongerInTheFolder: "これらの項目はもうフォルダーにありません。",
  selectedItemsLeftOut: {
    other: "選択した {count} 個の項目はもうフォルダーにないため、除外します。",
  },

  // components/RenameReportDialog
  renamingStopped: "名前の変更を中止しました",
  renamedOfItemsIn: {
    other: "{folder} の {total} 個中 {count} 個が新しい名前になりました。",
  },
  theOthersKeepTheirOldNamesButOne: "下の 1 個を除き、他の項目は元の名前のままです。",
  theOthersKeepTheirOldNames: "他の項目は元の名前のままです。",
  items: { other: "{count} 個の項目" },
  couldNotBeRenamedTo: "{from} の名前を {to} に変更できませんでした: {message}",
  isNowNamedBecause:
    "{from} は元の名前に戻せなかったため、現在の名前は {temporary} です: {message}。手動で名前を変更してください。",
  whatBecameOfEachRename: "各項目の名前の変更結果",
  noItems: "項目はありません。",
  renamed: "変更済み",
  notRenamed: "未変更",
  failed: "失敗",
  leftUnderAnotherName: "別の名前で残っています",

  // components/RenameTable
  newName: "新しい名前",
  status: "状態",
  noName: "名前なし",

  // lib/bulkRename
  theNameIsEmpty: "名前が空です",
  theNameIsTooLong: "名前が長すぎます: {length}/{maxLength} {lengthUnit}",
  nowNamed: "現在の名前は {temporary} です: {message}",
  nameWithoutTheExtension: "拡張子を除いた名前",
  dateModified: "更新日",
  timeModified: "更新時刻",
  nameOfTheFolder: "フォルダーの名前",
  isNotAName: "「{name}」は名前として使えません",
  namesCannotContainControlCharacters: "名前に制御文字は使えません",
  namesCannotContain: "名前に {character} は使えません",
  namesCannotEndWithASpaceOrADot: "名前の末尾に空白やピリオドは使えません",
  isAReservedNameOnWindows: "{name} は Windows の予約名です",
  keepsThisName: "{name} がこの名前のまま残ります",
  keepsANameDifferingInCase: "{name} が、大文字と小文字だけが異なる名前のまま残ります",
  keepsANameDifferingInAccents: "{name} が、アクセント記号の保存形式だけが異なる名前のまま残ります",
  getsTheSameName: "{name} も同じ名前になります",
  getsANameDifferingInCase: "{name} が、大文字と小文字だけが異なる名前になります",
  getsANameDifferingInAccents: "{name} が、アクセント記号の保存形式だけが異なる名前になります",
  anotherSelectedItemHasTheSameName:
    "選択した別の項目が同じ名前です。これらは 1 つずつ名前を変更してください",
  isSelectedTooAndCountsAsTheSameName:
    "{name} も選択されており、ここでは同じ名前とみなされます。これらは 1 つずつ名前を変更してください",
  noModifiedTimeFor: "この項目には {date} や {time} に使う更新日時がありません",
  anItemThatIsNotSelectedHasThisName: "選択されていない項目がこの名前を使っています",
  notSelectedAndHasANameDifferingInCase:
    "{name} は選択されておらず、大文字と小文字だけが異なる名前です",
  notSelectedAndHasANameDifferingInAccents:
    "{name} は選択されておらず、アクセント記号の保存形式だけが異なる名前です",
  problemSharedName: {
    other: "{count} 個の項目が、選択した別の項目と同じ名前です",
  },
  problemEmpty: { other: "{count} 個の名前が空です" },
  problemInvalid: {
    other: "{count} 個の名前はここでは使えません",
  },
  problemTooLong: { other: "{count} 個の名前が長すぎます" },
  problemUndated: {
    other: "{count} 個の項目に更新日時がありません",
  },
  problemDuplicate: {
    other: "{count} 個の項目が同じ名前になります",
  },
  problemTaken: { other: "{count} 個の名前は既に使われています" },
  theRegularExpressionIsNotValid: "正規表現が正しくありません: {error}",
  problemsSentence: "{problems}。",

  // state/bulkRename
  renamingCancelledRenamed: {
    other: "名前の変更をキャンセルしました。{count} 個中 {renamed} 個の名前を変更しました。",
  },
  renamingItemsIn: {
    other: "{folder} の {count} 個の項目の名前を変更中",
  },
} satisfies TranslatedArea<"rename">;
