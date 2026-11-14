import { useEffect, useMemo } from "react";
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
import { FILE_ICON_SIZE, systemIconLookup, type IconSubject } from "../lib/systemIcons";
import { useSettingsStore } from "../state/settingsStore";
import { iconCacheKey, requestSystemIcon, useSystemIconStore } from "../state/systemIconStore";

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

/**
 * A file's icon, the system's when Settings asks for those. `size` other than the file lists'
 * looks the system's icon up at that size, showing the lists' one until it comes, or Poros's
 * own when neither has come yet, so the icon is never blank.
 */
export function FileIcon({ entry, size = FILE_ICON_SIZE }: { entry: IconSubject; size?: number }) {
  const { name, kind, linkTarget } = entry;
  const systemIcons = useSettingsStore((state) => state.settings.appearance.systemIcons);
  const lookup = useMemo(
    () => (systemIcons ? systemIconLookup({ name, kind, linkTarget }) : null),
    [systemIcons, name, kind, linkTarget],
  );
  const scale = window.devicePixelRatio || 1;
  const pixels = Math.round(size * scale);
  const cacheKey = lookup ? iconCacheKey(lookup.key, pixels) : null;
  const listKey = lookup ? iconCacheKey(lookup.key, Math.round(FILE_ICON_SIZE * scale)) : null;
  const source = useSystemIconStore((state) => (cacheKey ? state.icons[cacheKey] : null));
  const listSource = useSystemIconStore((state) => (listKey ? state.icons[listKey] : null));
  useEffect(() => {
    if (lookup && cacheKey && source === undefined) {
      requestSystemIcon(cacheKey, pixels, lookup.request);
    }
  }, [lookup, cacheKey, pixels, source]);

  const shown = source || listSource;
  if (shown) {
    return (
      <img
        className="file-icon file-icon-system"
        src={shown}
        width={size}
        height={size}
        alt=""
        draggable={false}
      />
    );
  }
  // The system's icon is on its way: an empty space rather than Poros's icon for a moment.
  if (source === undefined && size === FILE_ICON_SIZE) {
    return <span className="file-icon file-icon-pending" aria-hidden />;
  }
  const [Icon, group] = iconFor(entry);
  return <Icon size={size} className={`file-icon file-icon-${group}`} aria-hidden />;
}
