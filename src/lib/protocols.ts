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
];

export const PROVIDER_NAMES: Record<CloudProvider, string> = {
  google: "Google",
  microsoft: "Microsoft",
};

export function protocolInfo(protocol: Protocol): ProtocolInfo {
  return PROTOCOLS.find((info) => info.value === protocol) ?? PROTOCOLS[0];
}

export function isCloud(protocol: Protocol): boolean {
  return protocolInfo(protocol).provider !== undefined;
}

export function isFtp(protocol: Protocol): boolean {
  return protocol === "ftp" || protocol === "ftps" || protocol === "ftpsImplicit";
}

export function defaultPort(protocol: Protocol): number {
  return protocolInfo(protocol).defaultPort;
}

/**
 * Address schemes quick connect understands. As in other FTP clients, `ftps://` is implicit TLS
 * and `ftpes://` is explicit TLS.
 */
const SCHEMES: Record<string, Protocol> = {
  sftp: "sftp",
  ssh: "sftp",
  scp: "sftp",
  ftp: "ftp",
  ftpes: "ftps",
  ftps: "ftpsImplicit",
};

export function protocolForScheme(scheme: string): Protocol | undefined {
  return SCHEMES[scheme.toLowerCase()];
}
