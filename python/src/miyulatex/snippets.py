"""Catálogos: comandos para autocompletar, entornos, símbolos y plantillas.

En los fragmentos, ``$0`` marca dónde queda el cursor tras insertarlos.
"""

from __future__ import annotations

from typing import NamedTuple


class Command(NamedTuple):
    name: str
    snippet: str
    help: str


def _c(name: str, snippet: str | None = None, help: str = "") -> Command:
    return Command(name, snippet if snippet is not None else "\\" + name, help)


COMMANDS: tuple[Command, ...] = (
    # Estructura
    _c("documentclass", "\\documentclass{$0}", "clase del documento"),
    _c("usepackage", "\\usepackage{$0}", "cargar paquete"),
    _c("begin", "\\begin{$0}", "abrir entorno"),
    _c("end", "\\end{$0}", "cerrar entorno"),
    _c("part", "\\part{$0}", "parte"),
    _c("chapter", "\\chapter{$0}", "capítulo"),
    _c("section", "\\section{$0}", "sección"),
    _c("subsection", "\\subsection{$0}", "subsección"),
    _c("subsubsection", "\\subsubsection{$0}", "sub-subsección"),
    _c("paragraph", "\\paragraph{$0}", "párrafo titulado"),
    _c("title", "\\title{$0}", "título"),
    _c("author", "\\author{$0}", "autor"),
    _c("date", "\\date{$0}", "fecha"),
    _c("maketitle", help="imprimir título"),
    _c("tableofcontents", help="índice"),
    _c("listoffigures", help="índice de figuras"),
    _c("listoftables", help="índice de tablas"),
    _c("appendix", help="apéndices"),
    _c("input", "\\input{$0}", "incluir archivo"),
    _c("include", "\\include{$0}", "incluir capítulo"),
    _c("newcommand", "\\newcommand{\\$0}[1]{}", "definir comando"),
    _c("renewcommand", "\\renewcommand{\\$0}{}", "redefinir comando"),
    _c("newtheorem", "\\newtheorem{$0}{}", "definir teorema"),
    _c("newpage", help="salto de página"),
    _c("clearpage", help="salto y vaciado de flotantes"),
    _c("noindent", help="sin sangría"),
    _c("vspace", "\\vspace{$0}", "espacio vertical"),
    _c("hspace", "\\hspace{$0}", "espacio horizontal"),
    _c("bigskip", help="salto grande"),
    _c("medskip", help="salto medio"),
    _c("smallskip", help="salto pequeño"),
    _c("centering", help="centrar"),
    _c("item", "\\item $0", "elemento de lista"),
    # Texto
    _c("textbf", "\\textbf{$0}", "negrita"),
    _c("textit", "\\textit{$0}", "cursiva"),
    _c("emph", "\\emph{$0}", "énfasis"),
    _c("underline", "\\underline{$0}", "subrayado"),
    _c("texttt", "\\texttt{$0}", "monoespaciado"),
    _c("textsc", "\\textsc{$0}", "versalitas"),
    _c("textsf", "\\textsf{$0}", "sin serifa"),
    _c("textcolor", "\\textcolor{$0}{}", "color de texto"),
    _c("footnote", "\\footnote{$0}", "nota al pie"),
    _c("url", "\\url{$0}", "enlace"),
    _c("href", "\\href{$0}{}", "enlace con texto"),
    _c("tiny"), _c("small"), _c("normalsize"), _c("large"), _c("Large"),
    _c("LARGE"), _c("huge"), _c("Huge"),
    # Referencias
    _c("label", "\\label{$0}", "etiqueta"),
    _c("ref", "\\ref{$0}", "referencia"),
    _c("eqref", "\\eqref{$0}", "referencia a ecuación"),
    _c("pageref", "\\pageref{$0}", "referencia a página"),
    _c("autoref", "\\autoref{$0}", "referencia automática"),
    _c("cite", "\\cite{$0}", "cita"),
    _c("citep", "\\citep{$0}", "cita entre paréntesis"),
    _c("citet", "\\citet{$0}", "cita textual"),
    _c("bibliography", "\\bibliography{$0}", "archivo .bib"),
    _c("bibliographystyle", "\\bibliographystyle{$0}", "estilo bibliográfico"),
    _c("printbibliography", help="imprimir bibliografía"),
    # Flotantes
    _c("includegraphics", "\\includegraphics[width=\\linewidth]{$0}", "imagen"),
    _c("caption", "\\caption{$0}", "pie de figura o tabla"),
    _c("hline", help="línea horizontal"),
    _c("toprule"), _c("midrule"), _c("bottomrule"),
    _c("multicolumn", "\\multicolumn{$0}{c}{}", "celda de varias columnas"),
    # Matemáticas
    _c("frac", "\\frac{$0}{}", "fracción"),
    _c("dfrac", "\\dfrac{$0}{}", "fracción grande"),
    _c("sqrt", "\\sqrt{$0}", "raíz"),
    _c("sum", "\\sum_{$0}^{}", "sumatoria"),
    _c("prod", "\\prod_{$0}^{}", "productoria"),
    _c("int", "\\int_{$0}^{}", "integral"),
    _c("iint"), _c("oint"),
    _c("lim", "\\lim_{$0}", "límite"),
    _c("partial", help="∂"),
    _c("nabla", help="∇"),
    _c("infty", help="∞"),
    _c("cdot", help="·"),
    _c("cdots", help="⋯"),
    _c("ldots", help="…"),
    _c("times", help="×"),
    _c("div", help="÷"),
    _c("pm", help="±"),
    _c("mp", help="∓"),
    _c("leq", help="≤"),
    _c("geq", help="≥"),
    _c("neq", help="≠"),
    _c("approx", help="≈"),
    _c("equiv", help="≡"),
    _c("sim", help="∼"),
    _c("propto", help="∝"),
    _c("in", help="∈"),
    _c("notin", help="∉"),
    _c("subset", help="⊂"),
    _c("subseteq", help="⊆"),
    _c("supset", help="⊃"),
    _c("cup", help="∪"),
    _c("cap", help="∩"),
    _c("emptyset", help="∅"),
    _c("forall", help="∀"),
    _c("exists", help="∃"),
    _c("neg", help="¬"),
    _c("land", help="∧"),
    _c("lor", help="∨"),
    _c("to", help="→"),
    _c("rightarrow", help="→"),
    _c("leftarrow", help="←"),
    _c("Rightarrow", help="⇒"),
    _c("Leftarrow", help="⇐"),
    _c("Leftrightarrow", help="⇔"),
    _c("leftrightarrow", help="↔"),
    _c("mapsto", help="↦"),
    _c("implies", help="⟹"),
    _c("iff", help="⟺"),
    _c("left", "\\left($0\\right)", "delimitador izquierdo"),
    _c("right", help="delimitador derecho"),
    _c("mathbb", "\\mathbb{$0}", "pizarra: ℝ ℕ ℤ"),
    _c("mathbf", "\\mathbf{$0}", "negrita matemática"),
    _c("mathcal", "\\mathcal{$0}", "caligráfica"),
    _c("mathrm", "\\mathrm{$0}", "redonda"),
    _c("mathit", "\\mathit{$0}", "cursiva matemática"),
    _c("text", "\\text{$0}", "texto dentro de fórmula"),
    _c("operatorname", "\\operatorname{$0}", "operador"),
    _c("overline", "\\overline{$0}", "barra superior"),
    _c("hat", "\\hat{$0}", "circunflejo"),
    _c("bar", "\\bar{$0}", "barra"),
    _c("vec", "\\vec{$0}", "vector"),
    _c("dot", "\\dot{$0}", "derivada temporal"),
    _c("ddot", "\\ddot{$0}", "segunda derivada"),
    _c("tilde", "\\tilde{$0}", "tilde"),
    _c("binom", "\\binom{$0}{}", "binomial"),
    _c("sin"), _c("cos"), _c("tan"), _c("log"), _c("ln"), _c("exp"),
    _c("min"), _c("max"), _c("det"), _c("dim"), _c("ker"), _c("gcd"),
    _c("quad", help="espacio"),
    _c("qquad", help="espacio doble"),
    # Griego
    _c("alpha", help="α"), _c("beta", help="β"), _c("gamma", help="γ"),
    _c("delta", help="δ"), _c("epsilon", help="ϵ"), _c("varepsilon", help="ε"),
    _c("zeta", help="ζ"), _c("eta", help="η"), _c("theta", help="θ"),
    _c("vartheta", help="ϑ"), _c("iota", help="ι"), _c("kappa", help="κ"),
    _c("lambda", help="λ"), _c("mu", help="μ"), _c("nu", help="ν"),
    _c("xi", help="ξ"), _c("pi", help="π"), _c("rho", help="ρ"),
    _c("sigma", help="σ"), _c("tau", help="τ"), _c("upsilon", help="υ"),
    _c("phi", help="ϕ"), _c("varphi", help="φ"), _c("chi", help="χ"),
    _c("psi", help="ψ"), _c("omega", help="ω"),
    _c("Gamma", help="Γ"), _c("Delta", help="Δ"), _c("Theta", help="Θ"),
    _c("Lambda", help="Λ"), _c("Xi", help="Ξ"), _c("Pi", help="Π"),
    _c("Sigma", help="Σ"), _c("Phi", help="Φ"), _c("Psi", help="Ψ"),
    _c("Omega", help="Ω"),
)

