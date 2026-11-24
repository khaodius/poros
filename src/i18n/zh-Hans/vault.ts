import type { TranslatedArea } from "../catalog";

export const vault = {
  // components/LockVaultDialog
  lock: "锁定 {name}？",
  pauseTransfersAndLock: "暂停传输并锁定",
  unmountAndLock: "卸载并锁定",
  transfersForThisVaultStillQueued: {
    other:
      "此保险库仍有 {count} 项传输在队列中。已锁定的保险库无法复制进出任何内容，因此这些传输会暂停等待，直到您再次解锁保险库并继续它们。",
  },

  // components/NewVaultDialog
  newVault: "新建保险库",
  aNewFolderInHolds: "{folder} 中的一个新文件夹将存放加密的文件。",
  password: "密码",
  atLeastCharacters: { other: "至少 {count} 个字符。" },
  repeatPassword: "重复密码",
  thePasswordsDoNotMatch: "两次输入的密码不一致。",
  thisPasswordCannotBeRecovered: "此密码无法找回。没有它，任何人都无法读取保险库中的文件。",
  theVaultAlsoOpensIn: "此保险库也可以在 Cryptomator 应用中打开。",
  creating: "正在创建...",
  createVault: "创建保险库",

  // components/VaultUnlockCard
  wrongPassword: "密码错误",
  encryptedVault: "加密的保险库",
  isACryptomatorVaultEnter: "{name} 是 Cryptomator 保险库。输入其密码即可在此处打开。",
  vaultPassword: "保险库密码",
  showEncryptedFiles: "显示加密的文件",
  unlocking: "正在解锁...",
  unlock: "解锁",
} satisfies TranslatedArea<"vault">;
