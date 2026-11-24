import type { HostKeyApproval, HostKeyInfo } from "./types";

/**
 * The answer that accepts the key a question shows. It names the server the question was about,
 * so that it cannot be taken for the answer about another server on the way, such as the one
 * behind a jump host.
 */
export function approvalOf(hostKey: HostKeyInfo, remember: boolean): HostKeyApproval {
  return { host: hostKey.host, port: hostKey.port, fingerprint: hostKey.fingerprint, remember };
}
