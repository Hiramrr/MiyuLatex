//! Operaciones de Git de la vista de control de versiones. Todas llaman al
//! ejecutable `git`, bloquean hasta que termina y están pensadas para un hilo
//! aparte: nunca se usan desde el dibujo de un cuadro.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::compiler;

/// Un archivo con cambios según `git status`.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Ruta relativa a la raíz del repositorio.
    pub path: String,
    /// Estado en el índice y en el árbol de trabajo (`X` e `Y` de git).
    pub index: char,
    pub tree: char,
    /// Ruta anterior si se renombró o copió.
    pub from: Option<String>,
}

impl Entry {
    pub fn untracked(&self) -> bool {
        self.index == '?'
    }
    pub fn staged(&self) -> bool {
        !matches!(self.index, ' ' | '?' | '!')
    }
    pub fn unstaged(&self) -> bool {
        !matches!(self.tree, ' ' | '?' | '!')
    }
    /// Rutas que hay que pasar a git. La anterior de un renombrado solo existe
    /// en el índice: sirve para quitar de preparados y para el diff de lo
    /// preparado, pero hace fallar a `add` y al diff del árbol de trabajo.
    pub fn paths(&self, old: bool) -> Vec<String> {
        let mut paths = vec![self.path.clone()];
        if old {
            paths.extend(self.from.clone());
        }
        paths
    }
}

pub struct Branch {
    pub name: String,
    pub current: bool,
}

pub struct Commit {
    pub hash: String,
    pub author: String,
    pub time: i64,
    pub subject: String,
}

pub struct Stash {
    /// Referencia, como `stash@{0}`.
    pub name: String,
    pub time: i64,
    pub subject: String,
}

/// Todo lo que muestra la vista, leído de una vez.
pub struct Snapshot {
    pub root: PathBuf,
    pub branch: String,
    pub entries: Vec<Entry>,
    pub branches: Vec<Branch>,
    pub log: Vec<Commit>,
    pub stashes: Vec<Stash>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Staged(Vec<String>),
    Unstaged(Vec<String>),
    Untracked(String),
    Commit(String),
}

pub struct Diff {
    pub text: String,
    /// Se cortó por ser enorme: no se puede aplicar por bloques.
    pub truncated: bool,
}

pub struct Blame {
    pub hash: String,
    pub author: String,
    pub time: i64,
    pub summary: String,
    pub committed: bool,
}

/// Cabecera de un diff de un solo archivo y sus bloques.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Patch {
    pub header: String,
    pub hunks: Vec<String>,
}

const DIFF_LIMIT: usize = 2_000_000;

struct Output {
    ok: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn execute(root: &Path, arguments: &[&str], input: Option<&str>) -> Result<Output, String> {
    let git = compiler::which("git").ok_or("Git no está instalado o no está en el PATH.")?;
    let mut command = Command::new(git);
    command
        .arg("-C")
        .arg(root)
        .arg("--no-optional-locks")
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        // Si git termina antes de leerlo todo, su error ya lo explica.
        let _ = stdin.write_all(text.as_bytes());
    }
    let done = child.wait_with_output().map_err(|e| e.to_string())?;
    Ok(Output {
        ok: done.status.success(),
        code: done.status.code(),
        stdout: String::from_utf8_lossy(&done.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&done.stderr).into_owned(),
    })
}

fn run(root: &Path, arguments: &[&str]) -> Result<String, String> {
    run_input(root, arguments, None)
}

fn run_input(root: &Path, arguments: &[&str], input: Option<&str>) -> Result<String, String> {
    let output = execute(root, arguments, input)?;
    if output.ok {
        return Ok(output.stdout);
    }
    // Los avisos de los hooks y «nothing to commit» llegan por stdout.
    let text = [output.stderr.trim(), output.stdout.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Err(if text.is_empty() {
        format!("git {} falló", arguments.first().copied().unwrap_or(""))
    } else {
        text
    })
}

fn with_paths<'a>(head: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    let mut arguments = head.to_vec();
    arguments.push("--");
    arguments.extend(paths.iter().map(String::as_str));
    arguments
}

