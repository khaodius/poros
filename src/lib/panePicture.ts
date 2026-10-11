// A still copy of a pane's page, so the drag preview window can draw the pane where it leaves
// the window it is dragged in, looking the same on both sides of the edge.

/** The pane as markup, with what it needs to look the same in another window. */
export interface PanePicture {
  /** Which drag it was taken in; the preview shows it only for that drag. */
  drag: number;
  html: string;
  width: number;
  height: number;
  /** The theme's custom properties and fonts, set on the document element. */
  rootStyle: string;
  theme: string | null;
}

/** Past this, a pane is drawn as a sketch instead: sending it would hold up the drag. */
const MAX_HTML_LENGTH = 2_000_000;

const UNSAFE_ELEMENTS = "script, iframe, object, embed, link, meta, base, frame, frameset";

/**
 * Copies a pane as it looks now: its markup, plus what markup leaves out, such as scroll
 * positions, typed values and canvases.
 */
export function takePanePicture(panel: HTMLElement, drag: number): PanePicture | null {
  const copy = panel.cloneNode(true) as HTMLElement;
  const originals = panel.querySelectorAll<HTMLElement>("*");
  const copies = copy.querySelectorAll<HTMLElement>("*");
  originals.forEach((original, index) => {
    const element = copies[index];
    if (original.scrollTop || original.scrollLeft) {
      element.dataset.pictureScroll = `${original.scrollTop},${original.scrollLeft}`;
    }
    if (original instanceof HTMLInputElement) element.setAttribute("value", original.value);
    if (original instanceof HTMLTextAreaElement) element.textContent = original.value;
    if (original instanceof HTMLCanvasElement) {
      try {
        const image = document.createElement("img");
        image.src = original.toDataURL();
        image.className = original.className;
        image.style.cssText = original.style.cssText;
        image.width = original.clientWidth;
        image.height = original.clientHeight;
        element.replaceWith(image);
      } catch {
        // A canvas that cannot be read stays blank in the picture.
      }
    }
  });
  const html = copy.innerHTML;
  if (html.length > MAX_HTML_LENGTH) return null;
  const root = document.documentElement;
  const { width, height } = panel.getBoundingClientRect();
  return {
    drag,
    html,
    width,
    height,
    rootStyle: root.style.cssText,
    theme: root.dataset.theme ?? null,
  };
}

/**
 * Draws a picture in an element: inert markup only, scrolled as the pane was. The pane's text,
 * such as file names, is shown as text, and nothing in it runs.
 */
export function drawPanePicture(target: HTMLElement, picture: PanePicture): void {
  const template = document.createElement("template");
  template.innerHTML = picture.html;
  const content = template.content;
  content.querySelectorAll(UNSAFE_ELEMENTS).forEach((element) => element.remove());
  content.querySelectorAll("*").forEach((element) => {
    for (const { name, value } of [...element.attributes]) {
      if (/^on/i.test(name) || /^\s*javascript:/i.test(value)) element.removeAttribute(name);
    }
  });
  target.replaceChildren(content);
  target.querySelectorAll<HTMLElement>("[data-picture-scroll]").forEach((element) => {
    const [top, left] = (element.dataset.pictureScroll ?? "").split(",").map(Number);
    element.scrollTop = top || 0;
    element.scrollLeft = left || 0;
  });
}
