<script>
  import { onMount } from "svelte";
  import { themes, resolveTheme } from "./themes.js";
  import Pets from "./Pets.svelte";
  import Gallery from "./Gallery.svelte";
  import { pages } from "./pages.js";

  const page =
    pages.find((item) => item.id === document.documentElement.dataset.page) ??
    pages[0];
  const base = document.documentElement.dataset.root ?? "./";
  const repository = "https://github.com/Hiramrr/MiyuLatex";
  const features = [
    {
      title: "Del código a la página",
      text: "Compila con Tectonic, latexmk, pdfLaTeX, XeLaTeX o LuaLaTeX. SyncTeX lleva del código al PDF y del PDF a su línea de origen.",
      detail: "Los errores abren el archivo en la línea que necesita atención.",
      code: "\\begin{equation} … \\end{equation}",
    },
    {
      title: "Consulta el PDF sin salir del editor",
      text: "Busca texto, copia una selección y ajusta el zoom con el trackpad o la rueda. Las páginas se recorren en una columna y conservan su posición al recompilar.",
      detail:
        "La vista previa de ecuaciones muestra la fórmula que rodea al cursor.",
      code: "PDF    Buscar    Ajustar al ancho",
    },
    {
      title: "Referencias sin perder el hilo",
      text: "Autocompleta etiquetas y citas. Importa una entrada BibTeX con su DOI o arXiv y revisa claves repetidas, campos que faltan y referencias sin resolver.",
      detail: "Renombra una etiqueta en todo el proyecto.",
      code: "\\cite{…}    \\ref{…}",
    },
    {
      title: "Un editor para el proyecto entero",
      text: "Edita con varios cursores, pliega bloques y divide el editor para trabajar en dos archivos. Busca en el proyecto y recupera hasta 100 versiones por archivo. Arrastra o pega imágenes para insertarlas como figuras.",
      detail:
        "Incluye ortografía, marcas de Git e importación y exportación ZIP compatible con Overleaf.",
      code: "main.tex    referencias.bib    capítulos/",
    },
    {
      title: "También notas y código",
      text: "Markdown tiene vista previa. Rust, Python, JavaScript, TypeScript, C, C++, Go y otros lenguajes tienen resaltado. La terminal abre la shell en tu proyecto.",
      detail:
        "Ejecuta, comprueba y prueba código con las herramientas instaladas.",
      code: "cargo run    npm run dev    python main.py",
    },
  ];
  const shortcuts = [
    ["F5", "Guardar y compilar"],
    ["MOD+Shift+P", "Abrir la paleta de comandos"],
    ["MOD+Shift+J", "Mostrar esta línea en el PDF"],
    ["MOD+Shift+M", "Ver la ecuación del cursor"],
    ["F8", "Ir al siguiente problema"],
    ["MOD+Shift+F", "Buscar en el proyecto"],
    ["MOD+D", "Añadir la siguiente aparición"],
    ["MOD+\\", "Dividir el editor"],
  ];

  let themeChoice = $state("system");
  let systemDark = $state(false);
  let ready = $state(false);
  let platform = $state("macos");
  let copied = $state(false);
  let copyMessage = $state("");
  let copyTimer;
  let theme = $derived(resolveTheme(themeChoice, systemDark));
  let screenshot = $derived(`${base}img/latex-${theme.id}.webp?v=2`);
  let command = $derived(
    platform === "macos"
      ? "brew install tectonic"
      : "git clone https://github.com/Hiramrr/MiyuLatex\ncd MiyuLatex\ncargo run --release",
  );

  onMount(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    systemDark = media.matches;
    try {
      const saved = localStorage.getItem("miyu-theme");
      if (saved === "system" || themes.some((item) => item.id === saved))
        themeChoice = saved;
    } catch {
      /* La página también funciona sin almacenamiento local. */
    }
    if (!/Mac|iPhone|iPad/.test(navigator.userAgent)) platform = "linux";
    ready = true;
    const update = (event) => {
      systemDark = event.matches;
    };
    media.addEventListener("change", update);
    return () => {
      media.removeEventListener("change", update);
      clearTimeout(copyTimer);
    };
  });

  $effect(() => {
    for (const color of [
      "primary",
      "secondary",
      "accent",
      "fg",
      "bg",
      "surface",
      "panel",
      "border",
      "success",
      "error",
    ]) {
      document.documentElement.style.setProperty(`--${color}`, theme[color]);
    }
    document.documentElement.style.colorScheme = theme.dark ? "dark" : "light";
    document.querySelector('meta[name="theme-color"]').content = theme.bg;
    if (ready) {
      try {
        localStorage.setItem("miyu-theme", themeChoice);
      } catch {
        /* Tema válido para esta visita. */
      }
    }
  });

  async function copyCommand() {
    clearTimeout(copyTimer);
    try {
      await navigator.clipboard.writeText(command);
      copied = true;
      copyMessage = "Comando copiado.";
      copyTimer = setTimeout(() => {
        copied = false;
        copyMessage = "";
      }, 2500);
    } catch {
      copyMessage = "Selecciona el comando para copiarlo.";
    }
  }
