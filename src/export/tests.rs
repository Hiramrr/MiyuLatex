use super::*;
use crate::editor::catalog;

fn folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("miyu-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

const SAMPLE: &str = "# Título & más\n\nTexto con **negrita**, *énfasis*, `x_y` y un [enlace](https://example.com/a?b=1&c=2).\n\n- [x] hecho\n- [ ] pendiente\n\n1. uno\n2. dos\n\n> Una cita.\n\n| A | B |\n|:--|--:|\n| 1 | 2 |\n\n```rust\nfn main() { println!(\"<hola>\"); }\n```\n";

#[test]
fn html_has_structure_and_escapes() {
    let html = markdown_to_html(SAMPLE, "respaldo", None);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<title>Título &amp; más</title>"));
    assert!(html.contains("<style>"));
    assert!(
        html.contains("<h1 id=\"título--más\">Título &amp; más</h1>"),
        "{html}"
    );
    assert!(html.contains("<strong>negrita</strong>") && html.contains("<em>énfasis</em>"));
    assert!(html.contains("<code>x_y</code>"));
    assert!(html.contains("href=\"https://example.com/a?b=1&amp;c=2\""));
    assert!(html.contains("<input type=\"checkbox\" disabled checked> hecho"));
    assert!(html.contains("<input type=\"checkbox\" disabled> pendiente"));
    assert!(html.contains("<ol>") && html.contains("<blockquote>"));
    assert!(html.contains("<th style=\"text-align:left\">A</th>"));
    assert!(html.contains("<td style=\"text-align:right\">2</td>"));
    assert!(html.contains(
        "<code class=\"language-rust\">fn main() { println!(&quot;&lt;hola&gt;&quot;); }"
    ));
    // Sin título propio se usa el nombre del archivo.
    assert!(markdown_to_html("solo texto", "mi-nota", None).contains("<title>mi-nota</title>"));
    assert!(markdown_to_html("", "vacío", None).contains("<main>"));
}

#[test]
fn html_images_stay_relative_or_get_embedded() {
    let dir = folder("html-img");
    fs::write(dir.join("foto 1.png"), [137, 80, 78, 71]).unwrap();
    let md = "![Una foto](foto%201.png) ![Falta](nada.png) ![Web](https://example.com/x.png)";
    let same = markdown_to_html(md, "t", None);
    assert!(same.contains("src=\"foto%201.png\" alt=\"Una foto\""));
    let embedded = markdown_to_html(md, "t", Some(&dir));
    assert!(
        embedded.contains("src=\"data:image/png;base64,iVBORw==\""),
        "{embedded}"
    );
    assert!(
        embedded.contains("src=\"nada.png\"")
            && embedded.contains("src=\"https://example.com/x.png\"")
    );
    // export_html decide solo según dónde se guarde.
    let elsewhere = folder("html-out");
    export_html(md, Some(&dir), "t", &elsewhere.join("a.html")).unwrap();
    export_html(md, Some(&dir), "t", &dir.join("a.html")).unwrap();
    assert!(
        fs::read_to_string(elsewhere.join("a.html"))
            .unwrap()
            .contains("data:image/png")
    );
    assert!(
        fs::read_to_string(dir.join("a.html"))
            .unwrap()
            .contains("src=\"foto%201.png\"")
    );
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(elsewhere).unwrap();
}

#[test]
fn latex_escapes_special_characters() {
    let md = "Un_guion 50% R&D #1 cuesta \\$5 y {llaves} ~ ^ \\\\\n\nCódigo `a_b%c` y $x_1^2$.\n\n$$\\int_0^1 x\\,dx$$\n";
    let tex = markdown_to_latex(md, None).text;
    assert!(
        tex.contains("Un\\_guion 50\\% R\\&D \\#1 cuesta \\$5 y \\{llaves\\}"),
        "{tex}"
    );
    assert!(tex.contains("\\textasciitilde{}") && tex.contains("\\textasciicircum{}"));
    assert!(tex.contains("\\texttt{a\\_b\\%c}"));
    assert!(tex.contains("$x_1^2$") && tex.contains("\\[\\int_0^1 x\\,dx\\]"));
    assert!(tex.contains("\\begin{document}") && tex.trim_end().ends_with("\\end{document}"));
}

#[test]
fn latex_converts_blocks() {
    let tex = markdown_to_latex(SAMPLE, None).text;
    assert!(tex.contains("\\section*{Título \\& más}"), "{tex}");
    assert!(tex.contains("\\textbf{negrita}") && tex.contains("\\emph{énfasis}"));
    assert!(
        tex.contains("\\href{https://example.com/a?b=1\\&c=2}{enlace}"),
        "{tex}"
    );
    assert!(
        tex.contains("\\begin{itemize}")
            && tex.contains("$\\boxtimes$")
            && tex.contains("$\\square$")
    );
    assert!(tex.contains("\\begin{enumerate}\n\\item uno"));
    assert!(tex.contains("\\begin{quote}") && tex.contains("Una cita."));
    assert!(tex.contains("\\begin{tabular}{|l|r|}"), "{tex}");
    assert!(tex.contains("A & B \\\\") && tex.contains("1 & 2 \\\\"));
    assert!(tex.contains("\\begin{verbatim}\nfn main() { println!(\"<hola>\"); }"));
    let tex = markdown_to_latex("# T\n\nPie[^1].\n\n[^1]: La nota.\n", None).text;
    assert!(tex.contains("Pie\\footnote{La nota.}."), "{tex}");
}

#[test]
fn latex_copies_supported_images() {
    let dir = folder("tex-img");
    fs::write(dir.join("a b.png"), "x").unwrap();
    fs::write(dir.join("v.svg"), "x").unwrap();
    let converted = markdown_to_latex(
        "![Pie _x_](a%20b.png)\n\n![](v.svg)\n\n![](https://e.com/i.png)",
        Some(&dir),
    );
    assert_eq!(
        converted.images,
        vec![("img-1.png".to_owned(), dir.join("a b.png"))]
    );
    assert!(
        converted
            .text
            .contains("\\includegraphics{img-1.png}\\\\[2pt]{\\small\\emph{Pie x}}"),
        "{}",
        converted.text
    );
    assert_eq!(converted.text.matches("imagen no disponible").count(), 2);
    fs::remove_dir_all(dir).unwrap();
}

const ARTICLE: &str = "\\documentclass{article}\n\\usepackage[spanish]{babel}\n\\usepackage{geometry}\n\\usepackage{amsthm}\n\\newcommand{\\R}{\\mathbb{R}}\n\\title{Mi \\emph{gran} trabajo}\n\\author{Ana}\n\\begin{document}\n\\maketitle\n% \\section{Comentada}\n\\section{Introducción}\\label{sec:intro}\nUn párrafo introductorio que explica de qué trata todo esto y que es bastante largo para que haya que recortarlo en algún punto razonable sin romper $x + y$ ni nada parecido, porque eso rompería la compilación de la diapositiva y nadie quiere una diapositiva rota justo antes de empezar a presentar el trabajo.\n\n\\subsection{Puntos}\n\\begin{itemize}\n\\item Primero con $a\\in\\R$\n\\item Segundo\n\\begin{itemize}\\item anidado\\end{itemize}\n\\item Tercero\n\\item Cuarto\n\\item Quinto\n\\end{itemize}\n\\section{Fin}\n\\end{document}\n";

#[test]
fn beamer_from_article() {
    let out = beamer_from_latex(ARTICLE, "art").unwrap();
    assert!(out.starts_with("\\documentclass[aspectratio=169]{beamer}"));
    assert!(out.contains("\\usepackage[spanish]{babel}") && out.contains("\\usepackage{amsthm}"));
    assert!(!out.contains("geometry") && !out.contains("Comentada"));
    assert!(out.contains("\\newcommand{\\R}{\\mathbb{R}}"));
    assert!(out.contains("\\title{Mi \\emph{gran} trabajo}") && out.contains("\\author{Ana}"));
    assert!(out.contains("\\date{\\today}"));
    assert!(out.contains("\\titlepage"));
    assert!(
        out.contains("\\section{Introducción}\n\\begin{frame}{Introducción}"),
        "{out}"
    );
    assert!(out.contains("\\subsection{Puntos}\n\\begin{frame}{Puntos}"));
    assert!(out.contains("\\section{Fin}\n\\begin{frame}{Fin}\n\\end{frame}"));
    assert_eq!(out.matches("\\begin{frame}").count(), 4);
    // El párrafo se recorta sin dejar el `$` abierto.
    let intro = out.split("\\subsection").next().unwrap();
    assert!(intro.contains("\\ldots{}"), "{intro}");
    assert_eq!(intro.matches('$').count() % 2, 0);
    // Cuatro elementos como mucho y sin la sublista.
    assert!(out.contains("\\item Primero con $a\\in\\R$") && out.contains("\\item Cuarto"));
    assert!(!out.contains("Quinto") && !out.contains("anidado"));
    assert!(
        beamer_from_latex(
            "\\documentclass{beamer}\n\\begin{document}\\section{A}\\end{document}",
            "x"
        )
        .is_err()
    );
    assert!(
        beamer_from_latex(
            "\\documentclass{article}\n\\begin{document}Hola\\end{document}",
            "x"
        )
        .is_err()
    );
}

#[test]
fn presentation_never_overwrites() {
    let dir = folder("presentation");
    let source = dir.join("tesis.tex");
    fs::write(&source, ARTICLE).unwrap();
    let first = create_presentation(&source, ARTICLE).unwrap();
    let second = create_presentation(&source, ARTICLE).unwrap();
    assert_eq!(first, dir.join("tesis-presentacion.tex"));
    assert_eq!(second, dir.join("tesis-presentacion-2.tex"));
    assert_eq!(fs::read_to_string(&source).unwrap(), ARTICLE);
    fs::write(&first, "mío").unwrap();
    assert_eq!(
        create_presentation(&source, ARTICLE).unwrap(),
        dir.join("tesis-presentacion-3.tex")
    );
    assert_eq!(fs::read_to_string(&first).unwrap(), "mío");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn small_helpers() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(percent_decode("a%20b%C3%B1%2"), "a bñ%2");
    let command = epub_command(
        Path::new("pandoc"),
        Path::new("/t/in.md"),
        Path::new("/t/out.epub"),
        Path::new("/d"),
        Some("Mi nota"),
    );
    let args: Vec<_> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "/t/in.md",
            "--from=markdown",
            "--to=epub",
            "--output",
            "/t/out.epub",
            "--metadata=title:Mi nota"
        ]
    );
    assert_eq!(command.get_current_dir(), Some(Path::new("/d")));
}

