import type { MessageArea } from "../catalog";

export const preview = {
  // components/PreviewImage
  fitInThePane: "Fit in the pane",
  showAtActualSize: "Show at actual size",

  // components/PreviewPane
  brokenLink: "Broken link",
  notARegularFile: "Not a regular file",
  theFileThisLinkLeads: "The file this link leads to is not there.",
  thereIsNothingInIt: "There is nothing in it to preview.",
  nothingToPreview: "Nothing to preview",
  nothingToPreviewDetail: "Move the cursor to a file or folder to see it here.",
  preview: "Preview",
  folders: { one: "{count} folder", other: "{count} folders" },
  files: { one: "{count} file", other: "{count} files" },
  noPermissionToReadThis: "No permission to read this {what}",
  theAccountSignedInHere: "The account signed in here is not allowed to open it.",
  thisIsNoLongerThere: "This {what} is no longer there",
  itWasDeletedMovedOr: "It was deleted, moved or renamed after it was listed.",
  connectionLost: "Connection lost",
  theVaultIsLocked: "The vault is locked",
  theVaultIsLockedDetail: "Unlock it again to preview its files.",
  thePreviewTriesAgainAs: "The preview tries again as soon as the connection is free.",
  itStayedBusyForA: "It stayed busy for a while. Try again in a moment.",
  thePreviewWaitsRatherThan: "The preview waits rather than slow the transfer down.",
  couldNotLoadThePreview: "Could not load the preview",
  openInEditorF4: "Open in editor (F4)",
  openInEditor: "Open in editor",
  thisFileChangedAfterIt: "This file changed after it was listed.",
  onlyTheBeginningIsShown: "Only the beginning is shown: {sizeInText} of {sizeInText2}.",
  listedAs0Bytes: "Listed as 0 bytes",
  listedAs0BytesDetail:
    "Some servers list no size for a file they make themselves, so it may not be empty. Reading it shows what is in it.",
  showPreview: "Show preview",
  previewNotLoaded: "Preview not loaded",
  previewNotLoadedDetail:
    "Reading from this storage may be charged for, so the file is read only when asked.",
  file: "file",
  emptyFile: "Empty file",
  theServerSaysThereIs: "The server says there is nothing in it.",
  thereIsNothingInIt2: "There is nothing in it.",
  tooLargeToPreview: "Too large to preview",
  loadAnyway: "Load anyway",
  thisPictureCannotBeShown: "This picture cannot be shown",
  thisPictureCannotBeShownDetail: "It may be damaged, or of a kind this system does not decode.",
  theFirstBytes: { one: "The first {count} byte:", other: "The first {count} bytes:" },
  bytes: { one: "{count} byte", other: "{count} bytes" },
  folderNotListed: "Folder not listed",
  folderNotListedDetail:
    "Listing on this storage may be charged for, so the folder is listed only when asked.",
  folder: "folder",
  cryptomatorVault: "Cryptomator vault",
  cryptomatorVaultDetail: "Open the folder to unlock it and see its files.",
  emptyFolder: "Empty folder",
  items: { one: "{count} item", other: "{count} items" },
  thumbnailsNotLoaded: "Thumbnails not loaded",
  thumbnailsNotLoadedDetail:
    "Reading from this storage may be charged for, so pictures are read only when asked.",
  itemsSelected: "{count} items selected",

  // components/PreviewSettings
  largestPictureLoaded: "Largest picture loaded",
  largestPictureLoadedHint: "Larger pictures show their details and a Load anyway button.",
  textReadFromAFile: "Text read from a file",
  textReadFromAFileHint: "A longer file shows its beginning.",
  thumbnailSize: "Thumbnail size",

  // components/PreviewSplit
  resizePreview: "Resize preview",
  dragToResizeDoubleClick: "Drag to resize, double-click to reset",

  // components/PreviewThumbnails
  andMore: "and {value} more",

  // lib/previewLoad
  theFileChangesAsIt: "The file changes as it is read",
  // components/PreviewPane
  theFirst: "The first {t}:",

  // components/PreviewSettings
  whenTheCursorRestsOn: "When the cursor rests on a file",
  onAClickOfShow: "On a click of Show preview",
  filesOnThisComputerLoad: "Files on this computer load",
  filesOnServersLoad: "Files on servers load",
  sftpFtpAndWebdav: "SFTP, FTP and WebDAV.",
  filesOnStorageThatCharges: "Files on storage that charges for reads load",
  s3BackblazeB2GoogleDrive:
    "S3, Backblaze B2, Google Drive, OneDrive and Box, and vaults kept on them.",

  // lib/previewSniff
  webp: "WebP",

  // lib/previewText
  utf8WithBom: "UTF-8 with BOM",
  latin1: "Latin-1",
} as const satisfies MessageArea;
