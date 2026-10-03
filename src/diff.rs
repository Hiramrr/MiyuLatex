//! Diferencias por líneas entre dos textos.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Same,
    Removed,
    Added,
}

/// Celdas de la tabla a partir de las cuales ya no se busca el detalle.
const LIMIT: usize = 4_000_000;

/// Líneas de `old` y `new` en orden, con las que sobran de uno y de otro.
pub fn lines<'a>(old: &'a str, new: &'a str) -> Vec<(Change, &'a str)> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (x, y) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let mut out: Vec<_> = a[..head].iter().map(|line| (Change::Same, *line)).collect();
    if x.len().saturating_mul(y.len()) > LIMIT {
        // Demasiado distinto para alinearlo: todo lo anterior y luego todo lo nuevo.
        out.extend(x.iter().map(|line| (Change::Removed, *line)));
        out.extend(y.iter().map(|line| (Change::Added, *line)));
    } else {
        // common[i][j]: líneas comunes entre x[i..] e y[j..].
        let width = y.len() + 1;
        let mut common = vec![0u32; (x.len() + 1) * width];
        for i in (0..x.len()).rev() {
            for j in (0..y.len()).rev() {
                common[i * width + j] = if x[i] == y[j] {
                    common[(i + 1) * width + j + 1] + 1
                } else {
                    common[(i + 1) * width + j].max(common[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < x.len() || j < y.len() {
            if i < x.len() && j < y.len() && x[i] == y[j] {
                out.push((Change::Same, x[i]));
                i += 1;
                j += 1;
            } else if j == y.len()
                || (i < x.len() && common[(i + 1) * width + j] >= common[i * width + j + 1])
            {
                out.push((Change::Removed, x[i]));
                i += 1;
            } else {
                out.push((Change::Added, y[j]));
                j += 1;
            }
        }
    }
    out.extend(a[a.len() - tail..].iter().map(|line| (Change::Same, *line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use Change::*;

    #[test]
    fn aligns_changed_lines() {
        assert_eq!(
            lines("a\nb\nc\nd", "a\nx\nc\nd\ne"),
            [
                (Same, "a"),
                (Removed, "b"),
                (Added, "x"),
                (Same, "c"),
                (Same, "d"),
                (Added, "e")
            ]
        );
        assert_eq!(lines("a\nb", "a\nb"), [(Same, "a"), (Same, "b")]);
        assert_eq!(lines("", "a"), [(Added, "a")]);
        assert_eq!(
            lines("a\nb\nc", "c"),
            [(Removed, "a"), (Removed, "b"), (Same, "c")]
        );
        // Una línea repetida no confunde el principio con el final.
        assert_eq!(lines("a\na", "a"), [(Same, "a"), (Removed, "a")]);
    }
}
