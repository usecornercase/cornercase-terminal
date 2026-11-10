type Styled = HTMLElement | SVGElement;

const styles = new Map<Styled, Map<string, string>>();
const classes = new Map<Element, Map<string, boolean>>();
const texts = new Map<Element, string>();

export function paint(el: Styled | null, prop: string, value: string): void {
  if (!el) return;
  let seen = styles.get(el);
  if (!seen) {
    seen = new Map();
    styles.set(el, seen);
  }
  if (seen.get(prop) === value) return;
  seen.set(prop, value);
  el.style.setProperty(prop, value);
}

export function flag(el: Element | null, name: string, on: boolean): void {
  if (!el) return;
  let seen = classes.get(el);
  if (!seen) {
    seen = new Map();
    classes.set(el, seen);
  }
  if (seen.get(name) === on) return;
  seen.set(name, on);
  el.classList.toggle(name, on);
}

export function fade(el: Styled | null, opacity: number): void {
  paint(el, 'opacity', opacity.toFixed(3));
  paint(el, 'pointer-events', opacity < 0.4 ? 'none' : 'auto');
}

export function text(el: Element | null, value: string): void {
  if (!el || el.textContent === value) return;
  if (!texts.has(el)) texts.set(el, el.textContent ?? '');
  el.textContent = value;
}

export function wipe(): void {
  for (const [el, seen] of styles) for (const prop of seen.keys()) el.style.removeProperty(prop);
  for (const [el, seen] of classes) for (const name of seen.keys()) el.classList.remove(name);
  for (const [el, was] of texts) el.textContent = was;
  styles.clear();
  classes.clear();
  texts.clear();
}

export function one<T extends Element = HTMLElement>(selector: string, root: ParentNode = document): T {
  const found = root.querySelector<T>(selector);
  if (!found) throw new Error(`nothing matches ${selector}`);
  return found;
}

export function all<T extends Element = HTMLElement>(selector: string, root: ParentNode = document): T[] {
  return [...root.querySelectorAll<T>(selector)];
}
