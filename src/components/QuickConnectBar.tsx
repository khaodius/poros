import { useState, type FormEvent } from "react";
import { Plug } from "lucide-react";
import {
  EMPTY_DRAFT,
  canConnect,
  profileFromDraft,
  withProtocol,
  type ConnectDraft,
} from "../lib/connectDraft";
import { parseHostInput } from "../lib/path";
import { PROTOCOLS, defaultPort, isCloud } from "../lib/protocols";
import type { Protocol } from "../lib/types";
import { quickConnect } from "../state/connectActions";
import { useToastStore } from "../state/toastStore";
import { useUiStore } from "../state/uiStore";

const SERVER_PROTOCOLS = PROTOCOLS.filter((info) => !isCloud(info.value));
const CLOUD_PROTOCOLS = PROTOCOLS.filter((info) => isCloud(info.value));

export function QuickConnectBar() {
  const [protocol, setProtocol] = useState<Protocol>("sftp");
  const [host, setHost] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [port, setPort] = useState("");
  const [connecting, setConnecting] = useState(false);
  const showToast = useToastStore((state) => state.show);
  const openDialog = useUiStore((state) => state.open);

  const chooseProtocol = (chosen: Protocol) => {
    // Cloud storage signs in through the browser, which the connect dialog walks through.
    if (isCloud(chosen)) {
      openDialog({ kind: "connect", draft: withProtocol(EMPTY_DRAFT, chosen) });
      return;
    }
    setProtocol(chosen);
  };

  const currentDraft = (): ConnectDraft => {
    const parsed = parseHostInput(host);
    const chosen = parsed.protocol ?? protocol;
    return {
      ...withProtocol(EMPTY_DRAFT, chosen),
      host: parsed.host,
      username: username.trim() || parsed.username || "",
      port: port || String(parsed.port ?? defaultPort(chosen)),
      password,
      initialPath: parsed.path ?? "",
    };
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const draft = currentDraft();
    if (!draft.host || connecting) return;
    if (!canConnect(draft)) {
      openDialog({ kind: "connect", draft });
      return;
    }
    setConnecting(true);
    const failure = await quickConnect(profileFromDraft(draft));
    setConnecting(false);
    if (failure) showToast("error", failure.message);
    else setPassword("");
  };

  return (
    <form className="quick-connect" onSubmit={submit}>
      <select
        className="quick-protocol"
        value={protocol}
        aria-label="Protocol"
        onChange={(event) => chooseProtocol(event.target.value as Protocol)}
      >
        {SERVER_PROTOCOLS.map((info) => (
          <option key={info.value} value={info.value}>
            {info.shortLabel}
          </option>
        ))}
        <optgroup label="Cloud storage">
          {CLOUD_PROTOCOLS.map((info) => (
            <option key={info.value} value={info.value}>
              {info.shortLabel}...
            </option>
          ))}
        </optgroup>
      </select>
      <input
        className="quick-host"
        value={host}
        placeholder="Host or user@host:port"
        aria-label="Host"
        spellCheck={false}
        autoCapitalize="off"
        onChange={(event) => setHost(event.target.value)}
      />
      <input
        value={username}
        placeholder="Username"
        aria-label="Username"
        spellCheck={false}
        autoCapitalize="off"
        onChange={(event) => setUsername(event.target.value)}
      />
      <input
        type="password"
        value={password}
        placeholder="Password"
        aria-label="Password"
        onChange={(event) => setPassword(event.target.value)}
      />
      <input
        className="quick-port"
        value={port}
        placeholder={String(defaultPort(protocol))}
        aria-label="Port"
        inputMode="numeric"
        onChange={(event) => setPort(event.target.value.replace(/\D/g, ""))}
      />
      <button type="submit" className="button button-primary" disabled={!host.trim() || connecting}>
        <Plug size={14} />
        {connecting ? "Connecting..." : "Connect"}
      </button>
    </form>
  );
}
