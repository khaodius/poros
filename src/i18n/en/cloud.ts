import type { MessageArea } from "../catalog";

export const cloud = {
  // components/CloudSettings
  googleDrive: "Google Drive",
  inGoogleCloudConsoleCreate:
    "In Google Cloud Console, create a project and enable the Google Drive API.",
  setUpTheOauthConsent:
    "Set up the OAuth consent screen. While the app is in testing, add your Google account as a test user.",
  createAnOauthClientId: "Create an OAuth client ID of type Desktop app.",
  clientId: "Client ID",
  endsInAppsGoogleusercontentCom: "Ends in apps.googleusercontent.com",
  clientSecret: "Client secret",
  clientSecretHint: "Google issues one with every desktop app client.",
  onedrive: "OneDrive",
  inTheMicrosoftEntraAdmin:
    "In the Microsoft Entra admin center, register an app for accounts in any organizational directory and personal Microsoft accounts.",
  underAuthenticationAddTheMobile:
    "Under Authentication, add the Mobile and desktop applications platform with the redirect URI",
  andAllowPublicClientFlows: "and allow public client flows.",
  httpLocalhost: "http://localhost",
  underApiPermissionsAddThe: "Under API permissions, add the Microsoft Graph delegated permissions",
  and: "and",
  filesReadwriteAll: "Files.ReadWrite.All",
  offlineAccess: "offline_access",
  applicationClientId: "Application (client) ID",
  directoryTenant: "Directory (tenant)",
  directoryTenantHint:
    "Leave empty for any account. Enter a directory ID to limit sign-in to one organization.",
  common: "common",
  box: "Box",
  inTheBoxDeveloperConsole:
    "In the Box Developer Console, select New App and choose the User app type, which signs in with OAuth 2.0.",
  onTheConfigurationTabAdd: "On the Configuration tab, add the OAuth 2.0 redirect URI",
  underApplicationScopesKeepRead:
    "Under Application Scopes, keep Read all files and folders stored in Box, turn on Write all files and folders stored in Box, and save the changes.",
  copyTheClientIdAnd:
    "Copy the client ID and client secret from OAuth 2.0 Credentials on the same tab.",
  clientSecretHint2: "Box asks for it with every sign-in.",

  // components/CloudSignIn
  openSettings: "Open Settings",
  finishSigningInToIn: "Finish signing in to {providerName} in your browser.",
  signedInAs: "Signed in as",
  signInAgain: "Sign in again",
  porosOpensTheSignIn: "Poros opens the {providerName} sign-in page in your browser.",
  signInWith: "Sign in with {providerName}",
} as const satisfies MessageArea;
