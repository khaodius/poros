import type { TranslatedArea } from "../catalog";

export const cloud = {
  // components/CloudSettings
  googleDrive: "Google Drive",
  inGoogleCloudConsoleCreate:
    "En Google Cloud Console, crea un proyecto y habilita la API de Google Drive.",
  setUpTheOauthConsent:
    "Configura la pantalla de consentimiento de OAuth. Mientras la aplicación esté en pruebas, agrega tu cuenta de Google como usuario de prueba.",
  createAnOauthClientId: "Crea un ID de cliente de OAuth del tipo App de escritorio.",
  clientId: "ID de cliente",
  endsInAppsGoogleusercontentCom: "Termina en apps.googleusercontent.com",
  clientSecret: "Secreto de cliente",
  clientSecretHint: "Google emite uno con cada cliente de aplicación de escritorio.",
  onedrive: "OneDrive",
  inTheMicrosoftEntraAdmin:
    "En el Centro de administración de Microsoft Entra, registra una aplicación para cuentas de cualquier directorio organizativo y cuentas personales de Microsoft.",
  underAuthenticationAddTheMobile:
    "En Autenticación, agrega la plataforma Aplicaciones móviles y de escritorio con el URI de redirección {uri} y permite los flujos de cliente público.",
  underApiPermissionsAddThe:
    "En Permisos de API, agrega los permisos delegados de Microsoft Graph {files} y {offline}.",
  applicationClientId: "ID de aplicación (cliente)",
  directoryTenant: "Directorio (inquilino)",
  directoryTenantHint:
    "Déjalo vacío para cualquier cuenta. Escribe un ID de directorio para limitar el inicio de sesión a una organización.",
  box: "Box",
  inTheBoxDeveloperConsole:
    "En Box Developer Console, selecciona New App y elige el tipo User app, que inicia sesión con OAuth 2.0.",
  onTheConfigurationTabAdd:
    "En la pestaña Configuration, agrega el URI de redirección de OAuth 2.0 {uri}.",
  underApplicationScopesKeepRead:
    "En Application Scopes, mantén Read all files and folders stored in Box, activa Write all files and folders stored in Box y guarda los cambios.",
  copyTheClientIdAnd:
    "Copia el ID de cliente y el secreto de cliente de OAuth 2.0 Credentials en la misma pestaña.",
  clientSecretHint2: "Box lo pide en cada inicio de sesión.",
  thisReleaseSignsInWith:
    "Esta versión inicia sesión con su propia aplicación de Google. Para usar la tuya:",
  porosSignsInToGoogle:
    "Poros inicia sesión en Google Drive con una aplicación de Google que registras tú:",
  thisReleaseSignsInWithIts:
    "Esta versión inicia sesión con su propia aplicación de Microsoft. Para usar la tuya:",
  porosSignsInToOnedrive:
    "Poros inicia sesión en OneDrive con una aplicación de Microsoft que registras tú:",
  thisReleaseSignsInWithItsOwn:
    "Esta versión inicia sesión con su propia aplicación de Box. Para usar la tuya:",
  porosSignsInToBox: "Poros inicia sesión en Box con una aplicación de Box que registras tú:",

  // components/CloudSignIn
  openSettings: "Abrir Configuración",
  finishSigningInToIn: "Termina de iniciar sesión en {providerName} en tu navegador.",
  signedInAs: "Sesión iniciada como {account}",
  signInAgain: "Volver a iniciar sesión",
  porosOpensTheSignIn:
    "Poros abre la página de inicio de sesión de {providerName} en tu navegador.",
  signInWith: "Iniciar sesión con {providerName}",
  needsOwnApp:
    "Para iniciar sesión en {service} necesitas registrar tu propia aplicación de {provider}. Agrega su ID de cliente en Configuración, Cuentas en la nube.",
  needsOwnAppWithSecret:
    "Para iniciar sesión en {service} necesitas registrar tu propia aplicación de {provider}. Agrega su ID de cliente y su secreto de cliente en Configuración, Cuentas en la nube.",
  browserSignedIn: "Poros ha iniciado sesión",
  browserSignedInDetail: "Puedes cerrar esta pestaña y volver a Poros.",
  browserNotCompleted: "No se completó el inicio de sesión",
  browserNotCompletedDetail: "Puedes cerrar esta pestaña y volver a intentarlo desde Poros.",
  browserExpired: "Este inicio de sesión ha caducado",
  browserExpiredDetail: "Vuelve a iniciar sesión desde Poros.",
} satisfies TranslatedArea<"cloud">;
