//! Lector de los archivos `.synctex(.gz)` que escriben Tectonic y los motores
//! de TeX Live. Evita depender del programa `synctex`, que solo viene con una
//! distribución TeX completa.

use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

/// Unidades de SyncTeX (sp) por punto PDF.
const SP_PER_BP: f64 = 65781.76;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Caja horizontal con contenido: `(`.
    Box,
    /// Caja vacía, kern, pegamento, matemática o posición: `h v k g $ x`.
    Point,
}

struct Record {
    kind: Kind,
    page: usize,
    tag: u32,
    line: usize,
    /// Puntos PDF desde la esquina superior izquierda; `y` es la línea base.
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    depth: f32,
    /// Caja horizontal que lo contiene.
    parent: Option<usize>,
}

pub struct Index {
    inputs: HashMap<u32, PathBuf>,
    records: Vec<Record>,
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || (a.file_name() == b.file_name()
            && a.canonicalize()
                .is_ok_and(|a| b.canonicalize().is_ok_and(|b| a == b)))
}

impl Index {
    /// Lee los datos de sincronización que acompañan a `pdf`.
    pub fn load(pdf: &Path) -> Result<Self, String> {
        let missing = "No hay datos de SyncTeX. Compila el documento de nuevo para generarlos.";
        let packed = pdf.with_extension("synctex.gz");
        let text = if packed.is_file() {
            let mut bytes = Vec::new();
            flate2::read::MultiGzDecoder::new(fs::File::open(&packed).map_err(|e| e.to_string())?)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("No pude leer {}: {e}", packed.display()))?;
            bytes
        } else {
            fs::read(pdf.with_extension("synctex")).map_err(|_| missing.to_string())?
        };
        Ok(Self::parse(
            &String::from_utf8_lossy(&text),
            pdf.parent().unwrap_or(Path::new(".")),
        ))
    }

    pub fn parse(text: &str, base: &Path) -> Self {
        let mut inputs = HashMap::new();
        let mut records: Vec<Record> = Vec::new();
        let (mut unit, mut magnification) = (1.0, 1000.0);
        let (mut x_offset, mut y_offset) = (0.0, 0.0);
        let mut content = false;
        let mut page = 0;
        let mut open: Vec<usize> = Vec::new();
        for line in text.lines() {
            if !content {
                let number = |v: &str| v.trim().parse::<f64>().ok();
                if let Some(rest) = line.strip_prefix("Input:") {
                    if let Some((tag, name)) = rest.split_once(':')
                        && let Ok(tag) = tag.parse()
                        && !name.is_empty()
                    {
                        let path = Path::new(name);
                        inputs.insert(
                            tag,
                            if path.is_absolute() {
                                path.into()
                            } else {
                                base.join(path)
                            },
                        );
                    }
                } else if let Some(v) = line.strip_prefix("Unit:") {
                    unit = number(v).unwrap_or(1.0);
                } else if let Some(v) = line.strip_prefix("Magnification:") {
                    magnification = number(v).unwrap_or(1000.0);
                } else if let Some(v) = line.strip_prefix("X Offset:") {
                    x_offset = number(v).unwrap_or(0.0);
                } else if let Some(v) = line.strip_prefix("Y Offset:") {
                    y_offset = number(v).unwrap_or(0.0);
                } else if line.starts_with("Content:") {
                    content = true;
                }
                continue;
            }
            let Some(mark) = line.chars().next() else {
                continue;
            };
            let rest = &line[mark.len_utf8()..];
            match mark {
                '{' => {
                    page = rest.trim().parse::<usize>().unwrap_or(page + 1);
                    open.clear();
                }
                ')' => {
                    open.pop();
                }
                // Los archivos se declaran también a mitad del contenido.
                'I' => {
                    if let Some(rest) = line.strip_prefix("Input:")
                        && let Some((tag, name)) = rest.split_once(':')
                        && let Ok(tag) = tag.parse()
                        && !name.is_empty()
                    {
                        let path = Path::new(name);
                        inputs.insert(
                            tag,
                            if path.is_absolute() {
                                path.into()
                            } else {
                                base.join(path)
                            },
                        );
                    }
                }
                'P' if line.starts_with("Postamble:") => break,
                '(' | 'h' | 'v' | 'k' | 'g' | '$' | 'x' => {
                    let Some((link, geometry)) = rest.split_once(':') else {
                        continue;
                    };
                    let mut link = link.split(',');
                    let (Some(tag), Some(source_line)) = (
                        link.next().and_then(|v| v.parse().ok()),
                        link.next().and_then(|v| v.parse().ok()),
                    ) else {
                        continue;
                    };
                    let (point, size) = geometry.split_once(':').unwrap_or((geometry, ""));
                    let Some((x, y)) = point.split_once(',') else {
                        continue;
                    };
                    let (Ok(x), Ok(y)) = (x.parse::<f64>(), y.parse::<f64>()) else {
                        continue;
                    };
                    let scale = unit * magnification / 1000.0 / SP_PER_BP;
                    let mut size = size.split(',').map(|v| v.parse::<f64>().unwrap_or(0.0));
                    let mut next = || (size.next().unwrap_or(0.0) * scale) as f32;
                    let kind = if mark == '(' { Kind::Box } else { Kind::Point };
                    records.push(Record {
                        kind,
                        page,
                        tag,
                        line: source_line,
                        x: ((x + x_offset) * scale) as f32,
                        y: ((y + y_offset) * scale) as f32,
                        width: next(),
                        height: next(),
                        depth: next(),
                        parent: open.last().copied(),
                    });
                    if kind == Kind::Box {
                        open.push(records.len() - 1);
                    }
                }
                _ => {}
            }
        }
        Self { inputs, records }
    }

    /// Página (desde 0) y posición en puntos PDF de una línea del código.
    pub fn forward(&self, source: &Path, line: usize) -> Option<(usize, f32, f32)> {
        let tags: Vec<u32> = self
            .inputs
            .iter()
            .filter(|(_, path)| same_file(path, source))
            .map(|(tag, _)| *tag)
            .collect();
        let candidates = || {
            self.records
                .iter()
                .filter(|r| tags.contains(&r.tag) && r.page > 0)
        };
        // Una línea sin material propio (en blanco, un comentario) lleva a la
        // siguiente que sí lo tiene y, si no hay, a la anterior.
        let best = candidates()
            .map(|r| r.line)
            .filter(|l| *l >= line)
            .min()
            .or_else(|| candidates().map(|r| r.line).max())?;
        let first = candidates().find(|r| r.line == best)?;
        let x = candidates()
            .filter(|r| r.line == best && r.page == first.page && (r.y - first.y).abs() < 1.0)
            .map(|r| r.x)
            .fold(first.x, f32::min);
        Some((first.page - 1, x, first.y))
    }

    /// Archivo y línea (desde 1) del punto del PDF, en puntos desde arriba a la izquierda.
    pub fn backward(&self, page: usize, x: f32, y: f32) -> Option<(PathBuf, usize)> {
        let on_page = || {
            self.records
                .iter()
                .enumerate()
                .filter(move |(_, r)| r.page == page + 1)
        };
        let bounds = |r: &Record| {
            let (left, right) = (r.x.min(r.x + r.width), r.x.max(r.x + r.width));
            (left, right, r.y - r.height, r.y + r.depth)
        };
        // La caja más pequeña bajo el puntero es la más cercana al texto.
        let boxed = on_page()
            .filter(|(_, r)| r.kind == Kind::Box && r.width != 0.0)
            .filter(|(_, r)| {
                let (left, right, top, bottom) = bounds(r);
                (left..=right).contains(&x) && (top - 1.0..=bottom + 1.0).contains(&y)
            })
            .min_by(|(_, a), (_, b)| {
                let area = |r: &Record| r.width.abs() * (r.height + r.depth);
                area(a).total_cmp(&area(b))
            });
        let found = match boxed {
            Some((index, parent)) => {
                // Dentro de un párrafo la caja lleva la línea donde acaba; sus
                // piezas llevan la línea de cada palabra.
                let pieces = self.records[index + 1..]
                    .iter()
                    .take_while(|r| r.page == parent.page)
                    .filter(|r| r.parent == Some(index) && r.kind == Kind::Point);
                pieces
                    .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
                    .unwrap_or(parent)
            }
            None => {
                on_page()
                    .map(|(_, r)| r)
                    .filter(|r| r.kind == Kind::Box && r.width != 0.0)
                    .min_by(|a, b| {
                        let distance = |r: &Record| {
                            let (left, right, top, bottom) = bounds(r);
                            let dx = (left - x).max(x - right).max(0.0);
                            let dy = (top - y).max(y - bottom).max(0.0);
                            // El margen lateral pertenece a la línea de al lado.
                            dy * 4.0 + dx
                        };
                        distance(a).total_cmp(&distance(b))
                    })?
            }
        };
        let path = self.inputs.get(&found.tag)?;
        Some((path.clone(), found.line.max(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "SyncTeX Version:1
Input:1:/proyecto/main.tex
Input:2:
Input:8:cap/uno.tex
Output:pdf
Magnification:1000
Unit:1
X Offset:0
Y Offset:0
Content:
!395
{1
[1,9:4736287,46220575:26673152,41484288,0
(1,4:8799519,8874073:22609920,664378,5661
g1,4:8799519,8874073
)
(1,6:8799519,10310163:22609920,462029,127139
k1,5:11279461,10310163:173075
k1,5:20959426,10310163:173074
k1,6:31409439,10310163:0
)
(8,1:8799519,11096595:22609920,462029,135003
h1,7:8799519,11096595:983040,0,0
g1,7:12404655,11096595
k8,1:31409439,11096595:12097944
)
]
}1
{2
(1,11:8799519,8874073:22609920,462029,127139
g1,11:9799519,8874073
)
}2
Postamble:
Count:20
";

    #[test]
    fn forward_and_backward() {
        let index = Index::parse(SAMPLE, Path::new("/proyecto"));
        let main = Path::new("/proyecto/main.tex");
        let included = Path::new("/proyecto/cap/uno.tex");
        // La línea 5 está en la segunda caja de la primera página.
        let (page, x, y) = index.forward(main, 5).unwrap();
        assert_eq!(page, 0);
        assert!((x - 11279461.0 / 65781.76).abs() < 0.01, "{x}");
        assert!((y - 10310163.0 / 65781.76).abs() < 0.01, "{y}");
        // Una línea sin material lleva a la siguiente con texto.
        assert_eq!(index.forward(main, 8).unwrap().0, 1);
        assert_eq!(index.forward(main, 99).unwrap().0, 1);
        assert_eq!(index.forward(included, 1).unwrap().0, 0);
        assert!(index.forward(Path::new("/otro/main.tex"), 1).is_none());

        let point = |sp: f64| (sp / 65781.76) as f32;
        // Sobre el párrafo, cerca del primer kern: línea 5 y no la 6 de la caja.
        assert_eq!(
            index.backward(0, point(11300000.0), point(10200000.0)),
            Some((main.into(), 5))
        );
        assert_eq!(
            index.backward(0, point(31000000.0), point(10200000.0)),
            Some((main.into(), 6))
        );
        // El archivo incluido, también desde el margen izquierdo.
        assert_eq!(
            index.backward(0, point(30000000.0), point(11000000.0)),
            Some((included.into(), 1))
        );
        assert_eq!(
            index.backward(1, 10.0, point(8800000.0)),
            Some((main.into(), 11))
        );
        assert!(index.backward(5, 10.0, 10.0).is_none());
    }
}
