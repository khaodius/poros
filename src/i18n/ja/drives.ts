import type { TranslatedArea } from "../catalog";

export const drives = {
  // components/DriveStatus
  filesNotUploaded: { other: "未アップロードのファイル {count} 個" },
  drivesMountedFromPoros: "Poros からマウントしたドライブ",
  mountedDrives: "マウント中のドライブ",
  driveAtMountPoint: "{mountPoint} の {label}",
  readOnly: "読み取り専用",
  factAt: "場所",
  factShows: "内容",
  factActivity: "状況",
  uploads: "アップロード",
  unmount: "マウント解除",
  refusedWhileProgramsHaveFiles:
    "プログラムがドライブ上のファイルを開いているか、アップロードが保留中の間は実行できません",
  forceUnmount: "強制的にマウント解除",
  unmountsAtOnceProgramsLose:
    "すぐにマウントを解除します。プログラムは開いているファイルを失い、まだアップロードされていないものはこのコンピューターに保持されます",
  notUploaded: "未アップロード",
  notUploadedKeptOnThis: "未アップロード (このコンピューターに保持)",
  keptFileFacts: "{connection}、{size}",
  keptFileFactsWithError: "{connection}、{size}: {error}",
  saveACopy: "コピーを保存...",
  discardForGood: "完全に破棄",

  // components/DrivesGoing
  unmountedWhenTheTabCloses: {
    other: "タブを閉じると、{paths} のドライブのマウントが解除されます。",
  },
  unmountedWhenTheVaultLocks: {
    other: "金庫がロックされると、{paths} のドライブのマウントが解除されます。",
  },
  unmountedWhenPorosQuits: {
    other: "Poros を終了すると、{paths} のドライブのマウントが解除されます。",
  },
  busyDrivesLoseOpenFiles:
    "プログラムがまだファイルを開いているか、アップロードが保留中です。開いているファイルは失われ、まだサーバーにないものはこのコンピューターに保持されて、ステータスバーのドライブの下に表示されます。",
  busyDrivesLoseOpenFilesWithVault:
    "プログラムがまだファイルを開いているか、アップロードが保留中です。開いているファイルは失われ、まだサーバーにないものはこのコンピューターに保持されて、ステータスバーのドライブの下に表示されます。金庫から保持したファイルは、保存するか破棄するまで暗号化されずに保存されます。",

  // components/MountDialog
  mountNameAsADrive: "{name} をドライブとしてマウント",
  theServersRoot: "サーバーのルート",
  theVaultsRoot: "金庫のルート",
  rootAt: "{root}: {path}",
  whatProgramsSaveServer:
    "プログラムが保存した内容は、ファイルを閉じたときにアップロードされます。タブを閉じるか Poros を終了すると、ドライブのマウントが解除されます。",
  whatProgramsSaveVault:
    "プログラムが保存した内容は、ファイルを閉じたときにアップロードされ、それまでは暗号化されずにこのコンピューターに保持されます。タブを閉じる、金庫がロックされる、または Poros を終了すると、ドライブのマウントが解除されます。",
  mountAsDrive: "ドライブとしてマウント",
  theConnectionHasClosed: "接続が閉じられました。",
  driveLetter: "ドライブ文字",
  folderOnThisComputer: "このコンピューターのフォルダー",
  aFreeLetterTheDrive: "ドライブの表示に使う、空いているドライブ文字です。",
  anEmptyFolderMadeIf: "空のフォルダーです。なければ作成し、後で削除します。",
  showOnTheDrive: "ドライブに表示する内容",
  thisFolder: "このフォルダー",
  readOnlyProgramsCanOpen: "読み取り専用: プログラムはファイルを開けますが、変更はできません",
  openInFileManagerWhen: "マウントしたらファイルマネージャーで開く",
  theConnectionIsLostReconnect: "接続が切れています。マウントするには再接続してください。",
  mounting: "マウントしています...",
  mount: "マウント",

  // components/QuitDialog
  quitPoros: "Poros を終了しますか?",
  unmountAndQuit: "マウントを解除して終了",

  // lib/mounts
  restartUnmountsDrivesInUse: {
    other:
      "{count} 個のドライブで、プログラムがファイルを開いているか、アップロードが保留中です。再起動するとマウントが解除され、まだアップロードされていないものはこのコンピューターに保持されます。",
  },
  restartUnmountsDrivesInUseWithVault: {
    other:
      "{count} 個のドライブで、プログラムがファイルを開いているか、アップロードが保留中です。再起動するとマウントが解除され、まだアップロードされていないものはこのコンピューターに保持されます。金庫のファイルは暗号化されずに保持されます。",
  },
  driveCount: { other: "{count} 個のドライブ" },
  drivesWithFailedUploads: {
    other: "{drives}、{count} 件のアップロードが失敗",
  },
  drivesUploading: { other: "{drives}、{count} 件をアップロード中" },
  filesOpen: { other: "{count} 個のファイルを使用中" },
  uploadsPending: { other: "{count} 件のアップロードが保留中" },
  sizeRead: "{size} 読み取り",
  sizeWritten: "{size} 書き込み",
  idle: "アイドル",
  uploadFailed: "失敗: {error}",
  uploadWaiting: "待機中",
  uploading: "アップロード中",
  uploadingPercent: "アップロード中、{percent}%",
  chooseADriveLetter: "ドライブ文字を選択してください。",
  chooseAFolder: "フォルダーを選択してください。",
  driveLetterSuchAs: "ドライブ文字 ({example} など)",
  fullPathSuchAs: "フルパス ({example} など)",

  // state/mountActions
  savedACopyAt: "{destination} にコピーを保存しました",
} satisfies TranslatedArea<"drives">;
