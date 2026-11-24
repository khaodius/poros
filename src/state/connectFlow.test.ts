import { beforeEach, describe, expect, it, vi } from "vitest";
import { approvalOf } from "../lib/hostKey";
import type { AppError, ConnectProfile, HostKeyInfo, SessionInfo } from "../lib/types";
import { connectWithPrompts, useHostKeyPrompt, type HostKeyQuestion } from "./connectFlow";
import { useSessionStore } from "./sessionStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

const profile: ConnectProfile = {
  protocol: "sftp",
  host: "target.example.com",
  port: 22,
  username: "deploy",
  auth: { type: "agent" },
  jumpConnectionId: "bastion",
};

const session: SessionInfo = {
  id: "s1",
  label: "deploy@target.example.com",
  host: "target.example.com",
  port: 22,
  username: "deploy",
  home: "/home/deploy",
  initialPath: "/home/deploy",
  protocol: "sftp",
};

const bastionKey: HostKeyInfo = {
  host: "bastion.example.com",
  port: 2222,
  algorithm: "ssh-ed25519",
  fingerprint: "SHA256:bastion",
};

const targetKey: HostKeyInfo = {
  host: "target.example.com",
  port: 22,
  algorithm: "ssh-ed25519",
  fingerprint: "SHA256:target",
};

function unknown(hostKey: HostKeyInfo): AppError {
  return { kind: "hostKeyUnknown", message: `${hostKey.host} is not known`, hostKey };
}

/** The question on screen once the attempt before it has failed. */
function nextQuestion(): Promise<HostKeyQuestion> {
  return vi.waitFor(() => {
    const { question } = useHostKeyPrompt.getState();
    if (!question) throw new Error("No host key question is on screen");
    return question;
  });
}

const approvalsSent = () =>
  invoke.mock.calls
    .filter(([command]) => command === "connect")
    .map(([, args]) => (args as { hostKeyApproval: unknown }).hostKeyApproval);

beforeEach(() => {
  invoke.mockReset();
  useSessionStore.setState({ sessions: {} });
  useHostKeyPrompt.setState({ question: null });
});

describe("host key questions while connecting", () => {
  it("answers each question for the server it names", async () => {
    invoke
      .mockRejectedValueOnce(unknown(bastionKey))
      .mockRejectedValueOnce(unknown(targetKey))
      .mockResolvedValueOnce(session);
    const connecting = connectWithPrompts(profile);

    const aboutBastion = await nextQuestion();
    expect(aboutBastion.hostKey).toEqual(bastionKey);
    aboutBastion.answer(approvalOf(aboutBastion.hostKey, false));

    const aboutTarget = await nextQuestion();
    expect(aboutTarget.hostKey).toEqual(targetKey);
    aboutTarget.answer(approvalOf(aboutTarget.hostKey, true));

    expect(await connecting).toEqual({ session });
    expect(approvalsSent()).toEqual([
      null,
      { host: "bastion.example.com", port: 2222, fingerprint: "SHA256:bastion", remember: false },
      { host: "target.example.com", port: 22, fingerprint: "SHA256:target", remember: true },
    ]);
  });

  it("stops when a key is not accepted", async () => {
    invoke.mockRejectedValueOnce(unknown(bastionKey));
    const connecting = connectWithPrompts(profile);
    (await nextQuestion()).answer(null);

    expect(await connecting).toEqual({
      error: { kind: "hostKeyUnknown", message: "The host key was not accepted." },
    });
    expect(approvalsSent()).toEqual([null]);
    expect(useHostKeyPrompt.getState().question).toBeNull();
  });
});
