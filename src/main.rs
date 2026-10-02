mod app;
mod backdrop;
mod compiler;
mod config;
mod custom;
mod editor;
mod format;
mod highlight;
mod latex;
mod layout;
mod marks;
mod preview;
mod spell;
mod synctex;
mod syntax;
mod theme;
use std::io;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => println!(
            "MiyuLaTeX · Editor de LaTeX, Markdown y código\n\n  miyu [carpeta | archivo]\n  miyu --render-pdf entrada.pdf salida.png [página]\n  miyu --render-background foto salida.png ancho alto [punto=2] [intensidad=1] [fondo=16131f] [dither|plain]\n\nAbre texto UTF-8, PDF e imágenes. Cmd/Ctrl+N nuevo, Cmd/Ctrl+O abrir, Cmd/Ctrl+S guardar, F5 compilar LaTeX, F1 ayuda."
        ),
        Some("--version" | "-V") => println!("miyu {}", env!("CARGO_PKG_VERSION")),
        Some("--render-pdf") => {
            if args.len() < 3 {
                return Err(io::Error::other(
                    "Uso: miyu --render-pdf entrada.pdf salida.png [página]",
                )
                .into());
            }
            let page = args
                .get(3)
                .map(|s| s.parse::<usize>())
                .transpose()?
                .unwrap_or(1);
            let (img, count, _) =
                preview::render_page(std::fs::read(&args[1])?, page.saturating_sub(1), false)
                    .map_err(io::Error::other)?;
            img.save(&args[2])?;
            println!("{count} páginas, guardada en {}", args[2]);
        }
        Some("--render-background") => {
            if args.len() < 5 {
                return Err(io::Error::other("Uso: miyu --render-background foto salida.png ancho alto [punto] [intensidad] [fondo] [dither|plain]").into());
            }
            let width = args[3].parse::<u32>()?;
            let height = args[4].parse::<u32>()?;
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                return Err(io::Error::other("Dimensiones entre 1 y 8192").into());
            }
            let dot = args
                .get(5)
                .map(|s| s.parse::<u32>())
                .transpose()?
                .unwrap_or(2);
            let intensity = args
                .get(6)
                .map(|s| s.parse::<f64>())
                .transpose()?
                .unwrap_or(1.0);
            if dot == 0 || !intensity.is_finite() || !(0.0..=1.0).contains(&intensity) {
                return Err(io::Error::other("Punto mayor que 0 e intensidad entre 0 y 1").into());
            }
            let base = u32::from_str_radix(
                args.get(7)
                    .map_or("16131f", String::as_str)
                    .trim_start_matches('#'),
                16,
            )?;
            if base > 0xffffff {
                return Err(io::Error::other("Fondo RGB de seis cifras hexadecimales").into());
            }
            let mut bg = backdrop::Backdrop::default();
            bg.load(&config::clean_path(&args[1]))
                .map_err(io::Error::other)?;
            bg.render_pixels(
                width,
                height,
                dot,
                theme::hex(base),
                intensity,
                args.get(8).is_some_and(|s| s == "plain"),
            )
            .save(&args[2])?;
        }
        Some(a) if a.starts_with('-') => {
            return Err(io::Error::other("Opción desconocida. Usa miyu --help").into());
        }
        _ => app::run(args.first().map(|a| config::clean_path(a)))?,
    }
    Ok(())
}
