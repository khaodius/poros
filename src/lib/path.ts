import type { PathStyle } from "./fileSource";
import { protocolForScheme } from "./protocols";
import type { Protocol } from "./types";

export interface Crumb {
  label: string;
  path: string;
}

/**
 * Breadcrumbs for an absolute path.
 * posix:   `/home/u`      -> `/`, `home`, `u`
 * windows: `C:\Users\u`   -> `C:`, `Users`, `u`
 * UNC:     `\\srv\share\d` -> `\\srv\share`, `d`
 */
export function breadcrumbs(path: string, style: PathStyle): Crumb[] {
  if (style === "posix") {
    const parts = path.split("/").filter(Boolean);
    const crumbs: Crumb[] = [{ label: "/", path: "/" }];
    let currentPath = "";
    for (const part of parts) {
      currentPath += `/${part}`;
      crumbs.push({ label: part, path: currentPath });
    }
    return crumbs;
  }

  const normalized = path.replace(/\//g, "\\");
  if (normalized.startsWith("\\\\")) {
    const parts = normalized.slice(2).split("\\").filter(Boolean);
    if (parts.length < 2) return [{ label: normalized, path: normalized }];
    const root = `\\\\${parts[0]}\\${parts[1]}`;
    const crumbs: Crumb[] = [{ label: root, path: `${root}\\` }];
    let currentPath = root;
    for (const part of parts.slice(2)) {
      currentPath += `\\${part}`;
      crumbs.push({ label: part, path: currentPath });
    }
    return crumbs;
  }

  const parts = normalized.split("\\").filter(Boolean);
  if (parts.length === 0) return [];
  const drive = parts[0];
  const crumbs: Crumb[] = [{ label: drive, path: `${drive}\\` }];
  let currentPath = drive;
  for (const part of parts.slice(1)) {
    currentPath += `\\${part}`;
    crumbs.push({ label: part, path: currentPath });
  }
  return crumbs;
}

/** Splits `name.ext` for rename preselection; dotfiles and folders select the whole name. */
export function stemLength(name: string, isDir: boolean): number {
  if (isDir) return name.length;
  const extensionDot = name.lastIndexOf(".");
  return extensionDot > 0 ? extensionDot : name.length;
}

function decodeUserInfo(encoded: string): string {
  try {
    return decodeURIComponent(encoded);
  } catch {
    return encoded;
  }
}

/**
 * Parses quick-connect host input: `user@host:port`, `sftp://user@host:port/path`, or another
 * scheme quick connect understands, such as `ftp://`.
 */
export function parseHostInput(input: string): {
  host: string;
  username?: string;
  port?: number;
  path?: string;
  protocol?: Protocol;
} {
  let rest = input.trim();
  let protocol: Protocol | undefined;
  const scheme = rest.match(/^([a-z]+):\/\//i);
  if (scheme) {
    protocol = protocolForScheme(scheme[1]);
    if (protocol) rest = rest.slice(scheme[0].length);
  }
  let path: string | undefined;
  const slash = rest.indexOf("/");
  if (slash >= 0) {
    path = rest.slice(slash);
    rest = rest.slice(0, slash);
  }
  let username: string | undefined;
  const at = rest.lastIndexOf("@");
  if (at >= 0) {
    username = decodeUserInfo(rest.slice(0, at));
    rest = rest.slice(at + 1);
  }
  let port: number | undefined;
  const bracketed = rest.match(/^\[(.+)\](?::(\d+))?$/);
  if (bracketed) {
    rest = bracketed[1];
    port = bracketed[2] ? Number(bracketed[2]) : undefined;
  } else {
    const colon = rest.lastIndexOf(":");
    // A single colon is host:port; more than one is a bare IPv6 address.
    if (colon >= 0 && rest.indexOf(":") === colon) {
      const parsedPort = Number(rest.slice(colon + 1));
      if (Number.isInteger(parsedPort) && parsedPort > 0) port = parsedPort;
      rest = rest.slice(0, colon);
    }
  }
  return { host: rest, username, port, path, protocol };
}
