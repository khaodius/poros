import { useEffect, useRef, useState } from "react";
import { CircleUserRound, LoaderCircle } from "lucide-react";
import { useCloudProviders } from "../hooks/useCloudProviders";
import { cloud, toAppError } from "../lib/ipc";
import { PROVIDER_NAMES, protocolInfo } from "../lib/protocols";
import type { Protocol, SignedIn, SignInPage } from "../lib/types";
import { useUiStore } from "../state/uiStore";
import { activeLocale, useT, type Translate } from "../i18n";

interface CloudSignInProps {
  protocol: Protocol;
  /** The account a fresh sign-in or the saved connection holds. */
  account: string | null;
  onSignedIn: (signedIn: SignedIn) => void;
  onError: (message: string | null) => void;
}

/** What the browser tab says once the provider sends it back, in the interface's language. */
function signInPage(t: Translate): SignInPage {
  return {
    lang: activeLocale(),
    signedIn: { heading: t("cloud.browserSignedIn"), detail: t("cloud.browserSignedInDetail") },
    notCompleted: {
      heading: t("cloud.browserNotCompleted"),
      detail: t("cloud.browserNotCompletedDetail"),
    },
    expired: { heading: t("cloud.browserExpired"), detail: t("cloud.browserExpiredDetail") },
  };
}

/** Signs in to a cloud storage account in the system browser. */
export function CloudSignIn({ protocol, account, onSignedIn, onError }: CloudSignInProps) {
  const t = useT();
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
      const signedIn = await cloud.signIn(requestId, provider, signInPage(t));
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
          {provider === "box"
            ? t("cloud.needsOwnAppWithSecret", { service: info.label, provider: providerName })
            : t("cloud.needsOwnApp", { service: info.label, provider: providerName })}
        </p>
        <button
          type="button"
          className="button"
          onClick={() => openDialog({ kind: "settings", section: "cloud" })}
        >
          {t("cloud.openSettings")}
        </button>
      </div>
    );
  }

  if (waiting) {
    return (
      <div className="cloud-account">
        <LoaderCircle size={18} className="cloud-account-icon spin" />
        <span className="cloud-account-text">
          {t("cloud.finishSigningInToIn", { providerName })}
        </span>
        <button type="button" className="button" onClick={cancel}>
          {t("common.cancel")}
        </button>
      </div>
    );
  }

  if (account) {
    return (
      <div className="cloud-account">
        <CircleUserRound size={18} className="cloud-account-icon" />
        <span className="cloud-account-text">
          {t.rich("cloud.signedInAs", { account: <strong>{account}</strong> })}
        </span>
        <button type="button" className="button" onClick={() => void signIn()}>
          {t("cloud.signInAgain")}
        </button>
      </div>
    );
  }

  return (
    <div className="cloud-account">
      <span className="cloud-account-text">{t("cloud.porosOpensTheSignIn", { providerName })}</span>
      <button type="button" className="button button-primary" onClick={() => void signIn()}>
        {t("cloud.signInWith", { providerName })}
      </button>
    </div>
  );
}
