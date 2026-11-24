import type { TranslatedArea } from "../catalog";

export const vault = {
  // components/LockVaultDialog
  lock: "{name} sperren?",
  pauseTransfersAndLock: "Übertragungen anhalten und sperren",
  unmountAndLock: "Trennen und sperren",
  transfersForThisVaultStillQueued: {
    one: "{count} Übertragung für diesen Tresor ist noch in der Warteschlange. In einen gesperrten Tresor oder aus ihm kann nichts kopiert werden, daher wartet sie angehalten, bis Sie den Tresor wieder entsperren und sie fortsetzen.",
    other:
      "{count} Übertragungen für diesen Tresor sind noch in der Warteschlange. In einen gesperrten Tresor oder aus ihm kann nichts kopiert werden, daher warten sie angehalten, bis Sie den Tresor wieder entsperren und sie fortsetzen.",
  },

  // components/NewVaultDialog
  newVault: "Neuer Tresor",
  aNewFolderInHolds: "Ein neuer Ordner in {folder} enthält die verschlüsselten Dateien.",
  password: "Passwort",
  atLeastCharacters: { one: "Mindestens {count} Zeichen.", other: "Mindestens {count} Zeichen." },
  repeatPassword: "Passwort wiederholen",
  thePasswordsDoNotMatch: "Die Passwörter stimmen nicht überein.",
  thisPasswordCannotBeRecovered:
    "Dieses Passwort lässt sich nicht wiederherstellen. Ohne es kann niemand die Dateien im Tresor lesen.",
  theVaultAlsoOpensIn: "Der Tresor lässt sich auch in den Cryptomator-Apps öffnen.",
  creating: "Wird erstellt...",
  createVault: "Tresor erstellen",

  // components/VaultUnlockCard
  wrongPassword: "Falsches Passwort",
  encryptedVault: "Verschlüsselter Tresor",
  isACryptomatorVaultEnter:
    "{name} ist ein Cryptomator-Tresor. Geben Sie sein Passwort ein, um ihn hier zu öffnen.",
  vaultPassword: "Tresorpasswort",
  showEncryptedFiles: "Verschlüsselte Dateien zeigen",
  unlocking: "Wird entsperrt...",
  unlock: "Entsperren",
} satisfies TranslatedArea<"vault">;
