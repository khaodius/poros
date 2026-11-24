// Spanish (Español), neutral international Spanish for Spain and Latin America. One module per
// area of the interface, each with every key of the English catalog.
//
// | What                   | English       | Español                                         |
// | ---------------------- | ------------- | ----------------------------------------------- |
// | Tag and name in itself | `en`, English | `es`, Español                                   |
// | Plural categories      | one, other    | one, many, other ("{count} de archivos")        |
// | `lowercaseMidSentence` | true          | true                                            |
// | Form of address        | you           | tú, in every text (Haz clic, Elige, tu cuenta)  |
// | Quotation marks        | "..."         | "..."                                           |
// | "Asks for more" ending | ...           | ...                                             |
// | List separator         | `, `          | `, `                                            |
// | Clause separator       | `; `          | `; `                                            |
// | Sentence separator     | ` `           | ` `                                             |
// | Label and message      | `: `          | `: `                                            |
//
// | Term              | Español                                                         |
// | ----------------- | --------------------------------------------------------------- |
// | pane              | panel                                                           |
// | tab               | pestaña                                                         |
// | queue             | cola                                                            |
// | transfer          | transferencia                                                   |
// | upload / download | subida / descarga; the verbs subir / descargar                  |
// | vault             | bóveda                                                          |
// | session           | sesión                                                          |
// | connection        | conexión                                                        |
// | saved connection  | conexión guardada                                               |
// | server            | servidor                                                        |
// | sync              | sincronizar, sincronización                                     |
// | compare           | comparar, comparación                                           |
// | preview           | vista previa                                                    |
// | share link        | enlace para compartir; enlace where it is clear                 |
// | host key          | clave de host                                                   |
// | known hosts       | hosts conocidos                                                 |
// | fingerprint       | huella digital                                                  |
// | key file          | archivo de clave                                                |
// | passphrase        | frase de contraseña                                             |
// | scheduled task    | tarea programada                                                |
// | conflict          | conflicto                                                       |
//
// Other choices: file archivo, folder carpeta, item elemento, link (symbolic) vínculo,
// computer equipo, Settings Configuración, Delete Eliminar, Remove Quitar, Add Agregar,
// Rename Cambiar nombre (renombrar inside running text), archive archivo comprimido,
// endpoint punto de conexión (address patterns stay literal: bucket.endpoint), bucket bucket,
// system keychain llavero del sistema, placeholder marcador, dry run simulación, log registro.
// Durations as 5 s, 3 min 20 s, 2 h 5 min. Date groups as Windows Explorer names them: Hoy, Ayer,
// A principios de esta semana, La semana pasada.
// In a plural, the words around the number agree with it ("No se pudo copiar 1 elemento",
// "1 omitido"), and `many` puts "de" only before a noun ("{count} de archivos", but
// "{count} omitidos").

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
  locale: "es",
  name: "Español",
  lowercaseMidSentence: true,
  messages,
});