/// Raíz del repositorio que contiene `folder`; `None` si no está en uno.
pub fn toplevel(folder: &Path) -> Result<Option<PathBuf>, String> {
    // Sin una carpeta .git no se llama a git: en macOS, sin las herramientas
    // de desarrollo, el propio comando abre un aviso de instalación.
    if !folder.ancestors().any(|dir| dir.join(".git").exists()) {
        return Ok(None);
    }
    let root = run(folder, &["rev-parse", "--show-toplevel"])?;
    Ok(Some(PathBuf::from(root.trim())))
}

fn has_head(root: &Path) -> bool {
    execute(root, &["rev-parse", "--verify", "-q", "HEAD"], None).is_ok_and(|o| o.ok)
}

/// Rama actual y archivos con cambios, a partir de `status --porcelain -z -b`.
pub fn parse_status(text: &str) -> (String, Vec<Entry>) {
    let mut items = text.split('\0');
    let mut branch = String::new();
    let mut entries = Vec::new();
    while let Some(item) = items.next() {
        if let Some(head) = item.strip_prefix("## ") {
            branch = if let Some(name) = head.strip_prefix("No commits yet on ") {
                name.to_string()
            } else if head.starts_with("HEAD (no branch)") {
                "(HEAD suelto)".to_string()
            } else {
                let name = head.split("...").next().unwrap_or(head);
                name.split(' ').next().unwrap_or(name).to_string()
            };
            continue;
        }
        let mut chars = item.chars();
        let (Some(index), Some(tree), Some(' ')) = (chars.next(), chars.next(), chars.next())
        else {
            continue;
        };
        let path = chars.as_str().to_string();
        // Los renombrados traen la ruta anterior en el elemento siguiente.
        let from = matches!(index, 'R' | 'C')
            .then(|| items.next().map(str::to_string))
            .flatten();
        entries.push(Entry {
            path,
            index,
            tree,
            from,
        });
    }
    (branch, entries)
}

pub fn snapshot(folder: &Path) -> Result<Option<Snapshot>, String> {
    let Some(root) = toplevel(folder)? else {
        return Ok(None);
    };
    let (branch, entries) = parse_status(&run(
        &root,
        &["status", "--porcelain", "-z", "--branch", "-uall"],
    )?);
    Ok(Some(Snapshot {
        branches: branches(&root)?,
        log: log(&root, 60)?,
        stashes: stashes(&root)?,
        root,
        branch,
        entries,
    }))
}

pub fn branches(root: &Path) -> Result<Vec<Branch>, String> {
    let text = run(
        root,
        &[
            "for-each-ref",
            "--format=%(HEAD)%(refname:short)",
            "refs/heads",
        ],
    )?;
    Ok(text
        .lines()
        .filter(|line| line.len() > 1)
        .map(|line| Branch {
            current: line.starts_with('*'),
            name: line[1..].to_string(),
        })
        .collect())
}

pub fn log(root: &Path, count: usize) -> Result<Vec<Commit>, String> {
    // Un repositorio recién creado aún no tiene historial.
    if !has_head(root) {
        return Ok(Vec::new());
    }
    let text = run(
        root,
        &[
            "log",
            &format!("-n{count}"),
            "--format=%h%x1f%an%x1f%at%x1f%s",
        ],
    )?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\u{1f}');
            Some(Commit {
                hash: fields.next()?.into(),
                author: fields.next()?.into(),
                time: fields.next()?.parse().ok()?,
                subject: fields.next().unwrap_or("").into(),
            })
        })
        .collect())
}

pub fn stashes(root: &Path) -> Result<Vec<Stash>, String> {
    let text = run(root, &["stash", "list", "--format=%gd%x1f%at%x1f%s"])?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\u{1f}');
            Some(Stash {
                name: fields.next()?.into(),
                time: fields.next()?.parse().ok()?,
                subject: fields.next().unwrap_or("").into(),
            })
        })
        .collect())
}

