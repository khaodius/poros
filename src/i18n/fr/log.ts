import type { TranslatedArea } from "../catalog";

export const log = {
  // components/LogView
  saveLog: "Enregistrer le journal",
  levels: "Niveaux",
  session: "Session",
  allSessions: "Toutes les sessions",
  searchLog: "Rechercher dans le journal",
  clearSearch: "Effacer la recherche",
  showTimes: "Afficher les heures",
  wrapLongLines: "Retour automatique à la ligne",
  followNewLines: "Suivre les nouvelles lignes",
  copyShownLines: "Copier les lignes affichées",
  saveShownLinesToA: "Enregistrer les lignes affichées dans un fichier",
  clearLog: "Effacer le journal",
  info: "Infos",
  warnings: "Avertissements",
  errors: "Erreurs",
  server: "Serveur",
  closedSession: "Session fermée {id}",
  connectionAndTransferActivityAppears: "L’activité des connexions et des transferts apparaît ici.",
  noLinesMatchTheFilters: "Aucune ligne ne correspond aux filtres.",
  logFiles: "Fichiers journaux",
} satisfies TranslatedArea<"log">;
