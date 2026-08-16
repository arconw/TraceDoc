import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const theme = await readFile(new URL('./theme.css', import.meta.url), 'utf8');

function variable(name) {
  const match = new RegExp(`--${name}:\\s*(#[0-9a-f]{6})`, 'i').exec(theme);
  assert.ok(match, `${name} must be a six-digit hex color`);
  return match[1];
}

function luminance(hex) {
  const channels = [1, 3, 5]
    .map((index) => Number.parseInt(hex.slice(index, index + 2), 16) / 255)
    .map((channel) =>
      channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
    );
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
}

function contrast(left, right) {
  const first = luminance(left);
  const second = luminance(right);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

test('muted-dim text meets WCAG AA on every authored text surface', () => {
  const foreground = variable('color-muted-dim');
  const backgrounds = [
    'color-background',
    'color-surface',
    'color-surface-raised',
    'color-surface-hover',
    'color-selection',
    'color-map-background',
    'color-map-document',
    'color-map-document-hover',
  ];

  for (const background of backgrounds) {
    assert.ok(
      contrast(foreground, variable(background)) >= 4.5,
      `${foreground} must meet 4.5:1 on ${background}`,
    );
  }
});
