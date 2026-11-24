// English, the source of every other catalog. One module per area of the interface.

import { defineTranslation } from "../catalog";
import { appearance } from "./appearance";
import { archives } from "./archives";
import { automation } from "./automation";
import { cloud } from "./cloud";
import { common } from "./common";
import { compare } from "./compare";
import { connect } from "./connect";
import { dialogs } from "./dialogs";
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
import { workspace } from "./workspace";

export const english = {
  appearance,
  archives,
  automation,
  cloud,
  common,
  compare,
  connect,
  dialogs,
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
  workspace,
};

export default defineTranslation({
  locale: "en",
  name: "English",
  lowercaseMidSentence: true,
  messages: english,
});
