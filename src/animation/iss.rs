use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crossterm::style::Color;

use rand::Rng;
use std::io;
use std::sync::OnceLock;

const ISS_ART: &str = include_str!("assets/iss.txt");

fn iss_art() -> &'static Vec<String> {
    static ART: OnceLock<Vec<String>> = OnceLock::new();
    ART.get_or_init(|| ISS_ART.lines().map(|l| l.to_string()).collect())
}

/// МКС: пролетает по верху экрана быстрее самолётов, всегда на восток
/// (наклонение орбиты 51.6°, над средними широтами движется с запада).
/// Летит ниже HUD-строк и рисуется поверх сияния, чтобы читалась целиком.
const ISS_ALTITUDE_ROW: i16 = 4;

pub struct IssSystem {
    // x позиции станции; None = станции на экране нет
    x: Option<f32>,
    label: String,
    terminal_width: u16,
    terminal_height: u16,
}

impl IssSystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            x: None,
            label: String::new(),
            terminal_width,
            terminal_height,
        }
    }

    fn art_width() -> f32 {
        iss_art().iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32
    }

    pub fn start_pass(&mut self, label: &str) {
        if self.x.is_none() {
            self.label = label.to_string();
            self.x = Some(-Self::art_width());
        }
    }
}

impl AnimationSystem for IssSystem {
    fn id(&self) -> &'static str {
        "iss"
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Background
    }

    fn is_active(&self, ctx: &FrameContext<'_>) -> bool {
        // Сквозь осадки и туман станцию с земли не видно
        !ctx.conditions.is_raining
            && !ctx.conditions.is_thunderstorm
            && !ctx.conditions.is_snowing
            && !ctx.conditions.is_foggy
    }

    fn on_resize(&mut self, size: TerminalSize) {
        self.terminal_width = size.width;
        self.terminal_height = size.height;
    }

    fn on_iss_pass(&mut self, label: &str) {
        self.start_pass(label);
    }

    fn update(&mut self, ctx: &FrameContext<'_>, _rng: &mut dyn Rng, _commands: &mut FrameCommands) {
        self.terminal_width = ctx.size.width;
        self.terminal_height = ctx.size.height;

        if let Some(x) = self.x.as_mut() {
            *x += 0.6;
            let gone = *x > ctx.size.width as f32 + self.label.chars().count() as f32;
            if gone {
                self.x = None;
            }
        }
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        _ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        let Some(x) = self.x else {
            return Ok(());
        };
        let x = x.floor() as i16;
        let art = iss_art();

        for (line_offset, line) in art.iter().enumerate() {
            let render_y = ISS_ALTITUDE_ROW + line_offset as i16;
            if render_y >= self.terminal_height as i16 {
                break;
            }
            for (char_offset, ch) in line.chars().enumerate() {
                let render_x = x + char_offset as i16;
                if render_x < 0 || render_x >= self.terminal_width as i16 {
                    continue;
                }
                if ch != ' ' {
                    // Панели и ферма золотые, корпус модуля белый, иллюминаторы голубые
                    let color = match ch {
                        '=' => Color::DarkYellow,
                        '|' => Color::Grey,
                        'o' => Color::Cyan,
                        _ => Color::White,
                    };
                    renderer.render_char(render_x as u16, render_y as u16, ch, color)?;
                }
            }
        }

        // Подпись под станцией
        let label_y = ISS_ALTITUDE_ROW + art.len() as i16;
        if label_y < self.terminal_height as i16 {
            for (char_offset, ch) in self.label.chars().enumerate() {
                let render_x = x + char_offset as i16;
                if render_x < 0 || render_x >= self.terminal_width as i16 {
                    continue;
                }
                renderer.render_char(render_x as u16, label_y as u16, ch, Color::DarkGrey)?;
            }
        }
        Ok(())
    }
}
