// French. One module per area of the interface, with the same keys as the English.
//
// | What                   | English       | French                                               |
// | ---------------------- | ------------- | ---------------------------------------------------- |
// | Tag and name in itself | `en`, English | `fr`, Français                                       |
// | Plural categories      | one, other    | one (0 and 1), many (whole millions), other          |
// | `lowercaseMidSentence` | true          | true                                                 |
// | Form of address        | you           | vous; buttons and menu items in the infinitive       |
// | Quotation marks        | "..."         | « ... », a no-break space inside each mark           |
// | "Asks for more" ending | ...           | ... (no space before it)                             |
// | List separator         | `, `          | `, `                                                 |
//
// Typography: a no-break space (U+00A0) before ":" and inside « », a narrow no-break space
// (U+202F) before ";", "?" and "!", and a no-break space between a number and its unit
// ("30 s", "9 px", "64 KiB"). The apostrophe is ’. Capitals keep their accents (État, À).
// Status texts that run are nouns ("Connexion...", "Enregistrement..."). Counts read as
// sentences that agree with the number ("3 supprimés", "1 élément n’a pas pu être copié"). The
// many form puts "de" before a noun ("{count} de fichiers") but not before an adjective or a
// participle ("{count} supprimés"), nor in the unit entries shown beside a number field
// (schedule.unit*, rsync.unit*). Separators: "\u202f; " between clauses, " " between
// sentences, "{label}\u00a0: {message}" for an error toast.
//
// | Term              | French                                                    |
// | ----------------- | --------------------------------------------------------- |
// | pane              | volet                                                     |
// | tab               | onglet                                                    |
// | queue             | file d’attente (la file in short buttons)                 |
// | transfer          | transfert                                                 |
// | upload / download | envoi, envoyer / téléchargement, télécharger              |
// | vault             | coffre                                                    |
// | session           | session                                                   |
// | connection        | connexion                                                 |
// | saved connection  | connexion enregistrée                                     |
// | server            | serveur                                                   |
// | sync              | synchronisation, synchroniser                             |
// | compare           | comparer, comparaison                                     |
// | preview           | aperçu                                                    |
// | share link        | lien de partage                                           |
// | host key          | clé d’hôte                                                |
// | known hosts       | hôtes connus                                              |
// | fingerprint       | empreinte                                                 |
// | key file          | fichier de clé                                            |
// | passphrase        | phrase secrète                                            |
// | scheduled task    | tâche planifiée                                           |
// | conflict          | conflit                                                   |
// | settings          | paramètres                                                |
// | log               | journal                                                   |
// | bucket            | compartiment                                              |
// | endpoint          | point de terminaison                                      |
// | keychain          | trousseau du système                                      |
// | mount / drive     | monter, démonter / lecteur                                |
// | jump host         | hôte de rebond                                            |
// | dry run           | simulation                                                |
// | speed schedule    | planification des limites de vitesse                      |
// | pause / resume    | suspendre, en pause / reprendre                           |
// | discard           | abandonner                                                |
// | skip              | ignorer                                                   |
// | item              | élément                                                   |

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
  locale: "fr",
  name: "Français",
  lowercaseMidSentence: true,
  messages,
});
