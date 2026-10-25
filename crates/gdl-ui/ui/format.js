// Tamanhos e tempos no idioma da janela.

const UNITS = ['KB', 'MB', 'GB', 'TB'];

// Base 1024 e uma casa decimal: `1536` → `1,5 KB` (pt-BR) ou `1.5 KB` (inglês).
export function formatSize(bytes, locale) {
  if (bytes < 1024) return `${bytes} ${bytes === 1 ? 'byte' : 'bytes'}`;
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const number = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value);
  return `${number} ${UNITS[unit]}`;
}

const SPANS = [
  ['day', 86_400],
  ['hour', 3_600],
  ['minute', 60],
];

// "há 5 minutos" / "5 minutes ago"; menos de um minuto é "agora".
export function formatAgo(ms, now, locale) {
  const seconds = Math.max(0, (now - ms) / 1000);
  const span = SPANS.find(([, size]) => seconds >= size);
  if (!span) return new Intl.RelativeTimeFormat(locale, { numeric: 'auto' }).format(0, 'second');
  const [unit, size] = span;
  return new Intl.RelativeTimeFormat(locale, { numeric: 'always' }).format(
    -Math.floor(seconds / size),
    unit,
  );
}

// Uma duração na maior unidade que couber, arredondada: `5 minutos`, `2 hours`.
export function formatDuration(seconds, locale) {
  const [unit, size] = SPANS.find(([, size]) => seconds >= size) ?? ['second', 1];
  return new Intl.NumberFormat(locale, { style: 'unit', unit, unitDisplay: 'long' }).format(
    Math.round(seconds / size),
  );
}
