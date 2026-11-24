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
    "Under Authentication, add the Mobile and desktop applications platform with the redirect URI {uri} and allow public client flows.",
  underApiPermissionsAddThe:
    "Under API permissions, add the Microsoft Graph delegated permissions {files} and {offline}.",
  applicationClientId: "Application (client) ID",
  directoryTenant: "Directory (tenant)",
  directoryTenantHint:
    "Leave empty for any account. Enter a directory ID to limit sign-in to one organization.",
  box: "Box",
  inTheBoxDeveloperConsole:
    "In the Box Developer Console, select New App and choose the User app type, which signs in with OAuth 2.0.",
  onTheConfigurationTabAdd: "On the Configuration tab, add the OAuth 2.0 redirect URI {uri}.",
  underApplicationScopesKeepRead:
    "Under Application Scopes, keep Read all files and folders stored in Box, turn on Write all files and folders stored in Box, and save the changes.",
  copyTheClientIdAnd:
    "Copy the client ID and client secret from OAuth 2.0 Credentials on the same tab.",
  clientSecretHint2: "Box asks for it with every sign-in.",
  thisReleaseSignsInWith: "This release signs in with its own Google app. To use yours instead:",
  porosSignsInToGoogle: "Poros signs in to Google Drive with a Google app you register:",
  thisReleaseSignsInWithIts:
    "This release signs in with its own Microsoft app. To use yours instead:",
  porosSignsInToOnedrive: "Poros signs in to OneDrive with a Microsoft app you register:",
  thisReleaseSignsInWithItsOwn: "This release signs in with its own Box app. To use yours instead:",
  porosSignsInToBox: "Poros signs in to Box with a Box app you register:",

  // components/CloudSignIn
  openSettings: "Open Settings",
  finishSigningInToIn: "Finish signing in to {providerName} in your browser.",
  signedInAs: "Signed in as {account}",
  signInAgain: "Sign in again",
  porosOpensTheSignIn: "Poros opens the {providerName} sign-in page in your browser.",
  signInWith: "Sign in with {providerName}",
  needsOwnApp:
    "Signing in to {service} needs your own {provider} app registration. Add its client ID under Settings, Cloud accounts.",
  needsOwnAppWithSecret:
    "Signing in to {service} needs your own {provider} app registration. Add its client ID and client secret under Settings, Cloud accounts.",
  browserSignedIn: "Poros is signed in",
  browserSignedInDetail: "You can close this tab and return to Poros.",
  browserNotCompleted: "Sign-in was not completed",
  browserNotCompletedDetail: "You can close this tab and try again from Poros.",
  browserExpired: "This sign-in has expired",
  browserExpiredDetail: "Start the sign-in again from Poros.",
} as const satisfies MessageArea;
