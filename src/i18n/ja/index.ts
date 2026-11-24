// Japanese. One module per area of the interface, as in English.
//
// | What                   | English       | Japanese                                               |
// | ---------------------- | ------------- | ------------------------------------------------------ |
// | Tag and name in itself | `en`, English | `ja`, 日本語                                           |
// | Plural categories      | one, other    | other                                                  |
// | `lowercaseMidSentence` | true          | left out (no case)                                     |
// | Form of address        | you           | です/ます in sentences, noun phrases for buttons and   |
// |                        |               | menu items; the user is not named (no あなた)          |
// | Quotation marks        | "..."         | 「...」                                                |
// | "Asks for more" ending | ...           | ... (as Windows menus write it: 名前を付けて保存...)   |
// | List separator         | `, `          | `、`                                                   |
//
// Punctuation: 。 and 、 in sentences; ?, :, ( ) half-width as Windows writes them. A half-width
// space between Japanese and Latin words, numbers or placeholders ("{count} 個のファイル",
// "SFTP サーバー"), none next to 。、「」. Katakana words ending in -er, -or, -ar take the long
// vowel mark as Windows writes them (フォルダー, サーバー, ユーザー, エディター, ブラウザー), words
// ending in -y do not (プロパティ, ディレクトリ, ライブラリ).
//
// | Term              | Japanese                 |
// | ----------------- | ------------------------ |
// | pane              | ペイン                   |
// | tab               | タブ                     |
// | queue             | キュー                   |
// | transfer          | 転送                     |
// | upload / download | アップロード / ダウンロード |
// | vault             | 金庫 (as Cryptomator)    |
// | session           | セッション               |
// | connection        | 接続                     |
// | saved connection  | 保存済みの接続           |
// | server            | サーバー                 |
// | sync              | 同期 (フォルダーの同期)  |
// | compare           | 比較                     |
// | preview           | プレビュー               |
// | share link        | 共有リンク               |
// | host key          | ホスト鍵                 |
// | known hosts       | 既知のホスト             |
// | fingerprint       | フィンガープリント       |
// | key file          | 鍵ファイル               |
// | passphrase        | パスフレーズ             |
// | scheduled task    | スケジュールタスク       |
// | conflict          | 競合                     |
//
// File operations as Windows Explorer names them: コピー, 切り取り, 貼り付け, 削除, 名前の変更,
// 新しいフォルダー, プロパティ, ごみ箱. Also: local ローカル, remote リモート, mount マウント,
// unmount マウント解除, sign in サインイン, private key 秘密鍵, keychain キーチェーン, permissions
// パーミッション, owner 所有者, default 既定, retry 再試行, resume 再開, pause 一時停止.

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
  locale: "ja",
  name: "日本語",
  messages,
});
