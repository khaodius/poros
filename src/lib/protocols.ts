import type { CloudProvider, Protocol } from "./types";

export interface ProtocolInfo {
  value: Protocol;
  label: string;
  /** For the quick connect bar, where space is short. */
  shortLabel: string;
  defaultPort: number;
  /** The account a cloud storage service signs in with. */
  provider?: CloudProvider;
}

export const PROTOCOLS: ProtocolInfo[] = [
  { value: "sftp", label: "SFTP", shortLabel: "SFTP", defaultPort: 22 },
  { value: "ftp", label: "FTP", shortLabel: "FTP", defaultPort: 21 },
  { value: "ftps", label: "FTPS (explicit TLS)", shortLabel: "FTPS", defaultPort: 21 },
  {
    value: "ftpsImplicit",
    label: "FTPS (implicit TLS)",
    shortLabel: "FTPS implicit",
    defaultPort: 990,
  },
  { value: "webDav", label: "WebDAV (HTTP)", shortLabel: "WebDAV", defaultPort: 80 },
  { value: "webDavs", label: "WebDAV (HTTPS)", shortLabel: "WebDAVS", defaultPort: 443 },
  { value: "s3", label: "S3 and compatible", shortLabel: "S3", defaultPort: 443 },
  { value: "backblazeB2", label: "Backblaze B2", shortLabel: "Backblaze B2", defaultPort: 443 },
  {
    value: "googleDrive",
    label: "Google Drive",
    shortLabel: "Google Drive",
    defaultPort: 443,
    provider: "google",
  },
  {
    value: "oneDrive",
    label: "OneDrive",
    shortLabel: "OneDrive",
    defaultPort: 443,
    provider: "microsoft",
  },
  { value: "box", label: "Box", shortLabel: "Box", defaultPort: 443, provider: "box" },
];

export const PROVIDER_NAMES: Record<CloudProvider, string> = {
  google: "Google",
  microsoft: "Microsoft",
  box: "Box",
};

export function protocolInfo(protocol: Protocol): ProtocolInfo {
  return PROTOCOLS.find((info) => info.value === protocol) ?? PROTOCOLS[0];
}

/** The account signs in through the browser instead of with a password or key. */
export function signsInWithBrowser(protocol: Protocol): boolean {
  return protocolInfo(protocol).provider !== undefined;
}

/** Signs in with an access key ID and its secret rather than a username. */
export function usesAccessKeys(protocol: Protocol): boolean {
  return protocol === "s3" || protocol === "backblazeB2";
}

/** A service at one fixed address, so the connection names no server. */
export function hasFixedHost(protocol: Protocol): boolean {
  return signsInWithBrowser(protocol) || protocol === "backblazeB2";
}

/** Storage in the cloud, as opposed to a server reached by name. */
export function isCloud(protocol: Protocol): boolean {
  return hasFixedHost(protocol) || protocol === "s3";
}

export function isFtp(protocol: Protocol): boolean {
  return protocol === "ftp" || protocol === "ftps" || protocol === "ftpsImplicit";
}

export function isWebDav(protocol: Protocol): boolean {
  return protocol === "webDav" || protocol === "webDavs";
}

export function defaultPort(protocol: Protocol): number {
  return protocolInfo(protocol).defaultPort;
}

/** What the username and password fields hold for services that sign in with keys. */
export function keyLabels(protocol: Protocol): { id: string; secret: string } | null {
  if (protocol === "s3") return { id: "Access key ID", secret: "Secret access key" };
  if (protocol === "backblazeB2") return { id: "Application key ID", secret: "Application key" };
  return null;
}

/**
 * Address schemes quick connect understands. As in other FTP clients, `ftps://` is implicit TLS
 * and `ftpes://` is explicit TLS; as in other WebDAV clients, web addresses are WebDAV.
 */
const SCHEMES: Record<string, Protocol> = {
  sftp: "sftp",
  ssh: "sftp",
  scp: "sftp",
  ftp: "ftp",
  ftpes: "ftps",
  ftps: "ftpsImplicit",
  dav: "webDav",
  webdav: "webDav",
  http: "webDav",
  davs: "webDavs",
  webdavs: "webDavs",
  https: "webDavs",
};

export function protocolForScheme(scheme: string): Protocol | undefined {
  return SCHEMES[scheme.toLowerCase()];
}
