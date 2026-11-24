import type { TranslatedArea } from "../catalog";

export const connect = {
  // components/ConnectDialog
  selectPrivateKey: "秘密鍵を選択",
  savedConnections: "保存済みの接続",
  importFromFilezillaWinscpAnd: "FileZilla、WinSCP などのアプリからインポート",
  importConnections: "接続のインポート",
  newConnection: "新しい接続",
  connectionsYouSaveAppearHere: "保存した接続がここに表示されます。",
  importThemFromAnotherApp: "他のアプリからインポート",
  protocol: "プロトコル",
  account: "アカウント",
  emptyForAmazonS3: "Amazon S3 の場合は空欄",
  port: "ポート",
  forAnotherServiceEnterIts:
    "他のサービスでは {example} のようなエンドポイントを、TLS を使わないサーバーでは {local} のように入力します。",
  anonymous: "anonymous",
  emptyIfTheServerNeeds: "サーバーが求めない場合は空欄",
  authentication: "認証",
  savedInTheSystemKeychain: "システムのキーチェーンに保存済み",
  privateKeyFile: "秘密鍵ファイル",
  sshIdEd25519OrC: "~/.ssh/id_ed25519 または C:\\keys\\server.ppk",
  opensshPemPkcs8And: "OpenSSH、PEM、PKCS#8、PuTTY .ppk (v2 と v3) の鍵に対応しています。",
  usesKeysLoadedInYour:
    "SSH エージェントに読み込まれた鍵を使用します。Linux と macOS では {variable}、Windows では OpenSSH Authentication Agent サービスまたは Pageant です。",
  region: "リージョン",
  fromTheEndpointOrUs: "エンドポイントから判断、または us-east-1",
  bucketAddress: "バケットのアドレス",
  automatic: "自動",
  pathEndpointBucket: "パス形式 (endpoint/bucket)",
  virtualHostedBucketEndpoint: "仮想ホスト形式 (bucket.endpoint)",
  remoteFolderOptional: "リモートフォルダー (省略可)",
  jumpHost: "踏み台ホスト",
  noneConnectStraightToThe: "なし (サーバーに直接接続)",
  aDeletedConnection: "削除された接続",
  logsInToThisSaved:
    "先にこの保存済みの接続にログインし、そこを経由してサーバーに接続します。OpenSSH の ProxyJump と同じしくみです。",
  rememberThePassword: "パスワードをシステムのキーチェーンに保存",
  rememberThePassphrase: "パスフレーズをシステムのキーチェーンに保存",
  rememberTheSecretAccessKey: "シークレットアクセスキーをシステムのキーチェーンに保存",
  rememberTheApplicationKey: "アプリケーションキーをシステムのキーチェーンに保存",
  activeModeTheServerOpens:
    "アクティブモード: サーバーからこのコンピューターへデータ接続を開きます",
  connectWithoutTheProxy: "プロキシを使わずに接続",
  rsyncAndSynchronization: "rsync と同期",
  rsyncOnTheServer: "サーバーの rsync",
  theCommandThatStartsRsync:
    "このサーバーで rsync を起動するコマンドです (sudo rsync、/usr/local/bin/rsync など)。空欄の場合は転送の設定にあるコマンドを使用します。",
  rsyncArgumentsForFolderSynchronizations: "フォルダーの同期に使う rsync の引数",
  rsyncArgumentsForFolderSynchronizationsHint:
    "このサーバーとの同期では、設定の既定値にここで指定した引数を上乗せして開始します。",
  password: "パスワード",
  keyFile: "鍵ファイル",
  sshAgent: "SSH エージェント",
  host: "ホスト",
  username: "ユーザー名",
  browserSignIn: "ブラウザーでサインイン",
  endpoint: "エンドポイント",
  saveThisConnectionAndIts: "この接続とサインイン情報を保存",
  saveThisConnection: "この接続を保存",
  deleteForGood: "完全に削除",
  saving: "保存しています...",
  connecting: "接続しています...",
  shownInSavedConnections: "保存済みの接続に表示される名前",
  topFolderOfTheDrive: "ドライブの最上位フォルダー",
  bucketOrBucketFolder: "{bucket} または {folder}",
  loginFolder: "ログイン時のフォルダー",
  theFolderInTheAddressOrTheTop: "アドレスのフォルダー、または最上位",
  homeFolder: "ホームフォルダー",
  passphrase: "パスフレーズ",
  passphraseIfTheKeyHasOne: "パスフレーズ (鍵に設定されている場合)",
  hostOrAddress: "{host} または {address}",

  // components/HostKeyDialog
  theKeyPresentedBy:
    "{target} が提示した鍵は、記録されている鍵と一致しません。サーバーが再インストールされたか、第三者が接続を傍受している可能性があります。鍵が変わった理由がわかっている場合にのみ続行してください。",
  hasNotBeenSeenBefore:
    "{target} にはこれまで接続したことがありません。続行する前に、サーバー管理者から伝えられたフィンガープリントと照合してください。",
  fingerprint: "フィンガープリント",
  connectOnce: "今回のみ接続",
  theCertificatePresentedBy:
    "{target} が提示した証明書は、以前に信頼した証明書と一致しません。{problem} 続行する前に、サーバー管理者から伝えられたフィンガープリントと照合してください。",
  thisComputerDoesNotTrust:
    "このコンピューターは {target} が提示した証明書を信頼していません。{problem} 続行する前に、サーバー管理者から伝えられたフィンガープリントと照合してください。",
  type: "種類",
  keyType: "鍵の種類",
  trustAndConnect: "信頼して接続",
  certificateChanged: "証明書が変更されました",
  untrustedCertificate: "信頼されていない証明書",
  hostKeyChanged: "ホスト鍵が変更されました",
  unknownHost: "不明なホスト",
  replaceCertificateAndConnect: "証明書を置き換えて接続",
  replaceKeyAndConnect: "鍵を置き換えて接続",

  // components/ImportConnectionsDialog
  importFrom: "{label} からインポート",
  importedFrom: "{label} から {imported}をインポートしました。",
  imported: "{imported}をインポートしました。",
  theseCouldNotBeSaved: "次の接続は保存できませんでした: {failures}",
  importedConnections: { other: "{count} 件の接続" },
  passwordsLeftOut: {
    other:
      "システムのキーチェーンを使用できないため、{count} 件のパスワードは取り込みませんでした。接続するときに Poros が入力を求めます。",
  },
  porosWorksOutWhichApp: "どのアプリが書いたファイルかは Poros が判別します",
  lookingForOtherApps: "他のアプリを探しています...",
  porosCopiesTheSavedSites:
    "これらのアプリに保存されたサイトを、Poros の保存済みの接続にコピーします。他のアプリでは何も変更されません。",
  chooseFile: "ファイルを選択...",
  alreadySaved: "保存済み",
  leftOutSincePorosCannot: "{count} 件は、そのままでは Poros から接続できないため除外しました",
  putFolderNamesInFront: "接続名の前にフォルダー名を付ける",
  reading: "読み込んでいます...",
  otherFile: "その他のファイル...",
  importing: "インポートしています...",
  import: "インポート",
  unnamed: "名前なし",
  noSavedSitesIn: "{origins} に保存されたサイトはありません。",
  noSavedSitesInThatFile: "そのファイルに保存されたサイトはありません。",
  importCountConnections: { other: "{count} 件の接続をインポート" },
  foundFiles: "{files} が見つかりました",
  notFoundInChoose: "{places} には見つかりませんでした。{choose}",
  keepThePasswordsInTheKeychain: {
    other: "{count} 件のパスワードをシステムのキーチェーンに保存",
  },
  sitesFrom: { other: "{source} のサイト {count} 件" },

  // components/QuickConnectBar
  cloudStorage: "クラウドストレージ",
  hostOrUserHostPort: "ホストまたは user@host:port",

  // components/SaveConnectionDialog
  saveConnection: "接続を保存",

  // lib/connectionImport
  filezilla: "FileZilla",
  winscp: "WinSCP",
  smartftp: "SmartFTP",
  cyberduckAndMountainDuck: "Cyberduck と Mountain Duck",
  puttyAndKitty: "PuTTY と KiTTY",
  mobaxterm: "MobaXterm",
  opensshConfig: "OpenSSH の設定",
  gnomeFilesAndKdeDolphin: "GNOME Files と KDE Dolphin",
  listOfAddresses: "アドレスの一覧",
  chooseFileZillaFiles:
    "sitemanager.xml、またはサイトマネージャーから書き出したファイル (ファイル > エクスポート) を選択してください。",
  chooseWinScpFiles:
    "WinSCP.ini、または「ツール > 設定のエクスポート/バックアップ」で書き出したファイルを選択してください。",
  chooseSmartFtpFiles:
    "「お気に入り > ツール > エクスポート」で書き出した一覧、または古いバージョンの .xml のお気に入りを選択してください。",
  chooseCyberduckFiles: "ブックマークファイル (.duck) を選択してください。",
  choosePuttyFiles:
    "Sessions レジストリキーを書き出した .reg ファイル、または PuTTY のセッションファイルを選択してください。",
  chooseMobaXtermFiles:
    "MobaXterm.ini、または Sessions > Export で書き出した .mxtsessions ファイルを選択してください。",
  chooseOpenSshFiles: "~/.ssh/config などの ssh 設定ファイルを選択してください。",
  chooseFileManagerFiles: "gtk-3.0/bookmarks または user-places.xbel を選択してください。",
  chooseUrlListFiles:
    "sftp:// または ftp:// のアドレスを 1 行に 1 つずつ書いたテキストファイルを選択してください。",

  // lib/protocols
  ftpsExplicitTls: "FTPS (明示的 TLS)",
  ftpsImplicitTls: "FTPS (暗黙的 TLS)",
  ftpsImplicit: "FTPS 暗黙的",
  webdavHttp: "WebDAV (HTTP)",
  webdav: "WebDAV",
  webdavHttps: "WebDAV (HTTPS)",
  webdavs: "WebDAVS",
  s3AndCompatible: "S3 と互換サービス",
  backblazeB2: "Backblaze B2",
  googleDrive: "Google Drive",
  onedrive: "OneDrive",
  box: "Box",
  cryptomatorVault: "Cryptomator 金庫",
  vault: "金庫",
  google: "Google",
  microsoft: "Microsoft",
  accessKeyId: "アクセスキー ID",
  secretAccessKey: "シークレットアクセスキー",
  applicationKeyId: "アプリケーションキー ID",
  applicationKey: "アプリケーションキー",

  // state/connectFlow
  theHostKeyWasNot: "ホスト鍵が承認されませんでした。",

  // state/sessionStore
  itsTabWasClosedMeanwhile: "その間にタブが閉じられました。",
} satisfies TranslatedArea<"connect">;
