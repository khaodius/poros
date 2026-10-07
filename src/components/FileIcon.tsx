import {
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileKey,
  FileQuestion,
  FileSpreadsheet,
  FileText,
  FileVideo,
  Folder,
  FolderSymlink,
  Link2Off,
  type LucideIcon,
} from "lucide-react";
import type { FileEntry } from "../lib/types";

const EXTENSION_GROUPS: [LucideIcon, string, string][] = [
  [FileImage, "image", "png jpg jpeg gif webp svg bmp ico tif tiff heic avif"],
  [FileVideo, "video", "mp4 mkv mov avi webm m4v wmv flv"],
  [FileAudio, "audio", "mp3 flac wav ogg m4a aac opus"],
  [FileArchive, "archive", "zip tar gz tgz bz2 xz zst 7z rar iso deb rpm"],
  [FileSpreadsheet, "data", "csv tsv xls xlsx ods parquet"],
  [FileKey, "key", "pem key ppk pub crt cer p12 pfx gpg asc"],
  [
    FileCode,
    "code",
    "rs ts tsx js jsx mjs py go c h cpp hpp cs java kt rb php sh bash zsh ps1 lua sql " +
      "html css scss json yaml yml toml xml ini conf dockerfile",
  ],
  [FileText, "text", "txt md log rst pdf doc docx odt rtf"],
];

const ICON_BY_EXTENSION = new Map<string, [LucideIcon, string]>(
  EXTENSION_GROUPS.flatMap(([icon, group, extensions]) =>
    extensions.split(" ").map((extension) => [extension, [icon, group]] as const),
  ),
);

type IconSubject = Pick<FileEntry, "name" | "kind" | "linkTarget">;

function iconFor(entry: IconSubject): [LucideIcon, string] {
  if (entry.kind === "dir") return [Folder, "folder"];
  if (entry.kind === "symlink") {
    if (entry.linkTarget === "dir") return [FolderSymlink, "folder"];
    if (entry.linkTarget === "broken") return [Link2Off, "broken"];
  }
  if (entry.kind === "other") return [FileQuestion, "other"];
  const extension = entry.name.includes(".")
    ? entry.name.slice(entry.name.lastIndexOf(".") + 1).toLowerCase()
    : entry.name.toLowerCase();
  return ICON_BY_EXTENSION.get(extension) ?? [File, "file"];
}

export function FileIcon({ entry }: { entry: IconSubject }) {
  const [Icon, group] = iconFor(entry);
  return <Icon size={16} className={`file-icon file-icon-${group}`} aria-hidden />;
}
