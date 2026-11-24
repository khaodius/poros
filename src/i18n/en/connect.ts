import type { MessageArea } from "../catalog";

export const connect = {
  // components/ConnectDialog
  selectPrivateKey: "Select private key",
  savedConnections: "Saved connections",
  importFromFilezillaWinscpAnd: "Import from FileZilla, WinSCP and other apps",
  importConnections: "Import connections",
  newConnection: "New connection",
  connectionsYouSaveAppearHere: "Connections you save appear here.",
  importThemFromAnotherApp: "Import them from another app",
  protocol: "Protocol",
  account: "Account",
  emptyForAmazonS3: "Empty for Amazon S3",
  port: "Port",
  forAnotherServiceEnterIts:
    "For another service, enter its endpoint, such as {example}, or {local} for a server without TLS.",
  anonymous: "anonymous",
  emptyIfTheServerNeeds: "Empty if the server needs none",
  authentication: "Authentication",
  savedInTheSystemKeychain: "Saved in the system keychain",
  privateKeyFile: "Private key file",
  sshIdEd25519OrC: "~/.ssh/id_ed25519 or C:\\keys\\server.ppk",
  opensshPemPkcs8And: "OpenSSH, PEM, PKCS#8 and PuTTY .ppk (v2 and v3) keys are supported.",
  usesKeysLoadedInYour:
    "Uses keys loaded in your SSH agent: {variable} on Linux and macOS, the OpenSSH Authentication Agent service or Pageant on Windows.",
  region: "Region",
  fromTheEndpointOrUs: "From the endpoint, or us-east-1",
  bucketAddress: "Bucket address",
  automatic: "Automatic",
  pathEndpointBucket: "Path (endpoint/bucket)",
  virtualHostedBucketEndpoint: "Virtual-hosted (bucket.endpoint)",
  remoteFolderOptional: "Remote folder (optional)",
  jumpHost: "Jump host",
  noneConnectStraightToThe: "None, connect straight to the server",
  aDeletedConnection: "A deleted connection",
  logsInToThisSaved:
    "Logs in to this saved connection first and reaches the server through it, like ProxyJump in OpenSSH.",
  rememberThePassword: "Remember the password in the system keychain",
  rememberThePassphrase: "Remember the passphrase in the system keychain",
  rememberTheSecretAccessKey: "Remember the secret access key in the system keychain",
  rememberTheApplicationKey: "Remember the application key in the system keychain",
  activeModeTheServerOpens: "Active mode: the server opens data connections to this computer",
  connectWithoutTheProxy: "Connect without the proxy",
  rsyncAndSynchronization: "rsync and synchronization",
  rsyncOnTheServer: "rsync on the server",
  theCommandThatStartsRsync:
    "The command that starts rsync here, such as sudo rsync or /usr/local/bin/rsync. Blank uses the one in the transfer settings.",
  rsyncArgumentsForFolderSynchronizations: "rsync arguments for folder synchronizations",
  rsyncArgumentsForFolderSynchronizationsHint:
    "Synchronizations with this server start from the defaults in Settings with these applied over them.",
  password: "Password",
  keyFile: "Key file",
  sshAgent: "SSH agent",
  host: "Host",
  username: "Username",
  browserSignIn: "Browser sign-in",
  endpoint: "Endpoint",
  saveThisConnectionAndIts: "Save this connection and its sign-in",
  saveThisConnection: "Save this connection",
  deleteForGood: "Delete for good",
  saving: "Saving...",
  connecting: "Connecting...",
  shownInSavedConnections: "Shown in saved connections",
  topFolderOfTheDrive: "Top folder of the drive",
  bucketOrBucketFolder: "{bucket} or {folder}",
  loginFolder: "Login folder",
  theFolderInTheAddressOrTheTop: "The folder in the address, or the top",
  homeFolder: "Home folder",
  passphrase: "Passphrase",
  passphraseIfTheKeyHasOne: "Passphrase (if the key has one)",
  hostOrAddress: "{host} or {address}",

  // components/HostKeyDialog
  theKeyPresentedBy:
    "The key presented by {target} does not match the one on record. The server may have been reinstalled, or someone may be intercepting the connection. Continue only if you know why the key changed.",
  hasNotBeenSeenBefore:
    "{target} has not been seen before. Compare the fingerprint with the one your server administrator gave you before continuing.",
  fingerprint: "Fingerprint",
  connectOnce: "Connect once",
  theCertificatePresentedBy:
    "The certificate presented by {target} does not match the one you trusted before. {problem} Compare the fingerprint with the one your server administrator gave you before continuing.",
  thisComputerDoesNotTrust:
    "This computer does not trust the certificate presented by {target}. {problem} Compare the fingerprint with the one your server administrator gave you before continuing.",
  type: "Type",
  keyType: "Key type",
  trustAndConnect: "Trust and connect",
  certificateChanged: "Certificate changed",
  untrustedCertificate: "Untrusted certificate",
  hostKeyChanged: "Host key changed",
  unknownHost: "Unknown host",
  replaceCertificateAndConnect: "Replace certificate and connect",
  replaceKeyAndConnect: "Replace key and connect",

  // components/ImportConnectionsDialog
  importFrom: "Import from {label}",
  importedFrom: "Imported {imported} from {label}.",
  imported: "Imported {imported}.",
  theseCouldNotBeSaved: "These could not be saved: {failures}",
  importedConnections: { one: "{count} connection", other: "{count} connections" },
  passwordsLeftOut: {
    one: "The system keychain is not available, so {count} password was left out; Poros asks for them when you connect.",
    other:
      "The system keychain is not available, so {count} passwords were left out; Poros asks for them when you connect.",
  },
  porosWorksOutWhichApp: "Poros works out which app wrote the file",
  lookingForOtherApps: "Looking for other apps...",
  porosCopiesTheSavedSites:
    "Poros copies the saved sites of these apps into its own saved connections. Nothing changes in the other app.",
  chooseFile: "Choose file...",
  alreadySaved: "Already saved",
  leftOutSincePorosCannot: {
    one: "{count} left out, since Poros cannot connect to them as they are",
    other: "{count} left out, since Poros cannot connect to them as they are",
  },
  putFolderNamesInFront: "Put folder names in front of connection names",
  reading: "Reading...",
  otherFile: "Other file...",
  importing: "Importing...",
  import: "Import",
  unnamed: "Unnamed",
  noSavedSitesIn: "There are no saved sites in {origins}.",
  noSavedSitesInThatFile: "There are no saved sites in that file.",
  importCountConnections: { one: "Import {count} connection", other: "Import {count} connections" },
  foundFiles: "Found {files}",
  notFoundInChoose: "Not found in {places}. {choose}",
  placeOrPlace: "{first} or {second}",
  keepThePasswordsInTheKeychain: {
    one: "Keep the {count} password in the system keychain",
    other: "Keep the {count} passwords in the system keychain",
  },
  sitesFrom: { one: "{count} site from {source}", other: "{count} sites from {source}" },

  // components/QuickConnectBar
  cloudStorage: "Cloud storage",
  hostOrUserHostPort: "Host or user@host:port",

  // components/SaveConnectionDialog
  saveConnection: "Save connection",

  // lib/connectionImport
  filezilla: "FileZilla",
  winscp: "WinSCP",
  smartftp: "SmartFTP",
  cyberduckAndMountainDuck: "Cyberduck and Mountain Duck",
  puttyAndKitty: "PuTTY and KiTTY",
  mobaxterm: "MobaXterm",
  opensshConfig: "OpenSSH config",
  gnomeFilesAndKdeDolphin: "GNOME Files and KDE Dolphin",
  listOfAddresses: "List of addresses",
  chooseFileZillaFiles:
    "Choose sitemanager.xml, or a file exported from the Site Manager (File > Export).",
  chooseWinScpFiles: "Choose WinSCP.ini, or a file from Tools > Export/Backup Configuration.",
  chooseSmartFtpFiles:
    "Choose the list from Favorites > Tools > Export, or .xml favorites from older versions.",
  chooseCyberduckFiles: "Choose their bookmark files (.duck).",
  choosePuttyFiles:
    "Choose a .reg export of their Sessions registry key, or PuTTY's session files.",
  chooseMobaXtermFiles: "Choose MobaXterm.ini, or a .mxtsessions file from Sessions > Export.",
  chooseOpenSshFiles: "Choose an ssh config file, such as ~/.ssh/config.",
  chooseFileManagerFiles: "Choose gtk-3.0/bookmarks or user-places.xbel.",
  chooseUrlListFiles: "Choose a text file with one sftp:// or ftp:// address per line.",

  // lib/protocols
  ftpsExplicitTls: "FTPS (explicit TLS)",
  ftpsImplicitTls: "FTPS (implicit TLS)",
  ftpsImplicit: "FTPS implicit",
  webdavHttp: "WebDAV (HTTP)",
  webdav: "WebDAV",
  webdavHttps: "WebDAV (HTTPS)",
  webdavs: "WebDAVS",
  s3AndCompatible: "S3 and compatible",
  backblazeB2: "Backblaze B2",
  googleDrive: "Google Drive",
  onedrive: "OneDrive",
  box: "Box",
  cryptomatorVault: "Cryptomator vault",
  vault: "Vault",
  google: "Google",
  microsoft: "Microsoft",
  accessKeyId: "Access key ID",
  secretAccessKey: "Secret access key",
  applicationKeyId: "Application key ID",
  applicationKey: "Application key",

  // state/connectFlow
  theHostKeyWasNot: "The host key was not accepted.",

  // state/sessionStore
  itsTabWasClosedMeanwhile: "Its tab was closed meanwhile.",
} as const satisfies MessageArea;