</script>

<a class="skip-link" href="#contenido">Ir al contenido</a>
<header class="site-header">
  <div class="header-inner">
    <a class="brand" href={base} aria-label="MiyuLaTeX, inicio"
      ><img src={`${base}img/icon.png`} alt="" width="32" height="32" /><span
        >MiyuLaTeX</span
      ></a
    >
    <nav aria-label="Navegación principal">
      {#each pages.filter((item) => item.id !== "inicio") as item}
        <a
          href={`${base}${item.path}`}
          aria-current={page.id === item.id ? "page" : undefined}>{item.name}</a
        >
      {/each}
    </nav>
    <label class="header-theme"
      ><span>Tema</span><select
        aria-label="Tema de la página"
        bind:value={themeChoice}
        ><option value="system">Sistema</option>{#each themes as item}<option
            value={item.id}>{item.name}</option
          >{/each}</select
      ></label
    >
  </div>
</header>

<main id="contenido">
  {#if page.id === "inicio"}
    <section class="hero" aria-labelledby="page-title">
      <div class="hero-copy">
        <p class="eyebrow">Un editor de escritorio, escrito en Rust</p>
        <h1 id="page-title">MiyuLaTeX</h1>
        <p class="hero-lead">LaTeX con el PDF al lado.</p>
        <p class="hero-description">
          Escribe un artículo, organiza tu tesis o toma apuntes. Compila tu
          documento, consulta el resultado y vuelve a la línea que quieres
          cambiar.
        </p>
        <div class="actions">
          <a class="button primary" href={`${base}instalar/`}
            >Descargar MiyuLaTeX <span aria-hidden="true">↓</span></a
          ><a class="text-link" href={repository}
            >Ver el código en GitHub <span aria-hidden="true">↗</span></a
          >
        </div>
        <p class="small muted">
          macOS con Apple Silicon y Linux · Código abierto · Licencia MIT
        </p>
      </div>
      <figure class="home-preview">
        <a href={`${base}capturas/`} aria-label="Ver las capturas del programa">
          <img
            class="app-capture"
            src={screenshot}
            alt={`MiyuLaTeX con LaTeX y su PDF en el tema ${theme.name}`}
            width="2560"
            height="1640"
            fetchpriority="high"
          />
        </a>
        <figcaption>
          El código y el PDF en la misma ventana. <a href={`${base}capturas/`}
            >Ver capturas</a
          >
        </figcaption>
      </figure>
    </section>

    <section class="home-overview" aria-labelledby="explore-title">
      <h2 id="explore-title">Explora MiyuLaTeX</h2>
      <ul class="page-links">
        {#each pages.filter((item) => item.id !== "inicio") as item}
          <li>
            <a href={`${base}${item.path}`}
              ><span>{item.name}</span>
              <p>{item.description}</p>
              <span aria-hidden="true">→</span></a
            >
          </li>
        {/each}
      </ul>
    </section>
  {:else if page.id === "funciones"}
    <section
      id="funciones"
      class="content-section"
      aria-labelledby="features-title"
    >
      <div class="section-intro">
        <p class="eyebrow">Para escribir y revisar</p>
        <h1 id="features-title">Funciones para tu proyecto</h1>
        <p>
          El archivo principal, la bibliografía, tus notas y el PDF forman parte
          del mismo proyecto.
        </p>
      </div>
      <div class="feature-list">
        {#each features as feature, index}<article>
            <span class="feature-number" aria-hidden="true">0{index + 1}</span>
            <div>
              <h2>{feature.title}</h2>
              <p>{feature.text}</p>
              <p class="small muted">{feature.detail}</p>
            </div>
            <code class="feature-code">{feature.code}</code>
          </article>{/each}
      </div>
    </section>

    <section
      id="atajos"
      class="content-section shortcuts-section"
      aria-labelledby="shortcuts-title"
    >
      <div class="section-intro">
        <p class="eyebrow">A mano en el teclado</p>
        <h2 id="shortcuts-title">Menos clics entre el código y el PDF</h2>
        <p>
          La paleta de comandos reúne las acciones y muestra el atajo de cada
          una.
        </p>
        <div
          class="platform-switch"
          role="group"
          aria-label="Plataforma para los atajos y la instalación"
        >
          <button
            aria-pressed={platform === "macos"}
            onclick={() => {
              platform = "macos";
              copied = false;
              copyMessage = "";
            }}>macOS</button
          ><button
            aria-pressed={platform === "linux"}
            onclick={() => {
              platform = "linux";
              copied = false;
              copyMessage = "";
            }}>Linux</button
          >
        </div>
      </div>
      <table class="shortcuts">
        <caption class="sr-only"
          >Atajos de MiyuLaTeX para {platform === "macos"
            ? "macOS"
            : "Linux"}</caption
        ><thead
          ><tr><th scope="col">Atajo</th><th scope="col">Acción</th></tr></thead
        ><tbody
          >{#each shortcuts as [key, action]}<tr
              ><td
                ><kbd
                  >{key.replace(
                    "MOD",
                    platform === "macos" ? "Cmd" : "Ctrl",
                  )}</kbd
                ></td
              ><td>{action}</td></tr
            >{/each}</tbody
        >
      </table>
    </section>
  {:else if page.id === "capturas"}
    <section class="page-intro" aria-labelledby="captures-title">
      <p class="eyebrow">La aplicación en uso</p>
      <h1 id="captures-title">Capturas del programa</h1>
      <p>
        Explora LaTeX, Markdown y Python. El selector de tema cambia los colores
        de la página y la captura de la aplicación.
      </p>
    </section>
    <Gallery {theme} />
    <p class="page-note small muted">
      27 capturas reales. Tres formatos en cada uno de los nueve temas.
    </p>
  {:else if page.id === "temas"}
    <section
      id="temas"
      class="content-section themes-section"
      aria-labelledby="themes-title"
    >
      <div class="section-intro">
        <p class="eyebrow">Los colores del programa</p>
        <h1 id="themes-title">Encuentra tu tema</h1>
        <p>
          Estas son las nueve paletas de MiyuLaTeX. Elige una para probarla en
          la página y en las capturas.
        </p>
      </div>
      <div class="theme-list" role="group" aria-label="Elegir un tema">
        {#each themes as item}<button
            class="theme-option"
            aria-pressed={theme.id === item.id}
            onclick={() => {
              themeChoice = item.id;
            }}
            ><span
              class="theme-swatch"
              style={`--swatch-bg:${item.bg};--swatch-border:${item.border}`}
              aria-hidden="true"
              ><i style={`background:${item.primary}`}></i><i
                style={`background:${item.secondary}`}
              ></i><i style={`background:${item.accent}`}></i></span
            ><span
              >{item.name}<small>{item.dark ? "Oscuro" : "Claro"}</small></span
            ><span class="theme-check" aria-hidden="true"
              >{theme.id === item.id ? "✓" : ""}</span
            ></button
          >{/each}
      </div>
      <div class="theme-details">
        <p class="small muted">
          En el programa también puedes cambiar la fuente, ajustar los colores y
          usar una foto como fondo liso o tramado.
        </p>
        <button
          class="text-button"
          onclick={() => {
            themeChoice = "system";
          }}
          >Usar el tema del sistema{themeChoice === "system"
            ? " ✓"
            : ""}</button
        >
      </div>
    </section>

    <section class="theme-preview" aria-label="Vista previa del tema elegido">
      <h2>Así se ve {theme.name} en el editor</h2>
      <Gallery {theme} />
    </section>
  {:else if page.id === "mascotas"}
    <Pets {theme} />
  {:else if page.id === "instalar"}
    <section
      id="instalar"
      class="content-section install-section"
      aria-labelledby="install-title"
    >
      <div class="section-intro">
        <p class="eyebrow">Empieza con un archivo</p>
        <h1 id="install-title">Instala MiyuLaTeX</h1>
        <p>
          Para editar Markdown y código puedes empezar directamente. Para
          compilar LaTeX necesitas un motor instalado.
        </p>
        <a class="button primary" href={`${repository}/releases`}
          >Ver descargas <span aria-hidden="true">↗</span></a
        >
      </div>
      <div class="install-steps">
        <div
          class="platform-switch"
          role="group"
          aria-label="Plataforma de instalación"
        >
          <button
            aria-pressed={platform === "macos"}
            onclick={() => {
              platform = "macos";
              copied = false;
              copyMessage = "";
            }}>macOS</button
          ><button
            aria-pressed={platform === "linux"}
            onclick={() => {
              platform = "linux";
              copied = false;
              copyMessage = "";
            }}>Linux</button
          >
        </div>
        <ol>
          <li>
            <h2>Descarga el programa</h2>
            <p>
              {platform === "macos"
                ? "Busca el archivo de macOS para Apple Silicon en GitHub Releases. Descomprime y mueve MiyuLaTeX a Aplicaciones."
                : "Busca el archivo de Linux x86_64 en GitHub Releases, descomprímelo y ejecuta el binario miyu. También puedes compilarlo desde el código con Rust."}
            </p>
            {#if platform === "macos"}<p class="small muted">
                La app no tiene certificado de Apple. La primera apertura se
                hace con clic derecho y Abrir.
              </p>{/if}
          </li>
          <li>
            <h2>
              {platform === "macos"
                ? "Instala un motor LaTeX"
                : "Elige cómo compilar"}
            </h2>
            <p>
              {platform === "macos"
                ? "Si usas Homebrew, puedes instalar Tectonic con este comando. Si ya tienes una distribución LaTeX, elige su motor en Preferencias."
                : "Miyu detecta Tectonic, latexmk, pdfLaTeX, XeLaTeX y LuaLaTeX. Instala uno con las instrucciones de tu distribución. Para construir el editor desde el código, ejecuta lo siguiente."}
            </p>
            <div class="command">
              <pre><code>{command}</code></pre>
              <button
                onclick={copyCommand}
                aria-label="Copiar comando de instalación"
                >{copied ? "Copiado" : "Copiar"}</button
              >
            </div>
            <p class="copy-status small" role="status">{copyMessage}</p>
          </li>
          <li>
            <h2>Abre tu documento</h2>
            <p>
              Abre un archivo o una carpeta de proyecto. En LaTeX, pulsa <kbd
                >F5</kbd
              > para guardar y compilar. El PDF aparece en el panel de vista previa.
            </p>
          </li>
        </ol>
      </div>
    </section>

    <section class="faq content-section" aria-labelledby="faq-title">
      <h2 id="faq-title">Antes de empezar</h2>
      <details>
        <summary>¿Puedo traer un proyecto de Overleaf?</summary>
        <p>
          Sí. Exporta el proyecto como ZIP desde Overleaf e impórtalo con el
          menú Proyecto de MiyuLaTeX. También puedes abrir una carpeta que ya
          tenga tus archivos.
        </p>
      </details>
      <details>
        <summary>¿Los documentos se guardan en mi equipo?</summary>
        <p>
          Sí. MiyuLaTeX trabaja con archivos locales. La importación de
          bibliografía por DOI o arXiv consulta esos servicios cuando la usas.
          Tectonic puede descargar los paquetes que necesite la primera vez.
        </p>
      </details>
      <details>
        <summary>¿Necesito Python o Electron para usarlo?</summary>
        <p>
          La aplicación actual es un binario Rust con egui. Para ejecutar código
          necesitas las herramientas de su lenguaje, y para compilar LaTeX
          necesitas un motor.
        </p>
      </details>
      <details>
        <summary
          >¿Dónde consulto la documentación o reporto un problema?</summary
        >
        <p>
          El <a href={`${repository}#readme`}>README del proyecto</a> explica
          los ajustes, los motores y los atajos. Puedes reportar un problema en
          <a href={`${repository}/issues`}>GitHub Issues</a>.
        </p>
      </details>
    </section>
  {/if}
</main>

<footer class="site-footer">
  <a class="brand" href={base}>MiyuLaTeX</a><span
    >Hecho por Hiram · Rust + egui</span
  >
  <div>
    <a href={repository}>GitHub</a><a href={`${repository}/blob/main/LICENSE`}
      >Licencia MIT</a
    >
  </div>
</footer>
