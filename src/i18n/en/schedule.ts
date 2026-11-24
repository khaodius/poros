import type { MessageArea } from "../catalog";

export const schedule = {
  // components/SchedulerDialog
  scheduledTasks: "Scheduled tasks",
  tasks: "Tasks",
  newTask: "New task",
  tasksRunWhilePorosIs: "Tasks run while Poros is open. Ones you save appear here.",
  shownInTheListOf: "Shown in the list of tasks",
  turnedOn: "Turned on",
  task: "Task",
  savedConnection: "Saved connection",
  thisConnectionsPasswordIsNot:
    "This connection's password is not saved in the system keychain, so the task cannot log in. Save it in the connection's settings.",
  ifPorosIsClosedAt: "If Poros is closed at the time, run when it next starts",
  nextRun: "Next run",
  runningNow: "Running now",
  lastRun: "Last run",
  chooseTheLocalFolder: "Choose the local folder",
  localFolder: "Local folder",
  theFolderOnThisComputer: "The folder on this computer",
  serverFolder: "Server folder",
  homeMeSite: "/home/me/site",
  direction: "Direction",
  conflictsAreLeftAloneAnd: "{hint} Conflicts are left alone and reported.",
  rsyncOptions: "rsync options",
  typedAsOnRsyncsCommand:
    "Typed as on rsync's command line; they apply over the options above each time the task runs.",
  command: "Command",
  backupSh: "./backup.sh",
  inFolderOptional: "In folder (optional)",
  theLoginFolder: "The login folder",
  when: "When",
  dateAndTime: "Date and time",
  every: "every",
  repeatEvery: "Repeat every",
  unit: "Unit",
  from: "from",
  starting: "Starting",
  at: "at",
  timeOfDay: "Time of day",
  daysOfTheWeek: "Days of the week",
  // components/SchedulerDialog
  onDays: "On days",
  repeat: "Repeat",
  once: "Once",
  // components/SchedulerDialog
  synchronizeFolders: "Synchronize folders",
  transferFiles: "Transfer files",
  runACommand: "Run a command",
  uploadTo: "Upload to",
  downloadFrom: "Download from",
  copyTo: "Copy to",
  tasksRunWhilePorosIsOpen:
    "Tasks run while Poros is open, or from poros-cli with cron or a timer. Ones you save appear here.",

  // lib/scheduleDraft
  porosNoLongerKnowsThe:
    "Poros no longer knows the server of these transfers. Schedule the files from a tab instead.",
  filesInAnUnlockedVault:
    "Files in an unlocked vault cannot be scheduled: a task could not unlock the vault.",
  saveTheConnectionToFirst:
    "Save the connection to {label} first: a scheduled task signs in with a saved connection.",
  selectTheTransfersToSchedule: "Select the transfers to schedule.",
  selectTransfersThatGoThe:
    "Select transfers that go the same way between the same places to schedule them.",
  theSelectedTransfersGoTo:
    "The selected transfers go to different folders. Select ones that go into one folder.",
  selectTheFilesToSchedule: "Select the files to schedule.",

  // lib/schedule
  items: { one: "{verb} {count} item", other: "{verb} {count} items" },
  overwriteIfTheSourceIs: "Overwrite if the source is newer",
  overwriteIfSizeOrDate: "Overwrite if size or date differ",
  keepBoth: "Keep both",
} as const satisfies MessageArea;
