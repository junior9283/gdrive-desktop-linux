// A faixa da versão nova na tela Sobre, sem DOM: o texto e os botões a partir do `UpdateInfo`
// do serviço (`{ version, can_install, step }`).

// O texto de cada passo da atualização (sem botões enquanto ela anda).
export const STEP_TEXTS = {
  downloading: 'about-update-downloading',
  'waiting-password': 'about-update-password',
  installing: 'about-update-installing',
};

// O texto da versão nova parada (com os botões).
export const AVAILABLE_TEXT = 'about-update-available';

// `null` sem versão nova; senão `{ text, version, actions }`, com `actions` entre `update`
// ("Atualizar agora", quando o app instala sozinho) e `release` ("Ver novidades").
export function updateBanner(info) {
  if (!info?.version) return null;
  const step = STEP_TEXTS[info.step];
  if (step) return { text: step, version: info.version, actions: [] };
  return {
    text: AVAILABLE_TEXT,
    version: info.version,
    actions: info.can_install ? ['update', 'release'] : ['release'],
  };
}
