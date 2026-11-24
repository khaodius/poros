# Translating Poros

Every word the interface shows comes from a catalog in this folder. English, in `en/`, is the
source: it fixes the keys, which messages are plurals and which placeholders each message
takes. A translation is a folder beside it with the same modules and the same keys, and one line
in `locales.ts` that ships it.

## What a translator touches

Only these, so that several languages can be translated side by side without touching each
other's files:

1. A new folder `src/i18n/<locale>/`.
2. One line of `src/i18n/locales.ts`, uncommented:

| Language           | Folder     | The line in `locales.ts`                         |
| ------------------ | ---------- | ------------------------------------------------ |
| German             | `de/`      | `export { default as de } from "./de";`          |
| Spanish            | `es/`      | `export { default as es } from "./es";`          |
| French             | `fr/`      | `export { default as fr } from "./fr";`          |
| Japanese           | `ja/`      | `export { default as ja } from "./ja";`          |
| Simplified Chinese | `zh-Hans/` | `export { default as zhHans } from "./zh-Hans";` |

Do not change the English, the components or any other file. Where an English text is wrong,
unclear or built in a way your language cannot follow, say so when you hand the translation in.

## Starting a language

Copy the English folder and turn its modules into translations (French as the example):

```sh
cp -r src/i18n/en src/i18n/fr
cd src/i18n/fr
for file in *.ts; do
  [ "$file" = index.ts ] && continue
  area="${file%.ts}"
  sed -i -e 's|^import type { MessageArea } from "../catalog";|import type { TranslatedArea } from "../catalog";|' \
         -e "s|^} as const satisfies MessageArea;|} satisfies TranslatedArea<\"$area\">;|" "$file"
done
```

Then, in `fr/index.ts`:

- the first comment names your language;
- `export const english = {` becomes `const messages = {`;
- the default export gets your tag, the language's name in itself, `lowercaseMidSentence` (see
  below) and `messages,`:

```ts
export default defineTranslation({
  locale: "fr",
  name: "Français",
  lowercaseMidSentence: true,
  messages,
});
```

The names the Language list shows are `Deutsch`, `Español`, `Français`, `日本語` and `简体中文`.

