// O pedaço do Fluent que os arquivos `ui.ftl` usam: `{ $var }` e seleção
// (`{ $count -> [0] … [one] … *[other] … }`) por valor exato ou por categoria
// de plural (`Intl.PluralRules`). As mensagens chegam cruas do Rust (`strings`).

export class I18n {
  constructor(messages, locale) {
    this.messages = messages;
    this.locale = locale;
    this.plural = new Intl.PluralRules(locale);
  }

  // O texto de `key` com `args`; a própria chave se ela não existir.
  t(key, args = {}) {
    const pattern = this.messages[key];
    return pattern === undefined ? key : this.render(pattern, args);
  }

  render(pattern, args) {
    let out = '';
    let i = 0;
    while (i < pattern.length) {
      const open = pattern.indexOf('{', i);
      if (open < 0) {
        out += pattern.slice(i);
        break;
      }
      out += pattern.slice(i, open);
      const close = matching(pattern, open);
      out += this.placeable(pattern.slice(open + 1, close).trim(), args);
      i = close + 1;
    }
    return out;
  }

  placeable(inner, args) {
    const select = /^\$([\w-]+)\s*->/.exec(inner);
    if (!select) {
      const name = inner.replace(/^\$/, '');
      return name in args ? String(args[name]) : inner;
    }
    const value = args[select[1]];
    const variants = parseVariants(inner.slice(select[0].length));
    const category = typeof value === 'number' ? this.plural.select(value) : undefined;
    const chosen =
      variants.find((v) => v.key === String(value)) ??
      variants.find((v) => v.key === category) ??
      variants.find((v) => v.fallback);
    return chosen ? this.render(chosen.body, args) : '';
  }
}

// O índice do `}` que fecha o `{` em `open`.
function matching(text, open) {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === '{') depth++;
    else if (text[i] === '}' && --depth === 0) return i;
  }
  return text.length;
}

// `[chave] texto` e `*[chave] texto` no nível de fora das chaves.
function parseVariants(source) {
  const marks = [];
  let depth = 0;
  for (let i = 0; i < source.length; i++) {
    const c = source[i];
    if (c === '{') depth++;
    else if (c === '}') depth--;
    else if (depth === 0 && (c === '[' || (c === '*' && source[i + 1] === '['))) {
      const fallback = c === '*';
      const start = fallback ? i + 1 : i;
      const end = source.indexOf(']', start);
      marks.push({ at: i, key: source.slice(start + 1, end).trim(), fallback, bodyAt: end + 1 });
      i = end;
    }
  }
  return marks.map((m, n) => ({
    key: m.key,
    fallback: m.fallback,
    body: source.slice(m.bodyAt, n + 1 < marks.length ? marks[n + 1].at : undefined).trim(),
  }));
}
