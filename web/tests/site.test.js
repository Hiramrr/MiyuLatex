import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { themes, resolveTheme } from "../src/themes.js";
import { pages } from "../src/pages.js";

const rgb = (hex) =>
  hex.match(/[\da-f]{2}/gi).map((channel) => parseInt(channel, 16));
const mix = (a, b, amount) =>
  rgb(a).map((channel, i) => channel * amount + rgb(b)[i] * (1 - amount));
const luminance = (channels) =>
  channels
    .map((channel) => {
      const value = channel / 255;
      return value <= 0.04045
        ? value / 12.92
        : ((value + 0.055) / 1.055) ** 2.4;
    })
    .reduce((sum, value, i) => sum + value * [0.2126, 0.7152, 0.0722][i], 0);
const contrast = (a, b) =>
  (Math.max(luminance(a), luminance(b)) + 0.05) /
  (Math.min(luminance(a), luminance(b)) + 0.05);

test("cada tema tiene capturas reales de los tres formatos y texto legible", async () => {
  assert.equal(new Set(themes.map((theme) => theme.id)).size, 9);
  for (const theme of themes) {
    assert.equal(resolveTheme(theme.id, !theme.dark), theme);
    assert.ok(
      contrast(mix(theme.fg, theme.bg, 0.76), rgb(theme.bg)) >= 4.5,
      `${theme.name}: texto secundario`,
    );
    assert.ok(
      contrast(mix(theme.primary, theme.fg, 0.7), rgb(theme.bg)) >= 4.5,
      `${theme.name}: enlaces y botones`,
    );
    for (const format of ["latex", "markdown", "codigo"]) {
      const capture = await readFile(
        new URL(`../public/img/${format}-${theme.id}.webp`, import.meta.url),
      );
      assert.equal(capture.toString("ascii", 8, 12), "WEBP");
      assert.ok(capture.length > 10000, `${theme.name}: ${format} está vacío`);
    }
  }
  assert.equal(resolveTheme("system", true).id, "miyu-noche");
  assert.equal(resolveTheme("system", false).id, "miyu-dia");
  assert.equal(resolveTheme("tema-inexistente", false).id, "miyu-dia");
});

test("las seis páginas se generan con sus propios títulos y rutas válidas a los archivos", async () => {
  assert.equal(new Set(pages.map((page) => page.path)).size, 6);
  for (const page of pages) {
    const entry = new URL(`../dist/${page.path}index.html`, import.meta.url);
    const html = await readFile(entry, "utf8");
    assert.ok(html.includes(`data-page="${page.id}"`), page.id);
    assert.ok(html.includes(`<title>${page.title}</title>`), page.id);
    assert.ok(html.includes(`content="${page.description}"`), page.id);
    assert.ok(
      html.includes(`data-root="${page.path ? "../" : "./"}"`),
      page.id,
    );
    const urls = [...html.matchAll(/(?:src|href)="([^"#]+)"/g)].map(
      (match) => match[1],
    );
    for (const url of urls.filter((url) => !url.startsWith("http"))) {
      const asset = await readFile(new URL(url, entry));
      assert.ok(asset.length > 0, `${page.id}: ${url}`);
    }
  }
});
