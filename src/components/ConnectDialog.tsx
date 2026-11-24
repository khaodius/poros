import { useRef, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Import, Plus } from "lucide-react";
import { useLatest } from "../hooks/useLatest";
import { useOptionCheck } from "../hooks/useOptionCheck";
import {
  EMPTY_DRAFT,
  authChoicesFor,
  canConnect,
  connectionDetail,
  draftFromSaved,
  draftSecret,
  profileFromDraft,
  savedFromDraft,
  withField,
  withProtocol,
  type AuthChoice,
  type ConnectDraft,
} from "../lib/connectDraft";
import { toAppError } from "../lib/ipc";
import {
  PROTOCOLS,
  isFtp,
  isWebDav,
  keyLabels,
  protocolInfo,
  signsInWithBrowser,
  usesAccessKeys,
} from "../lib/protocols";
import type { Protocol, S3Addressing } from "../lib/types";
import { connectInTab } from "../state/connectActions";
import { byRecentUse, useSavedConnections } from "../state/savedConnectionsStore";
import { CloudSignIn } from "./CloudSignIn";
import { ConnectionIcon } from "./ConnectionIcon";
import { useSettingsStore } from "../state/settingsStore";
import { useUiStore } from "../state/uiStore";
import { Dialog } from "./Dialog";
import { OptionCheckView, RsyncArgumentsField } from "./RsyncOptionsEditor";
import { useT, t } from "../i18n";

const AUTH_LABELS: Record<AuthChoice, string> = {
  get password() {
    return t("connect.password");
  },
  get publicKey() {
    return t("connect.keyFile");
  },
  get agent() {
    return t("connect.sshAgent");
  },
  get oauth() {
    return t("connect.browserSignIn");
  },
};

interface ConnectDialogProps {
  initialDraft: ConnectDraft;
  targetTabId?: string;
  initialError?: string;
  onClose: () => void;
}

