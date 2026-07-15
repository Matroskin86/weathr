//! Снеговик: лепится сам во время снегопада (ком за комом),
//! стоит пока холодно и медленно тает в плюсовую погоду, оставляя лужицу.

use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crate::scene::world::house::House;
use crossterm::style::Color;

use rand::Rng;
use std::io;

// Комья снизу вверх: нижний, средний с пуговицами, голова с морковкой
const BOTTOM: &str = "(  :  )";
const MIDDLE: &str = "-( : )-";
const HEAD: &str = "  (.)>";

pub struct SnowmanSystem {
    demo: bool,
    // 0..3 слепленных кома (дробное - процесс лепки/таяния)
    progress: f32,
    // Лужица после таяния (кадров осталось)
    puddle_frames: u32,
    terminal_width: u16,
    terminal_height: u16,
}

impl SnowmanSystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            demo: false,
            progress: 0.0,
            puddle_frames: 0,
            terminal_width,
            terminal_height,
        }
    }

    /// Двор слева от дома, между деревом и крыльцом
    fn spot_x(&self) -> i16 {
        let house_x = (self.terminal_width / 2).saturating_sub(House::WIDTH / 2);
        house_x.saturating_sub(9).max(1) as i16
    }

    fn render_row(
        &self,
        renderer: &mut TerminalRenderer,
        x: i16,
        y: i16,
        row: &str,
    ) -> io::Result<()> {
        if y < 0 || y >= self.terminal_height as i16 {
            return Ok(());
        }
        for (i, ch) in row.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let render_x = x + i as i16;
            if render_x < 0 || render_x >= self.terminal_width as i16 {
                continue;
            }
            let color = match ch {
                '>' => Color::Rgb { r: 255, g: 150, b: 60 },
                ':' | '.' => Color::DarkGrey,
                '-' => Color::Rgb { r: 150, g: 100, b: 60 },
                _ => Color::White,
            };
            renderer.render_char(render_x as u16, y as u16, ch, color)?;
        }
        Ok(())
    }
}

impl AnimationSystem for SnowmanSystem {
    fn id(&self) -> &'static str {
        "snowman"
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::PostScene
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

        let temperature = ctx
            .state
            .current_weather
            .as_ref()
            .map(|w| w.temperature)
            .unwrap_or(0.0);

        if ctx.conditions.is_snowing {
            // Лепится: ком примерно за 40 секунд (в демо за 5)
            let rate = if self.demo { 1.0 / 75.0 } else { 1.0 / 600.0 };
            if self.progress < 3.0 {
                self.progress = (self.progress + rate).min(3.0);
            }
            self.puddle_frames = 0;
        } else if self.progress > 0.0 && temperature > 2.0 {
            // Тает в плюс: полностью примерно за 5 минут (в демо за полминуты)
            let rate = if self.demo { 1.0 / 150.0 } else { 1.0 / 1_500.0 };
            self.progress -= rate;
            if self.progress <= 0.0 {
                self.progress = 0.0;
                self.puddle_frames = 450;
            }
        }

        self.puddle_frames = self.puddle_frames.saturating_sub(1);
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        let x = self.spot_x();
        let ground = ctx.horizon_y as i16 - 1;

        if self.puddle_frames > 0 {
            self.render_row(renderer, x, ground, " ~ ~ ~")?;
            return Ok(());
        }

        let balls = self.progress as i16;
        if balls >= 1 {
            self.render_row(renderer, x, ground, BOTTOM)?;
        }
        if balls >= 2 {
            self.render_row(renderer, x, ground - 1, MIDDLE)?;
        }
        if balls >= 3 {
            self.render_row(renderer, x, ground - 2, HEAD)?;
        }
        Ok(())
    }
}
