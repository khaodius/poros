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
  exampleComOrExampleCom: "example.com or example.com/remote.php/dav",
  exampleCom: "example.com",
  port: "Port",
  forAnotherServiceEnterIts: "For another service, enter its endpoint, such as",
  or: ", or",
  forAServerWithoutTls: "for a server without TLS.",
  s3EuCentral1Wasabisys: "s3.eu-central-1.wasabisys.com",
  httpLocalhost9000: "http://localhost:9000",
  anonymous: "anonymous",
  emptyIfTheServerNeeds: "Empty if the server needs none",
  authentication: "Authentication",
  savedInTheSystemKeychain: "Saved in the system keychain",
  privateKeyFile: "Private key file",
  sshIdEd25519OrC: "~/.ssh/id_ed25519 or C:\\keys\\server.ppk",
  opensshPemPkcs8And: "OpenSSH, PEM, PKCS#8 and PuTTY .ppk (v2 and v3) keys are supported.",
  usesKeysLoadedInYour: "Uses keys loaded in your SSH agent:",
  onLinuxAndMacosThe:
    "on Linux and macOS, the OpenSSH Authentication Agent service or Pageant on Windows.",
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
  rememberTheInTheSystem: "Remember the {secretLabel} in the system keychain",
  activeModeTheServerOpens: "Active mode: the server opens data connections to this computer",
  connectWithoutTheProxy: "Connect without the proxy",
  rsyncAndSynchronization: "rsync and synchronization",
  rsyncOnTheServer: "rsync on the server",
  theCommandThatStartsRsync:
    "The command that starts rsync here, such as sudo rsync or /usr/local/bin/rsync. Blank uses the one in the transfer settings.",
  rsyncArgumentsForFolderSynchronizations: "rsync arguments for folder synchronizations",
  rsyncArgumentsForFolderSynchronizationsHint:
    "Synchronizations with this server start from the defaults in Settings with these applied over them.",
  deleteChmodD755F644Exclude: "--delete --chmod=D755,F644 --exclude=.git/",

  // components/HostKeyDialog
  theKeyPresentedBy: "The key presented by",
  doesNotMatchTheOne:
    "does not match the one on record. The server may have been reinstalled, or someone may be intercepting the connection. Continue only if you know why the key changed.",
  hasNotBeenSeenBefore:
    "has not been seen before. Compare the fingerprint with the one your server administrator gave you before continuing.",
  fingerprint: "Fingerprint",
  connectOnce: "Connect once",

  // components/ImportConnectionsDialog
  importFrom: "Import from {label}",
  importedFrom: "Imported {imported} from {label}.{passwords}",
  importedTheseCouldNotBe: "Imported {imported}.{passwords} These could not be saved: {failures}",
  importedConnections: { one: "{count} connection", other: "{count} connections" },
  passwordsLeftOut: {
    one: " The system keychain is not available, so {count} password was left out; Poros asks for them when you connect.",
    other:
      " The system keychain is not available, so {count} passwords were left out; Poros asks for them when you connect.",
  },
  porosWorksOutWhichApp: "Poros works out which app wrote the file",
  lookingForOtherApps: "Looking for other apps...",
  porosCopiesTheSavedSites:
    "Poros copies the saved sites of these apps into its own saved connections. Nothing changes in the other app.",
  chooseFile: "Choose file...",
  alreadySaved: "Already saved",
  password: "Password",
  keyFile: "Key file",
  sshAgent: "SSH agent",
  leftOutSincePorosCannot: "{count} left out, since Poros cannot connect to them as they are",
  putFolderNamesInFront: "Put folder names in front of connection names",

  // components/QuickConnectBar
  cloudStorage: "Cloud storage",
  hostOrUserHostPort: "Host or user@host:port",
  host: "Host",
  username: "Username",

  // components/SaveConnectionDialog
  saveConnection: "Save connection",

  // state/connectFlow
  theHostKeyWasNot: "The host key was not accepted.",

  // state/sessionStore
  itsTabWasClosedMeanwhile: "Its tab was closed meanwhile.",
  // components/ConnectDialog
  browserSignIn: "Browser sign-in",

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
  // components/ConnectDialog
  endpoint: "Endpoint",
  saveThisConnectionAndIts: "Save this connection and its sign-in",
  saveThisConnection: "Save this connection",
  deleteForGood: "Delete for good",
  saving: "Saving...",
  connecting: "Connecting...",

  // components/HostKeyDialog
  theCertificatePresentedBy: "The certificate presented by ",
  thisComputerDoesNotTrust: "This computer does not trust the certificate presented by ",
  type: "Type",
  keyType: "Key type",
  trustAndConnect: "Trust and connect",

  // components/ImportConnectionsDialog
  reading: "Reading...",
  otherFile: "Other file...",
  importing: "Importing...",
  import: "Import",
  unnamed: "Unnamed",

  // lib/protocols
  accessKeyId: "Access key ID",
  secretAccessKey: "Secret access key",
  applicationKeyId: "Application key ID",
  applicationKey: "Application key",
} as const satisfies MessageArea;
