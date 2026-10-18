// OSC 52 (`ESC ] 52 ; targets ; base64 BEL`) is how a program in the terminal puts text on the
// clipboard. Programs reached over SSH copy this way, among them those that take over the mouse
// and draw their own selection, such as tmux in mouse mode, so the terminal never holds a
// selection of its own to copy.

/**
 * The text an OSC 52 sequence puts on the clipboard, from what follows its `52;`. Null when it
 * asks to read the clipboard, which a server is never allowed, or to clear it, or is not valid
 * base64.
 */
export function clipboardText(data: string): string | null {
  const separator = data.indexOf(";");
  if (separator < 0) return null;
  const payload = data.slice(separator + 1);
  if (payload === "" || payload === "?") return null;
  let binary: string;
  try {
    binary = atob(payload);
  } catch {
    return null;
  }
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}
