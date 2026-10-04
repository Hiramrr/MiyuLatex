import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { pages } from "./src/pages.js";

const template = readFileSync(new URL("./index.html", import.meta.url), "utf8");
const input = {};

for (const page of pages) {
  const entry = new URL(`${page.path}index.html`, import.meta.url);
  if (page.path) {
    mkdirSync(new URL(page.path, import.meta.url), { recursive: true });
    writeFileSync(entry, template);
  }
  input[page.id] = fileURLToPath(entry);
}

function redirectPageDirectories(server) {
  server.middlewares.use((request, response, next) => {
    const url = new URL(request.url, "http://localhost");
    const page = pages.find(
      (item) => item.path && url.pathname === `/${item.id}`,
    );
    if (!page) return next();
    response.writeHead(308, { Location: `/${page.path}${url.search}` });
    response.end();
  });
}

export default defineConfig({
  appType: "mpa",
  plugins: [
    svelte(),
    {
      name: "miyu-page-metadata",
      configureServer: redirectPageDirectories,
      configurePreviewServer: redirectPageDirectories,
      transformIndexHtml(html, context) {
        const page =
          pages.find((item) => context.path === `/${item.path}index.html`) ??
          pages[0];
        return html
          .replace(
            '<html lang="es">',
            `<html lang="es" data-page="${page.id}" data-root="${page.path ? "../" : "./"}">`,
          )
          .replace(/<title>[^<]*<\/title>/, `<title>${page.title}</title>`)
          .replace(
            /(name="description"\s+content=")[^"]*/,
            `$1${page.description}`,
          )
          .replace(/(property="og:title" content=")[^"]*/, `$1${page.title}`)
          .replace(
            /(property="og:description"\s+content=")[^"]*/,
            `$1${page.description}`,
          )
          .replace(
            'href="./img/icon.png"',
            `href="${page.path ? "../" : "./"}img/icon.png"`,
          );
      },
    },
  ],
  base: "./",
  build: { rolldownOptions: { input } },
});
