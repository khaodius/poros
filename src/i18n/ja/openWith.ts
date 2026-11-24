import type { TranslatedArea } from "../catalog";

export const openWith = {
  // components/ExternalEditDialogs
  isGoneFromTheServer: "{name} はサーバーから削除されています",
  changedOnTheServer: "{name} はサーバー上で変更されました",
  modified: "更新日時",
  onTheServer: "サーバー上",
  yourSavedCopy: "保存したコピー",
  keepBothUploadsYourCopy:
    "「両方とも保持」では、サーバーで使われていない名前でコピーをサーバーのファイルの横にアップロードします。",
  comparingReadsTheServersCopy: "比較では、サーバーのコピーを SFTP で読み込みます",
  dontUpload: "アップロードしない",
  uploadItAgain: "もう一度アップロード",
  keepBoth: "両方とも保持",
  upload: "{name} をアップロードしますか?",
  uploadEverySaveWithoutAsking: "保存するたびに確認せずにアップロードする",
  notNow: "後で",
  defaultProgram: "既定のプログラム",
  setUpForFilesLike: "このようなファイル用に設定したプログラム",
  builtInEditor: "内蔵エディター",
  editTheTextInA: "このアプリのタブでテキストを編集します",
  chooseAProgram: "プログラムを選択...",
  anyProgramOnThisComputer: "このコンピューター上の任意のプログラム",
  isAProgram: "{name} はプログラムです",
  open: "{name} を開く",
  programs: "プログラム",
  filesNotUploadedLastTime: "前回アップロードされなかったファイル",
  later: "後で",
  lock: "{label} をロックしますか?",
  close: "{label} を閉じますか?",
  theirSavesKeepBeingUploaded: "タブを閉じた後も、保存した内容はアップロードされ続けます。",
  stopEditingAndClose: "編集を終了して閉じる",
  uploadSavedFilesBeforeQuitting: "終了する前に保存したファイルをアップロードしますか?",
  someCouldNotBeUploaded: "一部をアップロードできませんでした。",
  quitWithoutUploading: "アップロードせずに終了",
  deleted: "削除済み",
  compare: "比較",
  compareSftpOnly: "比較 (SFTP のみ)",
  lockAnyway: "それでもロック",
  closeAnyway: "それでも閉じる",
  uploading2: "アップロードしています...",
  uploadAndLock: "アップロードしてロック",
  uploadAndClose: "アップロードして閉じる",
  keepEditing: "編集を続ける",
  uploadAndQuit: "アップロードして終了",
  uploadedAs: "{name} を {renamedTo} としてアップロードしました",
  uploadedName: "{name} をアップロードしました",
  couldNotUploadKept:
    "{name} をアップロードできませんでした: {message}。保存したファイルは保持しています。ステータスバーからもう一度お試しください。",
  couldNotOpen: "{name} を開けませんでした: {message}",
  someoneDeletedIt:
    "開いた後に、誰かが {server} からこのファイルを削除しました。保存した内容はまだアップロードされていません。",
  someoneChangedIt:
    "開いた後に、誰かが {server} 上でこのファイルを変更しました。保存した内容はまだアップロードされていません。",
  serverChangedThere: "{server} で変更された版",
  savedInProgramUploadNow:
    "{name} が {program} で保存されました。今すぐ {server} にアップロードしますか?",
  savedUploadNow: "{name} が保存されました。今すぐ {server} にアップロードしますか?",
  theProgramThisSystemOpensFilesWith: "このシステムで {extension} ファイルを開くプログラム",
  theProgramThisSystemOpensSuchFilesWith: "このシステムでこの種類のファイルを開くプログラム",
  holdsAProgramNotOpened:
    "{name} にはプログラムまたはスクリプトが含まれているため、開きませんでした。Poros は開いたファイルを実行しません。内容を見るエディターを選んでください。",
  isAProgramOrAScript:
    "{name} はプログラムまたはスクリプトです。Poros は開いたファイルを実行しません。内容を見るエディターを選んでください。",
  whenPorosLastClosed: {
    other:
      "前回 Poros を閉じたとき、これらのファイルは別のプログラムで保存されましたが、アップロードされていませんでした。サーバーに接続してからアップロードするか、変更を破棄してください。一覧はステータスバーに残ります。",
  },
  copiesFromVaultsStayUnencrypted:
    "それまでは、金庫のファイルのコピーは暗号化されないままこのコンピューターに残ります。",
  filesFromThisVaultOpen: {
    other: "この金庫の {count} 個のファイルが他のプログラムで開かれています:",
  },
  filesFromThisServerOpen: {
    other: "このサーバーの {count} 個のファイルが他のプログラムで開かれています:",
  },
  couldNotBeUploadedLockingWipes: {
    other: "これらはアップロードできませんでした。それでもロックすると、変更は消去されます。",
  },
  couldNotBeUploadedClosingWipes: {
    other:
      "これらはアップロードできませんでした。それでも閉じると、金庫のファイルの変更は消去されます。",
  },
  couldNotBeUploadedClosingLeaves: {
    other:
      "これらはアップロードできませんでした。それでも閉じた場合は、後でアップロードできるようにステータスバーの一覧に残ります。",
  },
  lockingEndsEditing: {
    other:
      "金庫をロックすると、金庫のファイルの編集は終了し、作業用のコピーは消去されます。保存した内容は先にアップロードされます。",
  },
  closingLocksVaultEndsEditing: {
    other:
      "タブを閉じると金庫がロックされ、金庫のファイルの編集は終了し、作業用のコピーは消去されます。保存した内容は先にアップロードされます。",
  },
  savedButNotReachedTheServer: {
    other: "次のファイルは他のプログラムで保存されましたが、サーバーに届いていません:",
  },
  quittingWithoutUploadingKeepsThem:
    "アップロードせずに終了してもファイルは保持され、次回の起動時に Poros がアップロードを提案します。",
  workingCopiesOfVaultFilesAreWiped: "金庫のファイルの作業用のコピーは消去されます。",

  // components/ExternalEditorSettings
  openWith: "プログラムから開く",
  openCtrlEnterUsesThe:
    "「開く」(Ctrl+Enter) は、ファイルの種類用に設定したプログラム、なければシステムの既定のプログラムを使います。サーバーのファイルはこのコンピューター上のコピーから開き、プログラムが保存するたびにアップロードされます。",
  uploadEachSaveWithoutAsking: "保存するたびに確認せずにアップロードする",
  uploadEachSaveWithoutAskingHint: "オフ: アップロードのたびに確認します。",
  keepCopiesAfterTheProgram: "プログラムを閉じた後もコピーを残す期間",
  keepCopiesAfterTheProgramHint:
    "最後の保存またはアップロードから数えます。金庫のファイルのコピーはすぐに消去されます。",
  removeProgram: "プログラムを削除",
  remove: "{programLabel} を削除",
  program: "プログラム",
  arguments: "引数",
  defaultFor: "既定で開くファイル",
  filesThisProgramOpensBy: "このプログラムで既定で開くファイル",
  psdXcf: "*.psd, *.xcf",
  nameOfTheNewProgram: "新しいプログラムの名前",
  newProgram: "新しいプログラム",
  inTheArgumentsStandsFor:
    "引数の {placeholder} はファイルを表し、省略するとファイルは最後に付きます。引用符で囲むと、空白を含めて 1 つの引数になります。「既定で開くファイル」のパターンに一致するファイルは、そのプログラムで開きます。",

  // components/ExternalEditsStatus
  filesOpenInOtherPrograms: "他のプログラムで開いているファイル",
  stopEditing: "{name} の編集を終了しますか?",
  stopAndDiscard: "終了して破棄",
  openInOtherPrograms: "他のプログラムで開いているもの",
  settingsForOpeningFiles: "ファイルを開く方法の設定",
  noFilesAreOpenIn:
    "他のプログラムで開いているファイルはありません。ファイルを右クリックして「プログラムから開く」を選んでください。",
  inAVault: "金庫内",
  uploaded: "{uploadTime} にアップロード",
  of: "{transferred} / {total}",
  decideWhatToUpload: "アップロードする内容を決める...",
  openInAnEditor: "エディターで開く...",
  uploadNow: "今すぐアップロード",
  uploadNow2: "今すぐ {name} をアップロード",
  openAgain: "もう一度開く",
  openAgain2: "{name} をもう一度開く",
  showTheWorkingCopysFolder: "作業用のコピーのフォルダーを表示",
  showTheFolderOf: "{name} のフォルダーを表示",
  stopEditingAndDeleteThe: "編集を終了して作業用のコピーを削除",
  stopEditing2: "{name} の編集を終了",
  savedButNotUploadedWhen: "前回 Poros を閉じたときに保存済みで未アップロード",
  saved: "{date} に保存",
  thisCopyOfAVault:
    "この金庫のファイルのコピーは、このコンピューター上で暗号化されていません。アップロードするか破棄してください。",
  uploadTo: "{label} にアップロード",
  connectToFirst: "先に {server} に接続してください",
  upload2: "{name} をアップロード",
  discardTheSavedChanges: "保存した変更を破棄",
  discard: "{name} を破棄",
  discard2: "{name} を破棄しますか?",
  notEditingAnyFiles: "編集中のファイルはありません",
  eachSaveIsUploadedAs: "プログラムが書き込むとすぐに、保存した内容をアップロードします。",
  savesWaitForYouTo: "保存した内容は、アップロードするまで待機します。",
  changesSavedInProgramNotReached:
    "{program} で保存した変更は {server} に届いていません。終了すると、変更ごと作業用のコピーが削除されます。",
  changesSavedInTheProgramNotReached:
    "プログラムで保存した変更は {server} に届いていません。終了すると、変更ごと作業用のコピーが削除されます。",
  connectAndUnlockToUpload:
    "アップロードするには、{server} に接続して金庫のロックを解除してください。",
  connectToUpload: "アップロードするには、{server} に接続してください。",
  neverReachedDiscardingWipes:
    "このファイルに保存した変更は {server} に届いていません。破棄すると変更は消去されます。",
  neverReachedDiscardingDeletes:
    "このファイルに保存した変更は {server} に届いていません。破棄すると変更は削除されます。",

  // components/openWithMenu
  openIn: "{programLabel} で開く",
  setUpPrograms: "プログラムを設定...",

  // lib/externalEditor
  downloading: "ダウンロード中",
  watchingForSaves: "保存を監視中",
  savedNotUploaded: "保存済み、未アップロード",
  waitingToUpload: "アップロード待ち",
  changedOnTheServer2: "サーバー上で変更あり",
  uploading: "アップロード中",
  uploadFailed: "アップロード失敗",
  notOpenedItIsAProgram: "プログラムのため開きませんでした",
  openingNamePercent: "{name} を開いています、{percent}",
  uploadingName: "{name} をアップロード中",
  nameNotOpenedItIsAProgram: "{name}: プログラムのため開きませんでした",
  nameChangedOnTheServer: "{name}: サーバー上で変更あり",
  nameUploadFailed: "{name}: アップロード失敗",
  editedFilesNeedAttention: {
    other: "{count} 個の編集中のファイルに対応が必要です",
  },
  editingName: "{name} を編集中",
  editingFiles: { other: "{count} 個のファイルを編集中" },
  filesNotUploaded: { other: "未アップロードのファイル {count} 個" },

  // state/externalEditStore
  isOpenAlreadyItsWorking: "{name} は既に開いています。作業用のコピーをもう一度開きます。",
  chooseAProgram2: "プログラムを選択",
} satisfies TranslatedArea<"openWith">;
