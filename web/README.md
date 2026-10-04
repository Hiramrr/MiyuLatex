# MiyuLaTeX, página del programa

Sitio de seis páginas en Svelte 5 y Vite. El contenido describe las funciones
de la aplicación Rust de este repositorio. No necesita un servidor de aplicación.

[Ver la página publicada](https://hiramrr.github.io/MiyuLatex/).

## Desarrollo

Necesitas Node.js 22.12 o posterior y npm.

```sh
cd web
npm install
npm run dev
```

## Comprobar y construir

```sh
npm run check
npm test
npm run build
npm run preview
```

La carpeta `dist/` contiene la web lista para cualquier alojamiento estático.
Cada ruta tiene su propio `index.html`. No necesita reglas para redirigir
todas las visitas al inicio. Los enlaces relativos también permiten alojarla
dentro de una subcarpeta.

## Publicación

`.github/workflows/pages.yml` comprueba, construye y publica la página en GitHub
Pages cuando se suben cambios de `web/` a `main`. También se puede ejecutar
manualmente desde Actions. GitHub Pages debe usar GitHub Actions como origen.

La dirección pública es https://hiramrr.github.io/MiyuLatex/.

## Páginas

| Ruta | Contenido |
| --- | --- |
| `/` | Presentación breve y enlaces a las demás páginas |
| `/funciones/` | Funciones y atajos de teclado |
| `/capturas/` | Galería de las 27 capturas reales |
| `/temas/` | Selector de temas y vista previa del editor |
| `/mascotas/` | Miyu, Coco, Congo y la demo interactiva |
| `/instalar/` | Descargas, instalación y preguntas frecuentes |

`src/pages.js` define los nombres, las rutas y los metadatos. Al iniciar Vite,
`vite.config.js` genera los documentos de entrada de las páginas a partir del
`index.html` principal. Los documentos generados están excluidos de Git.
Svelte comparte la navegación, la galería y los temas entre las páginas.
`npm test` construye el sitio y comprueba también las seis páginas y sus archivos.

## Temas y capturas

`src/themes.js` conserva los nueve temas de `../src/theme.rs`.
El selector cambia los colores y la captura. Guarda la elección localmente
y permite seguir el tema del sistema.

Las 27 imágenes de `public/img/` son capturas de la aplicación real, tomadas
el 3 de octubre de 2026 con un proyecto de cálculo de ejemplo. Cada tema tiene
una captura de LaTeX con su PDF, Markdown y Python. Las capturas se hicieron
después de renderizar el PDF y cargar los iconos, con una sesión de preferencias
separada. Se usó la captura nativa de ventanas de macOS, sin incluir el cursor
ni el distintivo de control de Codex. Conservan los botones reales de cerrar,
minimizar y ampliar. Las imágenes WebP conservan su resolución de 2560 × 1640.

`src/Pets.svelte` presenta a Miyu, Coco, Congo y el jazmín. La animación de
`src/mascotas.js` reutiliza los dibujos de la página original, basados en
`../src/mascot.rs`. Respeta la reducción de movimiento y se detiene
cuando la demo queda fuera de la pantalla o la pestaña está oculta.
