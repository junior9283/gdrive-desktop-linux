import { test } from 'node:test';
import assert from 'node:assert/strict';
import { updateBanner } from '../screens/about/model.js';

const offer = (step = 'available', can_install = true) => ({ version: '9.9.9', can_install, step });

test('about_shows_update_banner', () => {
  assert.deepEqual(updateBanner(offer()), {
    text: 'about-update-available',
    version: '9.9.9',
    actions: ['update', 'release'],
  });
});

test('about_hides_banner_without_offer', () => {
  assert.equal(updateBanner({ version: '', can_install: false, step: 'available' }), null);
  assert.equal(updateBanner(null), null);
});

test('about_banner_without_install_shows_only_release_link', () => {
  assert.deepEqual(updateBanner(offer('available', false)).actions, ['release']);
});

test('about_banner_shows_step_while_installing', () => {
  const texts = ['downloading', 'waiting-password', 'installing'].map((step) => {
    const banner = updateBanner(offer(step));
    assert.deepEqual(banner.actions, [], step);
    return banner.text;
  });
  assert.deepEqual(texts, ['about-update-downloading', 'about-update-password', 'about-update-installing']);
});
