import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { appPath } from './harness.mjs';

const css = readFileSync(resolve(appPath, 'app.css'), 'utf8');
function declarations(block) {
  return Object.fromEntries([...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map(([, key, value]) => [key, value.trim()]));
}
const dark = declarations(css.match(/:root\s*\{([^}]+)\}/s)[1]);
const light = { ...dark, ...declarations(css.match(/:root\[data-theme="light"\]\s*\{([^}]+)\}/s)[1]) };

function rgb(value) {
  if (value.startsWith('#')) {
    const hex = value.slice(1);
    const full = hex.length === 3 ? [...hex].map(char => char + char).join('') : hex;
    return [0, 2, 4].map(index => parseInt(full.slice(index, index + 2), 16));
  }
  const [, red, green, blue] = value.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\)/);
  return [Number(red), Number(green), Number(blue)];
}
function alpha(value) { return Number(value.match(/rgba\([^,]+,[^,]+,[^,]+,\s*([\d.]+)\)/)[1]); }
function composite(foreground, background) {
  const front = rgb(foreground), back = rgb(background), opacity = alpha(foreground);
  return `#${front.map((channel, index) => Math.round(channel * opacity + back[index] * (1 - opacity)).toString(16).padStart(2, '0')).join('')}`;
}
function luminance(value) {
  const channels = rgb(value).map(channel => channel / 255).map(channel => channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}
const ratio = (foreground, background) => {
  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
};
const stateColors = ['success', 'warning', 'danger', 'info', 'accent'];

for (const [themeName, tokens] of [['dark', dark], ['light', light]]) {
  test(`${themeName} theme text and essential control contrast meets WCAG thresholds`, () => {
    const backgrounds = [tokens.bg, tokens.sidebar, tokens.surface, tokens.elevated];
    for (const foreground of [tokens.text, tokens['text-2'], tokens['text-3'], tokens['tok-comment'], tokens.accent]) {
      for (const background of backgrounds) assert.ok(ratio(foreground, background) >= 4.5, `${themeName}: ${foreground} on ${background} = ${ratio(foreground, background).toFixed(2)}:1`);
    }
    for (const background of [tokens.bg, tokens.surface, tokens.elevated]) {
      assert.ok(ratio(tokens['control-border'], background) >= 3, `${themeName}: control border on ${background}`);
      assert.ok(ratio(tokens.focus, background) >= 3, `${themeName}: focus outline on ${background}`);
    }
    for (const background of [tokens.primary, tokens['primary-hover']]) assert.ok(ratio('#ffffff', background) >= 4.5, `${themeName}: button label on ${background}`);
    const primaryInk = tokens['primary-ink'];
    const primarySoft = composite(tokens['primary-soft'], tokens.elevated);
    assert.ok(ratio(primaryInk, primarySoft) >= 4.5, `${themeName}: primary badge`);
    for (const name of stateColors) {
      const soft = composite(tokens[`${name}-soft`] || tokens['accent-soft'], tokens.elevated);
      assert.ok(ratio(tokens[name], soft) >= 4.5, `${themeName}: ${name} badge`);
    }
  });
}
