import type { TranslatedArea } from "../catalog";

export const cloud = {
  // components/CloudSettings
  googleDrive: "Google Drive",
  inGoogleCloudConsoleCreate: "在 Google Cloud Console 中创建一个项目，并启用 Google Drive API。",
  setUpTheOauthConsent:
    "设置 OAuth 同意屏幕。在应用处于测试阶段时，将您的 Google 账户添加为测试用户。",
  createAnOauthClientId: "创建一个类型为“桌面应用”的 OAuth 客户端 ID。",
  clientId: "客户端 ID",
  endsInAppsGoogleusercontentCom: "以 apps.googleusercontent.com 结尾",
  clientSecret: "客户端密钥",
  clientSecretHint: "Google 会为每个桌面应用客户端签发一个。",
  onedrive: "OneDrive",
  inTheMicrosoftEntraAdmin:
    "在 Microsoft Entra 管理中心中，为任何组织目录中的账户和个人 Microsoft 账户注册一个应用。",
  underAuthenticationAddTheMobile:
    "在“身份验证”下，添加“移动和桌面应用程序”平台，重定向 URI 为 {uri}，并允许公共客户端流。",
  underApiPermissionsAddThe:
    "在“API 权限”下，添加 Microsoft Graph 委托的权限 {files} 和 {offline}。",
  applicationClientId: "应用程序（客户端）ID",
  directoryTenant: "目录（租户）",
  directoryTenantHint: "留空表示任何账户。输入目录 ID 可将登录限制为一个组织。",
  box: "Box",
  inTheBoxDeveloperConsole:
    "在 Box Developer Console 中选择 New App，并选择通过 OAuth 2.0 登录的 User 应用类型。",
  onTheConfigurationTabAdd: "在 Configuration 选项卡上，添加 OAuth 2.0 重定向 URI {uri}。",
  underApplicationScopesKeepRead:
    "在 Application Scopes 下，保留 Read all files and folders stored in Box，打开 Write all files and folders stored in Box，然后保存更改。",
  copyTheClientIdAnd: "从同一选项卡上的 OAuth 2.0 Credentials 中复制客户端 ID 和客户端密钥。",
  clientSecretHint2: "Box 在每次登录时都会要求提供它。",
  thisReleaseSignsInWith: "此版本使用自己的 Google 应用登录。如需改用您自己的应用：",
  porosSignsInToGoogle: "Poros 使用您注册的 Google 应用登录 Google Drive：",
  thisReleaseSignsInWithIts: "此版本使用自己的 Microsoft 应用登录。如需改用您自己的应用：",
  porosSignsInToOnedrive: "Poros 使用您注册的 Microsoft 应用登录 OneDrive：",
  thisReleaseSignsInWithItsOwn: "此版本使用自己的 Box 应用登录。如需改用您自己的应用：",
  porosSignsInToBox: "Poros 使用您注册的 Box 应用登录 Box：",

  // components/CloudSignIn
  openSettings: "打开设置",
  finishSigningInToIn: "请在浏览器中完成 {providerName} 登录。",
  signedInAs: "已登录为 {account}",
  signInAgain: "重新登录",
  porosOpensTheSignIn: "Poros 会在浏览器中打开 {providerName} 登录页面。",
  signInWith: "使用 {providerName} 登录",
  needsOwnApp:
    "登录 {service} 需要您自己注册的 {provider} 应用。请在“设置 > 云账户”中添加其客户端 ID。",
  needsOwnAppWithSecret:
    "登录 {service} 需要您自己注册的 {provider} 应用。请在“设置 > 云账户”中添加其客户端 ID 和客户端密钥。",
  browserSignedIn: "Poros 已登录",
  browserSignedInDetail: "您可以关闭此标签页并返回 Poros。",
  browserNotCompleted: "登录未完成",
  browserNotCompletedDetail: "您可以关闭此标签页，然后在 Poros 中重试。",
  browserExpired: "此次登录已过期",
  browserExpiredDetail: "请在 Poros 中重新开始登录。",
} satisfies TranslatedArea<"cloud">;
