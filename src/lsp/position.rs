//! Posiciones de LSP (línea y unidades UTF-16) frente a las del editor
//! (fila y número de carácter).

/// Unidades UTF-16 que ocupan los primeros `col` caracteres de `line`.
pub fn utf16_col(line: &str, col: usize) -> u32 {
    line.chars().take(col).map(|c| c.len_utf16() as u32).sum()
}

/// Carácter de `line` en que cae la unidad UTF-16 `units`. Más allá del final
/// queda el final de la línea; dentro de un par subrogado, el carácter entero.
pub fn char_col(line: &str, units: u32) -> usize {
    let mut seen = 0;
    for (col, c) in line.chars().enumerate() {
        let next = seen + c.len_utf16() as u32;
        if units < next {
            return col;
        }
        seen = next;
    }
    line.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_identity() {
        assert_eq!(utf16_col("hello", 3), 3);
        assert_eq!(char_col("hello", 3), 3);
    }

    #[test]
    fn accents_take_one_unit_and_emoji_two() {
        let line = "é😀añ𝔸x";
        // é(1) 😀(2) a(1) ñ(1) 𝔸(2) x(1)
        let units = [0, 1, 3, 4, 5, 7, 8];
        for (col, unit) in units.iter().enumerate() {
            assert_eq!(utf16_col(line, col), *unit, "col {col}");
            assert_eq!(char_col(line, *unit), col, "unit {unit}");
        }
    }

    #[test]
    fn edges_are_clamped() {
        assert_eq!(char_col("a😀", 99), 2);
        assert_eq!(utf16_col("a😀", 99), 3);
        // Mitad de un par subrogado: el emoji entero.
        assert_eq!(char_col("a😀b", 2), 1);
        assert_eq!(char_col("", 5), 0);
    }
}
