import type { TranslatedArea } from "../catalog";

export const vault = {
  // components/LockVaultDialog
  lock: "{name} をロックしますか?",
  pauseTransfersAndLock: "転送を一時停止してロック",
  unmountAndLock: "マウントを解除してロック",
  transfersForThisVaultStillQueued: {
    other:
      "この金庫の転送が {count} 件、まだキューにあります。ロックされた金庫にはファイルをコピーできず、金庫からコピーすることもできないため、金庫のロックをもう一度解除して再開するまで一時停止したまま待機します。",
  },

  // components/NewVaultDialog
  newVault: "新しい金庫",
  aNewFolderInHolds: "{folder} 内の新しいフォルダーに、暗号化されたファイルが保存されます。",
  password: "パスワード",
  atLeastCharacters: { other: "{count} 文字以上にしてください。" },
  repeatPassword: "パスワードの確認",
  thePasswordsDoNotMatch: "パスワードが一致しません。",
  thisPasswordCannotBeRecovered:
    "このパスワードは復元できません。パスワードがなければ、誰も金庫のファイルを読めません。",
  theVaultAlsoOpensIn: "この金庫は Cryptomator アプリでも開けます。",
  creating: "作成しています...",
  createVault: "金庫を作成",

  // components/VaultUnlockCard
  wrongPassword: "パスワードが違います",
  encryptedVault: "暗号化された金庫",
  isACryptomatorVaultEnter:
    "{name} は Cryptomator 金庫です。ここで開くには、パスワードを入力してください。",
  vaultPassword: "金庫のパスワード",
  showEncryptedFiles: "暗号化されたファイルを表示",
  unlocking: "ロックを解除しています...",
  unlock: "ロック解除",
} satisfies TranslatedArea<"vault">;
