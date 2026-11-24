import type { TranslatedArea } from "../catalog";

export const cloud = {
  // components/CloudSettings
  googleDrive: "Google Drive",
  inGoogleCloudConsoleCreate:
    "Google Cloud コンソールでプロジェクトを作成し、Google Drive API を有効にします。",
  setUpTheOauthConsent:
    "OAuth 同意画面を設定します。アプリがテスト中の間は、自分の Google アカウントをテストユーザーとして追加します。",
  createAnOauthClientId: "種類が「デスクトップ アプリ」の OAuth クライアント ID を作成します。",
  clientId: "クライアント ID",
  endsInAppsGoogleusercontentCom: "末尾は apps.googleusercontent.com",
  clientSecret: "クライアントシークレット",
  clientSecretHint: "デスクトップ アプリのクライアントには、Google が必ず 1 つ発行します。",
  onedrive: "OneDrive",
  inTheMicrosoftEntraAdmin:
    "Microsoft Entra 管理センターで、任意の組織ディレクトリ内のアカウントと個人用 Microsoft アカウントを対象にアプリを登録します。",
  underAuthenticationAddTheMobile:
    "「認証」で、リダイレクト URI {uri} を指定して「モバイル アプリケーションとデスクトップ アプリケーション」のプラットフォームを追加し、パブリッククライアントフローを許可します。",
  underApiPermissionsAddThe:
    "「API のアクセス許可」で、Microsoft Graph の委任されたアクセス許可 {files} と {offline} を追加します。",
  applicationClientId: "アプリケーション (クライアント) ID",
  directoryTenant: "ディレクトリ (テナント)",
  directoryTenantHint:
    "どのアカウントでもサインインできるようにするには空欄にします。1 つの組織に限るには、ディレクトリ ID を入力します。",
  box: "Box",
  inTheBoxDeveloperConsole:
    "Box 開発者コンソールで「新しいアプリ」を選択し、OAuth 2.0 でサインインする「ユーザーアプリ」の種類を選びます。",
  onTheConfigurationTabAdd: "「構成」タブで、OAuth 2.0 のリダイレクト URI {uri} を追加します。",
  underApplicationScopesKeepRead:
    "「アプリケーションスコープ」で、「Box に格納されているすべてのファイルとフォルダーの読み取り」はそのままにし、「Box に格納されているすべてのファイルとフォルダーへの書き込み」をオンにして、変更を保存します。",
  copyTheClientIdAnd:
    "同じタブの「OAuth 2.0 資格情報」から、クライアント ID とクライアントシークレットをコピーします。",
  clientSecretHint2: "Box はサインインのたびにこれを求めます。",
  thisReleaseSignsInWith:
    "このリリースは独自の Google アプリでサインインします。代わりに自分のアプリを使うには:",
  porosSignsInToGoogle: "Poros は、自分で登録した Google アプリで Google Drive にサインインします:",
  thisReleaseSignsInWithIts:
    "このリリースは独自の Microsoft アプリでサインインします。代わりに自分のアプリを使うには:",
  porosSignsInToOnedrive:
    "Poros は、自分で登録した Microsoft アプリで OneDrive にサインインします:",
  thisReleaseSignsInWithItsOwn:
    "このリリースは独自の Box アプリでサインインします。代わりに自分のアプリを使うには:",
  porosSignsInToBox: "Poros は、自分で登録した Box アプリで Box にサインインします:",

  // components/CloudSignIn
  openSettings: "設定を開く",
  finishSigningInToIn: "ブラウザーで {providerName} へのサインインを完了してください。",
  signedInAs: "{account} としてサインイン済み",
  signInAgain: "もう一度サインイン",
  porosOpensTheSignIn: "Poros がブラウザーで {providerName} のサインインページを開きます。",
  signInWith: "{providerName} でサインイン",
  needsOwnApp:
    "{service} にサインインするには、自分で登録した {provider} アプリが必要です。「設定」の「クラウドアカウント」で、そのクライアント ID を追加してください。",
  needsOwnAppWithSecret:
    "{service} にサインインするには、自分で登録した {provider} アプリが必要です。「設定」の「クラウドアカウント」で、そのクライアント ID とクライアントシークレットを追加してください。",
  browserSignedIn: "Poros のサインインが完了しました",
  browserSignedInDetail: "このタブを閉じて Poros に戻ってください。",
  browserNotCompleted: "サインインは完了しませんでした",
  browserNotCompletedDetail: "このタブを閉じて、Poros からもう一度お試しください。",
  browserExpired: "このサインインは期限切れです",
  browserExpiredDetail: "Poros からもう一度サインインを開始してください。",
} satisfies TranslatedArea<"cloud">;
