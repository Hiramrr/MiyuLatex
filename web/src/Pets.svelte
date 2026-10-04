<script>
  import { onMount } from "svelte";
  import { createMascots } from "./mascotas.js";

  let { theme } = $props();
  let stage;
  let section;
  let engine = $state();
  let sleeping = $state(false);
  let busy = $state(false);
  let status = $state("Miyu, Coco y Congo están de paseo.");
  const pets = [
    {
      kind: "cat",
      name: "Miyu",
      type: "El gato",
      text: "Saca su portátil cuando escribes, espera mientras compilas y salta cuando el documento sale bien.",
    },
    {
      kind: "crab",
      name: "Coco",
      type: "El cangrejo",
      text: "Pasea, chasquea sus pinzas y de vez en cuando se acerca a saludar a Miyu.",
    },
    {
      kind: "dog",
      name: "Congo",
      type: "El perrito",
      text: "Mueve la cola, acompaña a Miyu y ladra si la compilación encuentra un error.",
    },
    {
      kind: "jasmine",
      name: "Jazmín",
      type: "La planta",
      text: "Abre sus flores cuando Miyu duerme y también celebra una compilación sin errores.",
    },
  ];
  let colors = $derived({ ...theme, muted: theme.fg });

  onMount(() => {
    engine = createMascots(
      stage,
      section.querySelectorAll("[data-sprite]"),
      colors,
      (message, asleep, compiling) => {
        status = message;
        sleeping = asleep;
        busy = compiling;
      },
    );
    return () => engine.destroy();
  });
  $effect(() => {
    engine?.setColors(colors);
  });
</script>

<section
  id="mascotas"
  class="content-section pets-section"
  aria-labelledby="pets-title"
  bind:this={section}
>
  <div class="section-intro">
    <p class="eyebrow">Miyu y compañía</p>
    <h1 id="pets-title">Miyu, Coco y Congo</h1>
    <p>
      En el editor viven sobre la barra de estado. Aquí puedes probar sus
      reacciones al escribir, compilar o dejar descansar a Miyu.
    </p>
  </div>
  <div class="pet-profiles">
    {#each pets as pet}<article>
        <canvas data-sprite={pet.kind} aria-hidden="true"></canvas>
        <div>
          <h2>{pet.name}</h2>
          <span class="small muted">{pet.type}</span>
        </div>
        <p>{pet.text}</p>
      </article>{/each}
  </div>
  <div class="pet-demo">
    <div class="demo-toolbar">
      <span>Prueba sus reacciones</span><span class="small muted"
        >Demo interactiva</span
      >
    </div>
    <div class="demo-input">
      <label for="pet-note">Escribe una nota para que Miyu teclee contigo</label
      ><input
        id="pet-note"
        type="text"
        placeholder="Hoy por fin terminé ese capítulo…"
        oninput={() => engine?.wake()}
      />
    </div>
    <canvas class="pet-stage" bind:this={stage} aria-hidden="true"></canvas>
    <div class="demo-bottom">
      <div class="pet-actions">
        <button onclick={() => engine?.run("success")} disabled={busy}
          >Probar compilación</button
        ><button onclick={() => engine?.run("error")} disabled={busy}
          >Probar un error</button
        ><button onclick={() => engine?.run("sleep")} aria-pressed={sleeping}
          >{sleeping ? "Despertar a Miyu" : "Dormir a Miyu"}</button
        >
      </div>
      <p class="small muted" role="status">{status}</p>
    </div>
  </div>
  <p class="small muted pet-note">
    Puedes ocultar a cada uno desde Ver o Apariencia en el programa.
  </p>
</section>
