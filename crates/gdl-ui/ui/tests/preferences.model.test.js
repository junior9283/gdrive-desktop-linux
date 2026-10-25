import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diff, fromPrefs, gbToKb, hotkeyFrom, kbToGb, numberOrNull, proxyMode, proxyPatch, validate } from '../screens/preferences/model.js';

const GB = 1024 * 1024;

test('sem mudança, diff vazio', () => {
  const original = { UITheme: null, BandwidthRxKBPS: 100, AutoStartOnLogin: true };
  assert.deepEqual(diff(original, { ...original }), []);
});

test('diff leva só o que mudou, em JSON; null apaga', () => {
  const original = { UITheme: null, BandwidthRxKBPS: 100, AutoStartOnLogin: true };
  const edited = { ...original, UITheme: 'light', BandwidthRxKBPS: null };
  assert.deepEqual(diff(original, edited), [
    ['UITheme', '"light"'],
    ['BandwidthRxKBPS', 'null'],
  ]);
});

test('limite do cache fora de 1..máximo é inválido', () => {
  const limits = { maxCacheGb: 50 };
  assert.equal(validate('ContentCacheMaxKbytes', 10 * GB, limits), null);
  assert.equal(validate('ContentCacheMaxKbytes', null, limits), null);
  for (const bad of [0, GB / 2, 51 * GB, Number.NaN]) {
    assert.deepEqual(validate('ContentCacheMaxKbytes', bad, limits), ['pref-invalid-cache', { max: 50 }], String(bad));
  }
});

test('banda vazia vira null, que apaga a chave', () => {
  assert.equal(numberOrNull(''), null);
  assert.equal(numberOrNull('  '), null);
  assert.equal(numberOrNull('250'), 250);
  assert.equal(validate('BandwidthRxKBPS', null, {}), null);
  assert.equal(validate('BandwidthTxKBPS', 250, {}), null);
  assert.deepEqual(validate('BandwidthTxKBPS', -3, {}), ['pref-invalid-bandwidth']);
  assert.deepEqual(validate('BandwidthTxKBPS', 1.5, {}), ['pref-invalid-bandwidth']);
});

test('proxy manual precisa de esquema e servidor', () => {
  assert.equal(validate('ProxyServer', 'http://proxy:3128', {}), null);
  assert.equal(validate('ProxyServer', 'socks5h://10.0.0.1:1080', {}), null);
  assert.equal(validate('ProxyServer', null, {}), null);
  for (const bad of ['proxy:3128', 'ftp://x', 'http://']) {
    assert.deepEqual(validate('ProxyServer', bad, {}), ['pref-invalid-proxy'], bad);
  }
});

test('GB e KB', () => {
  assert.equal(gbToKb(2), 2 * GB);
  assert.equal(kbToGb(3 * GB), 3);
  assert.equal(kbToGb(null), null);
  assert.equal(gbToKb(null), null);
});

test('atalho a partir da tecla: só modificadores não vale', () => {
  assert.equal(hotkeyFrom({ ctrlKey: true, altKey: true, shiftKey: false, metaKey: false, key: 'g' }), 'Ctrl+Alt+G');
  assert.equal(hotkeyFrom({ ctrlKey: false, altKey: false, shiftKey: true, metaKey: true, key: 'F5' }), 'Shift+Super+F5');
  assert.equal(hotkeyFrom({ ctrlKey: true, altKey: false, shiftKey: false, metaKey: false, key: 'Control' }), null);
  assert.equal(hotkeyFrom({ ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, key: 'g' }), null);
  assert.equal(hotkeyFrom({ ctrlKey: true, altKey: false, shiftKey: false, metaKey: false, key: ' ' }), 'Ctrl+space');
});

test('as preferências do serviço viram valores e o que é gerenciado', () => {
  const { values, managed } = fromPrefs([
    { key: 'UITheme', value: 'null', managed: false, restart: false },
    { key: 'AutoStartOnLogin', value: 'false', managed: true, restart: false },
    { key: 'ContentCacheMaxKbytes', value: '1000', managed: false, restart: true },
  ]);
  assert.deepEqual(values, { UITheme: null, AutoStartOnLogin: false, ContentCacheMaxKbytes: 1000 });
  assert.deepEqual([...managed], ['AutoStartOnLogin']);
});

test('sem o espaço do disco, o cache só precisa de 1 GB', () => {
  assert.equal(validate('ContentCacheMaxKbytes', 900 * GB, { maxCacheGb: null }), null);
  assert.deepEqual(validate('ContentCacheMaxKbytes', 10, { maxCacheGb: null })?.[0], 'pref-invalid-cache');
});

test('proxy: o modo sai das duas chaves e volta a elas', () => {
  assert.equal(proxyMode({ DirectConnection: false, ProxyServer: null }), 'auto');
  assert.equal(proxyMode({ DirectConnection: true, ProxyServer: null }), 'direct');
  assert.equal(proxyMode({ DirectConnection: false, ProxyServer: 'http://p:1' }), 'manual');
  assert.deepEqual(proxyPatch('auto', 'http://p:1'), { DirectConnection: false, ProxyServer: null });
  assert.deepEqual(proxyPatch('direct', ''), { DirectConnection: true, ProxyServer: null });
  assert.deepEqual(proxyPatch('manual', '  http://p:1 '), { DirectConnection: false, ProxyServer: 'http://p:1' });
  assert.deepEqual(proxyPatch('manual', '  '), { DirectConnection: false, ProxyServer: null });
});
