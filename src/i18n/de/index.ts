// German. The choices every text follows:
//
// | What                   | English       | Deutsch                                  |
// | ---------------------- | ------------- | ---------------------------------------- |
// | Tag and name in itself | `en`, English | `de`, Deutsch                            |
// | Plural categories      | one, other    | one, other (one is 1 alone)              |
// | `lowercaseMidSentence` | true          | left out: nouns keep their capitals      |
// | Form of address        | you           | Sie, Ihr; imperatives with "Sie"         |
// | Quotation marks        | "..."         | „...“                                    |
// | "Asks for more" ending | ...           | ...                                      |
// | List separator         | `, `          | `, `                                     |
//
// | Term              | Deutsch                                                          |
// | ----------------- | ---------------------------------------------------------------- |
// | pane              | Bereich                                                          |
// | tab               | Tab (der Tab, die Tabs)                                          |
// | queue             | Warteschlange                                                    |
// | transfer          | Übertragung; übertragen                                          |
// | upload / download | Upload / Download as nouns; hochladen / herunterladen as verbs   |
// | vault             | Tresor (as in Cryptomator's own German)                          |
// | session           | Sitzung                                                          |
// | connection        | Verbindung                                                       |
// | saved connection  | gespeicherte Verbindung                                          |
// | server            | Server                                                           |
// | sync              | Synchronisierung; synchronisieren                                |
// | compare           | Vergleich; vergleichen                                           |
// | preview           | Vorschau                                                         |
// | share link        | Freigabelink; freigeben                                          |
// | host key          | Hostschlüssel                                                    |
// | known hosts       | bekannte Hosts                                                   |
// | fingerprint       | Fingerabdruck                                                    |
// | key file          | Schlüsseldatei                                                   |
// | passphrase        | Passphrase                                                       |
// | scheduled task    | geplante Aufgabe                                                 |
// | conflict          | Konflikt                                                         |
//
// Also kept the same everywhere: item Element, folder Ordner, file Datei, log Protokoll,
// settings Einstellungen, password Passwort, system keychain Schlüsselbund des Systems,
// link (symbolic) Verknüpfung, mount / unmount einbinden / trennen, extract entpacken,
// pause / resume anhalten / fortsetzen, retry wiederholen, home folder Benutzerordner,
// the login folder der Ordner nach der Anmeldung, cursor Cursor, dry run Probelauf.
// Progress phrases take the passive ("Wird gespeichert..."), hints that start with "Off:"
// read "Aus:" followed by a whole sentence. Key names stay as the menus show them (Ctrl,
// Shift, Alt, Enter), as do units (KB, MiB, KiB/s); durations are written "2 min 5 s".

import { defineTranslation } from "../catalog";
import { appearance } from "./appearance";
import { archives } from "./archives";
import { automation } from "./automation";
import { cloud } from "./cloud";
import { common } from "./common";
import { compare } from "./compare";
import { connect } from "./connect";
import { dialogs } from "./dialogs";
import { drives } from "./drives";
import { editor } from "./editor";
import { fileList } from "./fileList";
import { limits } from "./limits";
import { log } from "./log";
import { openWith } from "./openWith";
import { pane } from "./pane";
import { preview } from "./preview";
import { rename } from "./rename";
import { rsync } from "./rsync";
import { schedule } from "./schedule";
import { search } from "./search";
import { settings } from "./settings";
import { share } from "./share";
import { sync } from "./sync";
import { terminal } from "./terminal";
import { transfers } from "./transfers";
import { updates } from "./updates";
import { vault } from "./vault";
import { versions } from "./versions";
import { workspace } from "./workspace";

const messages = {
  appearance,
  archives,
  automation,
  cloud,
  common,
  compare,
  connect,
  dialogs,
  drives,
  editor,
  fileList,
  limits,
  log,
  openWith,
  pane,
  preview,
  rename,
  rsync,
  schedule,
  search,
  settings,
  share,
  sync,
  terminal,
  transfers,
  updates,
  vault,
  versions,
  workspace,
};

export default defineTranslation({
  locale: "de",
  name: "Deutsch",
  messages,
});
