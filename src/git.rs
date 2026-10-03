//! Lo mínimo de Git para el editor: la rama y qué líneas cambiaron desde el
//! último commit.

use std::{path::Path, process::Command};

use crate::{
    compiler,
    diff::{self, Change},
};

/// Estado de un archivo dentro de un repositorio.
pub struct Info {
    pub branch: String,
    /// Texto del archivo en el último commit; `None` si aún no está en él.
    pub base: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Added,
    Modified,
    /// Se borraron líneas justo antes de esta.
    Removed,
}

fn git(folder: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new(compiler::which("git")?)
        .arg("-C")
        .arg(folder)
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

/// Rama y versión confirmada de `path`, si está dentro de un repositorio.
pub fn info(path: &Path) -> Option<Info> {
    let folder = path.parent()?;
    // Sin una carpeta .git no se llama a git: en macOS, sin las herramientas
    // de desarrollo, el propio comando abre un aviso de instalación.
    if !folder.ancestors().any(|dir| dir.join(".git").exists()) {
        return None;
    }
    let branch = git(folder, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let name = path.file_name()?.to_str()?;
    let base = git(folder, &["show", &format!("HEAD:./{name}")]).map(|t| t.replace("\r\n", "\n"));
    Some(Info { branch, base })
}

/// Filas de `current` que cambiaron respecto a `base`, en orden.
pub fn marks(base: &str, current: &str) -> Vec<(usize, Mark)> {
    let mut marks = Vec::new();
    let (mut row, mut removed, mut added) = (0, 0, 0);
    let close = |row: usize, removed: &mut usize, added: &mut usize, marks: &mut Vec<_>| {
        // Las líneas nuevas que sustituyen a otras cuentan como modificadas.
        for i in 0..*added {
            let mark = if i < *removed { Mark::Modified } else { Mark::Added };
            marks.push((row - *added + i, mark));
        }
        if *added == 0 && *removed > 0 {
            marks.push((row, Mark::Removed));
        }
        (*removed, *added) = (0, 0);
    };
    for (change, _) in diff::lines(base, current) {
        match change {
            Change::Removed => removed += 1,
            Change::Added => {
                added += 1;
                row += 1;
            }
            Change::Same => {
                close(row, &mut removed, &mut added, &mut marks);
                row += 1;
            }
        }
    }
    close(row, &mut removed, &mut added, &mut marks);
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use Mark::*;

    #[test]
    fn marks_added_modified_and_removed_lines() {
        assert_eq!(marks("a\nb\nc", "a\nb\nc"), []);
        assert_eq!(marks("a\nb\nc", "a\nX\nc\nd\ne"), [(1, Modified), (3, Added), (4, Added)]);
        assert_eq!(marks("a\nb\nc\nd", "a\nd"), [(1, Removed)]);
        assert_eq!(marks("a\nb", "a\nX\nY"), [(1, Modified), (2, Added)]);
        assert_eq!(marks("a\nb", "a"), [(1, Removed)]);
        assert_eq!(marks("", "a\nb"), [(0, Added), (1, Added)]);
    }

    #[test]
    fn reads_branch_and_committed_text() {
        let folder = std::env::temp_dir().join(format!("miyu-git-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("cap")).unwrap();
        let file = folder.join("cap/uno.tex");
        std::fs::write(&file, "uno\r\ndos\r\n").unwrap();
        // Fuera de un repositorio no hay nada que decir.
        assert!(info(&file).is_none());
        if compiler::which("git").is_none() {
            return;
        }
        let run = |arguments: &[&str]| {
            let done = Command::new("git")
                .arg("-C")
                .arg(&folder)
                .args(["-c", "user.name=Miyu", "-c", "user.email=miyu@example.com"])
                .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
                .args(arguments)
                .output()
                .unwrap();
            assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
        };
        run(&["init", "-q", "-b", "trabajo"]);
        std::fs::write(folder.join("nuevo.tex"), "x").unwrap();
        run(&["add", "cap/uno.tex"]);
        run(&["commit", "-q", "-m", "inicio"]);
        let found = info(&file).unwrap();
        assert_eq!(found.branch, "trabajo");
        assert_eq!(found.base.as_deref(), Some("uno\ndos\n"));
        // Un archivo que aún no está en el commit no tiene versión anterior.
        assert!(info(&folder.join("nuevo.tex")).unwrap().base.is_none());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
