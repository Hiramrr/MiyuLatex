# MiyuLaTeX

Editor de LaTeX, Markdown y código escrito en Rust. Abre una ventana gráfica
con egui, controles con la fuente del sistema, código monoespaciado y bordes finos. Incluye
compilación LaTeX, vista previa de Markdown y un visor de PDF e imágenes.

![Ventana de MiyuLaTeX](docs/captura-rust.png)

## Ejecutar

```sh
cargo run --release
cargo run --release -- examples/articulo.tex
cargo run --release -- README.md
cargo run --release -- src/main.rs
cargo run --release -- examples/articulo.pdf
```

Estos comandos abren una ventana. También puedes crear la app macOS y abrirla
desde Finder, sin usar una terminal:

```sh
sh scripts/package-macos.sh
open dist/MiyuLaTeX.app
```

El icono original está en `assets/MiyuTeX.icon`. El empaquetado usa `actool`
de Xcode para compilarlo con sus apariencias nativas de macOS y conserva
`assets/icon.icns` como alternativa. `assets/icon.png` se usa al ejecutar con
Cargo y en otras plataformas. Puedes copiar `dist/MiyuLaTeX.app` a Aplicaciones.

Al publicar una etiqueta `vX.Y.Z`, el flujo `release.yml` de GitHub Actions
compila la app de macOS (Apple Silicon) y el binario de Linux y los adjunta a
la versión. La app va firmada solo de forma local, sin certificado de Apple:
la primera vez hay que abrirla con clic derecho y Abrir.

