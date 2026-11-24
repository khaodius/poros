import type { TranslatedArea } from "../catalog";

export const vault = {
  // components/LockVaultDialog
  lock: "¿Bloquear {name}?",
  pauseTransfersAndLock: "Pausar transferencias y bloquear",
  unmountAndLock: "Desmontar y bloquear",
  transfersForThisVaultStillQueued: {
    one: "{count} transferencia de esta bóveda sigue en la cola. No se puede copiar nada dentro ni fuera de una bóveda bloqueada, así que espera en pausa hasta que vuelvas a desbloquear la bóveda y la reanudes.",
    many: "{count} de transferencias de esta bóveda siguen en la cola. No se puede copiar nada dentro ni fuera de una bóveda bloqueada, así que esperan en pausa hasta que vuelvas a desbloquear la bóveda y las reanudes.",
    other:
      "{count} transferencias de esta bóveda siguen en la cola. No se puede copiar nada dentro ni fuera de una bóveda bloqueada, así que esperan en pausa hasta que vuelvas a desbloquear la bóveda y las reanudes.",
  },

  // components/NewVaultDialog
  newVault: "Nueva bóveda",
  aNewFolderInHolds: "Una carpeta nueva en {folder} contiene los archivos cifrados.",
  password: "Contraseña",
  atLeastCharacters: {
    one: "Al menos {count} carácter.",
    many: "Al menos {count} de caracteres.",
    other: "Al menos {count} caracteres.",
  },
  repeatPassword: "Repetir contraseña",
  thePasswordsDoNotMatch: "Las contraseñas no coinciden.",
  thisPasswordCannotBeRecovered:
    "Esta contraseña no se puede recuperar. Sin ella, nadie puede leer los archivos de la bóveda.",
  theVaultAlsoOpensIn: "La bóveda también se abre en las aplicaciones de Cryptomator.",
  creating: "Creando...",
  createVault: "Crear bóveda",

  // components/VaultUnlockCard
  wrongPassword: "Contraseña incorrecta",
  encryptedVault: "Bóveda cifrada",
  isACryptomatorVaultEnter:
    "{name} es una bóveda de Cryptomator. Escribe su contraseña para abrirla aquí.",
  vaultPassword: "Contraseña de la bóveda",
  showEncryptedFiles: "Mostrar archivos cifrados",
  unlocking: "Desbloqueando...",
  unlock: "Desbloquear",
} satisfies TranslatedArea<"vault">;
