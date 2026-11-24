import { describe, expect, it } from "vitest";
import {
  commandSucceeded,
  describeResult,
  expandCommand,
  needsSelection,
  prepareCommands,
  shellArgument,
  shellQuote,
} from "./commands";

const target = {
  folder: "/srv/site",
  items: [
    { path: "/srv/site/index.html", name: "index.html" },
    { path: "/srv/site/it's here", name: "it's here" },
  ],
};

describe("shellQuote", () => {
  it("makes one word of anything", () => {
    expect(shellQuote("plain")).toBe("'plain'");
    expect(shellQuote("a b; rm -rf /")).toBe("'a b; rm -rf /'");
    expect(shellQuote("it's")).toBe("'it'\\''s'");
    expect(shellQuote("")).toBe("''");
  });
});

describe("shellArgument", () => {
  it("keeps names that start with a dash from reading as options", () => {
    expect(shellArgument("-rf")).toBe("'./-rf'");
    expect(shellArgument("--checkpoint-action=exec=sh x.sh")).toBe(
      "'./--checkpoint-action=exec=sh x.sh'",
    );
    expect(shellArgument("-it's")).toBe("'./-it'\\''s'");
    expect(shellArgument("a-b")).toBe("'a-b'");
    expect(shellArgument("/srv/site/-rf")).toBe("'/srv/site/-rf'");
    expect(shellArgument("")).toBe("''");
  });
});

describe("expandCommand", () => {
  it("runs once per item for {path} and {name}", () => {
    expect(expandCommand("chmod 644 {path}", target)).toEqual([
      "chmod 644 '/srv/site/index.html'",
      "chmod 644 '/srv/site/it'\\''s here'",
    ]);
    expect(expandCommand("gzip -k {name}", target)).toEqual([
      "gzip -k 'index.html'",
      "gzip -k 'it'\\''s here'",
    ]);
  });

  it("runs once with every item for {paths} and {names}", () => {
    expect(expandCommand("du -sh {paths}", target)).toEqual([
      "du -sh '/srv/site/index.html' '/srv/site/it'\\''s here'",
    ]);
    expect(expandCommand("tar -czf backup.tgz {names}", target)).toEqual([
      "tar -czf backup.tgz 'index.html' 'it'\\''s here'",
    ]);
  });

  it("never passes a selected name as an option", () => {
    const dashed = {
      folder: "/srv/site",
      items: [
        { path: "/srv/site/-rf", name: "-rf" },
        {
          path: "/srv/site/--checkpoint-action=exec=sh x.sh",
          name: "--checkpoint-action=exec=sh x.sh",
        },
      ],
    };
    expect(expandCommand("rm {name}", dashed)).toEqual([
      "rm './-rf'",
      "rm './--checkpoint-action=exec=sh x.sh'",
    ]);
    expect(expandCommand("tar -cf backup.tar {names}", dashed)).toEqual([
      "tar -cf backup.tar './-rf' './--checkpoint-action=exec=sh x.sh'",
    ]);
    expect(expandCommand("rm {path}", dashed)).toEqual([
      "rm '/srv/site/-rf'",
      "rm '/srv/site/--checkpoint-action=exec=sh x.sh'",
    ]);
    expect(expandCommand("ls {paths}", dashed)).toEqual([
      "ls '/srv/site/-rf' '/srv/site/--checkpoint-action=exec=sh x.sh'",
    ]);
  });

  it("fills in the folder and leaves other braces to the shell", () => {
    expect(expandCommand("ls {folder} ${HOME} {other}", { ...target, items: [] })).toEqual([
      "ls '/srv/site' ${HOME} {other}",
    ]);
  });

  it("knows which commands need a selection", () => {
    expect(needsSelection("du -sh {paths}")).toBe(true);
    expect(needsSelection("rm {name}")).toBe(true);
    expect(needsSelection("git -C {folder} pull")).toBe(false);
    expect(needsSelection("uptime")).toBe(false);
  });
});

