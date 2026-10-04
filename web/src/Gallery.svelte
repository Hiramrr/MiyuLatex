<script>
  let { theme } = $props();
  const base = document.documentElement.dataset.root ?? "./";
  const modes = [
    {
      id: "latex",
      file: "main.tex",
      label: "LaTeX",
      description: "El documento y su PDF compilado, uno al lado del otro.",
    },
    {
      id: "markdown",
      file: "notas.md",
      label: "Markdown",
      description:
        "Notas con tablas, listas de tareas y vista previa mientras escribes.",
    },
    {
      id: "codigo",
      file: "grafica.py",
      label: "Código",
      description:
        "Python con resaltado de sintaxis, guías de sangría y archivos del proyecto.",
    },
  ];
  let mode = $state("latex");
  let galleryDialog;
  let tabs;
  let currentMode = $derived(modes.find((item) => item.id === mode));
  let screenshot = $derived(`${base}img/${mode}-${theme.id}.webp?v=2`);
  function moveTab(event) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const index = modes.findIndex((item) => item.id === mode);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? modes.length - 1
          : (index + (event.key === "ArrowRight" ? 1 : -1) + modes.length) %
            modes.length;
    mode = modes[next].id;
    tabs.querySelectorAll('[role="tab"]')[next].focus();
  }
</script>

<section id="capturas" class="gallery" aria-label="Capturas de MiyuLaTeX">
  <div class="gallery-toolbar">
    <div
      class="tabs"
      role="tablist"
      aria-label="Formato del documento"
      bind:this={tabs}
    >
      {#each modes as item}<button
          role="tab"
          id={`tab-${item.id}`}
          aria-selected={mode === item.id}
          aria-controls="capture-panel"
          tabindex={mode === item.id ? 0 : -1}
          onclick={() => {
            mode = item.id;
          }}
          onkeydown={moveTab}
          ><span class="file-extension" aria-hidden="true"
            >{item.label === "Código"
              ? "py"
              : item.id === "latex"
                ? "TₑX"
                : "M↓"}</span
          >{item.file}</button
        >{/each}
    </div>
    <button class="expand-button" onclick={() => galleryDialog.showModal()}
      >Ampliar captura <span aria-hidden="true">⤢</span></button
    >
  </div>
  <div
    id="capture-panel"
    role="tabpanel"
    aria-labelledby={`tab-${mode}`}
    tabindex="0"
  >
    <figure>
      <img
        class="app-capture"
        src={screenshot}
        alt={`MiyuLaTeX abierto con ${currentMode.label} y el tema ${theme.name}`}
        width="2560"
        height="1640"
        fetchpriority="high"
      />
      <figcaption>
        <span>{currentMode.description}</span><span class="capture-theme"
          >{theme.name} · Captura real</span
        >
      </figcaption>
    </figure>
  </div>
  <div class="gallery-hint">
    <span class="status-dot" aria-hidden="true"></span> Cambia el tema de la página
    para ver el mismo tema en el editor.
  </div>
</section>

<dialog class="capture-dialog" bind:this={galleryDialog}>
  <div class="dialog-toolbar">
    <span>{currentMode.label} · {theme.name}</span><button
      onclick={() => galleryDialog.close()}>Cerrar <kbd>Esc</kbd></button
    >
  </div>
  <img
    src={screenshot}
    alt={`Captura ampliada de MiyuLaTeX con ${currentMode.label} y el tema ${theme.name}`}
    width="2560"
    height="1640"
  />
</dialog>
