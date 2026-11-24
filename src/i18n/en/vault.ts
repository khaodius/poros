import type { MessageArea } from "../catalog";

export const vault = {
  // components/LockVaultDialog
  lock: "Lock {name}?",
  transfers: { one: "{count} transfer", other: "{count} transfers" },
  pauseTransfersAndLock: "Pause transfers and lock",

  // components/NewVaultDialog
  newVault: "New vault",
  aNewFolderInHolds: "A new folder in {folder} holds the encrypted files.",
  password: "Password",
  atLeastCharacters: "At least {value} characters.",
  repeatPassword: "Repeat password",
  thePasswordsDoNotMatch: "The passwords do not match.",
  thisPasswordCannotBeRecovered:
    "This password cannot be recovered. Without it, nobody can read the files in the vault.",
  theVaultAlsoOpensIn: "The vault also opens in the Cryptomator apps.",

  // components/VaultUnlockCard
  wrongPassword: "Wrong password",
  encryptedVault: "Encrypted vault",
  isACryptomatorVaultEnter: "is a Cryptomator vault. Enter its password to open it here.",
  vaultPassword: "Vault password",
  showEncryptedFiles: "Show encrypted files",
  // components/NewVaultDialog
  creating: "Creating...",
  createVault: "Create vault",

  // components/VaultUnlockCard
  unlocking: "Unlocking...",
  unlock: "Unlock",
} as const satisfies MessageArea;
