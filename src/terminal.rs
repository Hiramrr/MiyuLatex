//! Terminal interactiva con PTY, historial acotado y secuencias ANSI.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};

use eframe::egui::{self, Color32, FontId, Id, Key, Stroke};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize};

use crate::compiler;

#[derive(Default)]
struct Replies(Vec<u8>);

impl vt100::Callbacks for Replies {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        match (i1, params.first().and_then(|p| p.first()), c) {
            (None, Some(5), 'n') => self.0.extend_from_slice(b"\x1b[0n"),
            (None, Some(6), 'n') => {
                let (row, col) = screen.cursor_position();
                self.0
                    .extend_from_slice(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
            }
            (None, _, 'c') => self.0.extend_from_slice(b"\x1b[?1;2c"),
            _ => {}
        }
    }
}

pub struct Terminal {
    pub id: Id,
    pub name: String,
    pub directory: PathBuf,
    pub exit: Option<Result<portable_pty::ExitStatus, String>>,
    pub error: Option<String>,
    parser: vt100::Parser<Replies>,
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    input: Option<SyncSender<Vec<u8>>>,
    output: Receiver<Vec<u8>>,
    done: Receiver<Result<portable_pty::ExitStatus, String>>,
    selection: Option<[(u16, u16); 2]>,
}

pub fn shell() -> CommandBuilder {
    CommandBuilder::new_default_prog()
}