pub fn stage(root: &Path, paths: &[String]) -> Result<(), String> {
    run(root, &with_paths(&["add"], paths)).map(drop)
}

pub fn unstage(root: &Path, paths: &[String]) -> Result<(), String> {
    // Sin ningún commit no hay a qué volver: se quitan del índice.
    if has_head(root) {
        run(root, &with_paths(&["restore", "--staged"], paths)).map(drop)
    } else {
        run(root, &with_paths(&["rm", "--cached", "-r", "-q"], paths)).map(drop)
    }
}

/// Crea el commit con lo preparado; devuelve la primera línea que imprime git.
pub fn commit(root: &Path, message: &str) -> Result<String, String> {
    let out = run_input(root, &["commit", "-F", "-"], Some(message))?;
    Ok(out.lines().next().unwrap_or("").to_string())
}

pub fn checkout(root: &Path, branch: &str) -> Result<(), String> {
    run(root, &["switch", branch]).map(drop)
}

pub fn create_branch(root: &Path, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Escribe un nombre para la rama.".into());
    }
    run(root, &["check-ref-format", "--branch", name])
        .map_err(|_| format!("«{name}» no es un nombre de rama válido."))?;
    run(root, &["switch", "-c", name]).map(drop)
}

pub fn stash_push(root: &Path, message: &str) -> Result<String, String> {
    let message = message.trim();
    let out = if message.is_empty() {
        run(root, &["stash", "push"])?
    } else {
        run(root, &["stash", "push", "-m", message])?
    };
    // Sin cambios git termina bien pero no guarda nada.
    if out.starts_with("No local changes") {
        return Err("No hay cambios en archivos con seguimiento que guardar.".into());
    }
    Ok(out.lines().next().unwrap_or("").to_string())
}

pub fn stash_pop(root: &Path, name: &str) -> Result<(), String> {
    run(root, &["stash", "pop", name]).map(drop)
}

pub fn diff(root: &Path, target: &Target) -> Result<Diff, String> {
    let text = match target {
        Target::Staged(paths) => run(
            root,
            &with_paths(&["diff", "--cached", "--no-color", "--no-ext-diff"], paths),
        )?,
        Target::Unstaged(paths) => run(
            root,
            &with_paths(&["diff", "--no-color", "--no-ext-diff"], paths),
        )?,
        Target::Untracked(path) => {
            // `--no-index` termina con 1 cuando hay diferencias.
            let output = execute(
                root,
                &[
                    "diff",
                    "--no-index",
                    "--no-color",
                    "--no-ext-diff",
                    "--",
                    "/dev/null",
                    path,
                ],
                None,
            )?;
            if !output.ok && output.code != Some(1) {
                return Err(output.stderr.trim().to_string());
            }
            output.stdout
        }
        Target::Commit(hash) => run(
            root,
            &[
                "show",
                "--no-color",
                "--no-ext-diff",
                "--stat",
                "--patch",
                "--date=format:%Y-%m-%d %H:%M",
                "--format=%h  %an <%ae>%n%ad%n%n%s%n%n%b",
                hash,
            ],
        )?,
    };
    let truncated = text.len() > DIFF_LIMIT;
    let text = if truncated {
        let mut end = DIFF_LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!(
            "{}\n… diff demasiado largo, se muestra el principio\n",
            &text[..end]
        )
    } else {
        text
    };
    Ok(Diff { text, truncated })
}

/// Divide el diff de un archivo en su cabecera y sus bloques. Con varios
/// archivos no se puede aplicar nada por bloques: devuelve la lista vacía.
pub fn parse_patch(text: &str) -> Patch {
    if text.matches("\ndiff --git ").count() + usize::from(text.starts_with("diff --git ")) != 1 {
        return Patch::default();
    }
    let mut patch = Patch::default();
    for line in text.split_inclusive('\n') {
        if line.starts_with("@@") {
            patch.hunks.push(String::new());
        }
        match patch.hunks.last_mut() {
            Some(hunk) => hunk.push_str(line),
            None => patch.header.push_str(line),
        }
    }
    patch
}