Before translating, fill the table under [Your language first](#your-language-first) and put it
as a comment at the top of your `index.ts`, so that every text follows the same choices.

## How the keys are grouped

One module per area of the interface. Inside a module, keys are grouped under a comment naming
the file that uses them first, such as `// components/ConnectDialog`; open that file when a text
needs its context. A key's name was made from its English text and stays when the English
changes, so read the text, not the name.

| Area         | What it holds                                                                  |
| ------------ | ------------------------------------------------------------------------------ |
| `appearance` | Settings > Appearance: themes, colors, fonts, progress bar looks               |
| `archives`   | Packing and unpacking archives, Settings > Archives                            |
| `automation` | What happens when the queue finishes, server commands, Settings > Automation   |
| `cloud`      | Cloud accounts, browser sign-in and the browser tab it ends on                 |
| `common`     | Words used everywhere: Cancel, Close, Save, Upload, durations, the list comma  |
| `compare`    | Comparing two files, comparing two folders                                     |
| `connect`    | The connect dialog, quick connect, host keys, importing connections, protocols |
| `dialogs`    | Delete, properties, permissions, name clashes                                  |
| `editor`     | The built-in editor and its unsaved changes                                    |
| `fileList`   | A pane's columns, date groups and empty folders                                |
| `limits`     | Speed limits and their schedule, in Settings > Transfers and the status bar    |
| `log`        | The log panel                                                                  |
| `openWith`   | Opening files in other programs and uploading their saves                      |
| `pane`       | A pane's toolbar, path bar, footer, context menu and file operations           |
| `preview`    | The preview panel and its settings                                             |
| `rename`     | Renaming many items at once                                                    |
| `rsync`      | rsync options                                                                  |
| `schedule`   | Scheduled tasks                                                                |
| `search`     | Searching below a folder                                                       |
| `settings`   | The settings window and the pages without an area of their own                 |
| `share`      | Share links                                                                    |
| `sync`       | Synchronizing folders                                                          |
| `terminal`   | The SSH terminal                                                               |
| `transfers`  | The rows of the transfer queue and their states                                |
| `updates`    | Checking for, downloading and installing updates                               |
| `vault`      | Cryptomator vaults                                                             |
| `versions`   | Object versions and deleted files on S3 and B2                                 |
| `workspace`  | The window: tabs, the transfer panel, the status bar, the splash screen        |

There is no area for errors: the texts the interface gives for an error live with the screen
that shows them, and errors written by the backend stay in English.

## The voice of the English

- Plain and direct, in short sentences. No exclamation marks, no jokes, no "please".
- Sentence case: only the first word and names take a capital, in titles and buttons too.
- Buttons and menu items name what they do: "Connect", "Keep both", "Pause queue".
- A menu item or button that asks for more before it acts ends in "...": "Move to...".
- A hint under a switch that starts with "Off:" says what happens while the switch is off.
- The user is "you", and only where it cannot be avoided.

Write your language the way its own operating systems write their interfaces, in the form of
address they use, and keep it the same everywhere.

## Placeholders, plurals and the rest of the format

**Placeholders.** `{name}` stands for something filled in when the text is shown: a file name, a
size, a number. Keep every placeholder of the English, spelled exactly the same, and move it
wherever your sentence needs it. Never translate what is inside the braces. A message never has
a brace that is not a placeholder; where the interface must show a brace, such as the `{date}`
token of bulk rename, it comes in through a placeholder.

**Numbers, sizes and dates** come into placeholders already written the way your language writes
them (`1.234,5` in German), and units stay as they are (KB, MB, GB, KiB, MiB, KiB/s). Durations
are messages in `common` (`durationSeconds`, `durationMinutes`, `durationHours`): write their
units your language's way.

**Plurals.** A message that follows a number is an object with one text per plural category, and
the number is `{count}`. English has `one` and `other`. Yours has exactly the categories
`Intl.PluralRules` gives your language, no more and no fewer:

| Language | Categories             | Notes                                                                                                                |
| -------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------- |
| de       | `one`, `other`         | `one` is 1 alone.                                                                                                    |
| es       | `one`, `many`, `other` | `one` is 1 alone. `many` is whole millions: "{count} de archivos".                                                   |
| fr       | `one`, `many`, `other` | `one` is 0 and 1, so keep `{count}` in it where the count can be 0. `many` is whole millions: "{count} de fichiers". |
| ja       | `other`                | One form for every number.                                                                                           |
| zh-Hans  | `other`                | One form for every number.                                                                                           |

The `other` form must show `{count}` when the English one does. Another form may leave the number
out, as English "Delete this file?" does for one.

**Rich messages.** Some messages are shown with parts in bold or as code, such as
`pane.isAlreadyIn` ("{name} is already in {folder}."), where the name is bold. The interface
does that through the placeholders, so they look like any other message: keep the placeholders.

**Line breaks.** `\n` in a message, as in `transfers.rowTitle`, is a line break in a tooltip.
Keep it where it is or where your sentence breaks best.

**Labels inside sentences.** Some labels are also used inside a sentence or a list, such as a
search described as "files only, at least 1 MB" from the labels "Files only" and "At least 1 MB".
`lowercaseMidSentence: true` writes them in lower case there, as English, French and Spanish
do. Leave it out for German, whose nouns keep their capitals, and for Japanese and Chinese,
which have no case.

**Lists.** `common.listSeparator` joins the parts of a status line ("10 items, 2 hidden"). Use
your language's own, such as `、` in Japanese and `，` in Chinese.

## What stays in English

- Protocol and product names: SFTP, FTP, FTPS, WebDAV, S3, rsync, SSH, Cryptomator, Poros,
  Google Drive, OneDrive, Box, Backblaze B2, FileZilla, WinSCP, PuTTY, MobaXterm, OpenSSH.
- Units: KB, MB, GB, KiB, MiB, KiB/s.
- Key names in shortcuts, as the menus show them: Ctrl, Shift, Alt, Enter, Backspace, Del, Home,
  Left, Right, F2 to F7, Ctrl+L.
- File names, paths, command-line options, environment variables, addresses and code, such as
  `~/.ssh/config`, `--size-only`, `name.sha256` and `apps.googleusercontent.com`.
- What the interface does not write: messages from servers and from the backend, the log, the
  command-line tool `poros-cli`, file names and anything a server sends.

## Room on screen

German and French run about a third longer than English, and the layout gives way: column
headings and toolbar labels shorten with "..." and show their full text on hover, dialogs and
settings wrap. Shorter words still read better where room is tight: the headings of the
transfer queue (Size, Speed), the labels of the transfer panel (Workers, When done), tab names
and buttons. Do not add line breaks to make a text fit.

## Checking a translation

From the repository:

```sh
npx vitest run src/i18n                     # every shipped language, the runtime and the guard
npx vitest run src/i18n -t "fr fits"        # one language
npm run typecheck                           # a missing key, or a plural where English has none
npm run dev                                 # then Settings > Interface > Language
```

The catalog test lists every way a language does not fit the English, one line each:

```
pane.isAlreadyIn: placeholders {name}; English has {folder}, {name}
pane.items: plural forms one, other; fr uses one, many, other
pane.items: the "other" form must show {count}
settings.language: missing
settings.langauge: a key English does not have
transfers.speed: empty
rename.token: a brace that is not a placeholder
```

It checks the shape of a translation, not whether a text is translated: a text left in English
passes. The interface falls back to the English text for a key a catalog lacks, and says so once
in the browser console while developing.

`src/i18n/hardcoded.test.ts` fails on interface text written into a component rather than into
the English catalog; translators do not need it, but it runs with the rest.

## Your language first

Fill this in before translating, and keep it at the top of your `index.ts` as a comment.

| What                   | English       | Your language |
| ---------------------- | ------------- | ------------- |
| Tag and name in itself | `en`, English |               |
| Plural categories      | one, other    |               |
| `lowercaseMidSentence` | true          |               |
| Form of address        | you           |               |
| Quotation marks        | "..."         |               |
| "Asks for more" ending | ...           |               |
| List separator         | `, `          |               |

| Term              | What it is in Poros                                                      | Your language |
| ----------------- | ------------------------------------------------------------------------ | ------------- |
| pane              | One of the side-by-side file lists, local or on a server                 |               |
| tab               | One folder view in a pane; a pane holds several                          |               |
| queue             | The list of transfers waiting or running, in the transfer panel          |               |
| transfer          | Copying one file or folder up, down or between servers                   |               |
| upload / download | A transfer to a server / from a server                                   |               |
| vault             | A Cryptomator folder whose files are encrypted, unlocked with a password |               |
| session           | One open connection to a server, as a tab uses it                        |               |
| connection        | The way to reach a server: protocol, host, user; also a saved one        |               |
| saved connection  | A connection kept in the list of the connect dialog                      |               |
| server            | The computer or storage service on the other side                        |               |
| sync              | Making one folder match another (Synchronize folders)                    |               |
| compare           | Showing how two files or two folders differ                              |               |
| preview           | The panel that shows a file's picture or text without opening it         |               |
| share link        | An address that lets others download a cloud file                        |               |
| host key          | The key a server proves itself with on connecting                        |               |
| known hosts       | The list of host keys already trusted                                    |               |
| fingerprint       | The short form of a host key the user compares                           |               |
| key file          | A private key file used to sign in instead of a password                 |               |
| passphrase        | The password that protects a key file                                    |               |
| scheduled task    | A sync, transfer or command that runs at set times                       |               |
| conflict          | A file that already exists where a transfer would put it                 |               |
