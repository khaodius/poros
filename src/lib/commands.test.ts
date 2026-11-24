import { describe, expect, it } from "vitest";
import {
  commandSucceeded,
  describeResult,
  needsSelection,
  prepareCommands,
  readsUncertainly,
  shellQuote,
} from "./commands";

const target = {
  folder: "/srv/site",
  items: [
    { path: "/srv/site/index.html", name: "index.html" },
    { path: "/srv/site/it's here", name: "it's here" },
  ],
};

/** The command that runs for one item named `a b`. */
function commandFor(template: string): string {
  return prepareCommands(template, {
    folder: "/srv",
    items: [{ path: "/srv/a b", name: "a b" }],
  })[0].command;
}

describe("shellQuote", () => {
  it("makes one word of anything", () => {
    expect(shellQuote("plain")).toBe("'plain'");
    expect(shellQuote("a b; rm -rf /")).toBe("'a b; rm -rf /'");
    expect(shellQuote("it's")).toBe("'it'\\''s'");
    expect(shellQuote("")).toBe("''");
  });
});

describe("needsSelection", () => {
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

  it("runs once per item for {path} and {name}", () => {
    expect(prepareCommands("chmod 644 {path}", target)).toEqual([
      {
        display: "chmod 644 \"${1}\" with $1 = '/srv/site/index.html'",
        command: 'chmod 644 "${1}"',
        arguments: ["/srv/site/index.html"],
      },
      {
        display: "chmod 644 \"${1}\" with $1 = '/srv/site/it'\\''s here'",
        command: 'chmod 644 "${1}"',
        arguments: ["/srv/site/it's here"],
      },
    ]);
    expect(prepareCommands("gzip -k {name}", target).map((run) => run.arguments)).toEqual([
      ["index.html"],
      ["it's here"],
    ]);
  });

  it("runs once with every item for {paths} and {names}", () => {
    expect(prepareCommands("du -sh {paths}", target)).toEqual([
      {
        display:
          "du -sh \"${1}\" \"${2}\" with $1 = '/srv/site/index.html', $2 = '/srv/site/it'\\''s here'",
        command: 'du -sh "${1}" "${2}"',
        arguments: ["/srv/site/index.html", "/srv/site/it's here"],
      },
    ]);
    const [run] = prepareCommands("tar -czf backup.tgz {names}", target);
    expect(run.arguments).toEqual(["index.html", "it's here"]);
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
    expect(prepareCommands("rm {name}", dashed).map((run) => run.arguments)).toEqual([
      ["./-rf"],
      ["./--checkpoint-action=exec=sh x.sh"],
    ]);
    expect(prepareCommands("ls {paths}", dashed)[0].arguments).toEqual([
      "/srv/site/-rf",
      "/srv/site/--checkpoint-action=exec=sh x.sh",
    ]);
  });

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
    expect(commandFor('echo "file: {name}"')).toBe('echo "file: ${1}"');
    expect(commandFor("echo 'file: {name}'")).toBe("echo 'file: '\"${1}\"''");
    expect(commandFor('echo "$(basename "{path}")"')).toBe('echo "$(basename "${1}")"');
    expect(commandFor('echo "it\\"s {name}"')).toBe('echo "it\\"s ${1}"');
    expect(commandFor("echo \\' {name}")).toBe('echo \\\' "${1}"');
    expect(commandFor('du -sh "{paths}"')).toBe('du -sh "${1}"');
  });

  it("keeps a name one word in backticks, inside double quotes or not", () => {
    expect(commandFor('echo "`md5sum {name}`"')).toBe('echo "`md5sum "${1}"`"');
    expect(commandFor('tar czf "/backup/`basename {name}`.tgz" {name}')).toBe(
      'tar czf "/backup/`basename "${1}"`.tgz" "${1}"',
    );
    expect(commandFor("echo `ls {name}` {name}")).toBe('echo `ls "${1}"` "${1}"');
    expect(commandFor('echo "`date` {name}"')).toBe('echo "`date` ${1}"');
    expect(commandFor("echo \"`echo '{name}'`\"")).toBe("echo \"`echo ''\"${1}\"''`\"");
  });

  it("follows parentheses and case patterns inside $(...)", () => {
    expect(commandFor('echo "$(cd /tmp && (ls) | grep {name})"')).toBe(
      'echo "$(cd /tmp && (ls) | grep "${1}")"',
    );
    expect(commandFor('echo "$(case x in a) echo {name};; esac) {name}"')).toBe(
      'echo "$(case x in a) echo "${1}";; esac) ${1}"',
    );
    expect(commandFor('echo "$(case x in (a) echo {name};; esac) {name}"')).toBe(
      'echo "$(case x in (a) echo "${1}";; esac) ${1}"',
    );
    expect(commandFor('echo "$((1 + 2)) {name}"')).toBe('echo "$((1 + 2)) ${1}"');
    expect(commandFor('echo "$(echo ")") {name}"')).toBe('echo "$(echo ")") ${1}"');
    // A word that only holds "case" is no case statement.
    expect(commandFor('echo "$(echo suitcase) {name}"')).toBe('echo "$(echo suitcase) ${1}"');
  });

  it("counts case as a statement only when a word and in follow", () => {
    expect(commandFor('echo "$(echo case)" "{name}"')).toBe('echo "$(echo case)" "${1}"');
    expect(commandFor('echo "$(case "$x" in a) echo {name};; esac) {name}"')).toBe(
      'echo "$(case "$x" in a) echo "${1}";; esac) ${1}"',
    );
    expect(commandFor('echo "$(case $(uname) in Linux) echo {name};; esac) {name}"')).toBe(
      'echo "$(case $(uname) in Linux) echo "${1}";; esac) ${1}"',
    );
  });

  it("quotes every placeholder as outside quotes when the command cannot be read for sure", () => {
    expect(commandFor("echo $'it\\'s' {path} '\"' {name}")).toBe(
      "echo $'it\\'s' \"${1}\" '\"' \"${2}\"",
    );
    expect(commandFor('cat <<EOF\n5" disk\nEOF\nrm {name}')).toBe(
      'cat <<EOF\n5" disk\nEOF\nrm "${1}"',
    );
    expect(commandFor('echo "unclosed {name}')).toBe('echo "unclosed "${1}"');
  });

  it("knows which commands it cannot read for sure", () => {
    for (const template of [
      "echo $'x' {name}",
      "cat <<EOF\n{name}\nEOF",
      "echo $((1 << 2)) {name}",
      'echo "{name}',
      "echo '{name}",
      "(ls {name}",
      "echo `ls {name}",
    ]) {
      expect(readsUncertainly(template), template).toBe(true);
    }
    for (const template of [
      "du -sh {paths}",
      'echo "$(echo case)" "{name}"',
      'echo "it\'s {name}"',
      "cat <<EOF",
      "echo $'x'",
    ]) {
      expect(readsUncertainly(template), template).toBe(false);
    }
  });

  it("applies a backslash before a placeholder to the name's first character", () => {
    expect(commandFor("echo \\{name}")).toBe('echo "${1}"');
    expect(commandFor('echo "\\{name}"')).toBe('echo "\\\\${1}"');
    expect(commandFor("echo '\\{name}'")).toBe("echo '\\'\"${1}\"''");
    expect(commandFor("echo \\\\{name}")).toBe('echo \\\\"${1}"');
  });

  it("reads past comments", () => {
    expect(commandFor("ls {name} # it's here\necho {name}")).toBe(
      'ls "${1}" # it\'s here\necho "${1}"',
    );
    expect(commandFor('echo "$(ls # (it\'s)\n) {name}"')).toBe('echo "$(ls # (it\'s)\n) ${1}"');
    // Inside a word, # starts no comment.
    expect(commandFor("echo a#b'{name}'")).toBe("echo a#b''\"${1}\"''");
  });

  it("runs a command with only {folder} in the folder, as written for the login shell", () => {
    expect(prepareCommands("git -C {folder} pull", { folder: "/srv/up'loads", items: [] })).toEqual(
      [{ display: 'git -C "$PWD" pull', command: 'git -C "$PWD" pull', arguments: [] }],
    );
    expect(commandFor('echo "in {folder}"')).toBe('echo "in $PWD"');
    expect(commandFor("echo 'in {folder}'")).toBe("echo 'in '\"$PWD\"''");
    // With selected items, the folder is an argument like them.
    const [run] = prepareCommands("cp {names} {folder}/old", target);
    expect(run.command).toBe('cp "${1}" "${2}" "${3}"/old');
    expect(run.arguments).toEqual(["index.html", "it's here", "/srv/site"]);
  });

  it("runs a command without placeholders as written, with no arguments", () => {
    expect(prepareCommands("uptime; echo $HOME", { ...target, items: [] })).toEqual([
      { display: "uptime; echo $HOME", command: "uptime; echo $HOME", arguments: [] },
    ]);
    expect(commandFor("ls ${HOME} {other}")).toBe("ls ${HOME} {other}");
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
