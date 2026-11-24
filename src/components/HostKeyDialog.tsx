import { ShieldAlert, ShieldQuestion } from "lucide-react";
import { approvalOf } from "../lib/hostKey";
import { useHostKeyPrompt } from "../state/connectFlow";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

export function HostKeyDialog() {
  const t = useT();
  const question = useHostKeyPrompt((state) => state.question);
  if (!question) return null;
  const { hostKey, changed, answer } = question;
  const problem = hostKey.certificateProblem;
  // FTPS, WebDAV and S3 servers prove themselves with a TLS certificate instead of an SSH host
  // key.
  const certificate = problem !== undefined;
  const usualPorts = certificate ? [21, 443] : [22];
  const target = usualPorts.includes(hostKey.port)
    ? hostKey.host
    : `${hostKey.host}:${hostKey.port}`;
  const title = certificate
    ? changed
      ? t("connect.certificateChanged")
      : t("connect.untrustedCertificate")
    : changed
      ? t("connect.hostKeyChanged")
      : t("connect.unknownHost");
  const shownTarget = <strong>{target}</strong>;

  return (
    <Dialog
      title={title}
      tone={changed ? "danger" : "default"}
      width={520}
      onClose={() => answer(null)}
    >
      <div className="host-key">
        <div className={`host-key-icon ${changed ? "danger" : ""}`}>
          {changed ? <ShieldAlert size={28} /> : <ShieldQuestion size={28} />}
        </div>
        <div>
          {certificate ? (
            <p>
              {t.rich(
                changed ? "connect.theCertificatePresentedBy" : "connect.thisComputerDoesNotTrust",
                { target: shownTarget, problem },
              )}
            </p>
          ) : changed ? (
            <p>{t.rich("connect.theKeyPresentedBy", { target: shownTarget })}</p>
          ) : (
            <p>{t.rich("connect.hasNotBeenSeenBefore", { target: shownTarget })}</p>
          )}
          <dl className="host-key-details">
            <dt>{certificate ? t("connect.type") : t("connect.keyType")}</dt>
            <dd>{hostKey.algorithm}</dd>
            <dt>{t("connect.fingerprint")}</dt>
            <dd className="mono selectable">{hostKey.fingerprint}</dd>
          </dl>
        </div>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={() => answer(null)}>
          {t("common.cancel")}
        </button>
        <button type="button" className="button" onClick={() => answer(approvalOf(hostKey, false))}>
          {t("connect.connectOnce")}
        </button>
        <button
          type="button"
          data-autofocus={changed ? undefined : true}
          className={`button ${changed ? "button-danger" : "button-primary"}`}
          onClick={() => answer(approvalOf(hostKey, true))}
        >
          {changed
            ? certificate
              ? t("connect.replaceCertificateAndConnect")
              : t("connect.replaceKeyAndConnect")
            : t("connect.trustAndConnect")}
        </button>
      </div>
    </Dialog>
  );
}
