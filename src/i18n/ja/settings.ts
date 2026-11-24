import type { TranslatedArea } from "../catalog";

export const settings = {
  // components/LanguagePicker
  languageSystem: "システム ({language})",

  // components/SettingsDialog
  settingsSections: "設定のセクション",
  queue: "キュー",
  simultaneousTransfers: "同時転送数",
  simultaneousTransfersHint: "同時に転送するファイルの数です。",
  startTransfersAsSoonAs: "キューに追加したらすぐに転送を開始する",
  startTransfersAsSoonAsHint: "オフ: 「キューを開始」を押すまでキューは待機します。",
  whenTheTargetFileExists: "転送先にファイルが既にある場合",
  keepFinishedTransfersInThe: "完了した転送を一覧に残す",
  keepUnfinishedTransfersWhenPoros: "Poros の終了時に未完了の転送を保持する",
  keepUnfinishedTransfersWhenPorosHint:
    "次回の起動時に一時停止した状態で戻ります。パスワードが一緒に保存されることはありません。",
  connections: "接続",
  giveEachTransferItsOwn: "転送ごとに専用の接続を使う",
  giveEachTransferItsOwnHint:
    "オフ: 転送は閲覧用の接続を共有します。ログイン数を制限しているサーバー向けです。",
  splitLargeFilesAcrossConnections: "大きなファイルを複数の接続に分割する",
  splitLargeFilesAcrossConnectionsHint:
    "空いている転送スロットが大きなファイルに加わり、ファイルの別々の部分を転送します。",
  splitFilesLargerThan: "分割するファイルの最小サイズ",
  connectionsPerFile: "ファイルあたりの接続数",
  copyDirectlyBetweenFtpServers: "FTP サーバー間で直接コピーする (FXP)",
  copyDirectlyBetweenFtpServersHint:
    "両方のサーバーが許可している必要があります。許可していない場合や他の種類のサーバー間では、ファイルは Poros を経由して転送されます。",
  bandwidth: "帯域幅",
  uploadLimit: "アップロードの上限",
  uploadLimitHint: "0 は無制限です。すべての転送で共有します。",
  downloadLimit: "ダウンロードの上限",
  buffers: "バッファー",
  requestSize: "リクエストのサイズ",
  requestSizeHint:
    "SFTP の 1 回の読み書きのバイト数です。ほとんどのサーバーは 255 KiB まで受け付けますが、32 KiB までに制限するサーバーもあります。",
  requestsInFlight: "同時リクエスト数",
  requestsInFlightHint:
    "接続ごとに応答を待たずに送るリクエストの数です。大きくするほどネットワークの遅延の影響が小さくなります。",
  files: "ファイル",
  keepModificationTimes: "更新日時を保持する",
  keepPermissions: "パーミッションを保持する",
  keepPermissionsHint:
    "Unix のパーミッションビットをコピーします。Windows のファイルには効果がありません。",
  integrity: "整合性",
  writeToATemporaryName: "一時的な名前で書き込んでから名前を変更する",
  writeToATemporaryNameHint:
    "ファイルは完成して確認が済んでから本来の名前で現れるため、置き換えたファイルが書きかけのまま残ることはありません。FTP サーバーへのアップロードは直接書き込まれます。",
  verifyChecksums: "チェックサムを検証する",
  verifyChecksumsHint:
    "ファイルごとに両方のコピーの SHA-256 を比較します。SFTP サーバーではサーバー自身の sha256sum で計算します。SHA-1 と MD5 は偽造できるため、この用途には使用しません。両側ですべてのファイルをもう一度読み込みます。",
  readFilesBackWhenThe: "サーバーでハッシュを計算できない場合はファイルを読み戻す",
  readFilesBackWhenTheHint:
    "sha256sum のない SFTP サーバー、FTP サーバー、WebDAV サーバーでは、転送の終わった各ファイルを読み戻してこのコンピューターでハッシュを計算します。これらのサーバーとの通信量が 2 倍になります。クラウドストレージは読み戻しません。",
  repairOnlyTheDamagedParts: "破損した部分だけを修復する",
  repairOnlyTheDamagedPartsHint:
    "不一致があると、ファイルの各部分を個別に比較し、異なる部分だけをファイルの半分まで再送信します。サーバーの GNU split か、読み戻しが必要です。",
  checkTheTransferredPartBefore: "再開する前に転送済みの部分を確認する",
  checkTheTransferredPartBeforeHint:
    "既にある部分を転送元と部分ごとに比較し、最初に異なる部分から続行します。サーバーでファイルの部分ごとのハッシュを計算できない場合は、最後の 64 KiB を比較します。",
  flushToDiskBeforeFinishing: "完了する前にディスクへ書き出す",
  flushToDiskBeforeFinishingHint:
    "ファイルは保存されてはじめて完了とみなされるため、直後に停電しても失われません。対応しているサーバーでのみ有効です。小さなファイルが多いと遅くなります。",
  checkTimesAndPermissionsAfter: "日時とパーミッションを設定した後に確認する",
  checkTimesAndPermissionsAfterHint:
    "ファイルを配置した後に読み戻し、サーバーやファイルシステムが保持しなかったものをログに記録します。",
  checkPublishedChecksumsAndSignatures: "公開されているチェックサムと署名を確認する",
  checkPublishedChecksumsAndSignaturesHint:
    "ダウンロードしたファイルを、同じ場所にある SHA256SUMS、name.sha256 などのファイルや .minisig 署名と照合し、.asc と .sig の署名は gpg で照合します。照合に失敗したファイルは保存されません。",
  errors: "エラー",
  retries: "再試行の回数",
  retriesHint: "タイムアウト、確認の失敗、接続直後にまた切れる接続が対象です。",
  waitBetweenRetries: "再試行の間隔",
  trustedMinisignKeys: "信頼する minisign 鍵",
  onePublicKeyPerLine:
    "公開鍵を 1 行に 1 つずつ、発行元の minisign.pub と同じ形式で入力します。他の鍵による署名はログに記録され、確認されません。",
  autoTuneReceiveBuffer: "受信バッファーを自動調整する",
  autoTuneReceiveBufferHint:
    "接続に合わせてシステムが TCP の受信ウィンドウを広げます。新しい接続に適用されます。",
  receiveBufferSize: "受信バッファーのサイズ",
  receiveBufferSizeHint: "SO_RCVBUF です。自動調整がオフのときに使用します。",
  autoTuneSendBuffer: "送信バッファーを自動調整する",
  autoTuneSendBufferHint:
    "接続に合わせてシステムが TCP の送信バッファーのサイズを決めます。新しい接続に適用されます。",
  sendBufferSize: "送信バッファーのサイズ",
  sendBufferSizeHint: "SO_SNDBUF です。自動調整がオフのときに使用します。",
  connectionTimeout: "接続のタイムアウト",
  keepaliveInterval: "キープアライブの間隔",
  keepaliveIntervalHint: "0 にするとキープアライブを送信しません。新しい接続に適用されます。",
  compressTraffic: "通信を圧縮する",
  compressTrafficHint:
    "低速な回線でのテキストの転送が速くなります。圧縮済みのファイルは遅くなります。新しい接続に適用されます。",
  reconnecting: "再接続",
  reconnectLostTabsAutomatically: "切断されたタブを自動的に再接続する",
  reconnectLostTabsAutomaticallyHint:
    "間隔を広げながら再試行します。入力済みのパスワードを再び求めることはありません。",
  keepTryingFor: "再試行を続ける時間",
  keepTryingForHint:
    "タブと転送は、切断されたサーバーをこの時間だけ待ちます。その間、転送の再試行回数は減りません。0 にするとすぐにあきらめます。",
  savedConnections: "保存済みの接続",
  saveQuickConnectionsAutomatically: "クイック接続を自動的に保存する",
  saveQuickConnectionsAutomaticallyHint:
    "クイック接続バーから新しく接続したサーバーを、パスワードを除いて追加します。",
  proxy: "プロキシ",
  newSftpConnectionsReachTheir:
    "新しい SFTP 接続は、プロキシを使わない設定の接続を除き、このプロキシを経由してサーバーに接続します。踏み台ホストを使う場合、プロキシを使うのは踏み台ホストへの接続だけです。FTP とクラウドの接続はプロキシを使いません。",
  type: "種類",
  host: "ホスト",
  port: "ポート",
  username: "ユーザー名",
  usernameHint: "ログインが不要なプロキシでは空欄にします。SOCKS4 はユーザー名だけを送信します。",
  password: "パスワード",
  passwordHint: "システムのキーチェーンに保存されます。",
  forget: "消去",
  lookUpServerNamesThrough: "サーバー名の名前解決をプロキシ経由で行う",
  lookUpServerNamesThroughHint:
    "接続先のサーバー名が、このコンピューターの DNS に伝わることはありません。HTTP プロキシでは常にこの動作になります。",
  proxyPassword: "プロキシのパスワード",
  saved: "保存済み",
  doubleClickingAFile: "ファイルをダブルクリックしたとき",
  transfersItToTheOther: "反対側に転送する",
  opensItInTheBuilt: "内蔵エディターで開く",
  opensItInItsProgram: "関連付けられたプログラムで開く",
  asksHowToOpenIt: "開き方を確認する",
  doesNothing: "何もしない",
  showHiddenFilesInNew: "新しいタブで隠しファイルを表示する",
  foldersFirst: "フォルダーを先頭に表示する",
  foldersFirstHint: "並べ替えの順序にかかわらず、フォルダーをファイルより上に表示します。",
  dates: "日付",
  columns: "列",
  columnsHint:
    "列の見出しを右クリックすると、列の表示と非表示や並べ替えを変更できます。見出しの前の線をドラッグすると列の幅を変更でき、線をダブルクリックすると内容に合わせます。",
  serverFolders: "サーバーのフォルダー",
  showCachedListingsWhileRefreshing: "更新中はキャッシュした一覧を表示する",
  showCachedListingsWhileRefreshingHint:
    "以前に開いたフォルダーは前回の状態ですぐに表示され、サーバーが応答すると更新されます。一覧はメモリにだけ保持され、ディスクには保存されません。",
  loadNearbyFoldersAhead: "近くのフォルダーを先読みする",
  loadNearbyFoldersAheadHint:
    "サーバーが空いている間に、親フォルダーとカーソル位置のフォルダーの一覧を 1 つずつ取得します。FTP では転送中は一時停止します。",
  alsoOnCloudStorage: "クラウドストレージでも先読みする",
  alsoOnCloudStorageHint:
    "オフ: リクエストが数えられて制限されるクラウドストレージでは先読みしません。",
  tabsAndWindows: "タブとウィンドウ",
  askBeforeClosingATab: "キューに転送が残っているタブを閉じる前に確認する",
  rememberTabsAndLayout: "タブとレイアウトを記憶する",
  rememberTabsAndLayoutHint:
    "前回の終了時に開いていたウィンドウ、ペイン、タブを、それぞれのフォルダーで再び開きます。",
  reconnectToServersOnStart: "起動時にサーバーへ再接続する",
  reconnectToServersOnStartHint: "オフ: 再び開いたサーバーのタブは、接続するまで待機します。",
  useTheSystemTitleBar: "システムのタイトルバーを使用する",
  useTheSystemTitleBarHint:
    "オフ: 上部のバーにウィンドウのボタンを表示し、バーのドラッグでウィンドウを移動します。",
  log: "ログ",
  linesToKeep: "保持する行数",
  showTimes: "時刻を表示する",
  wrapLongLines: "長い行を折り返す",
  logEveryTransferredFile: "転送したファイルをすべてログに記録する",
  logEveryTransferredFileHint: "オフ: キューが終わったときに概要を 1 行だけ記録します。",
  updates: "アップデート",
  checkForUpdatesWhenPoros: "Poros の起動時にアップデートを確認する",
  checkForUpdatesWhenPorosHint:
    "GitHub で新しいリリースを探します。インストールを選ぶまで何もダウンロードしません。",
  poros: "Poros {version}",
  restartNow: "今すぐ再起動",
  updating: "アップデートしています...",
  showUpdate: "アップデートを表示",
  isNotAvailable: "{version} は利用できません: {message}",
  installAnotherVersion: "別のバージョンをインストール",
  version: "バージョン",
  install: "インストール",
  transfers: "転送",
  sync: "同期",
  connection: "接続",
  cloudAccounts: "クラウドアカウント",
  automation: "自動化",
  archives: "アーカイブ",
  interface: "インターフェイス",
  appearance: "外観",
  overwriteIfTheSourceIs: "転送元が新しい場合は上書き",
  overwriteIfSizeOrDate: "サイズか日時が異なる場合は上書き",
  keepBoth: "両方とも保持",
  checking: "確認しています...",
  checkForUpdates: "アップデートを確認",
  listing: "一覧を取得しています...",
  showVersions: "バージョンを表示",
  language: "言語",
  languageHint:
    "「システム」はコンピューターの言語に従います。サーバーからのメッセージとログは英語のままです。",

  // components/Stepper
  decrease: "{label}を減らす",
  increase: "{label}を増やす",
} satisfies TranslatedArea<"settings">;
