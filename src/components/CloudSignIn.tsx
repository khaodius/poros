import { useEffect, useRef, useState } from "react";
import { CircleUserRound, LoaderCircle } from "lucide-react";
import { useCloudProviders } from "../hooks/useCloudProviders";
import { cloud, toAppError } from "../lib/ipc";
import { PROVIDER_NAMES, protocolInfo } from "../lib/protocols";
import type { Protocol, SignedIn } from "../lib/types";
import { useUiStore } from "../state/uiStore";

interface CloudSignInProps {
  protocol: Protocol;
  /** The account a fresh sign-in or the saved connection holds. */
  account: string | null;
  onSignedIn: (signedIn: SignedIn) => void;
  onError: (message: string | null) => void;
}

/** Signs in to a cloud storage account in the system browser. */
export function CloudSignIn({ protocol, account, onSignedIn, onError }: CloudSignInProps) {
  const info = protocolInfo(protocol);
  const provider = info.provider ?? "google";
  const providerName = PROVIDER_NAMES[provider];
  const statuses = useCloudProviders();
  const openDialog = useUiStore((state) => state.open);
  const [waiting, setWaiting] = useState(false);
  const request = useRef<string | null>(null);

  useEffect(
    () => () => {
      if (request.current) void cloud.cancelSignIn(request.current).catch(() => undefined);
      request.current = null;
    },
    [],
  );

  const signIn = async () => {
    const requestId = crypto.randomUUID();
    request.current = requestId;
    setWaiting(true);
    onError(null);
    try {
      const signedIn = await cloud.signIn(requestId, provider);
      if (request.current === requestId) onSignedIn(signedIn);
    } catch (caught) {
      const error = toAppError(caught);
      if (request.current === requestId && error.kind !== "cancelled") onError(error.message);
    }
    if (request.current === requestId) {
      request.current = null;
      setWaiting(false);
    }
  };

  const cancel = () => {
    if (request.current) void cloud.cancelSignIn(request.current).catch(() => undefined);
    request.current = null;
    setWaiting(false);
  };

  if (statuses && !statuses[provider].configured) {
    return (
      <div className="cloud-account">
        <p className="field-hint">
          Signing in to {info.label} needs your own {providerName} app registration. Add its client
          ID under Settings, Cloud accounts.
        </p>
        <button
          type="button"
          className="button"
          onClick={() => openDialog({ kind: "settings", section: "cloud" })}
        >
          Open Settings
        </button>
      </div>
    );
  }

  if (waiting) {
    return (
      <div className="cloud-account">
        <LoaderCircle size={18} className="cloud-account-icon spin" />
        <span className="cloud-account-text">
          Finish signing in to {providerName} in your browser.
        </span>
        <button type="button" className="button" onClick={cancel}>
          Cancel
        </button>
      </div>
    );
  }

  if (account) {
    return (
      <div className="cloud-account">
        <CircleUserRound size={18} className="cloud-account-icon" />
        <span className="cloud-account-text">
          Signed in as <strong>{account}</strong>
        </span>
        <button type="button" className="button" onClick={() => void signIn()}>
          Sign in again
        </button>
      </div>
    );
  }

  return (
    <div className="cloud-account">
      <span className="cloud-account-text">
        Poros opens the {providerName} sign-in page in your browser.
      </span>
      <button type="button" className="button button-primary" onClick={() => void signIn()}>
        Sign in with {providerName}
      </button>
    </div>
  );
}