impl Terminal {
    pub fn spawn(
        id: Id,
        name: String,
        directory: &Path,
        mut command: CommandBuilder,
        ctx: &egui::Context,
    ) -> Result<Self, String> {
        command.cwd(directory);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        if let Ok(path) = std::env::join_paths(compiler::search_paths()) {
            command.env("PATH", path);
        }
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| e.to_string())?;
        drop(pair.slave);
        let killer = child.clone_killer();
        // Las colas y el historial tienen límites aunque un proceso escriba sin parar.
        let (out_tx, output) = mpsc::sync_channel(64);
        let read_ctx = ctx.clone();
        thread::spawn(move || {
            let mut buffer = [0; 8192];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 || out_tx.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
                read_ctx.request_repaint();
            }
        });
        let (input, in_rx) = mpsc::sync_channel::<Vec<u8>>(32);
        thread::spawn(move || {
            while let Ok(bytes) = in_rx.recv() {
                if writer
                    .write_all(&bytes)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let (done_tx, done) = mpsc::channel();
        let wait_ctx = ctx.clone();
        thread::spawn(move || {
            let _ = done_tx.send(child.wait().map_err(|e| e.to_string()));
            wait_ctx.request_repaint();
        });
        Ok(Self {
            id,
            name,
            directory: directory.into(),
            exit: None,
            error: None,
            parser: vt100::Parser::new_with_callbacks(24, 80, 5000, Replies::default()),
            master: pair.master,
            killer,
            input: Some(input),
            output,
            done,
            selection: None,
        })
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        // Un límite por cuadro mantiene accesible la interfaz durante salidas largas.
        let mut processed = 0;
        for _ in 0..128 {
            match self.output.try_recv() {
                Ok(bytes) => {
                    self.parser.process(&bytes);
                    processed += 1;
                }
                Err(_) => break,
            }
        }
        let replies = std::mem::take(&mut self.parser.callbacks_mut().0);
        if !replies.is_empty() {
            self.send(replies);
        }
        if self.exit.is_none() {
            match self.done.try_recv() {
                Ok(exit) => self.exit = Some(exit),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.exit = Some(Err("La sesión se interrumpió".into()));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.exit.is_some() {
            self.input.take();
        }
        // Puede quedar salida en la cola después de la última petición del lector.
        if processed == 128 {
            ctx.request_repaint();
        }
    }

    pub fn send(&mut self, bytes: Vec<u8>) {
        if self.exit.is_none()
            && let Some(input) = &self.input
            && let Err(e) = input.try_send(bytes)
        {
            self.error = Some(match e {
                mpsc::TrySendError::Full(_) => {
                    "La entrada está ocupada. Espera y vuelve a pegar.".into()
                }
                mpsc::TrySendError::Disconnected(_) => "La sesión ya no acepta entrada.".into(),
            });
        }
    }

    pub fn interrupt(&mut self) {
        self.send(vec![3]);
    }

    pub fn clear(&mut self) {
        let (rows, cols) = self.parser.screen().size();
        self.parser = vt100::Parser::new_with_callbacks(rows, cols, 5000, Replies::default());
        self.selection = None;
        self.send(vec![12]);
    }

    fn scroll(&mut self, lines: isize) {
        let offset = self
            .parser
            .screen()
            .scrollback()
            .saturating_add_signed(lines);
        self.parser.screen_mut().set_scrollback(offset);
        self.selection = None;
    }

    fn copied_text(&self) -> String {
        match self.selection {
            Some([a, b]) => {
                let (start, end) = if a <= b { (a, b) } else { (b, a) };
                self.parser
                    .screen()
                    .contents_between(start.0, start.1, end.0, end.1 + 1)
            }
            None => self.parser.screen().contents(),
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        focus: bool,
        font_size: f32,
        fg: Color32,
        bg: Color32,
    ) {
        let font = FontId::monospace(font_size);
        let cell_width = ui.fonts_mut(|f| f.glyph_width(&font, 'M')).max(1.0);
        let line_height = ui.fonts_mut(|f| f.row_height(&font)).max(1.0);
        let size = ui.available_size();
        if size.x < cell_width * 2.0 || size.y < line_height * 2.0 {
            ui.label("Amplía el panel para ver la terminal.");
            return;
        }
        let rows = ((size.y / line_height).floor() as u16).clamp(2, 200);
        let cols = ((size.x / cell_width).floor() as u16).clamp(2, 500);
        if self.parser.screen().size() != (rows, cols) {
            match self.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            }) {
                Ok(()) => self.parser.screen_mut().set_size(rows, cols),
                Err(e) => self.error = Some(format!("No pude ajustar la terminal: {e}")),
            }
            self.selection = None;
        }
        let (_, rect) = ui.allocate_space(size);
        let response = ui.interact(rect, self.id, egui::Sense::click_and_drag());
        if focus || response.clicked() || response.drag_started() {
            response.request_focus();
        }
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                self.id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            )
        });
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::TextEdit,
                true,
                format!("Terminal {}", self.name),
            )
        });
        if response.hovered() {
            let scroll = ui.input_mut(|i| {
                let delta = i.smooth_scroll_delta.y;
                i.smooth_scroll_delta.y = 0.0;
                delta
            });
            if scroll.abs() >= 1.0 {
                self.scroll((scroll / line_height).round() as isize);
            }
        }
        if let Some(pos) = response.interact_pointer_pos() {
            let cell = (
                (((pos.y - rect.top()) / line_height) as u16).min(rows - 1),
                (((pos.x - rect.left()) / cell_width) as u16).min(cols - 1),
            );
            if response.drag_started() {
                self.selection = Some([cell, cell]);
            }
            if response.dragged()
                && let Some(selection) = &mut self.selection
            {
                selection[1] = cell;
            }
        }
        if response.clicked() {
            self.selection = None;
        }
        response.context_menu(|ui| {
            if ui.button("Copiar selección o pantalla").clicked() {
                ui.ctx().copy_text(self.copied_text());
                ui.close();
            }
            if ui.button("Volver al final").clicked() {
                self.scroll(isize::MIN);
                ui.close();
            }
        });
        if ui.memory(|m| m.has_focus(self.id)) {
            let modifiers = ui.input(|i| i.modifiers);
            let events = ui.input_mut(|i| std::mem::take(&mut i.events));
            for event in events {
                // egui-winit transforma Ctrl+C, Ctrl+X y Ctrl+V en eventos de
                // portapapeles en Linux y Windows, sin emitir la tecla original.
                if !cfg!(target_os = "macos") && modifiers.ctrl && !modifiers.shift {
                    match event {
                        egui::Event::Copy => {
                            self.interrupt();
                            continue;
                        }
                        egui::Event::Cut => {
                            self.send(vec![24]);
                            continue;
                        }
                        egui::Event::Paste(_) => {
                            self.send(vec![22]);
                            continue;
                        }
                        _ => {}
                    }
                }
                if matches!(event, egui::Event::Copy | egui::Event::Cut)
                    || matches!(event, egui::Event::Key { key: Key::C, pressed: true, modifiers, .. } if modifiers.ctrl && modifiers.shift)
                {
                    ui.ctx().copy_text(self.copied_text());
                    continue;
                }
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = &event
                    && modifiers.shift
                    && matches!(key, Key::PageUp | Key::PageDown)
                {
                    self.scroll(if *key == Key::PageUp {
                        rows as isize
                    } else {
                        -(rows as isize)
                    });
                    continue;
                }
                if let Some(bytes) = input_bytes(&event, self.parser.screen()) {
                    self.scroll(isize::MIN);
                    self.send(bytes);
                }
            }
        }
        let screen = self.parser.screen();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, bg);
        for row in 0..rows {
            for column in 0..cols {
                let Some(cell) = screen.cell(row, column) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let mut foreground = ansi_color(cell.fgcolor(), fg, cell.bold());
                let mut background = ansi_color(cell.bgcolor(), bg, false);
                if cell.inverse() {
                    std::mem::swap(&mut foreground, &mut background);
                }
                let selected = self.selection.is_some_and(|[a, b]| {
                    let (start, end) = if a <= b { (a, b) } else { (b, a) };
                    start <= (row, column) && (row, column) <= end
                });
                if selected {
                    background = ui.visuals().selection.bg_fill;
                    foreground = ui.visuals().selection.stroke.color;
                }
                let pos =
                    rect.min + egui::vec2(column as f32 * cell_width, row as f32 * line_height);
                let width = cell_width * if cell.is_wide() { 2.0 } else { 1.0 };
                let cell_rect = egui::Rect::from_min_size(pos, egui::vec2(width, line_height));
                if background != bg {
                    painter.rect_filled(cell_rect, 0.0, background);
                }
                if cell.has_contents() {
                    painter.text(
                        pos,
                        egui::Align2::LEFT_TOP,
                        cell.contents(),
                        font.clone(),
                        foreground,
                    );
                }
                if cell.underline() {
                    painter.line_segment(
                        [cell_rect.left_bottom(), cell_rect.right_bottom()],
                        Stroke::new(1.0, foreground),
                    );
                }
            }
        }
        if !screen.hide_cursor() && screen.scrollback() == 0 && self.exit.is_none() {
            let (row, column) = screen.cursor_position();
            let pos = rect.min + egui::vec2(column as f32 * cell_width, row as f32 * line_height);
            painter.rect_stroke(
                egui::Rect::from_min_size(pos, egui::vec2(cell_width, line_height)),
                0.0,
                Stroke::new(if response.has_focus() { 2.0 } else { 1.0 }, fg),
                egui::StrokeKind::Inside,
            );
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(group) = self.master.process_group_leader().filter(|p| *p > 0) {
            // SAFETY: tcgetpgrp viene de esta PTY. Nunca señalamos el grupo 0
            // ni el grupo de Miyu; cerrar la pestaña detiene su proceso activo.
            unsafe {
                libc::killpg(group, libc::SIGKILL);
            }
        }
        if self.exit.is_none() && self.done.try_recv().is_err() {
            let _ = self.killer.kill();
        }
    }
}

