import { useState, type FormEvent } from "react";
import { Plug, SlidersHorizontal } from "lucide-react";
import { parseHostInput } from "../lib/path";
import { connectWithPrompts } from "../state/connectFlow";
import { useConnectionStore } from "../state/connectionStore";
import { useToastStore } from "../state/toastStore";
import { EMPTY_DRAFT, profileFromDraft, type ConnectDraft } from "../lib/connectDraft";

interface QuickConnectBarProps {
  onOpenDialog: (draft: ConnectDraft) => void;
}

export function QuickConnectBar({ onOpenDialog }: QuickConnectBarProps) {
  const [host, setHost] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [port, setPort] = useState("");
  const status = useConnectionStore((state) => state.status);
  const showToast = useToastStore((state) => state.show);
  const connecting = status === "connecting";

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
      onOpenDialog(draft);
      return;
    }
    const failure = await connectWithPrompts(profileFromDraft(draft));
    if (failure) showToast("error", failure.message);
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
      <button
        type="button"
        className="icon-button"
        title="More connection options (key file, SSH agent)"
        aria-label="More connection options"
        onClick={() => onOpenDialog(currentDraft())}
      >
        <SlidersHorizontal size={15} />
      </button>
    </form>
  );
}
