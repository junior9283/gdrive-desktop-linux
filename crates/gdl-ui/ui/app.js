// Entrada de todas as janelas: carrega os textos, aplica o tema, mostra a tela da query e o aviso
// de serviço fora do ar (que some sozinho quando ele volta, recarregando a tela).

import { api } from './api.js';
import { el, icon } from './components.js';
import { I18n, parseFtl } from './i18n.js';
import { currentScreen } from './router.js';

// As telas que já existem; as outras mostram o aviso de "em construção".
const READY = new Set(['about', 'home']);

const screen = currentScreen(location.search);
// Os textos e o código da tela chegam juntos.
const [{ locale, source }, module] = await Promise.all([
  api.strings(),
  import(READY.has(screen) ? `./screens/${screen}.js` : './screens/pending.js'),
]);
const i18n = new I18n(parseFtl(source), locale);
document.documentElement.lang = locale;

const root = el('div', { id: 'screen' });
const banner = el(
  'div',
  { class: 'banner', role: 'status', hidden: true },
  icon('cloud_off'),
  el('span', {}, i18n.t('service-unavailable')),
  el('button', { onclick: () => show() }, i18n.t('action-retry')),
);
document.body.append(banner, root);

const context = { api, locale, t: i18n.t };

async function render() {
  try {
    await module.render(root, context);
  } catch (error) {
    // Sem serviço o aviso já aparece; outros erros vão para o console.
    if (error?.name !== 'Unavailable') console.error(error);
  }
}

// `UITheme`: `light` força o claro; senão vale o do sistema.
async function applyTheme() {
  try {
    const prefs = await api.call('preferences');
    const theme = JSON.parse(prefs.find((p) => p.key === 'UITheme')?.value ?? 'null');
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  } catch {
    // Sem serviço, fica o tema do sistema.
  }
}

// Tema e tela juntos, para não piscar do tema do sistema para o escolhido.
const show = () => Promise.all([applyTheme(), render()]);
await Promise.all([
  api.onService((up) => {
    banner.hidden = up;
    if (up) show();
  }),
  api.on('preferences-changed', applyTheme),
]);
show();
