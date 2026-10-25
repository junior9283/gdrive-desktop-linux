// A lógica das Preferências sem DOM: as preferências do serviço como valores, o que mudou (para
// `set_preferences`), a validação de cada campo e as conversões da tela.

const KB_PER_GB = 1024 * 1024;
// Uma conferência rápida para o botão Salvar; quem decide é o serviço (`Layer::check`).
const PROXY = /^(https?|socks5h?):\/\/[^\s/]+/;
const MODIFIERS = new Set(['Control', 'Alt', 'Shift', 'Meta', 'Super', 'OS', 'AltGraph']);
// Teclas cujo nome no navegador não é o do keysym que o portal entende.
const KEYS = { ' ': 'space', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right' };

// `[{ key, value (JSON), managed }]` do serviço como `{ values, managed }` (`values` com os
// valores já lidos; `managed`, o conjunto das chaves da política).
export function fromPrefs(prefs) {
  return {
    values: Object.fromEntries(prefs.map((p) => [p.key, JSON.parse(p.value)])),
    managed: new Set(prefs.filter((p) => p.managed).map((p) => p.key)),
  };
}

// As chaves de `edited` que diferem de `original`, como `[chave, JSON]` (`null` apaga a chave).
export function diff(original, edited) {
  return Object.keys(edited)
    .map((key) => [key, JSON.stringify(edited[key] ?? null)])
    .filter(([key, json]) => json !== JSON.stringify(original[key] ?? null));
}

// O erro de `value` em `key`, como `[chave, argumentos]` do `ui.ftl`, ou `null` se vale.
// `limits.maxCacheGb` é o maior cache que cabe no disco (`null`: sem saber, só o mínimo vale).
export function validate(key, value, limits) {
  if (value === null) return null;
  switch (key) {
    case 'ContentCacheMaxKbytes': {
      const max = limits.maxCacheGb ?? Infinity;
      return Number.isInteger(value) && value >= KB_PER_GB && value <= max * KB_PER_GB
        ? null
        : ['pref-invalid-cache', { max }];
    }
    case 'BandwidthRxKBPS':
    case 'BandwidthTxKBPS':
      return Number.isInteger(value) && value > 0 ? null : ['pref-invalid-bandwidth'];
    case 'ProxyServer':
      return PROXY.test(value) ? null : ['pref-invalid-proxy'];
    default:
      return null;
  }
}

// O número de uma caixa de texto: vazia é `null` (apaga a chave).
export function numberOrNull(text) {
  return text.trim() === '' ? null : Number(text);
}

export const gbToKb = (gb) => (gb === null ? null : gb * KB_PER_GB);
export const kbToGb = (kb) => (kb === null ? null : Math.round(kb / KB_PER_GB));

// O proxy na tela (`auto`, `direct` ou `manual`) a partir das duas chaves, e as duas chaves a
// partir da escolha (`address` só vale no manual).
export function proxyMode(values) {
  if (values.DirectConnection) return 'direct';
  return values.ProxyServer ? 'manual' : 'auto';
}

export function proxyPatch(mode, address) {
  return {
    DirectConnection: mode === 'direct',
    ProxyServer: mode === 'manual' ? address.trim() || null : null,
  };
}

// O atalho de um `keydown` no formato de `SearchHotKey` (`Ctrl+Alt+G`), ou `null` enquanto só há
// modificadores (ou nenhum).
export function hotkeyFrom(event) {
  if (MODIFIERS.has(event.key)) return null;
  const parts = [];
  if (event.ctrlKey) parts.push('Ctrl');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  if (event.metaKey) parts.push('Super');
  if (!parts.length) return null;
  const key = KEYS[event.key] ?? (event.key.length === 1 ? event.key.toUpperCase() : event.key);
  return [...parts, key].join('+');
}