Puedes descargar la app en [Releases](https://github.com/Hiramrr/MiyuLatex/releases).
Para publicar otra versión, cambia `version` en `Cargo.toml`, guarda los cambios
en un commit y sube la etiqueta correspondiente:

```sh
git tag vX.Y.Z
git push origin main vX.Y.Z
```

Para compilar documentos LaTeX necesitas un motor LaTeX. Miyu detecta `tectonic`, `latexmk`, `pdflatex`,
`xelatex` y `lualatex`, en ese orden. Puedes elegir uno en Preferencias.

```sh
brew install tectonic
```

Para `biblatex` con `backend=biber`, Tectonic 0.17 exige biber 2.17, que
corresponde a su biblatex 3.17. El de Homebrew es más nuevo y no sirve. Pon el
binario de la [versión 2.17](https://sourceforge.net/projects/biblatex-biber/files/biblatex-biber/2.17/binaries/)
en `~/.config/miyulatex/bin/biber`. Miyu busca ahí antes que en el `PATH`. En
macOS reciente el binario universal no arranca; extrae tu arquitectura con
`lipo biber -thin arm64 -output biber`.

Con Tectonic, Miyu lee el registro de LaTeX y el de biber para mostrar las
citas y referencias sin resolver, con el archivo y la línea donde aparecen.

## Uso

Nuevo documento abre un archivo sin guardar. Nuevo proyecto crea una carpeta,
y Abrir proyecto permite elegir una carpeta existente. Abrir archivo acepta
texto, código, PDF e imágenes. También puedes arrastrarlos a la ventana.
Si el documento activo es LaTeX o Markdown y ya está guardado, una imagen
arrastrada se inserta en el cursor como figura o como imagen de Markdown, y
se copia a `images/` cuando está fuera de la carpeta del documento. Pegar
imagen del portapapeles, en Insertar, hace lo mismo con una imagen copiada,
por ejemplo una captura de pantalla; `Cmd+V` también la pega cuando el
portapapeles no tiene texto.
Los botones explican su acción al pasar el cursor. Las acciones que requieren
un documento editable, un PDF o una compilación en curso se desactivan cuando
no están disponibles. Los archivos y el esquema están a la izquierda,
las pestañas del editor en el centro y la vista previa a la derecha. El menú Ver
permite ocultar cada panel o pasarlo al otro lado de la ventana. El botón Panel
muestra u oculta el panel lateral y señala si está abierto. En macOS, los menús
están en la barra del sistema. La barra de título reúne el proyecto, el panel
lateral y las acciones de guardar, compilar y abrir la paleta. En ventanas
estrechas, Guardar sigue disponible en Archivo y con `Cmd+S`. En otras
plataformas, los menús están dentro de la ventana. El menú Proyecto reúne la
creación, apertura e intercambio de proyectos. Los problemas de compilación
abren el archivo en su línea.

La paleta de comandos (`Cmd+Shift+P`) reúne las acciones de los menús. Busca
por cualquier parte del nombre, con o sin tildes, y muestra el atajo de cada
una. Las acciones que no están disponibles quedan al final, desactivadas.

Nuevo proyecto, en el menú Proyecto, crea una carpeta
con el nombre y la ubicación que elijas. Puedes dejarla vacía, usar una
plantilla LaTeX o crear un archivo de código vacío para Python, Rust,
JavaScript, TypeScript, C, C++ o Go. El proyecto se abre al crearlo y aparece
en los recientes. Las pestañas abiertas se conservan. Si la carpeta ya existe,
el diálogo pide otro nombre.

Markdown tiene vista previa mientras escribes, tablas, listas de tareas,
enlaces, imágenes locales y bloques de código. El esquema muestra los títulos
y permite saltar a ellos. Los enlaces a otros archivos locales abren una
pestaña. `Cmd+B` y `Cmd+I` insertan el formato de Markdown. Enter continúa las
listas y deja una casilla nueva sin marcar.

Los archivos de código usan las gramáticas de Syntect para resaltado, más
las de [two-face](https://crates.io/crates/two-face). Incluye Rust, Python,
JavaScript, TypeScript, C, C++, Java, Kotlin, Swift, Go, SQL, HTML, CSS, JSON,
TOML y YAML, entre otros. `Cmd+/` usa los comentarios del lenguaje cuando tiene un marcador
definido. JSON no admite comentarios. Los archivos UTF-8 sin una gramática
conocida se editan como texto. Guardar como conserva la extensión y cambia el
resaltado según el nuevo nombre. Se conservan los saltos de línea de Windows.

El árbol de archivos muestra logos por formato y lenguaje, con variantes para
temas claros. Reconoce también nombres como `Dockerfile`, `Cargo.toml` y
`CMakeLists.txt`, y extensiones compuestas como `.d.ts`. Los iconos de
[Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
están incluidos en la aplicación. Su licencia está en `assets/file-icons-LICENSE.txt`.

El editor detecta la sangría del archivo y la usa al pulsar Tab o Enter.
Mayús+Tab quita un nivel. Las guías de sangría se activan en Preferencias.
El esquema lista funciones, clases y tipos. El completado es local y depende
del lenguaje: ofrece las palabras del documento (las más cercanas primero),
las palabras clave y la biblioteca que traen las gramáticas del resaltado, los
miembros habituales tras un punto y plantillas como `for`, `main` o `class`.
Las flechas eligen una sugerencia, Tab la inserta y Esc cierra la lista;
Ctrl+Espacio la abre sin haber escrito nada. Copiar o cortar sin selección toma la línea
entera. Deshacer agrupa las letras escritas seguidas.

Dividir el editor (`Cmd+\`), en Ver, muestra dos documentos uno junto al
otro, por ejemplo un capítulo y la bibliografía. Un clic en un panel lo
activa: el teclado, la búsqueda y la vista previa siguen al panel activo, y
elegir otra pestaña cambia su documento. La pestaña del otro panel queda
subrayada en gris. La ortografía solo se revisa en el panel activo.

`F9` pliega el bloque del cursor bajo su primera línea y lo vuelve a mostrar:
una sección de LaTeX o Markdown, un entorno `\begin…\end` o un bloque de
código con más sangría. También sirve el triángulo que aparece en el margen
al pasar el puntero. Una línea plegada lleva la marca `⋯`. Las flechas saltan
lo oculto, y buscar, ir a una línea o abrir un problema despliegan lo que
haga falta. Ver tiene Plegar todo y Desplegar todo (`Mayús+F9`). Los pliegues
no se guardan al cerrar el documento.

El editor admite varios cursores. `Cmd+D` selecciona la palabra y, al
repetirlo, añade su siguiente aparición; `Cmd+Alt+↑` y `Cmd+Alt+↓` añaden un
cursor en la línea de arriba o de abajo, y `Alt`+clic lo pone donde señales.
Lo que escribas, pegues o borres se aplica en todos, igual que las flechas,
Inicio y Fin; copiar toma todas las selecciones, una por línea. Esc o un clic
normal vuelven a un solo cursor. Con varios cursores no se cierran pares ni
se ofrecen sugerencias.

Formatear documento (`Cmd+Shift+I`), en Editar, pasa el texto por la
herramienta del lenguaje si está instalada: `latexindent` para LaTeX,
`rustfmt`, `gofmt`, `ruff` para Python, `clang-format` para C, C++ y Java, y
`prettier` para JavaScript, TypeScript, JSON, CSS, HTML, Markdown y YAML. El
cambio queda en el editor sin guardar y se deshace en un paso.

Buscar resalta las coincidencias y muestra la posición actual. Enter y
Mayús+Enter avanzan o retroceden desde el campo de búsqueda, y Esc lo cierra.
`Aa` distingue mayúsculas, `ab` busca palabras completas y `.*` permite
expresiones regulares. Sin `Aa`, escribir una mayúscula también hace la
búsqueda exacta. Reemplazar coincidencia cambia solo la coincidencia seleccionada y avanza.
Reemplazar todas cambia todas las coincidencias del documento activo y permite
deshacerlas en un paso. Ambos botones se desactivan si no hay coincidencias.

Los PDF se abren en pestañas de solo lectura. Las páginas van seguidas en una
columna que se recorre con la rueda o el trackpad. Una sola barra reúne el
número de página, el menú de zoom, la lupa de búsqueda y el menú de acciones.
Este último permite abrir el visor externo, exportar, recargar y mostrar la
línea del cursor. El zoom también se ajusta con el gesto de pellizco o con
`Cmd` y la rueda. Ajustar al ancho, dentro del menú de zoom, vuelve al
ancho del panel. Cada página se rasteriza a la resolución de la pantalla y solo
cuando se ve. Al recompilar se conserva la posición. Cada pestaña conserva su
página y su zoom.

La lupa abre el campo Buscar del visor, que resalta las coincidencias en todas las páginas, sin
distinguir mayúsculas ni tildes. Enter y Mayús+Enter van a la siguiente o a la anterior.
En una pestaña de PDF, `Cmd+F` lleva al campo. Una palabra partida con guion
al final de una línea se encuentra entera. Arrastrar sobre una página
selecciona su texto, y `Cmd+C` o el clic derecho lo copian. La selección no
pasa de una página a otra. Los PDF escaneados sin capa de texto no tienen
nada que buscar.

También puedes ver PNG, JPEG, WebP, GIF y BMP. La compilación automática
solo se aplica a LaTeX. PDF e imágenes no pasan por el guardado de texto.

El diálogo Nuevo documento permite crear LaTeX, bibliografía, Markdown, texto,
Python, Rust, JavaScript, TypeScript, C, C++ y Go, además de las plantillas LaTeX.
Abre y guarda otros lenguajes con su extensión.

| Tecla en macOS | Acción |
| --- | --- |
| `Cmd+N` | Nuevo documento desde plantilla |
| `Cmd+Shift+N` | Crear una carpeta de proyecto |
| `Cmd+O` | Abrir archivo con el diálogo del sistema |
| `Cmd+S` / `Cmd+Shift+S` | Guardar / guardar como |
| `Cmd+W` / `Ctrl+Tab` | Cerrar / cambiar pestaña |
| `F5` / `Cmd+R` | Guardar los documentos LaTeX y compilar |
| `F6` | Abrir el PDF en el visor del sistema |
| `F2` / `F3` / `F4` | Mostrar archivos, vista previa o problemas |
| `Cmd+\` | Dividir el editor en dos paneles |
| `F9` / `Shift+F9` | Plegar o desplegar el bloque del cursor / desplegar todo |
| `Cmd+F` / `Cmd+G` | Buscar y reemplazar / ir a línea |
| `Cmd+Shift+F` | Buscar en el proyecto |
| `Cmd+Shift+O` | Abrir rápido un archivo del proyecto |
| `Cmd+Shift+P` | Paleta de comandos |
| `F8` / `Shift+F8` | Problema siguiente / anterior de la compilación |
| `F12` / `Cmd`+clic | Ir a la etiqueta, la cita o el archivo señalado |
| `Cmd+Shift+J` | Mostrar en el PDF la línea del cursor |
| `Cmd+Shift+M` | Vista previa de la ecuación del cursor |
| `Cmd+T` | Insertar un símbolo LaTeX |
| `Cmd+B` / `Cmd+I` / `Cmd+/` | Negrita, cursiva o comentar líneas |
| `Cmd+Shift+I` | Formatear el documento |
| `Tab` / `Shift+Tab` | Completar o añadir sangría / quitar sangría |
| `Ctrl+Espacio` | Mostrar sugerencias |
| `Alt+↑` / `Alt+↓` | Subir / bajar las líneas seleccionadas |
| `Cmd+Shift+D` / `Alt+Shift+↓` | Duplicar las líneas seleccionadas |
| `Cmd+Shift+K` / `Cmd+L` | Borrar / seleccionar líneas enteras |
| `Cmd+Enter` / `Cmd+Shift+Enter` | Abrir una línea debajo / encima |
| `Cmd+D` | Seleccionar la palabra y añadir su siguiente aparición |
| `Cmd+Alt+↑` / `Cmd+Alt+↓` / `Alt`+clic | Añadir un cursor |
| `Cmd+Shift+\` / `Ctrl+M` | Ir al corchete emparejado |
| `Cmd+Z` / `Cmd+Shift+Z` | Deshacer / rehacer |
| `Cmd+C` / `Cmd+X` / `Cmd+V` | Copiar, cortar y pegar |
| `Cmd+P` / `Cmd+,` | Preferencias |
| `F1` / `Cmd+Q` | Ayuda / salir |

En otras plataformas, usa `Ctrl` en lugar de `Cmd`.

En archivos LaTeX, el editor resalta la sintaxis, cierra pares y conserva la sangría. Autocompleta 186
comandos, 37 entornos, etiquetas y citas. `Tab` acepta una sugerencia.
`\begin{` permite insertar el cuerpo y el cierre del entorno. Hay seis
plantillas y 101 símbolos en los catálogos integrados.

La compilación y el renderizado del PDF corren en hilos separados. Hay un
límite de 240 segundos para compilar. Se respeta `% !TEX root = ../main.tex`.
El guardado escribe en un archivo temporal y lo renombra al terminar. Si otro
programa cambia el archivo, Miyu rechaza el guardado para evitar sobrescribirlo.
Al cerrar un documento modificado, pide guardar o descartar los cambios.

## Código y terminal

Terminal, en la barra superior o en el menú Desarrollo, abre la shell del sistema
en la carpeta del proyecto. `Ctrl` y la tecla de acento grave la muestran u
ocultan. Con Mayús se abre otra sesión. El panel inferior se puede ajustar y
comparte espacio con Problemas y Registro. Ocultarlo conserva los procesos.
Cada sesión mantiene su carpeta inicial aunque cambies de proyecto.

La terminal admite colores ANSI, entrada interactiva y 5000 líneas de historial.
La rueda y Mayús+RePág recorren el historial; escribir vuelve al final.
Arrastra para seleccionar texto. Copia con `Cmd+C` en macOS o `Ctrl+Shift+C`
en Linux y Windows, y pega con `Cmd+V` o `Ctrl+Shift+V`. `Ctrl+C` o Interrumpir,
en Sesión, detienen el programa activo. Limpiar borra la pantalla y el historial.
Cerrar una sesión detiene su proceso activo; salir cierra todas las terminales.

En código, `F5` o `Cmd+R` guardan los cambios y ejecutan la tarea detectada.
Desarrollo también permite ejecutar pruebas con `Cmd+Shift+U` y comprobar código
con `Cmd+Shift+B`. En otras plataformas, usa `Ctrl` en lugar de `Cmd`.
Cada tarea abre una terminal propia y muestra su código de salida al terminar.
La lista del panel permite elegir otra tarea y consultar su comando.

Miyu reconoce `cargo run/test/check/build`, `go run/test/vet/build` y los scripts
de `package.json`, con npm, pnpm, Yarn o Bun según el archivo de bloqueo.
Ejecutar código elige `dev`, `start` o `serve`; los otros scripts se eligen en
la lista. Python permite ejecutar el archivo, comprobar su sintaxis y ejecutar
pruebas de un proyecto con `pyproject.toml`. Usa `.venv` si existe. También
puedes ejecutar archivos JavaScript, shell y Ruby sin un manifiesto.
Las herramientas de cada lenguaje deben estar instaladas.

## Proyectos LaTeX

Proyecto importa y exporta el proyecto como ZIP, compatible con Overleaf.
Archivo exporta el PDF compilado. Añadir archivos copia imágenes, bibliografías u
otros fuentes a la carpeta del proyecto.

El panel Referencias lista las etiquetas y las citas de todo el proyecto para
insertarlas con `\ref` o `\cite`. Insertar tiene asistentes de tabla y figura,
además de los entornos del catálogo. Buscar en el proyecto recorre los archivos de texto e incluye los cambios
abiertos sin guardar. Cada resultado abre el archivo en su línea.

El menú LaTeX permite detener la compilación, recompilar desde cero, limpiar
archivos auxiliares, compilar al dejar de escribir y guardar automáticamente.
Detener compilación sigue disponible al cambiar de pestaña. Configurar proyecto
LaTeX abre una ventana dedicada al archivo principal y al motor de esa carpeta.
Preferencias conserva los ajustes generales del editor y la apariencia. Contar palabras usa `texcount`
si está instalado y, si no, una estimación propia.

Tras compilar, las líneas con problemas llevan un punto en el margen y un
subrayado ondulado, rojo para los errores y ámbar para los avisos. El mensaje
aparece al pasar el cursor por el margen. `F8` y `Mayús+F8` recorren los
problemas. Las marcas son las de la última compilación: si editas líneas por
encima, se desplazan hasta que vuelvas a compilar.

`F12` o `Cmd`+clic sobre `\ref`, `\eqref` o `\cref` lleva a su `\label`; sobre
`\cite` y sus variantes, a la entrada de la bibliografía; y sobre `\input`,
`\include`, `\addbibresource` o `\includegraphics`, abre el archivo.

Vista previa de la ecuación (`Cmd+Shift+M`), en LaTeX, abre una ventana con
la fórmula que rodea al cursor: `$…$`, `\[…\]` o un entorno como `equation`
o `align`. Se compila aparte con el preámbulo del documento, así que valen
sus paquetes y macros, y se actualiza al dejar de escribir. Si el preámbulo
no funciona por separado, por ejemplo con `beamer`, se usa uno mínimo con
`amsmath`. Tarda lo que el motor en compilar un documento de una línea, uno
o dos segundos con Tectonic.

Renombrar etiqueta LaTeX, en Editar, cambia la etiqueta bajo el cursor en su
`\label` y en todas sus referencias del proyecto, sin tocar los comentarios.
Los documentos abiertos quedan sin guardar y el cambio se deshace en ellos;
los archivos cerrados se reescriben y guardan la versión anterior en su
historial.

Mostrar esta línea en el PDF (`Cmd+Shift+J`) lleva del código a la página y
resalta la línea. Un doble clic o `Cmd`+clic en el PDF abre el archivo y la
línea de origen. Miyu lee el `.synctex.gz` que deja la compilación, sin el
programa `synctex`, así que funciona con Tectonic solo.

Cada guardado deja una versión en `~/.config/miyulatex/history`. Historial del archivo LaTeX, en Archivo, muestra las últimas 100 y restaura
cualquiera en el editor. La ventana enseña las líneas que cambian entre la
versión elegida y el texto actual, en rojo las anteriores y en verde las
nuevas; también puede mostrar las dos versiones completas.

## Bibliografía

Cita por DOI o arXiv, en Insertar, descarga la entrada BibTeX de un DOI
(`10.1145/359576.359579`) o de un artículo de arXiv (`arXiv:1706.03762`) y la
añade al primer archivo `.bib` del proyecto. Puede insertar además `\cite` con
la clave nueva en el cursor. Usa `curl` y consulta doi.org o arxiv.org; no se
envía nada más que el identificador.

Revisar bibliografía, en LaTeX, lista las claves repetidas, los campos
obligatorios que faltan en cada entrada y las entradas que ningún documento
cita. Cada aviso abre la entrada en el editor.

## Sesión y archivos

Sin argumentos, Miyu vuelve al proyecto y a las pestañas de la última vez.
`miyu .` abre la carpeta actual. Se desactiva en Preferencias. Archivo guarda
los diez proyectos recientes.

Abrir rápido (`Cmd+Shift+O`) busca entre los archivos del proyecto por
cualquier parte del nombre o por sus letras en orden.

Archivos muestra el proyecto como un árbol de carpetas que se pliegan con un
clic. El filtro deja solo las carpetas con coincidencias. Crear archivo crea un archivo dentro del proyecto. Puedes elegir cualquier
formato por su extensión. Sin extensión, crea texto con `.txt`. Añadir archivos
copia archivos existentes y Actualizar lista vuelve a leer la carpeta. El clic derecho sobre un archivo permite renombrarlo,
duplicarlo, mostrarlo en su carpeta y copiar su ruta. Sobre una carpeta, crea
un archivo dentro de ella.

Si otro programa cambia un archivo abierto, Miyu lo recarga al volver a la
ventana. Deshacer recupera el texto anterior. Si además tenías cambios sin
guardar, pregunta si recargar o conservar tu versión.

## Git

Si el archivo está en un repositorio, la barra de estado muestra la rama y el
margen del editor marca lo que cambió desde el último commit: verde para las
líneas nuevas, ámbar para las modificadas y una señal roja donde se borraron
líneas. La rama y el commit se vuelven a leer al cambiar de pestaña.

## Ortografía

El editor subraya las palabras que no están en el diccionario. Revisa la
prosa de LaTeX, Markdown y texto. Deja fuera comandos, matemáticas,
comentarios, claves de citas y referencias, rutas y el preámbulo. El clic
derecho sobre una palabra subrayada ofrece sugerencias, la añade al
diccionario o la ignora durante la sesión. El idioma se elige en Preferencias
y por defecto es español.

En macOS usa el diccionario del sistema. En Linux y otros sistemas usa
diccionarios Hunspell: los de `/usr/share/hunspell` y `/usr/share/myspell`
(por ejemplo, el paquete `hunspell-es`) y los que pongas en
`~/.config/miyulatex/dictionaries`, con sus dos archivos `.aff` y `.dic`. Las
palabras añadidas se guardan en `~/.config/miyulatex/palabras.txt`. Sin un
diccionario del idioma elegido no se subraya nada.

## Personalización

Preferencias tiene dos secciones plegables, Editor y Apariencia.

Editor permite elegir la fuente entre todas las familias instaladas en el
equipo, con buscador y filtro de monoespaciadas, o desde un archivo TTF, OTF
o TTC. También ajusta el tamaño del código, el interlineado y los espacios
por sangría. También muestra u oculta los números de línea, resalta la línea
actual, activa el cierre de pares y las sugerencias, y ajusta la espera antes
de compilar.

Apariencia cambia el tamaño del texto de la interfaz y el redondeo de las
esquinas. Los colores propios sustituyen el primario, el secundario, el
acento, el fondo y el texto del tema elegido. Con una foto de fondo puedes
ajustar el tamaño del punto y la sombra bajo el texto, que mantiene legible
el código sobre la foto en temas claros y oscuros.

Sobre la barra de estado vive un gatito de píxeles. Mientras escribes saca
un portátil y teclea contigo; durante la compilación espera con unos puntos
sobre la cabeza, salta si sale bien y se asusta si falla. A ratos pasea, se
lame una pata, juega con un ovillo o se estira, y tras un minuto sin
actividad se duerme. Lo acompañan un cangrejito que chasquea las pinzas y
un schnauzer que menea la cola y ladra si la compilación falla: pasean por
su cuenta y de vez en cuando van a saludarlo, y entonces saltan juntos.
En la esquina crece un jazmín en su maceta que se mece con la brisa. Como el
de verdad, abre sus flores blancas de noche: mientras el gatito duerme
florece y suelta su perfume. También florece un rato al compilar bien o al
hacerle clic. Cada uno se oculta con clic derecho sobre él, en Ver o en
Apariencia.

## Fondo tramado

En Preferencias puedes elegir una foto, ajustar la intensidad, usar sus
colores como tema y alternar entre tramado y liso.

El tramado usa la fórmula de `BetterThanEminus/src/bg.js`, con matriz Bayer
8 por 8, luminancia ponderada, contraste contra el fondo, atenuación vertical
y redondeo del canvas. La ventana dibuja puntos de dos píxeles lógicos,
también en Retina, y usa muestreo Nearest para conservarlos. El encuadre
cubre el 95 % de la altura.

La prueba de referencia compara 1.292 píxeles de temas claros y oscuros en
ambos estilos. Para regenerarla desde el proyecto de referencia:

```sh
node tests/tramado_reference.mjs ../BetterThanEminus/src/bg.js
cargo test matches_better_than_eminus
```

## Desarrollo

Las preferencias viven en `~/.config/miyulatex/config.json`. Se respeta
`XDG_CONFIG_HOME` y se conserva el formato de la versión Python. Los fondos se
copian a la subcarpeta `backgrounds`. La versión anterior está en `python/`.
El ejecutable Rust no necesita Python.

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Las pruebas usan una carpeta de preferencias temporal y no tocan la real.

Las pruebas de integración de egui simulan edición Unicode, deshacer, pares,
autocompletado, guardado y protección de cambios externos. También verifican
Markdown, comentarios de código, apertura de PDF e imágenes, navegación de
páginas y protección de archivos binarios. Si Tectonic está
instalado, también compila un documento real y verifica el PDF de la ventana.

Puedes rasterizar fuera de la interfaz para revisar resultados:

```sh
miyu --render-pdf documento.pdf pagina.png 1
miyu --render-background foto.png fondo.png 1280 800 2 1 16131f dither
```

`app.rs` dibuja la ventana y maneja los eventos. `editor.rs` edita texto, con
`cursors.rs` para los cursores múltiples y `folds.rs` para el plegado,
`highlight.rs` resalta LaTeX, `syntax.rs` mantiene el resaltado por líneas y
`layout.rs` maqueta solo las que cambian. `format.rs` detecta formatos y extrae
el esquema de Markdown, `latex.rs` reúne fuentes, etiquetas, citas, historial y
ZIP del proyecto, `compiler.rs` compila y lee problemas, `bib.rs` revisa la bibliografía, `diff.rs` compara versiones, `git.rs` lee la rama y los cambios, `formatter.rs` llama a los formateadores, `synctex.rs` relaciona
el código con el PDF, `preview.rs` rasteriza y dibuja el PDF, `pdftext.rs` lee su texto, `spell.rs` revisa
la ortografía (con `hunspell.rs` fuera de macOS), `workspace.rs` lleva la sesión y los archivos, `backdrop.rs` trama la foto, `theme.rs` define
los temas, `custom.rs` aplica la personalización y `config.rs` guarda
preferencias. `snippets.json` conserva los catálogos de la versión Python.

## Licencia

[MIT](LICENSE).
