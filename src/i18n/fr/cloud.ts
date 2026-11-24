import type { TranslatedArea } from "../catalog";

export const cloud = {
  // components/CloudSettings
  googleDrive: "Google Drive",
  inGoogleCloudConsoleCreate:
    "Dans la Google Cloud Console, créez un projet et activez l’API Google Drive.",
  setUpTheOauthConsent:
    "Configurez l’écran de consentement OAuth. Tant que l’application est en test, ajoutez votre compte Google comme utilisateur test.",
  createAnOauthClientId: "Créez un ID client OAuth de type Application de bureau.",
  clientId: "ID client",
  endsInAppsGoogleusercontentCom: "Se termine par apps.googleusercontent.com",
  clientSecret: "Code secret du client",
  clientSecretHint: "Google en fournit un avec chaque client d’application de bureau.",
  onedrive: "OneDrive",
  inTheMicrosoftEntraAdmin:
    "Dans le centre d’administration Microsoft Entra, inscrivez une application pour les comptes de tout annuaire d’organisation et les comptes Microsoft personnels.",
  underAuthenticationAddTheMobile:
    "Sous Authentification, ajoutez la plateforme Applications mobiles et de bureau avec l’URI de redirection {uri} et autorisez les flux de client public.",
  underApiPermissionsAddThe:
    "Sous Autorisations d’API, ajoutez les autorisations déléguées Microsoft Graph {files} et {offline}.",
  applicationClientId: "ID d’application (client)",
  directoryTenant: "Annuaire (locataire)",
  directoryTenantHint:
    "Laissez vide pour tout compte. Saisissez un ID d’annuaire pour limiter la connexion à une seule organisation.",
  box: "Box",
  inTheBoxDeveloperConsole:
    "Dans la Box Developer Console, sélectionnez New App et choisissez le type User app, qui se connecte avec OAuth 2.0.",
  onTheConfigurationTabAdd:
    "Dans l’onglet Configuration, ajoutez l’URI de redirection OAuth 2.0 {uri}.",
  underApplicationScopesKeepRead:
    "Sous Application Scopes, gardez Read all files and folders stored in Box, activez Write all files and folders stored in Box, puis enregistrez les modifications.",
  copyTheClientIdAnd:
    "Copiez l’ID client et le code secret du client depuis OAuth 2.0 Credentials, dans le même onglet.",
  clientSecretHint2: "Box le demande à chaque connexion.",
  thisReleaseSignsInWith:
    "Cette version se connecte avec sa propre application Google. Pour utiliser la vôtre à la place\u00a0:",
  porosSignsInToGoogle:
    "Poros se connecte à Google Drive avec une application Google que vous inscrivez\u00a0:",
  thisReleaseSignsInWithIts:
    "Cette version se connecte avec sa propre application Microsoft. Pour utiliser la vôtre à la place\u00a0:",
  porosSignsInToOnedrive:
    "Poros se connecte à OneDrive avec une application Microsoft que vous inscrivez\u00a0:",
  thisReleaseSignsInWithItsOwn:
    "Cette version se connecte avec sa propre application Box. Pour utiliser la vôtre à la place\u00a0:",
  porosSignsInToBox: "Poros se connecte à Box avec une application Box que vous inscrivez\u00a0:",

  // components/CloudSignIn
  openSettings: "Ouvrir les paramètres",
  finishSigningInToIn: "Terminez la connexion à {providerName} dans votre navigateur.",
  signedInAs: "Connecté en tant que {account}",
  signInAgain: "Se reconnecter",
  porosOpensTheSignIn: "Poros ouvre la page de connexion de {providerName} dans votre navigateur.",
  signInWith: "Se connecter avec {providerName}",
  needsOwnApp:
    "La connexion à {service} nécessite votre propre inscription d’application {provider}. Ajoutez son ID client dans Paramètres, Comptes cloud.",
  needsOwnAppWithSecret:
    "La connexion à {service} nécessite votre propre inscription d’application {provider}. Ajoutez son ID client et son code secret dans Paramètres, Comptes cloud.",
  browserSignedIn: "Poros est connecté",
  browserSignedInDetail: "Vous pouvez fermer cet onglet et revenir à Poros.",
  browserNotCompleted: "La connexion n’a pas abouti",
  browserNotCompletedDetail: "Vous pouvez fermer cet onglet et réessayer depuis Poros.",
  browserExpired: "Cette connexion a expiré",
  browserExpiredDetail: "Relancez la connexion depuis Poros.",
} satisfies TranslatedArea<"cloud">;
