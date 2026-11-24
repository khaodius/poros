// Simplified Chinese (zh-Hans), as written in mainland China. One module per area of the
// interface, with the same keys as the English.
//
// | What                   | English       | Simplified Chinese                                  |
// | ---------------------- | ------------- | --------------------------------------------------- |
// | Tag and name in itself | `en`, English | `zh-Hans`, 简体中文                                 |
// | Plural categories      | one, other    | other                                               |
// | `lowercaseMidSentence` | true          | left out (no letter case)                           |
// | Form of address        | you           | 您, and only where it cannot be avoided             |
// | Quotation marks        | "..."         | “...”                                               |
// | "Asks for more" ending | ...           | ... (as Windows menus write it: 移动到...)          |
// | List separator         | `, `          | `，`                                                |
//
// Sentences take full-width punctuation (，。：；？（）“”). Chinese characters have no spaces
// between them; a half-width space separates Chinese from Latin text, numbers and placeholders
// that hold them (已选择 3 项, 连接到 {label}), as Windows and macOS write it, but never sits
// next to full-width punctuation. Labels that are all Latin, such as key names, keep half-width
// parentheses after a space: 刷新 (F5). Buttons and menu items are verb phrases, headings and
// labels noun phrases; "please" (请) only where an instruction reads curtly without it.
//
// | Term              | Simplified Chinese |
// | ----------------- | ------------------ |
// | pane              | 窗格               |
// | tab               | 标签页             |
// | queue             | 队列               |
// | transfer          | 传输               |
// | upload / download | 上传 / 下载        |
// | vault             | 保险库             |
// | session           | 会话               |
// | connection        | 连接               |
// | saved connection  | 已保存的连接       |
// | server            | 服务器             |
// | sync              | 同步               |
// | compare           | 比较               |
// | preview           | 预览               |
// | share link        | 共享链接           |
// | host key          | 主机密钥           |
// | known hosts       | 已知主机           |
// | fingerprint       | 指纹               |
// | key file          | 密钥文件           |
// | passphrase        | 密钥密码           |
// | scheduled task    | 计划任务           |
// | conflict          | 冲突               |
//
// Other recurring words: this computer 这台计算机, local 本地, folder 文件夹, item 项目,
// archive 压缩文件, bucket 存储桶, mount 挂载, unmount 卸载, drive 驱动器, keychain 密钥链,
// jump host 跳板机, endpoint 端点, dry run 试运行, for good 永久, restore 还原, settings 设置.
// Units, protocol and product names, key names, paths and code stay as in English.

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
  locale: "zh-Hans",
  name: "简体中文",
  messages,
});
