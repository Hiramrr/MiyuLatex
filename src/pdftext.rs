//! Texto de las páginas de un PDF con la caja de cada carácter, para buscarlo,
//! seleccionarlo y copiarlo en el visor.

use hayro::{
    hayro_interpret::{
        BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache,
        InterpreterSettings, Paint, PathDrawMode, SoftMask, TransformExt, font::Glyph,
        hayro_cmap::BfString, interpret_page,
    },
    hayro_syntax::page::Page,
    vello_cpu::kurbo::{Affine, BezPath, Point, Rect},
};

/// Caja de un carácter en puntos PDF desde la esquina superior izquierda de
/// la página: izquierda, arriba, derecha y abajo.
pub type Bounds = [f32; 4];

/// Texto de una página en orden de lectura; las líneas acaban en `\n`.
#[derive(Default)]
pub struct PageText {
    chars: Vec<char>,
    boxes: Vec<Bounds>,
}

/// Una letra tal como la dibuja el PDF.
struct Piece {
    text: String,
    x: f32,
    /// Línea base.
    y: f32,
    width: f32,
    size: f32,
}

#[derive(Default)]
struct Collector {
    pieces: Vec<Piece>,
}

impl<'a> Device<'a> for Collector {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'a>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'a>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        _: &Paint<'a>,
        _: &GlyphDrawMode,
    ) {
        let text = match glyph.as_unicode() {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => return,
        };
        let matrix = transform * glyph_transform;
        let origin = matrix * Point::ZERO;
        let previous = self.pieces.last().map_or(10.0, |p| p.size);
        let (width, size) = match glyph {
            // El contorno mide 1000 unidades por eme.
            Glyph::Outline(outline) => {
                let advance = outline.advance_width().unwrap_or(500.0) as f64;
                let right = matrix * Point::new(advance, 0.0);
                let up = matrix * Point::new(0.0, 1000.0);
                (
                    (right - origin).hypot() as f32,
                    (up - origin).hypot() as f32,
                )
            }
            // Las fuentes Type3 no dicen su tamaño: se supone el de la letra anterior.
            Glyph::Type3(_) => (previous * 0.5, previous),
        };
        if size.is_finite() && size > 0.0 && width.is_finite() {
            self.pieces.push(Piece {
                text,
                x: origin.x as f32,
                y: origin.y as f32,
                width,
                size,
            });
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// Lee el texto de una página. `cache` se comparte entre las páginas de un PDF.
pub fn extract<'a>(page: &'a Page<'a>, cache: &InterpreterCache<'a>) -> PageText {
    let (width, height) = page.render_dimensions();
    let mut context = Context::new(
        page.initial_transform(true).to_kurbo(),
        Rect::new(0.0, 0.0, width as f64, height as f64),
        cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut collector = Collector::default();
    interpret_page(page, &mut context, &mut collector);
    PageText::from_pieces(&collector.pieces)
}

/// Las ligaduras se buscan y se copian como sus letras.
fn letters(c: char) -> &'static str {
    match c {
        'ﬀ' => "ff",
        'ﬁ' => "fi",
        'ﬂ' => "fl",
        'ﬃ' => "ffi",
        'ﬄ' => "ffl",
        _ => "",
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

impl PageText {
    /// TeX no escribe espacios ni saltos de línea: se deducen de los huecos.
    fn from_pieces(pieces: &[Piece]) -> Self {
        let mut page = Self::default();
        // Línea base, borde derecho y tamaño de la letra anterior.
        let mut last: Option<(f32, f32, f32)> = None;
        for piece in pieces {
            let (top, bottom) = (piece.y - 0.8 * piece.size, piece.y + 0.22 * piece.size);
            if let Some((y, right, size)) = last {
                let size = size.max(piece.size);
                let gap = piece.x - right;
                if (piece.y - y).abs() > 0.6 * size || gap < -2.0 * size {
                    page.chars.push('\n');
                    page.boxes
                        .push([right, y - 0.8 * size, right, y + 0.22 * size]);
                } else if gap > 0.18 * size {
                    page.chars.push(' ');
                    page.boxes.push([right, top, piece.x, bottom]);
                }
            }
            let text: String = piece
                .text
                .chars()
                .map(|c| match letters(c) {
                    "" => c.to_string(),
                    plain => plain.to_string(),
                })
                .collect();
            let count = text.chars().count().max(1) as f32;
            for (i, c) in text.chars().enumerate() {
                let left = piece.x + piece.width * i as f32 / count;
                page.chars.push(c);
                page.boxes
                    .push([left, top, left + piece.width / count, bottom]);
            }
            last = Some((piece.y, piece.x + piece.width, piece.size));
        }
        page
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// Tramos `[inicio, fin)` donde aparece `query`, sin distinguir mayúsculas.
    /// Un salto de línea vale por un espacio y una palabra partida con guion
    /// al final de la línea se busca entera.
    pub fn find(&self, query: &str) -> Vec<(usize, usize)> {
        let query: Vec<char> = query.chars().map(lower).collect();
        if query.is_empty() {
            return Vec::new();
        }
        // Carácter con que se compara y su posición en la página.
        let mut flat = Vec::with_capacity(self.chars.len());
        let mut i = 0;
        while i < self.chars.len() {
            match self.chars[i] {
                '-' if self.chars.get(i + 1) == Some(&'\n') => i += 1,
                '\n' => flat.push((' ', i)),
                c => flat.push((lower(c), i)),
            }
            i += 1;
        }
        let mut found = Vec::new();
        let mut start = 0;
        while start + query.len() <= flat.len() {
            if flat[start..start + query.len()]
                .iter()
                .map(|(c, _)| c)
                .eq(query.iter())
            {
                found.push((flat[start].1, flat[start + query.len() - 1].1 + 1));
                start += query.len();
            } else {
                start += 1;
            }
        }
        found
    }

    /// Texto de un tramo, como se copia al portapapeles.
    pub fn text(&self, start: usize, end: usize) -> String {
        self.chars[start.min(self.chars.len())..end.min(self.chars.len())]
            .iter()
            .collect()
    }

    /// Cajas que cubren un tramo, una por línea.
    pub fn rects(&self, start: usize, end: usize) -> Vec<Bounds> {
        let mut rects: Vec<Bounds> = Vec::new();
        let mut open = false;
        for i in start..end.min(self.chars.len()) {
            if self.chars[i] == '\n' {
                open = false;
                continue;
            }
            let b = self.boxes[i];
            match rects.last_mut() {
                Some(rect) if open => {
                    *rect = [
                        rect[0].min(b[0]),
                        rect[1].min(b[1]),
                        rect[2].max(b[2]),
                        rect[3].max(b[3]),
                    ];
                }
                _ => rects.push(b),
            }
            open = true;
        }
        rects
    }

    /// Carácter más cercano a un punto de la página.
    pub fn nearest(&self, x: f32, y: f32) -> Option<usize> {
        let distance = |b: &Bounds| {
            let dx = (b[0] - x).max(x - b[2]).max(0.0);
            let dy = (b[1] - y).max(y - b[3]).max(0.0);
            // Cuenta más acertar con la línea que con la letra.
            dx + 4.0 * dy
        };
        (0..self.chars.len())
            .filter(|i| self.chars[*i] != '\n')
            .min_by(|a, b| distance(&self.boxes[*a]).total_cmp(&distance(&self.boxes[*b])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(text: &str, x: f32, y: f32) -> Piece {
        Piece {
            text: text.into(),
            x,
            y,
            width: 5.0,
            size: 10.0,
        }
    }

    #[test]
    fn reads_the_example_article() {
        let data = std::fs::read("examples/articulo.pdf").unwrap();
        let pdf = hayro::hayro_syntax::Pdf::new(data).unwrap();
        let (width, height) = pdf.pages()[0].render_dimensions();
        let page = extract(&pdf.pages()[0], &InterpreterCache::new());
        let text = page.text(0, page.len());
        assert!(text.contains("Introducción"), "{text}");
        // Las cajas caen dentro de la página.
        let (start, end) = page.find("introducción")[0];
        let rect = page.rects(start, end)[0];
        assert!(rect[0] > 0.0 && rect[2] < width && rect[1] > 0.0 && rect[3] < height);
        assert!(
            rect[2] - rect[0] > 30.0 && rect[3] - rect[1] < 30.0,
            "{rect:?}"
        );
    }

    #[test]
    fn spaces_lines_search_and_boxes() {
        // «La ﬁ-» en una línea y «nal Ya» en la siguiente.
        let pieces = [
            piece("L", 0.0, 10.0),
            piece("a", 5.0, 10.0),
            piece("ﬁ", 14.0, 10.0),
            piece("-", 19.0, 10.0),
            piece("n", 0.0, 22.0),
            piece("a", 5.0, 22.0),
            piece("l", 10.0, 22.0),
            piece("Y", 19.0, 22.0),
            piece("a", 24.0, 22.0),
        ];
        let page = PageText::from_pieces(&pieces);
        assert_eq!(page.text(0, page.len()), "La fi-\nnal Ya");
        // La palabra partida se encuentra entera, sin distinguir mayúsculas.
        assert_eq!(page.find("FINAL"), [(3, 10)]);
        assert_eq!(page.find("la"), [(0, 2)]);
        assert_eq!(page.find("a"), [(1, 2), (8, 9), (12, 13)]);
        assert_eq!(page.find("nal ya"), [(7, 13)]);
        assert!(page.find("").is_empty());
        assert!(page.find("zzz").is_empty());
        // Un tramo que cruza de línea da una caja por línea.
        let rects = page.rects(3, 10);
        assert_eq!(rects.len(), 2);
        assert_eq!((rects[0][0], rects[0][2]), (14.0, 24.0));
        assert_eq!((rects[1][0], rects[1][2]), (0.0, 15.0));
        assert_eq!(page.nearest(6.0, 21.0), Some(8));
        assert_eq!(page.nearest(100.0, 9.0), Some(5));
    }
}
