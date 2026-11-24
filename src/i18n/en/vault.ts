import type { MessageArea } from "../catalog";

export const vault = {
  // components/LockVaultDialog
  lock: "Lock {name}?",
  pauseTransfersAndLock: "Pause transfers and lock",
  transfersForThisVaultStillQueued: {
    one: "{count} transfer for this vault is still in the queue. Nothing can be copied into or out of a locked vault, so it waits paused until you unlock the vault again and resume it.",
    other:
      "{count} transfers for this vault are still in the queue. Nothing can be copied into or out of a locked vault, so they wait paused until you unlock the vault again and resume them.",
  },

  // components/NewVaultDialog
  newVault: "New vault",
  aNewFolderInHolds: "A new folder in {folder} holds the encrypted files.",
  password: "Password",
  atLeastCharacters: { one: "At least {count} character.", other: "At least {count} characters." },
  repeatPassword: "Repeat password",
  thePasswordsDoNotMatch: "The passwords do not match.",
  thisPasswordCannotBeRecovered:
    "This password cannot be recovered. Without it, nobody can read the files in the vault.",
  theVaultAlsoOpensIn: "The vault also opens in the Cryptomator apps.",
  creating: "Creating...",
  createVault: "Create vault",

  // components/VaultUnlockCard
  wrongPassword: "Wrong password",
  encryptedVault: "Encrypted vault",
  isACryptomatorVaultEnter: "{name} is a Cryptomator vault. Enter its password to open it here.",
  vaultPassword: "Vault password",
  showEncryptedFiles: "Show encrypted files",
  unlocking: "Unlocking...",
  unlock: "Unlock",
} as const satisfies MessageArea;