/// Prepara un bloque (o con `reverse`, lo quita de los preparados).
pub fn apply_hunk(root: &Path, header: &str, hunk: &str, reverse: bool) -> Result<(), String> {
    let patch = format!("{header}{hunk}");
    let mut arguments = vec!["apply", "--cached", "--recount", "--whitespace=nowarn"];
    if reverse {
        arguments.push("--reverse");
    }
    arguments.push("-");
    run_input(root, &arguments, Some(&patch)).map(drop)
}

/// Autor y commit de la línea `row` (desde 0) de `file`, según el disco.
pub fn blame(file: &Path, row: usize) -> Option<Blame> {
    let folder = file.parent()?;
    let name = file.file_name()?.to_str()?;
    let range = format!("{0},{0}", row + 1);
    let text = run(folder, &["blame", "-L", &range, "--porcelain", "--", name]).ok()?;
    let hash = text.lines().next()?.split(' ').next()?;
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(' '))
            .map(str::to_string)
    };
    Some(Blame {
        committed: hash.chars().any(|c| c != '0'),
        hash: hash.chars().take(7).collect(),
        author: field("author")?,
        time: field("author-time")?.parse().ok()?,
        summary: field("summary").unwrap_or_default(),
    })
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// «hace 3 días», para una fecha dada en segundos Unix.
pub fn relative(now: i64, then: i64) -> String {
    let seconds = (now - then).max(0);
    let unit = |count: i64, one: &str, many: &str| {
        format!("hace {count} {}", if count == 1 { one } else { many })
    };
    match seconds {
        0..60 => "hace un momento".into(),
        60..3600 => unit(seconds / 60, "minuto", "minutos"),
        3600..86_400 => unit(seconds / 3600, "hora", "horas"),
        86_400..604_800 => unit(seconds / 86_400, "día", "días"),
        604_800..2_592_000 => unit(seconds / 604_800, "semana", "semanas"),
        2_592_000..31_536_000 => unit(seconds / 2_592_000, "mes", "meses"),
        _ => unit(seconds / 31_536_000, "año", "años"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Repositorio temporal con identidad local; `None` si no hay git.
    fn repository(name: &str) -> Option<PathBuf> {
        compiler::which("git")?;
        let folder = std::env::temp_dir().join(format!("miyu-repo-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        // La ruta real: en macOS /tmp es un enlace y git devuelve la resuelta.
        let folder = folder.canonicalize().unwrap();
        for arguments in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Miyu"],
            vec!["config", "user.email", "miyu@example.com"],
            vec!["config", "commit.gpgsign", "false"],
            vec!["config", "core.autocrlf", "false"],
        ] {
            run(&folder, &arguments).unwrap();
        }
        Some(folder)
    }

    fn commit_all(root: &Path, message: &str) {
        stage(root, &[".".into()]).unwrap();
        commit(root, message).unwrap();
    }

    #[test]
    fn parses_status_output() {
        let (branch, entries) = parse_status(
            "## main...origin/main [ahead 1]\0 M a.tex\0A  b.tex\0R  nuevo.tex\0viejo.tex\0?? c.tex\0",
        );
        assert_eq!(branch, "main");
        assert_eq!(entries.len(), 4);
        assert!(entries[0].unstaged() && !entries[0].staged());
        assert!(entries[1].staged() && !entries[1].unstaged());
        assert_eq!(entries[2].from.as_deref(), Some("viejo.tex"));
        assert_eq!(entries[2].paths(true), ["nuevo.tex", "viejo.tex"]);
        assert_eq!(entries[2].paths(false), ["nuevo.tex"]);
        assert!(entries[3].untracked() && !entries[3].staged() && !entries[3].unstaged());
        assert_eq!(parse_status("## No commits yet on trabajo\0").0, "trabajo");
        assert_eq!(parse_status("## HEAD (no branch)\0").0, "(HEAD suelto)");
    }

    #[test]
    fn formats_relative_dates_in_spanish() {
        assert_eq!(relative(1000, 990), "hace un momento");
        assert_eq!(relative(1000, 1000 - 60), "hace 1 minuto");
        assert_eq!(relative(10_000, 10_000 - 7200), "hace 2 horas");
        assert_eq!(relative(1_000_000, 1_000_000 - 86_400 * 3), "hace 3 días");
        assert_eq!(relative(10_000_000, 10_000_000 - 604_800), "hace 1 semana");
        assert_eq!(relative(100_000_000, 0), "hace 3 años");
    }

    #[test]
    fn splits_a_patch_into_hunks() {
        let text = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n@@ -9 +9 @@\n-x\n+y\n";
        let patch = parse_patch(text);
        assert_eq!(patch.header.lines().count(), 4);
        assert_eq!(patch.hunks.len(), 2);
        assert!(patch.hunks[1].starts_with("@@ -9"));
        let two = format!("{text}diff --git a/g b/g\n@@ -1 +1 @@\n-a\n+b\n");
        assert!(parse_patch(&two).hunks.is_empty());
    }

    #[test]
    fn outside_a_repository_there_is_nothing() {
        let folder = std::env::temp_dir().join(format!("miyu-repo-none-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        assert!(snapshot(&folder).unwrap().is_none());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn stages_unstages_and_commits() {
        let Some(root) = repository("commit") else {
            return;
        };
        fs::write(root.join("a.tex"), "uno\n").unwrap();
        let snap = snapshot(&root).unwrap().unwrap();
        assert_eq!(snap.branch, "main");
        assert!(snap.log.is_empty() && snap.entries[0].untracked());
        // Sin nada preparado git se niega y su motivo llega entero.
        assert!(commit(&root, "vacío").unwrap_err().contains("nothing"));
        stage(&root, &["a.tex".into()]).unwrap();
        let snap = snapshot(&root).unwrap().unwrap();
        assert!(snap.entries[0].staged());
        unstage(&root, &["a.tex".into()]).unwrap();
        assert!(snapshot(&root).unwrap().unwrap().entries[0].untracked());
        stage(&root, &["a.tex".into()]).unwrap();
        commit(&root, "primero\n\ncuerpo").unwrap();
        let snap = snapshot(&root).unwrap().unwrap();
        assert!(snap.entries.is_empty());
        assert_eq!(snap.log.len(), 1);
        assert_eq!(snap.log[0].subject, "primero");
        assert_eq!(snap.log[0].author, "Miyu");
        fs::write(root.join("a.tex"), "uno\ndos\n").unwrap();
        let snap = snapshot(&root).unwrap().unwrap();
        assert!(snap.entries[0].unstaged());
        let diff = diff(&root, &Target::Unstaged(vec!["a.tex".into()])).unwrap();
        assert!(diff.text.contains("+dos"));
        let shown = super::diff(&root, &Target::Commit(snap.log[0].hash.clone())).unwrap();
        assert!(shown.text.contains("primero") && shown.text.contains("+uno"));
        let new = super::diff(&root, &Target::Untracked("a.tex".into())).unwrap();
        assert!(new.text.contains("+uno"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stages_one_hunk_at_a_time() {
        let Some(root) = repository("hunk") else {
            return;
        };
        let original: String = (1..=30).map(|n| format!("línea {n}\n")).collect();
        fs::write(root.join("f.txt"), &original).unwrap();
        commit_all(&root, "base");
        let changed = original
            .replace("línea 2\n", "línea dos\n")
            .replace("línea 29\n", "línea veintinueve\n");
        fs::write(root.join("f.txt"), changed).unwrap();
        let files = vec!["f.txt".to_string()];
        let patch = parse_patch(&diff(&root, &Target::Unstaged(files.clone())).unwrap().text);
        assert_eq!(patch.hunks.len(), 2);
        apply_hunk(&root, &patch.header, &patch.hunks[0], false).unwrap();
        let staged = diff(&root, &Target::Staged(files.clone())).unwrap().text;
        assert!(staged.contains("+línea dos") && !staged.contains("veintinueve"));
        let rest = diff(&root, &Target::Unstaged(files.clone())).unwrap().text;
        assert!(rest.contains("+línea veintinueve") && !rest.contains("línea dos"));
        // Y se puede quitar de los preparados sin tocar el archivo.
        let patch = parse_patch(&staged);
        apply_hunk(&root, &patch.header, &patch.hunks[0], true).unwrap();
        assert!(diff(&root, &Target::Staged(files)).unwrap().text.is_empty());
        assert!(
            fs::read_to_string(root.join("f.txt"))
                .unwrap()
                .contains("línea dos")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn handles_a_renamed_and_modified_file() {
        let Some(root) = repository("rename") else {
            return;
        };
        fs::write(root.join("notas.md"), "uno\ndos\ntres\n").unwrap();
        commit_all(&root, "inicio");
        run(&root, &["mv", "notas.md", "apuntes.md"]).unwrap();
        fs::write(root.join("apuntes.md"), "uno\ndos\ntres\ncuatro\n").unwrap();
        let entry = snapshot(&root).unwrap().unwrap().entries.remove(0);
        assert_eq!((entry.index, entry.tree), ('R', 'M'));
        let unstaged = diff(&root, &Target::Unstaged(entry.paths(false))).unwrap();
        assert!(unstaged.text.contains("+cuatro"));
        let staged = diff(&root, &Target::Staged(entry.paths(true))).unwrap();
        assert!(staged.text.contains("rename from notas.md"));
        stage(&root, &entry.paths(false)).unwrap();
        let entry = snapshot(&root).unwrap().unwrap().entries.remove(0);
        assert!(entry.staged() && !entry.unstaged());
        unstage(&root, &entry.paths(true)).unwrap();
        let entries = snapshot(&root).unwrap().unwrap().entries;
        assert!(entries.iter().all(|e| !e.staged()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn creates_lists_and_switches_branches() {
        let Some(root) = repository("branch") else {
            return;
        };
        fs::write(root.join("a.txt"), "a\n").unwrap();
        commit_all(&root, "inicio");
        create_branch(&root, "trabajo").unwrap();
        fs::write(root.join("a.txt"), "b\n").unwrap();
        commit_all(&root, "cambio");
        let list = branches(&root).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|b| b.name == "trabajo" && b.current));
        assert!(create_branch(&root, "mala rama~").is_err());
        assert!(create_branch(&root, "").is_err());
        checkout(&root, "main").unwrap();
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "a\n");
        assert_eq!(snapshot(&root).unwrap().unwrap().branch, "main");
        assert!(checkout(&root, "no-existe").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stashes_and_restores_changes() {
        let Some(root) = repository("stash") else {
            return;
        };
        fs::write(root.join("a.txt"), "a\n").unwrap();
        commit_all(&root, "inicio");
        assert!(stash_push(&root, "").is_err());
        fs::write(root.join("a.txt"), "cambiado\n").unwrap();
        stash_push(&root, "a medias").unwrap();
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "a\n");
        let list = stashes(&root).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].subject.contains("a medias") && list[0].name == "stash@{0}");
        stash_pop(&root, &list[0].name).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "cambiado\n"
        );
        assert!(stashes(&root).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn blames_a_line() {
        let Some(root) = repository("blame") else {
            return;
        };
        let file = root.join("a.txt");
        fs::write(&file, "uno\ndos\n").unwrap();
        commit_all(&root, "inicio");
        fs::write(&file, "uno\ndos\ntres\n").unwrap();
        let first = blame(&file, 0).unwrap();
        assert!(first.committed && first.author == "Miyu" && first.summary == "inicio");
        assert_eq!(first.hash.len(), 7);
        assert!(!blame(&file, 2).unwrap().committed);
        assert!(blame(&root.join("nada.txt"), 0).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
