import type { TranslatedArea } from "../catalog";

export const cloud = {
  // components/CloudSettings
  googleDrive: "Google Drive",
  inGoogleCloudConsoleCreate:
    "Erstellen Sie in der Google Cloud Console ein Projekt und aktivieren Sie die Google Drive API.",
  setUpTheOauthConsent:
    "Richten Sie den OAuth-Zustimmungsbildschirm ein. Solange die App im Test ist, fügen Sie Ihr Google-Konto als Testnutzer hinzu.",
  createAnOauthClientId: "Erstellen Sie eine OAuth-Client-ID vom Typ Desktop-App.",
  clientId: "Client-ID",
  endsInAppsGoogleusercontentCom: "Endet auf apps.googleusercontent.com",
  clientSecret: "Clientschlüssel",
  clientSecretHint: "Google vergibt einen für jeden Desktop-App-Client.",
  onedrive: "OneDrive",
  inTheMicrosoftEntraAdmin:
    "Registrieren Sie im Microsoft Entra Admin Center eine App für Konten in allen Organisationsverzeichnissen und persönliche Microsoft-Konten.",
  underAuthenticationAddTheMobile:
    "Fügen Sie unter Authentifizierung die Plattform Mobile Anwendungen und Desktopanwendungen mit dem Umleitungs-URI {uri} hinzu und lassen Sie öffentliche Clientflows zu.",
  underApiPermissionsAddThe:
    "Fügen Sie unter API-Berechtigungen die delegierten Microsoft Graph-Berechtigungen {files} und {offline} hinzu.",
  applicationClientId: "Anwendungs-ID (Client)",
  directoryTenant: "Verzeichnis (Mandant)",
  directoryTenantHint:
    "Für jedes Konto leer lassen. Geben Sie eine Verzeichnis-ID ein, um die Anmeldung auf eine Organisation zu beschränken.",
  box: "Box",
  inTheBoxDeveloperConsole:
    "Wählen Sie in der Box Developer Console New App und den App-Typ User, der sich mit OAuth 2.0 anmeldet.",
  onTheConfigurationTabAdd:
    "Fügen Sie auf dem Tab Configuration den OAuth 2.0 Redirect URI {uri} hinzu.",
  underApplicationScopesKeepRead:
    "Lassen Sie unter Application Scopes Read all files and folders stored in Box eingeschaltet, schalten Sie Write all files and folders stored in Box ein und speichern Sie die Änderungen.",
  copyTheClientIdAnd:
    "Kopieren Sie Client ID und Client Secret aus OAuth 2.0 Credentials auf demselben Tab.",
  clientSecretHint2: "Box verlangt ihn bei jeder Anmeldung.",
  thisReleaseSignsInWith:
    "Diese Version meldet sich mit einer eigenen Google-App an. So verwenden Sie stattdessen Ihre:",
  porosSignsInToGoogle:
    "Poros meldet sich bei Google Drive mit einer Google-App an, die Sie registrieren:",
  thisReleaseSignsInWithIts:
    "Diese Version meldet sich mit einer eigenen Microsoft-App an. So verwenden Sie stattdessen Ihre:",
  porosSignsInToOnedrive:
    "Poros meldet sich bei OneDrive mit einer Microsoft-App an, die Sie registrieren:",
  thisReleaseSignsInWithItsOwn:
    "Diese Version meldet sich mit einer eigenen Box-App an. So verwenden Sie stattdessen Ihre:",
  porosSignsInToBox: "Poros meldet sich bei Box mit einer Box-App an, die Sie registrieren:",

  // components/CloudSignIn
  openSettings: "Einstellungen öffnen",
  finishSigningInToIn: "Schließen Sie die Anmeldung bei {providerName} in Ihrem Browser ab.",
  signedInAs: "Angemeldet als {account}",
  signInAgain: "Erneut anmelden",
  porosOpensTheSignIn: "Poros öffnet die Anmeldeseite von {providerName} in Ihrem Browser.",
  signInWith: "Mit {providerName} anmelden",
  needsOwnApp:
    "Die Anmeldung bei {service} braucht eine eigene App-Registrierung bei {provider}. Tragen Sie deren Client-ID unter Einstellungen, Cloud-Konten ein.",
  needsOwnAppWithSecret:
    "Die Anmeldung bei {service} braucht eine eigene App-Registrierung bei {provider}. Tragen Sie deren Client-ID und Clientschlüssel unter Einstellungen, Cloud-Konten ein.",
  browserSignedIn: "Poros ist angemeldet",
  browserSignedInDetail: "Sie können diesen Tab schließen und zu Poros zurückkehren.",
  browserNotCompleted: "Die Anmeldung wurde nicht abgeschlossen",
  browserNotCompletedDetail: "Sie können diesen Tab schließen und es in Poros erneut versuchen.",
  browserExpired: "Diese Anmeldung ist abgelaufen",
  browserExpiredDetail: "Starten Sie die Anmeldung in Poros erneut.",
} satisfies TranslatedArea<"cloud">;
