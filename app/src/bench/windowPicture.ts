// A picture of the whole window from inside the app, for the bench: the page's own elements copied with the style
// sheets they're drawn by, laid out again inside an SVG and drawn to a canvas, with the view's canvas as the picture it
// holds. Nothing reads the screen. What it leaves out: scroll positions, web fonts, and text typed into a box since it
// was drawn (typed values are copied over as attributes).
import type { Viewport } from '../viewport/Viewport';

const XHTML = 'http://www.w3.org/1999/xhtml';

/** Every rule the page is drawn by, as one sheet. A sheet from elsewhere that can't be read is left out. */
function pageCss(): string {
  let css = '';
  for (const sheet of document.styleSheets) {
    try {
      for (const rule of sheet.cssRules) css += `${rule.cssText}\n`;
    } catch {
      // Not ours to read.
    }
  }
  return css;
}

/** A copy of an element and what's in it, canvases as pictures and inputs holding what they show. */
function copyOf(node: Element, pictures: Map<HTMLCanvasElement, string>): Element {
  if (node instanceof HTMLCanvasElement) {
    const img = document.createElement('img');
    for (const attr of node.attributes) if (attr.name !== 'width' && attr.name !== 'height') img.setAttribute(attr.name, attr.value);
    img.setAttribute('src', pictures.get(node) ?? '');
    img.style.width = `${node.clientWidth}px`;
    img.style.height = `${node.clientHeight}px`;
    return img;
  }
  const copy = node.cloneNode(false) as Element;
  if (node instanceof HTMLInputElement) {
    copy.setAttribute('value', node.value);
    if (node.checked) copy.setAttribute('checked', '');
  }
  if (node instanceof HTMLTextAreaElement) copy.textContent = node.value;
  // A select shows the option it holds, not the first: the copy only knows by the attribute.
  if (node instanceof HTMLOptionElement) {
    if (node.selected) copy.setAttribute('selected', '');
    else copy.removeAttribute('selected');
  }
  for (const child of node.childNodes) {
    if (child instanceof HTMLScriptElement) continue;
    if (child instanceof Element) copy.appendChild(copyOf(child, pictures));
    else if (child.nodeType === Node.TEXT_NODE && !(node instanceof HTMLTextAreaElement)) copy.appendChild(child.cloneNode());
  }
  return copy;
}

/** The window as it stands, as a PNG data URL, once the view has drawn. */
export async function windowPicture(viewport: Viewport): Promise<string> {
  for (let i = 0; i < 3; i++) await viewport.nextFrame();
  viewport.draw();
  const pictures = new Map<HTMLCanvasElement, string>();
  for (const canvas of document.querySelectorAll('canvas')) {
    try {
      pictures.set(canvas, canvas.toDataURL('image/png'));
    } catch {
      // Left blank.
    }
  }
  const { innerWidth: width, innerHeight: height } = window;
  const page = document.createElementNS(XHTML, 'html');
  for (const attr of document.documentElement.attributes) page.setAttribute(attr.name, attr.value);
  const head = document.createElementNS(XHTML, 'head');
  const style = document.createElementNS(XHTML, 'style');
  // The page is the SVG's width and height, as it is the window's.
  style.textContent = `${pageCss()}\nhtml, body { width: ${width}px; height: ${height}px; margin: 0; overflow: hidden; }`;
  head.appendChild(style);
  page.append(head, copyOf(document.body, pictures));
  const xml = new XMLSerializer().serializeToString(page);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><foreignObject x="0" y="0" width="${width}" height="${height}">${xml}</foreignObject></svg>`;
  const image = new Image();
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  await image.decode();
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('no 2D canvas to draw the window on');
  context.drawImage(image, 0, 0, width, height);
  return canvas.toDataURL('image/png');
}
