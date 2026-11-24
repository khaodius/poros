import type { CloudProvider, Protocol } from "./types";
import { t } from "../i18n";

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
  {
    value: "ftps",
    get label() {
      return t("connect.ftpsExplicitTls");
    },
    shortLabel: "FTPS",
    defaultPort: 21,
  },
  {
    value: "ftpsImplicit",
    get label() {
      return t("connect.ftpsImplicitTls");
    },
    get shortLabel() {
      return t("connect.ftpsImplicit");
    },
    defaultPort: 990,
  },
  {
    value: "webDav",
    get label() {
      return t("connect.webdavHttp");
    },
    get shortLabel() {
      return t("connect.webdav");
    },
    defaultPort: 80,
  },
  {
    value: "webDavs",
    get label() {
      return t("connect.webdavHttps");
    },
    get shortLabel() {
      return t("connect.webdavs");
    },
    defaultPort: 443,
  },
  {
    value: "s3",
    get label() {
      return t("connect.s3AndCompatible");
    },
    shortLabel: "S3",
    defaultPort: 443,
  },
  {
    value: "backblazeB2",
    get label() {
      return t("connect.backblazeB2");
    },
    get shortLabel() {
      return t("connect.backblazeB2");
    },
    defaultPort: 443,
  },
  {
    value: "googleDrive",
    get label() {
      return t("connect.googleDrive");
    },
    get shortLabel() {
      return t("connect.googleDrive");
    },
    defaultPort: 443,
    provider: "google",
  },
  {
    value: "oneDrive",
    get label() {
      return t("connect.onedrive");
    },
    get shortLabel() {
      return t("connect.onedrive");
    },
    defaultPort: 443,
    provider: "microsoft",
  },
  {
    value: "box",
    get label() {
      return t("connect.box");
    },
    get shortLabel() {
      return t("connect.box");
    },
    defaultPort: 443,
    provider: "box",
  },
];

/** An unlocked vault's session. Opened from a server's pane, so no list to connect from has it. */
const VAULT_PROTOCOL: ProtocolInfo = {
  value: "cryptomator",
  get label() {
    return t("connect.cryptomatorVault");
  },
  get shortLabel() {
    return t("connect.vault");
  },
  defaultPort: 0,
};

export const PROVIDER_NAMES: Record<CloudProvider, string> = {
  get google() {
    return t("connect.google");
  },
  get microsoft() {
    return t("connect.microsoft");
  },
  get box() {
    return t("connect.box");
  },
};

export function protocolInfo(protocol: Protocol): ProtocolInfo {
  if (isVault(protocol)) return VAULT_PROTOCOL;
  return PROTOCOLS.find((info) => info.value === protocol) ?? PROTOCOLS[0];
}

/** The session of an unlocked vault, rather than of a server or a cloud account. */
export function isVault(protocol: Protocol): boolean {
  return protocol === "cryptomator";
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
  if (protocol === "s3") {
    return { id: t("connect.accessKeyId"), secret: t("connect.secretAccessKey") };
  }
  if (protocol === "backblazeB2") {
    return { id: t("connect.applicationKeyId"), secret: t("connect.applicationKey") };
  }
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