fn input_bytes(event: &egui::Event, screen: &vt100::Screen) -> Option<Vec<u8>> {
    match event {
        egui::Event::Text(text) => Some(text.as_bytes().to_vec()),
        egui::Event::Paste(text) => {
            // El portapapeles no puede cerrar el modo de pegado e inyectar teclas.
            let text = text.replace('\x1b', "").replace("\r\n", "\n");
            Some(if screen.bracketed_paste() {
                format!("\x1b[200~{text}\x1b[201~").into_bytes()
            } else {
                text.replace('\n', "\r").into_bytes()
            })
        }
        egui::Event::Key {
            key,
            modifiers,
            pressed: true,
            ..
        } => {
            if modifiers.ctrl
                && !modifiers.alt
                && !modifiers.shift
                && (matches!(key, Key::Space | Key::Backslash) || (Key::A..=Key::Z).contains(key))
            {
                let control = match key {
                    Key::A => 1,
                    Key::B => 2,
                    Key::C => 3,
                    Key::D => 4,
                    Key::E => 5,
                    Key::F => 6,
                    Key::G => 7,
                    Key::H => 8,
                    Key::I => 9,
                    Key::J => 10,
                    Key::K => 11,
                    Key::L => 12,
                    Key::M => 13,
                    Key::N => 14,
                    Key::O => 15,
                    Key::P => 16,
                    Key::Q => 17,
                    Key::R => 18,
                    Key::S => 19,
                    Key::T => 20,
                    Key::U => 21,
                    Key::V => 22,
                    Key::W => 23,
                    Key::X => 24,
                    Key::Y => 25,
                    Key::Z => 26,
                    Key::Space => 0,
                    Key::Backslash => 28,
                    _ => return None,
                };
                return Some(vec![control]);
            }
            if modifiers.mac_cmd {
                return None;
            }
            if modifiers.ctrl || modifiers.alt || modifiers.shift {
                let suffix = match key {
                    Key::ArrowUp => Some('A'),
                    Key::ArrowDown => Some('B'),
                    Key::ArrowRight => Some('C'),
                    Key::ArrowLeft => Some('D'),
                    Key::Home => Some('H'),
                    Key::End => Some('F'),
                    _ => None,
                };
                if let Some(suffix) = suffix {
                    let modifier = 1
                        + u8::from(modifiers.shift)
                        + 2 * u8::from(modifiers.alt)
                        + 4 * u8::from(modifiers.ctrl);
                    return Some(format!("\x1b[1;{modifier}{suffix}").into_bytes());
                }
                if *key == Key::Backspace && (modifiers.ctrl || modifiers.alt) {
                    return Some(if modifiers.ctrl {
                        vec![23]
                    } else {
                        b"\x1b\x7f".to_vec()
                    });
                }
            }
            let prefix = if screen.application_cursor() {
                "\x1bO"
            } else {
                "\x1b["
            };
            let sequence = match key {
                Key::Enter => "\r".into(),
                Key::Backspace => "\x7f".into(),
                Key::Tab if modifiers.shift => "\x1b[Z".into(),
                Key::Tab => "\t".into(),
                Key::Escape => "\x1b".into(),
                Key::ArrowUp => format!("{prefix}A"),
                Key::ArrowDown => format!("{prefix}B"),
                Key::ArrowRight => format!("{prefix}C"),
                Key::ArrowLeft => format!("{prefix}D"),
                Key::Home => format!("{prefix}H"),
                Key::End => format!("{prefix}F"),
                Key::Insert => "\x1b[2~".into(),
                Key::Delete => "\x1b[3~".into(),
                Key::PageUp => "\x1b[5~".into(),
                Key::PageDown => "\x1b[6~".into(),
                Key::F1 => "\x1bOP".into(),
                Key::F2 => "\x1bOQ".into(),
                Key::F3 => "\x1bOR".into(),
                Key::F4 => "\x1bOS".into(),
                Key::F5 => "\x1b[15~".into(),
                Key::F6 => "\x1b[17~".into(),
                Key::F7 => "\x1b[18~".into(),
                Key::F8 => "\x1b[19~".into(),
                Key::F9 => "\x1b[20~".into(),
                Key::F10 => "\x1b[21~".into(),
                Key::F11 => "\x1b[23~".into(),
                Key::F12 => "\x1b[24~".into(),
                _ => return None,
            };
            Some(sequence.into_bytes())
        }
        _ => None,
    }
}

