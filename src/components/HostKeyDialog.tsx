import { ShieldAlert, ShieldQuestion } from "lucide-react";
import { useHostKeyPrompt } from "../state/connectFlow";
import { Dialog } from "./Dialog";

export function HostKeyDialog() {
  const question = useHostKeyPrompt((state) => state.question);
  if (!question) return null;
  const { hostKey, changed, answer } = question;
  const target = hostKey.port === 22 ? hostKey.host : `${hostKey.host}:${hostKey.port}`;

  return (
    <Dialog
      title={changed ? "Host key changed" : "Unknown host"}
      tone={changed ? "danger" : "default"}
      width={520}
      onClose={() => answer(null)}
    >
      <div className="host-key">
        <div className={`host-key-icon ${changed ? "danger" : ""}`}>
          {changed ? <ShieldAlert size={28} /> : <ShieldQuestion size={28} />}
        </div>
        <div>
          {changed ? (
            <p>
              The key presented by <strong>{target}</strong> does not match the one on record. The
              server may have been reinstalled, or someone may be intercepting the connection.
              Continue only if you know why the key changed.
            </p>
          ) : (
            <p>
              <strong>{target}</strong> has not been seen before. Compare the fingerprint with the
              one your server administrator gave you before continuing.
            </p>
          )}
          <dl className="host-key-details">
            <dt>Key type</dt>
            <dd>{hostKey.algorithm}</dd>
            <dt>Fingerprint</dt>
            <dd className="mono selectable">{hostKey.fingerprint}</dd>
          </dl>
        </div>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={() => answer(null)}>
          Cancel
        </button>
        <button
          type="button"
          className="button"
          onClick={() => answer({ fingerprint: hostKey.fingerprint, remember: false })}
        >
          Connect once
        </button>
        <button
          type="button"
          data-autofocus={changed ? undefined : true}
          className={`button ${changed ? "button-danger" : "button-primary"}`}
          onClick={() => answer({ fingerprint: hostKey.fingerprint, remember: true })}
        >
          {changed ? "Replace key and connect" : "Trust and connect"}
        </button>
      </div>
    </Dialog>
  );
}