LIST_ENVS = frozenset({"itemize", "enumerate", "description"})

# Entorno -> cuerpo que se inserta entre \begin y \end.
ENVIRONMENTS: dict[str, str] = {
    "document": "$0",
    "itemize": "\\item $0",
    "enumerate": "\\item $0",
    "description": "\\item[$0] ",
    "equation": "$0",
    "equation*": "$0",
    "align": "$0",
    "align*": "$0",
    "gather": "$0",
    "gather*": "$0",
    "multline": "$0",
    "cases": "$0",
    "matrix": "$0",
    "pmatrix": "$0",
    "bmatrix": "$0",
    "vmatrix": "$0",
    "array": "$0",
    "figure": "\\centering\n\\includegraphics[width=0.8\\linewidth]{$0}\n\\caption{}\n\\label{fig:}",
    "table": "\\centering\n\\caption{$0}\n\\label{tab:}\n\\begin{tabular}{lcr}\n    \\hline\n     &  &  \\\\\n    \\hline\n\\end{tabular}",
    "tabular": "$0",
    "center": "$0",
    "flushleft": "$0",
    "flushright": "$0",
    "abstract": "$0",
    "quote": "$0",
    "verbatim": "$0",
    "lstlisting": "$0",
    "minipage": "$0",
    "theorem": "$0",
    "lemma": "$0",
    "proof": "$0",
    "definition": "$0",
    "frame": "\\frametitle{$0}",
    "columns": "$0",
    "block": "$0",
    "tikzpicture": "$0",
    "thebibliography": "\\bibitem{$0} ",
}

