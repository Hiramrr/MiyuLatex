//! Formateo con las herramientas de cada lenguaje, si están instaladas.

use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
};

use crate::compiler;

/// Programa y argumentos que formatean lo que reciben por la entrada
/// estándar, según la extensión del archivo. `{}` es el nombre del archivo.
const TOOLS: &[(&[&str], &str, &[&str])] = &[
    (&["rs"], "rustfmt", &["--edition", "2024", "--emit", "stdout"]),
    (&["go"], "gofmt", &[]),
    (&["py"], "ruff", &["format", "--stdin-filename", "{}", "-"]),
    (
        &["c", "h", "cc", "cpp", "cxx", "hpp", "java"],
        "clang-format",
        &["--assume-filename", "{}"],
    ),
    (
        &["js", "jsx", "ts", "tsx", "json", "css", "scss", "html", "md", "yaml", "yml"],
        "prettier",
        &["--stdin-filepath", "{}"],
    ),
    (&["tex", "sty", "cls", "bib"], "latexindent", &["{}"]),
];

/// Herramienta que formatea un archivo con este nombre, si se conoce alguna.
pub fn tool(name: &Path) -> Option<&'static str> {
    let extension = name.extension()?.to_str()?.to_lowercase();
    TOOLS
        .iter()
        .find(|(extensions, ..)| extensions.contains(&extension.as_str()))
        .map(|(_, program, _)| *program)
}

/// Texto formateado de un archivo llamado `name`, sin tocar el disco.
pub fn format(name: &Path, text: &str) -> Result<String, String> {
    let extension = name
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let (_, program, arguments) = TOOLS
        .iter()
        .find(|(extensions, ..)| extensions.contains(&extension.as_str()))
        .ok_or("No conozco un formateador para este tipo de archivo")?;
    let executable = compiler::which(program)
        .ok_or(format!("Instala {program} para formatear archivos .{extension}"))?;
    let mut command = Command::new(executable);
    // latexindent no lee la entrada estándar: trabaja sobre una copia.
    let copy = (*program == "latexindent").then(|| {
        let folder = std::env::temp_dir().join(format!("miyu-formato-{}", std::process::id()));
        (folder.join(format!("documento.{extension}")), folder)
    });
    let file = match &copy {
        Some((file, folder)) => {
            std::fs::create_dir_all(folder)
                .and_then(|()| std::fs::write(file, text))
                .map_err(|e| e.to_string())?;
            // Deja su registro junto a la copia, no en el proyecto.
            command.current_dir(folder);
            file.as_path()
        }
        None => {
            if let Some(folder) = name.parent().filter(|p| p.is_dir()) {
                command.current_dir(folder);
            }
            name
        }
    };
    for argument in *arguments {
        if *argument == "{}" {
            command.arg(file);
        } else {
            command.arg(argument);
        }
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("No pude ejecutar {program}: {e}"))?;
    // Se escribe desde otro hilo: el programa puede llenar su salida antes de leerlo todo.
    let mut input = child.stdin.take().unwrap();
    let source = text.to_owned();
    let writer = thread::spawn(move || input.write_all(source.as_bytes()));
    let output = child.wait_with_output().map_err(|e| e.to_string());
    let _ = writer.join();
    if let Some((_, folder)) = &copy {
        let _ = std::fs::remove_dir_all(folder);
    }
    let output = output?;
    let formatted = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() || formatted.trim().is_empty() && !text.trim().is_empty() {
        let error = String::from_utf8_lossy(&output.stderr);
        let reason = error.lines().find(|line| !line.trim().is_empty());
        return Err(format!(
            "{program} no pudo formatear el archivo: {}",
            reason.unwrap_or("sin detalles")
        ));
    }
    Ok(formatted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_tool_and_formats_rust() {
        assert_eq!(tool(Path::new("a/main.RS")), Some("rustfmt"));
        assert_eq!(tool(Path::new("tesis.tex")), Some("latexindent"));
        assert_eq!(tool(Path::new("notas.txt")), None);
        assert_eq!(tool(Path::new("Makefile")), None);
        assert!(format(Path::new("notas.txt"), "x").is_err());
        if compiler::which("rustfmt").is_none() {
            return;
        }
        assert_eq!(
            format(Path::new("main.rs"), "fn main(){let x=1;}").unwrap(),
            "fn main() {\n    let x = 1;\n}\n"
        );
        // Un error de sintaxis deja el motivo y no devuelve texto.
        let error = format(Path::new("main.rs"), "fn main( {").unwrap_err();
        assert!(error.starts_with("rustfmt no pudo formatear"), "{error}");
    }
}