describe("prepareCommands", () => {
  const hostile = {
    folder: "/srv/up'loads",
    items: [
      { path: "/srv/up'loads/x\\';touch pwned;'", name: "x\\';touch pwned;'" },
      { path: "/srv/up'loads/-rf", name: "-rf" },
      { path: "/srv/up'loads/$(reboot)\n`id`", name: "$(reboot)\n`id`" },
    ],
  };

  it("sends names as arguments, never inside the command", () => {
    const runs = prepareCommands("chmod 644 {path} && ls -l {names} {folder}", hostile);
    expect(runs.map((run) => run.command)).toEqual([
      'chmod 644 "${1}" && ls -l "${2}" "${3}" "${4}" "${5}"',
      'chmod 644 "${1}" && ls -l "${2}" "${3}" "${4}" "${5}"',
      'chmod 644 "${1}" && ls -l "${2}" "${3}" "${4}" "${5}"',
    ]);
    expect(runs[0].arguments).toEqual([
      "/srv/up'loads/x\\';touch pwned;'",
      "x\\';touch pwned;'",
      "./-rf",
      "$(reboot)\n`id`",
      "/srv/up'loads",
    ]);
    expect(runs[2].arguments[0]).toBe("/srv/up'loads/$(reboot)\n`id`");
    for (const run of runs) {
      for (const item of hostile.items) expect(run.command).not.toContain(item.name);
    }
    expect(runs[1].display).toBe(
      expandCommand("chmod 644 {path} && ls -l {names} {folder}", hostile)[1],
    );
  });

  it("gives a repeated placeholder the same parameters", () => {
    const [run] = prepareCommands("cp {name} {name}.bak; du {paths} {paths}", {
      folder: "/srv",
      items: [{ path: "/srv/a", name: "a" }],
    });
    expect(run.command).toBe('cp "${1}" "${1}".bak; du "${2}" "${2}"');
    expect(run.arguments).toEqual(["a", "/srv/a"]);
  });

  it("keeps each name one word inside the user's own quotes", () => {
    const target = { folder: "/srv", items: [{ path: "/srv/a b", name: "a b" }] };
    const command = (template: string) => prepareCommands(template, target)[0].command;
    expect(command('echo "file: {name}"')).toBe('echo "file: ${1}"');
    expect(command("echo 'file: {name}'")).toBe("echo 'file: '\"${1}\"''");
    expect(command('echo "$(basename "{path}")"')).toBe('echo "$(basename "${1}")"');
    expect(command('echo "it\\"s {name}"')).toBe('echo "it\\"s ${1}"');
    expect(command("echo \\' {name}")).toBe('echo \\\' "${1}"');
    expect(command('du -sh "{paths}"')).toBe('du -sh "${1}"');
  });

  it("runs a command without placeholders as written, with no arguments", () => {
    expect(prepareCommands("uptime; echo $HOME", { ...target, items: [] })).toEqual([
      { display: "uptime; echo $HOME", command: "uptime; echo $HOME", arguments: [] },
    ]);
  });
});

describe("describeResult", () => {
  const result = { exitStatus: 0, signal: null, stopped: false, elapsedMillis: 1500 };

  it("says how a command ended", () => {
    expect(describeResult(result)).toBe("Finished in 2s");
    expect(commandSucceeded(result)).toBe(true);
    expect(describeResult({ ...result, exitStatus: 2 })).toBe("Exited with status 2 after 2s");
    expect(describeResult({ ...result, exitStatus: null, stopped: true })).toBe("Stopped after 2s");
    expect(describeResult({ ...result, exitStatus: null, signal: "TERM" })).toBe(
      "Ended by signal TERM after 2s",
    );
    expect(describeResult({ ...result, exitStatus: null })).toBe(
      "Ended without an exit status after 2s",
    );
    expect(commandSucceeded({ ...result, exitStatus: null })).toBe(false);
    expect(commandSucceeded({ ...result, signal: "KILL" })).toBe(false);
  });
});