fn ansi_color(color: vt100::Color, default: Color32, bold: bool) -> Color32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        vt100::Color::Idx(index) => {
            const PALETTE: [[u8; 3]; 16] = [
                [34, 34, 34],
                [190, 55, 55],
                [67, 145, 67],
                [176, 137, 37],
                [66, 115, 190],
                [153, 78, 166],
                [47, 144, 158],
                [204, 204, 204],
                [118, 118, 118],
                [240, 94, 94],
                [117, 198, 107],
                [230, 193, 91],
                [125, 170, 245],
                [211, 138, 226],
                [105, 206, 220],
                [238, 238, 238],
            ];
            let [r, g, b] = match index {
                0..=15 => PALETTE[if bold && index < 8 { index + 8 } else { index } as usize],
                16..=231 => {
                    let n = index - 16;
                    let levels = [0, 95, 135, 175, 215, 255];
                    [
                        levels[(n / 36) as usize],
                        levels[(n / 6 % 6) as usize],
                        levels[(n % 6) as usize],
                    ]
                }
                _ => [8 + 10 * (index - 232); 3],
            };
            Color32::from_rgb(r, g, b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Modifiers;
    use std::time::{Duration, Instant};

    #[test]
    fn ansi_input_and_safe_paste() {
        let mut parser = vt100::Parser::new(4, 40, 10);
        parser.process(b"\x1b[31mrojo\x1b[0m\r\n\x1b[?1h\x1b[?2004h");
        assert_eq!(
            parser.screen().cell(0, 0).unwrap().fgcolor(),
            vt100::Color::Idx(1)
        );
        let key = |key, modifiers| egui::Event::Key {
            key,
            modifiers,
            pressed: true,
            repeat: false,
            physical_key: None,
        };
        assert_eq!(
            input_bytes(&key(Key::C, Modifiers::CTRL), parser.screen()),
            Some(vec![3])
        );
        assert_eq!(
            input_bytes(&key(Key::ArrowUp, Modifiers::NONE), parser.screen()),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            input_bytes(&key(Key::ArrowLeft, Modifiers::CTRL), parser.screen()),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            input_bytes(
                &egui::Event::Paste("ñ\r\n\x1b[201~".into()),
                parser.screen()
            ),
            Some("\x1b[200~ñ\n[201~\x1b[201~".as_bytes().to_vec())
        );
        assert_eq!(
            ansi_color(vt100::Color::Idx(255), Color32::WHITE, false),
            Color32::from_gray(238)
        );
    }

    #[cfg(unix)]
    #[test]
    fn pty_directory_input_resize_and_exit() {
        let ctx = egui::Context::default();
        let directory = std::env::temp_dir();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "stty size; test -t 0 && printf 'PTY_READY\\n'; read answer; stty size; printf 'ANSWER=%s\\n' \"$answer\"; pwd; exit 7"]);
        let mut terminal = Terminal::spawn(
            Id::new("test-terminal"),
            "Prueba".into(),
            &directory,
            command,
            &ctx,
        )
        .unwrap();
        let start = Instant::now();
        while !terminal.parser.screen().contents().contains("PTY_READY") {
            terminal.poll(&ctx);
            assert!(start.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(10));
        }
        terminal
            .master
            .resize(PtySize {
                rows: 10,
                cols: 50,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        terminal.send("hola ñ\n".as_bytes().to_vec());
        while terminal.exit.is_none()
            || !terminal
                .parser
                .screen()
                .contents()
                .contains("ANSWER=hola ñ")
        {
            terminal.poll(&ctx);
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "La terminal no terminó"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let text = terminal.parser.screen().contents();
        assert!(text.contains("24 80"));
        assert!(text.contains("PTY_READY"));
        assert!(text.contains("10 50"));
        assert!(text.contains(directory.canonicalize().unwrap().to_string_lossy().as_ref()));
        assert_eq!(
            terminal
                .exit
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .exit_code(),
            7
        );
    }

    #[cfg(unix)]
    #[test]
    fn closing_terminal_stops_foreground_process() {
        let ctx = egui::Context::default();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "printf READY; exec sleep 30"]);
        let mut terminal = Terminal::spawn(
            Id::new("close-test"),
            "Cerrar".into(),
            &std::env::temp_dir(),
            command,
            &ctx,
        )
        .unwrap();
        let start = Instant::now();
        while !terminal.parser.screen().contents().contains("READY") {
            terminal.poll(&ctx);
            assert!(start.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(10));
        }
        let (_, dummy) = mpsc::channel();
        let done = std::mem::replace(&mut terminal.done, dummy);
        drop(terminal);
        assert!(
            !done
                .recv_timeout(Duration::from_secs(3))
                .unwrap()
                .unwrap()
                .success()
        );
    }
}