export function ConnectDialog({
  initialDraft,
  targetTabId,
  initialError,
  onClose,
}: ConnectDialogProps) {
  const t = useT();
  const connections = useSavedConnections((state) => state.connections);
  const saveConnection = useSavedConnections((state) => state.save);
  const removeConnection = useSavedConnections((state) => state.remove);
  const openDialog = useUiStore((state) => state.open);
  const latestOnClose = useLatest(onClose);
  const [draft, setDraft] = useState(initialDraft);
  const [busy, setBusy] = useState<"saving" | "connecting" | null>(null);
  const [error, setError] = useState<string | null>(initialError ?? null);
  const [needsPassphrase, setNeedsPassphrase] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const passphraseRef = useRef<HTMLInputElement>(null);
  const proxy = useSettingsStore((state) => state.settings.connection.proxy);
  const proxyEnabled = proxy.kind !== "none" && proxy.host.trim() !== "";
  // Only SSH connections can carry a tunnel.
  const jumpHosts = connections
    .filter((connection) => connection.protocol === "sftp" && connection.id !== draft.savedId)
    .sort((first, second) => first.name.localeCompare(second.name));
  const jumpHostMissing =
    draft.jumpConnectionId !== "" &&
    !jumpHosts.some((connection) => connection.id === draft.jumpConnectionId);

  const importConnections = () =>
    openDialog({ kind: "importConnections", returnTo: { kind: "connect", draft, targetTabId } });

  const update = <K extends keyof ConnectDraft>(key: K, value: ConnectDraft[K]) =>
    setDraft((current) => withField(current, key, value));

  const select = (next: ConnectDraft) => {
    setDraft(next);
    setError(null);
    setNeedsPassphrase(false);
    setConfirmingDelete(false);
  };

  const changeProtocol = (protocol: Protocol) => {
    setDraft((current) => withProtocol(current, protocol));
    setError(null);
    setNeedsPassphrase(false);
  };

  const sftp = draft.protocol === "sftp";
  const s3 = draft.protocol === "s3";
  const b2 = draft.protocol === "backblazeB2";
  const browser = signsInWithBrowser(draft.protocol);
  const ftp = isFtp(draft.protocol);
  const webDav = isWebDav(draft.protocol);
  const keys = keyLabels(draft.protocol);
  const authChoices = authChoicesFor(draft.protocol);
  const ready = canConnect(draft);
  const canSave = browser
    ? ready
    : usesAccessKeys(draft.protocol)
      ? draft.username.trim() !== ""
      : draft.host.trim() !== "";
  const account =
    draft.signedIn?.account ?? (draft.hasSavedSecret && draft.username ? draft.username : null);
  const rememberSecretLabel =
    draft.protocol === "s3"
      ? t("connect.rememberTheSecretAccessKey")
      : b2
        ? t("connect.rememberTheApplicationKey")
        : draft.authChoice === "publicKey"
          ? t("connect.rememberThePassphrase")
          : t("connect.rememberThePassword");
  const namePlaceholder = browser
    ? `${protocolInfo(draft.protocol).label}${account ? ` (${account})` : ""}`
    : b2
      ? "Backblaze B2"
      : draft.host.trim() || (s3 ? "Amazon S3" : t("connect.shownInSavedConnections"));
  const folderPlaceholder = browser
    ? t("connect.topFolderOfTheDrive")
    : s3 || b2
      ? t("connect.bucketOrBucketFolder", { bucket: "/bucket", folder: "/bucket/folder" })
      : ftp
        ? t("connect.loginFolder")
        : webDav
          ? t("connect.theFolderInTheAddressOrTheTop")
          : t("connect.homeFolder");

  const browseForKey = async () => {
    // No extension filter: OpenSSH keys such as `id_ed25519` have none.
    const selected = await openFileDialog({
      title: t("connect.selectPrivateKey"),
      multiple: false,
      directory: false,
    });
    if (typeof selected === "string") update("keyPath", selected);
  };

  /** Stores the draft as a saved connection; returns it with its id. */
  const store = async (): Promise<ConnectDraft> => {
    const secret = draftSecret(draft);
    const grant = draft.signedIn?.grantId ?? null;
    const saved = await saveConnection(savedFromDraft(draft), secret || null, grant);
    const stored = {
      ...draft,
      savedId: saved.id,
      name: saved.name,
      username: saved.username,
      hasSavedSecret: saved.saveSecret && (draft.hasSavedSecret || secret !== "" || grant !== null),
    };
    setDraft(stored);
    return stored;
  };

  const saveOnly = async () => {
    if (!canSave || busy) return;
    setBusy("saving");
    setError(null);
    try {
      await store();
    } catch (caught) {
      setError(toAppError(caught).message);
    }
    setBusy(null);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready || busy) return;
    setBusy("connecting");
    setError(null);
    let connectDraft = draft;
    if (draft.saveConnection) {
      try {
        connectDraft = await store();
      } catch (caught) {
        setError(toAppError(caught).message);
        setBusy(null);
        return;
      }
    }
    const result = await connectInTab(profileFromDraft(connectDraft), targetTabId);
    setBusy(null);
    if (!("error" in result)) {
      latestOnClose.current();
      return;
    }
    setError(result.error.message);
    if (result.error.kind === "passphraseRequired") {
      setNeedsPassphrase(true);
      requestAnimationFrame(() => passphraseRef.current?.select());
    }
  };

  const deleteSelected = async () => {
    if (!draft.savedId) return;
    if (!confirmingDelete) {
      setConfirmingDelete(true);
      return;
    }
    try {
      await removeConnection(draft.savedId);
      select(EMPTY_DRAFT);
    } catch (caught) {
      setError(toAppError(caught).message);
    }
  };

  return (
    <Dialog title={t("common.connect")} onClose={onClose} width={800}>
      <div className="site-manager">
        <aside className="site-list">
          <div className="site-list-header">
            <span>{t("connect.savedConnections")}</span>
            <span className="site-list-actions">
              <button
                type="button"
                className="icon-button"
                title={t("connect.importFromFilezillaWinscpAnd")}
                aria-label={t("connect.importConnections")}
                onClick={importConnections}
              >
                <Import size={15} />
              </button>
              <button
                type="button"
                className="icon-button"
                title={t("connect.newConnection")}
                aria-label={t("connect.newConnection")}
                onClick={() => select(EMPTY_DRAFT)}
              >
                <Plus size={15} />
              </button>
            </span>
          </div>
          {connections.length === 0 ? (
            <p className="site-list-empty">
              {t("connect.connectionsYouSaveAppearHere")}{" "}
              <button type="button" className="link-button" onClick={importConnections}>
                {t("connect.importThemFromAnotherApp")}
              </button>
            </p>
          ) : (
            <ul>
              {byRecentUse(connections).map((connection) => (
                <li key={connection.id}>
                  <button
                    type="button"
                    className={`site-item ${draft.savedId === connection.id ? "is-selected" : ""}`}
                    onClick={() => select(draftFromSaved(connection))}
                    onDoubleClick={() => select(draftFromSaved(connection))}
                  >
                    <ConnectionIcon protocol={connection.protocol} size={14} />
                    <span className="site-item-text">
                      <span className="site-item-name">{connection.name}</span>
                      <span className="site-item-detail">{connectionDetail(connection)}</span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>

        <form className="form site-form" onSubmit={submit}>
          <div className="form-row">
            <label className="field protocol">
              <span>{t("connect.protocol")}</span>
              <select
                value={draft.protocol}
                onChange={(event) => changeProtocol(event.target.value as Protocol)}
              >
                {PROTOCOLS.map((protocol) => (
                  <option key={protocol.value} value={protocol.value}>
                    {protocol.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="field grow">
              <span>{t("common.name")}</span>
              <input
                value={draft.name}
                placeholder={namePlaceholder}
                spellCheck={false}
                onChange={(event) => update("name", event.target.value)}
              />
            </label>
          </div>

          {browser ? (
            <div className="field">
              <span>{t("connect.account")}</span>
              <CloudSignIn
                key={draft.protocol}
                protocol={draft.protocol}
                account={account}
                onSignedIn={(signedIn) =>
                  setDraft((current) => ({ ...current, signedIn, username: signedIn.account }))
                }
                onError={setError}
              />
            </div>
          ) : (
            <>
              {!b2 && (
                <div className="form-row">
                  <label className="field grow">
                    <span>{s3 ? t("connect.endpoint") : t("connect.host")}</span>
                    <input
                      data-autofocus
                      value={draft.host}
                      placeholder={
                        s3
                          ? t("connect.emptyForAmazonS3")
                          : webDav
                            ? t("connect.hostOrAddress", {
                                host: "example.com",
                                address: "example.com/remote.php/dav",
                              })
                            : "example.com"
                      }
                      spellCheck={false}
                      autoCapitalize="off"
                      onChange={(event) => update("host", event.target.value)}
                    />
                  </label>
                  <label className="field port">
                    <span>{t("connect.port")}</span>
                    <input
                      value={draft.port}
                      inputMode="numeric"
                      onChange={(event) => update("port", event.target.value.replace(/\D/g, ""))}
                    />
                  </label>
                </div>
              )}
              {s3 && (
                <small className="field-hint">
                  {t.rich("connect.forAnotherServiceEnterIts", {
                    example: <code>s3.eu-central-1.wasabisys.com</code>,
                    local: <code>http://localhost:9000</code>,
                  })}
                </small>
              )}

              <label className="field">
                <span>{keys ? keys.id : t("connect.username")}</span>
                <input
                  data-autofocus={b2 ? true : undefined}
                  value={draft.username}
                  placeholder={
                    ftp ? t("connect.anonymous") : webDav ? t("connect.emptyIfTheServerNeeds") : ""
                  }
                  spellCheck={false}
                  autoCapitalize="off"
                  onChange={(event) => update("username", event.target.value)}
                />
              </label>
            </>
          )}

          {authChoices.length > 1 && (
            <div className="field">
              <span>{t("connect.authentication")}</span>
              <div className="segmented" role="radiogroup">
                {authChoices.map((choice) => (
                  <button
                    key={choice}
                    type="button"
                    role="radio"
                    aria-checked={draft.authChoice === choice}
                    className={draft.authChoice === choice ? "is-selected" : ""}
                    onClick={() => update("authChoice", choice)}
                  >
                    {AUTH_LABELS[choice]}
                  </button>
                ))}
              </div>
            </div>
          )}

          {draft.authChoice === "password" && (
            <label className="field">
              <span>{keys ? keys.secret : t("connect.password")}</span>
              <input
                type="password"
                value={draft.password}
                placeholder={draft.hasSavedSecret ? t("connect.savedInTheSystemKeychain") : ""}
                onChange={(event) => update("password", event.target.value)}
              />
            </label>
          )}

          {draft.authChoice === "publicKey" && (
            <>
              <div className="field">
                <span>{t("connect.privateKeyFile")}</span>
                <div className="input-with-button">
                  <input
                    value={draft.keyPath}
                    placeholder={t("connect.sshIdEd25519OrC")}
                    spellCheck={false}
                    onChange={(event) => update("keyPath", event.target.value)}
                  />
                  <button type="button" className="button" onClick={browseForKey}>
                    <FolderOpen size={14} /> {t("common.browse")}
                  </button>
                </div>
                <small className="field-hint">{t("connect.opensshPemPkcs8And")}</small>
              </div>
              <label className="field">
                <span>
                  {needsPassphrase
                    ? t("connect.passphrase")
                    : t("connect.passphraseIfTheKeyHasOne")}
                </span>
                <input
                  ref={passphraseRef}
                  type="password"
                  value={draft.passphrase}
                  placeholder={draft.hasSavedSecret ? t("connect.savedInTheSystemKeychain") : ""}
                  className={needsPassphrase ? "has-error" : ""}
                  onChange={(event) => update("passphrase", event.target.value)}
                />
              </label>
            </>
          )}

          {draft.authChoice === "agent" && (
            <p className="field-hint">
              {t.rich("connect.usesKeysLoadedInYour", { variable: <code>SSH_AUTH_SOCK</code> })}
            </p>
          )}

          {s3 && (
            <div className="form-row">
              <label className="field grow">
                <span>{t("connect.region")}</span>
                <input
                  value={draft.s3Region}
                  placeholder={t("connect.fromTheEndpointOrUs")}
                  spellCheck={false}
                  autoCapitalize="off"
                  onChange={(event) => update("s3Region", event.target.value)}
                />
              </label>
              <label className="field grow">
                <span>{t("connect.bucketAddress")}</span>
                <select
                  value={draft.s3Addressing}
                  onChange={(event) => update("s3Addressing", event.target.value as S3Addressing)}
                >
                  <option value="automatic">{t("connect.automatic")}</option>
                  <option value="path">{t("connect.pathEndpointBucket")}</option>
                  <option value="virtualHosted">{t("connect.virtualHostedBucketEndpoint")}</option>
                </select>
              </label>
            </div>
          )}

          <label className="field">
            <span>{t("connect.remoteFolderOptional")}</span>
            <input
              value={draft.initialPath}
              placeholder={folderPlaceholder}
              spellCheck={false}
              onChange={(event) => update("initialPath", event.target.value)}
            />
          </label>

          {sftp && (
            <label className="field">
              <span>{t("connect.jumpHost")}</span>
              <select
                value={draft.jumpConnectionId}
                disabled={jumpHosts.length === 0 && !jumpHostMissing}
                onChange={(event) => update("jumpConnectionId", event.target.value)}
              >
                <option value="">{t("connect.noneConnectStraightToThe")}</option>
                {jumpHostMissing && (
                  <option value={draft.jumpConnectionId}>{t("connect.aDeletedConnection")}</option>
                )}
                {jumpHosts.map((connection) => (
                  <option key={connection.id} value={connection.id}>
                    {connection.name}
                  </option>
                ))}
              </select>
              <small className="field-hint">{t("connect.logsInToThisSaved")}</small>
            </label>
          )}

          {sftp && (
            <SiteRsyncFields
              key={draft.savedId ?? ""}
              rsyncPath={draft.rsyncPath}
              syncArguments={draft.syncArguments}
              onRsyncPath={(value) => update("rsyncPath", value)}
              onSyncArguments={(value) => update("syncArguments", value)}
            />
          )}

          <div className="checks">
            <label className="check">
              <input
                type="checkbox"
                checked={draft.saveConnection}
                onChange={(event) => update("saveConnection", event.target.checked)}
              />
              {browser ? t("connect.saveThisConnectionAndIts") : t("connect.saveThisConnection")}
            </label>
            {!browser && (
              <label className={`check ${draft.authChoice === "agent" ? "is-disabled" : ""}`}>
                <input
                  type="checkbox"
                  checked={draft.authChoice !== "agent" && draft.saveSecret}
                  disabled={draft.authChoice === "agent"}
                  onChange={(event) => update("saveSecret", event.target.checked)}
                />
                {rememberSecretLabel}
              </label>
            )}
            {ftp && (
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.ftpActive}
                  onChange={(event) => update("ftpActive", event.target.checked)}
                />
                {t("connect.activeModeTheServerOpens")}
              </label>
            )}
            {sftp && proxyEnabled && !draft.jumpConnectionId && (
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.bypassProxy}
                  onChange={(event) => update("bypassProxy", event.target.checked)}
                />
                {t("connect.connectWithoutTheProxy")}
              </label>
            )}
          </div>

          {error && <p className="form-error">{error}</p>}

          <div className="dialog-actions">
            {draft.savedId && (
              <button
                type="button"
                className={`button ${confirmingDelete ? "button-danger" : ""} push-left`}
                onClick={() => void deleteSelected()}
              >
                {confirmingDelete ? t("connect.deleteForGood") : t("common.delete")}
              </button>
            )}
            <button type="button" className="button" onClick={onClose}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="button"
              disabled={!canSave || busy !== null}
              onClick={() => void saveOnly()}
            >
              {busy === "saving" ? t("connect.saving") : t("common.save")}
            </button>
            <button
              type="submit"
              className="button button-primary"
              disabled={!ready || busy !== null}
            >
              {busy === "connecting" ? t("connect.connecting") : t("common.connect")}
            </button>
          </div>
        </form>
      </div>
    </Dialog>
  );
}

interface SiteRsyncFieldsProps {
  rsyncPath: string;
  syncArguments: string;
  onRsyncPath: (value: string) => void;
  onSyncArguments: (value: string) => void;
}

/** This server's rsync program, and the rsync arguments its synchronizations start from. */
function SiteRsyncFields({
  rsyncPath,
  syncArguments,
  onRsyncPath,
  onSyncArguments,
}: SiteRsyncFieldsProps) {
  const t = useT();
  const defaults = useSettingsStore((state) => state.settings.sync);
  const transferRsync = useSettingsStore((state) => state.settings.transfers.rsyncPath);
  const [open, setOpen] = useState(() => rsyncPath.trim() !== "" || syncArguments.trim() !== "");
  const check = useOptionCheck(
    defaults.options,
    [defaults.arguments, syncArguments],
    defaults.direction,
  );
  return (
    <details
      className="connect-rsync"
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>{t("connect.rsyncAndSynchronization")}</summary>
      <label className="field">
        <span>{t("connect.rsyncOnTheServer")}</span>
        <input
          className="mono"
          value={rsyncPath}
          placeholder={transferRsync}
          spellCheck={false}
          autoCapitalize="off"
          onChange={(event) => onRsyncPath(event.target.value)}
        />
        <small className="field-hint">{t("connect.theCommandThatStartsRsync")}</small>
      </label>
      <RsyncArgumentsField
        value={syncArguments}
        label={t("connect.rsyncArgumentsForFolderSynchronizations")}
        hint={t("connect.rsyncArgumentsForFolderSynchronizationsHint")}
        placeholder="--delete --chmod=D755,F644 --exclude=.git/"
        onChange={onSyncArguments}
      />
      {syncArguments.trim() !== "" && <OptionCheckView check={check} />}
    </details>
  );
}