# Argumento que acompaña a \begin{entorno}.
ENV_ARGS: dict[str, str] = {
    "tabular": "{lcr}",
    "array": "{lcr}",
    "figure": "[htbp]",
    "table": "[htbp]",
    "minipage": "{0.5\\linewidth}",
    "thebibliography": "{99}",
    "block": "{Título}",
}


class Symbol(NamedTuple):
    glyph: str
    command: str
    name: str
    group: str


def _group(group: str, rows: str) -> list[Symbol]:
    symbols = []
    for row in rows.strip().splitlines():
        glyph, command, name = row.strip().split(" ", 2)
        symbols.append(Symbol(glyph, command, name, group))
    return symbols


SYMBOLS: tuple[Symbol, ...] = tuple(
    _group(
        "Griego",
        r"""
        α \alpha alfa
        β \beta beta
        γ \gamma gamma
        δ \delta delta
        ε \varepsilon épsilon
        ζ \zeta zeta
        η \eta eta
        θ \theta theta
        κ \kappa kappa
        λ \lambda lambda
        μ \mu mu
        ν \nu nu
        ξ \xi xi
        π \pi pi
        ρ \rho rho
        σ \sigma sigma
        τ \tau tau
        φ \varphi phi
        χ \chi ji
        ψ \psi psi
        ω \omega omega
        Γ \Gamma Gamma mayúscula
        Δ \Delta Delta mayúscula
        Θ \Theta Theta mayúscula
        Λ \Lambda Lambda mayúscula
        Π \Pi Pi mayúscula
        Σ \Sigma Sigma mayúscula
        Φ \Phi Phi mayúscula
        Ψ \Psi Psi mayúscula
        Ω \Omega Omega mayúscula
        """,
    )
    + _group(
        "Operadores",
        r"""
        ± \pm más menos
        ∓ \mp menos más
        × \times por
        ÷ \div entre
        · \cdot punto centrado
        ∘ \circ composición
        ⊕ \oplus suma directa
        ⊗ \otimes producto tensorial
        ∑ \sum sumatoria
        ∏ \prod productoria
        ∫ \int integral
        ∬ \iint integral doble
        ∮ \oint integral de contorno
        ∂ \partial derivada parcial
        ∇ \nabla nabla gradiente
        √ \sqrt{} raíz cuadrada
        ∞ \infty infinito
        """,
    )
    + _group(
        "Relaciones",
        r"""
        ≤ \leq menor o igual
        ≥ \geq mayor o igual
        ≠ \neq distinto
        ≈ \approx aproximadamente
        ≡ \equiv equivalente idéntico
        ∼ \sim similar
        ≅ \cong congruente
        ∝ \propto proporcional
        ≪ \ll mucho menor
        ≫ \gg mucho mayor
        ⊥ \perp perpendicular
        ∥ \parallel paralelo
        """,
    )
    + _group(
        "Conjuntos y lógica",
        r"""
        ∈ \in pertenece
        ∉ \notin no pertenece
        ⊂ \subset subconjunto
        ⊆ \subseteq subconjunto o igual
        ⊃ \supset superconjunto
        ∪ \cup unión
        ∩ \cap intersección
        ∅ \emptyset conjunto vacío
        ∖ \setminus diferencia
        ℝ \mathbb{R} reales
        ℕ \mathbb{N} naturales
        ℤ \mathbb{Z} enteros
        ℚ \mathbb{Q} racionales
        ℂ \mathbb{C} complejos
        ∀ \forall para todo
        ∃ \exists existe
        ¬ \neg negación
        ∧ \land y lógico
        ∨ \lor o lógico
        ∴ \therefore por lo tanto
        """,
    )
    + _group(
        "Flechas",
        r"""
        → \to flecha derecha
        ← \leftarrow flecha izquierda
        ↔ \leftrightarrow flecha doble
        ⇒ \Rightarrow implica
        ⇐ \Leftarrow implicado por
        ⇔ \Leftrightarrow si y solo si
        ↦ \mapsto aplica en
        ↑ \uparrow flecha arriba
        ↓ \downarrow flecha abajo
        ⟶ \longrightarrow flecha larga
        """,
    )
    + _group(
        "Varios",
        r"""
        … \ldots puntos suspensivos
        ⋯ \cdots puntos centrados
        ⋮ \vdots puntos verticales
        ⋱ \ddots puntos diagonales
        ° ^\circ grados
        ℏ \hbar hbar constante de Planck
        ℓ \ell ele cursiva
        ′ \prime prima
        † \dagger daga
        ★ \star estrella
        ∠ \angle ángulo
        △ \triangle triángulo
        """,
    )
)


