import type { MessageArea } from "../catalog";

export const automation = {
  // components/AutomationSettings
  whenTheTransferQueueFinishes: "When the transfer queue finishes",
  then: "Then",
  thenHint:
    "Also in the queue's toolbar. Everything but a command counts down for 30 seconds first, so you can stop it.",
  onlyTheNextTime: "Only the next time",
  onlyTheNextTimeHint: "Goes back to Do nothing once it has happened.",
  command: "Command",
  commandHint:
    "For Run a command; runs on this computer. POROS_FILES_DONE, POROS_FILES_FAILED, POROS_FILES_SKIPPED and POROS_BYTES hold the totals.",
  aCommandOrScript: "A command or script",
  showANotification: "Show a notification",
  onlyWhenPorosIsIn: "Only when Poros is in the background",
  playASound: "Play a sound",
  serverCommands: "Server commands",
  listedUnderCommandsWhenYou:
    "Listed under Commands when you right-click in a server pane. They run through the server's shell in the folder shown, with these filled in:",
  scheduledTasks: "Scheduled tasks",
  synchronizeFoldersOrRunCommands: "Synchronize folders or run commands at set times",
  synchronizeFoldersOrRunCommandsHint:
    "Tasks run while Poros is open, in the background, through saved connections.",
  output: "Output",
  openAWindowWithWhat: "Open a window with what the command prints; off runs it quietly",
  reloadTheServersFoldersWhen: "Reload the server's folders when the command finishes",
  deleteCommand: "Delete command",
  nameOfTheNewCommand: "Name of the new command",
  newCommand: "New command",
  duSh: "du -sh {paths}",

  // components/CommandOutputDialog
  earlierOutputWasDropped: "Earlier output was dropped.",
  theCommandPrintedNothing: "The command printed nothing.",
  running: "Running",

  // components/RunCommandDialog
  runACommand: "Run a command",
  runACommandOnThe: "Run a command on the server",
  commandToRunIn: "Command to run in",
  refreshTheServersFoldersWhen: "Refresh the server's folders when it finishes",
  selectFilesOrFoldersTo: "Select files or folders to fill in the placeholders.",
  run: "Run",

  // components/WhenDoneControl
  whatHappensWhenTheQueue: "What happens when the queue finishes",
  whenDone: "When done",
  whenTheQueueFinishes: "When the queue finishes",

  // components/WhenDoneCountdown
  theTransferQueueHasFinished: "The transfer queue has finished. {title} in",
  seconds: { one: "{count} second", other: "{count} seconds" },

  // components/commandMenu
  commands: "Commands",
  runACommand2: "Run a command...",
  editCommands: "Edit commands...",

  // lib/automation
  filesTransferredIn: {
    one: "{count} file transferred ({size}) in {duration}",
    other: "{count} files transferred ({size}) in {duration}",
  },

  // state/commandStore
  stopped: "Stopped",
  nothingToRun: "Nothing to run",
  allCommandsFinished: "All {finished} commands finished",
  ofCommandsFailed: "{failed} of {finished} commands failed",
  // lib/automation
  doNothing: "Do nothing",
  closePoros: "Close Poros",
  lockTheComputer: "Lock the computer",
  sleep: "Sleep",
  hibernate: "Hibernate",
  logOff: "Log off",
  shutDown: "Shut down",
  closingPoros: "Closing Poros",
  lockingTheComputer: "Locking the computer",
  puttingTheComputerToSleep: "Putting the computer to sleep",
  hibernatingTheComputer: "Hibernating the computer",
  loggingOff: "Logging off",
  shuttingDownTheComputer: "Shutting down the computer",

  // lib/commands
  eachSelectedItemsFullPath: "each selected item's full path, running once per item",
  eachSelectedItemsNameRunning: "each selected item's name, running once per item",
  theFullPathsOfAll: "the full paths of all selected items",
  theNamesOfAllSelected: "the names of all selected items",
  theFolderShown: "the folder shown",
} as const satisfies MessageArea;
