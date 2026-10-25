// A lógica dos arquivos off-line sem DOM: a barra de uso do disco.

const KEYS = ['offline', 'cache', 'free'];

// As partes da barra, `[{ key, bytes, percent }]`: o off-line, o resto do cache (o
// `cache_bytes` do serviço já conta o off-line) e o espaço livre. As porcentagens são inteiras e
// somam 100 (os pontos que o arredondamento tira vão para as maiores sobras); sem nada, é tudo
// livre.
export function segments(summary) {
  const bytes = [
    summary.pinned_bytes,
    Math.max(0, summary.cache_bytes - summary.pinned_bytes),
    summary.free_bytes,
  ];
  const total = bytes.reduce((a, b) => a + b, 0);
  const exact = total ? bytes.map((b) => (b * 100) / total) : [0, 0, 100];
  const percent = exact.map(Math.floor);
  const order = exact.map((value, i) => [value - percent[i], i]).sort((a, b) => b[0] - a[0]);
  for (let left = 100 - percent.reduce((a, b) => a + b, 0), k = 0; left > 0; left--, k++) percent[order[k][1]]++;
  return KEYS.map((key, i) => ({ key, bytes: bytes[i], percent: percent[i] }));
}
