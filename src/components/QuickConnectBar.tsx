import { useState, type FormEvent } from "react";
import { Plug } from "lucide-react";
import { EMPTY_DRAFT, profileFromDraft, type ConnectDraft } from "../lib/connectDraft";
import { parseHostInput } from "../lib/path";
import { quickConnect } from "../state/connectActions";
import { useToastStore } from "../state/toastStore";
import { useUiStore } from "../state/uiStore";

export function QuickConnectBar() {
  const [host, setHost] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [port, setPort] = useState("");
  const [connecting, setConnecting] = useState(false);
  const showToast = useToastStore((state) => state.show);
  const openDialog = useUiStore((state) => state.open);

  const currentDraft = (): ConnectDraft => {
    const parsed = parseHostInput(host);
    return {
      ...EMPTY_DRAFT,
      host: parsed.host,
      username: username.trim() || parsed.username || "",
      port: port || (parsed.port ? String(parsed.port) : "22"),
      password,
      initialPath: parsed.path ?? "",
    };
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const draft = currentDraft();
    if (!draft.host || connecting) return;
    if (!draft.username) {
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
        placeholder="22"
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
