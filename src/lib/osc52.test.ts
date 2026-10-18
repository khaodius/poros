import { describe, expect, it } from "vitest";
import { clipboardText } from "./osc52";

const encoded = (text: string) => btoa(String.fromCharCode(...new TextEncoder().encode(text)));

describe("clipboardText", () => {
  it("decodes the text a program copies, whichever selection it names", () => {
    expect(clipboardText(`c;${encoded("ls -la")}`)).toBe("ls -la");
    expect(clipboardText(`;${encoded("default selection")}`)).toBe("default selection");
    expect(clipboardText(`pc;${encoded("primary and clipboard")}`)).toBe("primary and clipboard");
  });

  it("keeps text beyond ASCII and across lines", () => {
    const text = "Fichier déjà copié ✓\n日本語のテキスト\r\n";
    expect(clipboardText(`c;${encoded(text)}`)).toBe(text);
  });

  it("refuses reads, clears and broken sequences", () => {
    expect(clipboardText("c;?")).toBeNull();
    expect(clipboardText("c;")).toBeNull();
    expect(clipboardText("c")).toBeNull();
    expect(clipboardText("c;not base64!")).toBeNull();
  });
});
