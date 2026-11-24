import type { TranslatedArea } from "../catalog";

export const vault = {
  // components/LockVaultDialog
  lock: "Verrouiller {name}\u202f?",
  pauseTransfersAndLock: "Suspendre les transferts et verrouiller",
  unmountAndLock: "Démonter et verrouiller",
  transfersForThisVaultStillQueued: {
    one: "{count} transfert pour ce coffre est encore dans la file d’attente. Rien ne peut être copié vers ou depuis un coffre verrouillé\u202f; il attend donc en pause jusqu’à ce que vous déverrouilliez de nouveau le coffre et le repreniez.",
    many: "{count} de transferts pour ce coffre sont encore dans la file d’attente. Rien ne peut être copié vers ou depuis un coffre verrouillé\u202f; ils attendent donc en pause jusqu’à ce que vous déverrouilliez de nouveau le coffre et les repreniez.",
    other:
      "{count} transferts pour ce coffre sont encore dans la file d’attente. Rien ne peut être copié vers ou depuis un coffre verrouillé\u202f; ils attendent donc en pause jusqu’à ce que vous déverrouilliez de nouveau le coffre et les repreniez.",
  },

  // components/NewVaultDialog
  newVault: "Nouveau coffre",
  aNewFolderInHolds: "Un nouveau dossier dans {folder} contient les fichiers chiffrés.",
  password: "Mot de passe",
  atLeastCharacters: {
    one: "Au moins {count} caractère.",
    many: "Au moins {count} de caractères.",
    other: "Au moins {count} caractères.",
  },
  repeatPassword: "Confirmer le mot de passe",
  thePasswordsDoNotMatch: "Les mots de passe ne correspondent pas.",
  thisPasswordCannotBeRecovered:
    "Ce mot de passe ne peut pas être récupéré. Sans lui, personne ne peut lire les fichiers du coffre.",
  theVaultAlsoOpensIn: "Le coffre s’ouvre aussi dans les applications Cryptomator.",
  creating: "Création...",
  createVault: "Créer le coffre",

  // components/VaultUnlockCard
  wrongPassword: "Mot de passe incorrect",
  encryptedVault: "Coffre chiffré",
  isACryptomatorVaultEnter:
    "{name} est un coffre Cryptomator. Saisissez son mot de passe pour l’ouvrir ici.",
  vaultPassword: "Mot de passe du coffre",
  showEncryptedFiles: "Afficher les fichiers chiffrés",
  unlocking: "Déverrouillage...",
  unlock: "Déverrouiller",
} satisfies TranslatedArea<"vault">;
