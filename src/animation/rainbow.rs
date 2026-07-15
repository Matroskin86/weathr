//! Радуга: появляется на полминуты, когда дождь сменился солнцем.
//! Пять цветных дуг труколором, рисуется за облаками.

use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crossterm::style::Color;

use rand::Rng;
use std::io;

const RAINBOW_BANDS: [Color; 5] = [
    Color::Rgb { r: 255, g: 105, b: 100 },
    Color::Rgb { r: 255, g: 215, b: 90 },
    Color::Rgb { r: 120, g: 230, b: 130 },
    Color::Rgb { r: 100, g: 200, b: 255 },
    Color::Rgb { r: 195, g: 130, b: 255 },
];

pub struct RainbowSystem {
    demo: bool,
    frame: u32,
    // Дождь в прошлом кадре: переход дождь->солнце рождает радугу
    was_raining: bool,
    // Кадров показа осталось
    show_frames: u32,
    terminal_width: u16,
    terminal_height: u16,
}

impl RainbowSystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            demo: false,
            frame: 0,
            was_raining: false,
            show_frames: 0,
            terminal_width,
            terminal_height,
        }
    }
}

impl AnimationSystem for RainbowSystem {
    fn id(&self) -> &'static str {
        "rainbow"
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Background
    }

    fn on_resize(&mut self, size: TerminalSize) {
        self.terminal_width = size.width;
        self.terminal_height = size.height;
    }

    fn on_demo_mode(&mut self, demo: bool) {
        self.demo = demo;
    }

    fn update(&mut self, ctx: &FrameContext<'_>, _rng: &mut dyn Rng, _commands: &mut FrameCommands) {
        self.terminal_width = ctx.size.width;
        self.terminal_height = ctx.size.height;
        self.frame = self.frame.wrapping_add(1);

        let raining = ctx.conditions.is_raining || ctx.conditions.is_thunderstorm;

        // Дождь кончился при дневном свете - радуга
        if self.was_raining && !raining && ctx.conditions.sun.is_day {
            self.show_frames = 700;
        }
        self.was_raining = raining;

        // В демо радуга показывается сама на 40-й секунде каждого 3-минутного цикла
        if self.demo && ctx.conditions.sun.is_day && self.frame % 2_700 == 600 {
            self.show_frames = 700;
        }

        if self.show_frames > 0 && !raining {
            self.show_frames -= 1;
        } else {
            self.show_frames = 0;
        }
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        if self.show_frames == 0 {
            return Ok(());
        }

        // Затухание в конце: дуги редеют
        let fading = self.show_frames < 120;

        let cx = self.terminal_width as f32 * 0.62;
        let base_y = ctx.horizon_y as f32;
        let radius = (self.terminal_width as f32 / 3.4).min(base_y * 1.9);

        for (band, color) in RAINBOW_BANDS.iter().enumerate() {
            let rb = radius - band as f32 * 1.4;
            if rb < 4.0 {
                continue;
            }
            let x_from = (cx - rb).max(0.0) as i16;
            let x_to = ((cx + rb) as i16).min(self.terminal_width as i16 - 1);
            for x in x_from..=x_to {
                if fading && (x + band as i16 + (self.frame / 4) as i16) % 3 == 0 {
                    continue;
                }
                let dx = x as f32 - cx;
                let inside = rb * rb - dx * dx;
                if inside <= 0.0 {
                    continue;
                }
                // Вертикаль сплюснута вдвое под пропорции терминальной клетки
                let y = base_y - inside.sqrt() * 0.5;
                let y = y.floor() as i16;
                if y >= 1 && y < ctx.horizon_y as i16 {
                    renderer.render_char(x as u16, y as u16, '~', *color)?;
                }
            }
        }
        Ok(())
    }
}