fn compile(dir: &Path, name: &str) -> compiler::CompileResult {
    let root = dir.join(name);
    let (engine, executable) = compiler::select_engine(&root, "tectonic").unwrap();
    compiler::compile(root, engine, executable, Arc::new(AtomicBool::new(false))).unwrap()
}

#[test]
fn generated_documents_compile_with_tectonic() {
    if compiler::which("tectonic").is_none() {
        return;
    }
    // Markdown a PDF de extremo a extremo, con imagen, tabla ancha y caracteres especiales.
    let dir = folder("compile-md");
    image::RgbImage::new(40, 20)
        .save(dir.join("foto 1.png"))
        .unwrap();
    let md = format!(
        "---\ntitle: X\n---\n\n{SAMPLE}\n![Foto _1_](foto%201.png)\n\nTexto_con 100% & # $ ^ ~ \\\\ {{}} ñ é “comillas” — fin[^n].\n\n[^n]: Nota con `código`.\n\n- uno\n  - anidado\n\n| Columna larga uno | Columna larga dos | Tres |\n|---|---|---|\n| {} | {} | c |\n\n$$E = mc^2$$\n\n~~tachado~~ y $a_1$\n",
        "palabra ".repeat(8),
        "otra ".repeat(8)
    );
    let pdf = dir.join("salida.pdf");
    let engine = export_pdf(&md, Some(&dir), &pdf).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(engine, "tectonic");
    assert!(fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    assert_eq!(
        fs::read_dir(&dir).unwrap().count(),
        2,
        "no debe dejar auxiliares junto al Markdown"
    );
    // Un documento vacío no genera una página en blanco.
    assert!(export_pdf("  \n", None, &dir.join("vacio.pdf")).is_err());
    fs::remove_dir_all(dir).unwrap();

    // Beamer generado a partir de un artículo.
    let dir = folder("compile-beamer");
    fs::write(
        dir.join("p.tex"),
        beamer_from_latex(ARTICLE, "art").unwrap(),
    )
    .unwrap();
    let result = compile(&dir, "p.tex");
    assert!(result.ok, "{}", result.output);
    fs::remove_dir_all(dir).unwrap();

    // Todas las plantillas de presentación.
    let dir = folder("compile-templates");
    for template in catalog()
        .templates
        .iter()
        .filter(|t| t.text.contains("{beamer}"))
    {
        fs::write(dir.join(&template.filename), &template.text).unwrap();
        let result = compile(&dir, &template.filename);
        assert!(result.ok, "{}: {}", template.title, result.output);
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pdf_export_without_engine_says_so() {
    // Con la ruta de búsqueda vacía no hay motor: el mensaje debe nombrarlo.
    let message = "No encontré un motor LaTeX";
    if compiler::engines().is_empty() {
        let dir = folder("no-engine");
        let error = export_pdf("hola", None, &dir.join("a.pdf")).unwrap_err();
        assert!(error.starts_with(message), "{error}");
        fs::remove_dir_all(dir).unwrap();
    }
}