class Template(NamedTuple):
    key: str
    title: str
    description: str
    filename: str
    body: str


_PREAMBLE = r"""\usepackage[T1]{fontenc}
\usepackage[spanish]{babel}
\usepackage{amsmath, amssymb, amsthm}
\usepackage{graphicx}
\usepackage{hyperref}
"""

TEMPLATES: tuple[Template, ...] = (
    Template(
        "articulo",
        "Artículo",
        "Documento corto con secciones, resumen y matemáticas",
        "articulo.tex",
        r"""\documentclass[11pt, a4paper]{article}
"""
        + _PREAMBLE
        + r"""\usepackage[margin=2.5cm]{geometry}

\title{Título del artículo}
\author{Tu nombre}
\date{\today}

\begin{document}

\maketitle

\begin{abstract}
    Un resumen breve del trabajo.
\end{abstract}

\section{Introducción}

Escribe aquí. Una fórmula en línea: $e^{i\pi} + 1 = 0$.

\begin{equation}
    \int_{-\infty}^{\infty} e^{-x^2} \, dx = \sqrt{\pi}
    \label{eq:gauss}
\end{equation}

\section{Desarrollo}

Como muestra la ecuación~\eqref{eq:gauss}, \ldots

\section{Conclusiones}

\end{document}
""",
    ),
    Template(
        "tarea",
        "Tarea",
        "Problemas numerados con encabezado de curso",
        "tarea.tex",
        r"""\documentclass[11pt, a4paper]{article}
"""
        + _PREAMBLE
        + r"""\usepackage[margin=2.5cm]{geometry}
\usepackage{enumitem}

\newcommand{\problema}[1]{\section*{Problema #1}}

\title{Tarea 1 \\ \large Nombre del curso}
\author{Tu nombre}
\date{\today}

\begin{document}

\maketitle

\problema{1}

Enunciado del problema.

\textbf{Solución.} Desarrollo de la solución:
\begin{align*}
    a^2 + b^2 &= c^2 \\
    c &= \sqrt{a^2 + b^2}
\end{align*}

\problema{2}

\begin{enumerate}[label=(\alph*)]
    \item Primer inciso.
    \item Segundo inciso.
\end{enumerate}

\end{document}
""",
    ),
    Template(
        "informe",
        "Informe",
        "Documento largo con capítulos e índice",
        "informe.tex",
        r"""\documentclass[12pt, a4paper]{report}
"""
        + _PREAMBLE
        + r"""\usepackage[margin=2.5cm]{geometry}

\title{Título del informe}
\author{Tu nombre}
\date{\today}

\begin{document}

\maketitle
\tableofcontents

\chapter{Introducción}

\section{Contexto}

Texto del informe.

\chapter{Metodología}

\chapter{Resultados}

\begin{table}[htbp]
    \centering
    \caption{Una tabla de ejemplo.}
    \label{tab:ejemplo}
    \begin{tabular}{lcr}
        \hline
        Concepto & Valor & Unidad \\
        \hline
        Masa & 1.5 & kg \\
        Tiempo & 20 & s \\
        \hline
    \end{tabular}
\end{table}

\chapter{Conclusiones}

\end{document}
""",
    ),
    Template(
        "presentacion",
        "Presentación",
        "Diapositivas con Beamer",
        "presentacion.tex",
        r"""\documentclass[aspectratio=169]{beamer}
\usepackage[T1]{fontenc}
\usepackage[spanish]{babel}
\usepackage{amsmath, amssymb}
\usetheme{Madrid}

\title{Título de la presentación}
\author{Tu nombre}
\date{\today}

\begin{document}

\begin{frame}
    \titlepage
\end{frame}

\begin{frame}
    \frametitle{Contenido}
    \tableofcontents
\end{frame}

\section{Introducción}

\begin{frame}
    \frametitle{Primera diapositiva}
    \begin{itemize}
        \item Un punto importante.
        \item Otro punto, con fórmula: $E = mc^2$.
    \end{itemize}
\end{frame}

\begin{frame}
    \frametitle{Un bloque}
    \begin{block}{Teorema}
        Para todo $n > 2$ no existen enteros positivos tales que $a^n + b^n = c^n$.
    \end{block}
\end{frame}

\end{document}
""",
    ),
    Template(
        "carta",
        "Carta",
        "Carta formal",
        "carta.tex",
        r"""\documentclass[11pt, a4paper]{letter}
\usepackage[T1]{fontenc}
\usepackage[spanish]{babel}

\signature{Tu nombre}
\address{Tu dirección \\ Ciudad}

\begin{document}

\begin{letter}{Destinatario \\ Institución \\ Ciudad}

\opening{Estimado/a:}

Cuerpo de la carta.

\closing{Atentamente,}

\end{letter}

\end{document}
""",
    ),
    Template("vacio", "En blanco", "Archivo vacío", "sin-titulo.tex", ""),
)
