// Peças de tela, montadas pelo DOM (sem `innerHTML` e sem estilo embutido, por causa da CSP).

const SVG = 'http://www.w3.org/2000/svg';

// `el('p', { class: 'x', onclick }, 'texto', outroElemento)`.
export function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (key.startsWith('on')) node.addEventListener(key.slice(2), value);
    else if (value !== false && value != null) node.setAttribute(key, value === true ? '' : value);
  }
  node.append(...children.flat());
  return node;
}

// Um símbolo do `icons/sprite.svg`.
export function icon(name) {
  const svg = document.createElementNS(SVG, 'svg');
  svg.setAttribute('class', 'icon');
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS(SVG, 'use');
  use.setAttribute('href', `icons/sprite.svg#${name}`);
  svg.append(use);
  return svg;
}
