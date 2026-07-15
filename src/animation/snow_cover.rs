//! Снежный покров: во время снегопада земля и крыша дома постепенно
//! белеют, в оттепель покров тает. Плотность растёт с покрытием.

use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crate::scene::world::house::House;
use crossterm::style::Color;

use rand::Rng;
use std::io;

pub struct SnowCoverSystem {
    demo: bool,
    // 0.0 - голая земля, 1.0 - всё белым-бело
    coverage: f32,
    terminal_width: u16,
    terminal_height: u16,
}

impl SnowCoverSystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            demo: false,
            coverage: 0.0,
            terminal_width,
            terminal_height,
        }
    }

    /// Детерминированная маска: клетка засыпана при данном покрытии?
    /// Псевдослучайный порог по координате даёт постепенное неровное зарастание
    fn covered(x: i16, salt: i16, coverage: f32) -> bool {
        let threshold = ((x * 37 + salt * 71).rem_euclid(97)) as f32 / 97.0;
        threshold < coverage
    }
}

impl AnimationSystem for SnowCoverSystem {
    fn id(&self) -> &'static str {
        "snow_cover"
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
            // Полный покров примерно за 3 минуты (в демо за 20 секунд)
            let rate = if self.demo { 1.0 / 300.0 } else { 1.0 / 2_700.0 };
            self.coverage = (self.coverage + rate).min(1.0);
        } else if self.coverage > 0.0 && temperature > 2.0 {
            // Тает примерно за 5 минут (в демо за полминуты)
            let rate = if self.demo { 1.0 / 450.0 } else { 1.0 / 4_500.0 };
            self.coverage = (self.coverage - rate).max(0.0);
        }
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        if self.coverage <= 0.01 {
            return Ok(());
        }

        let width = self.terminal_width as i16;
        let horizon = ctx.horizon_y as i16;

        // Земля: травяная линия зарастает белым
        if horizon >= 0 && horizon < self.terminal_height as i16 {
            for x in 0..width {
                if Self::covered(x, 1, self.coverage) {
                    let ch = if (x * 13 + 5) % 3 == 0 { '*' } else { '.' };
                    renderer.render_char(x as u16, horizon as u16, ch, Color::White)?;
                }
            }
        }
        // Плотный покров наметает сугробы на строку ниже
        let drift_y = horizon + 1;
        if self.coverage > 0.6 && drift_y >= 0 && drift_y < self.terminal_height as i16 {
            for x in 0..width {
                if Self::covered(x, 2, (self.coverage - 0.6) * 2.0) {
                    renderer.render_char(x as u16, drift_y as u16, '.', Color::White)?;
                }
            }
        }

        // Крыша дома: карниз и скат белеют
        let house_x = (self.terminal_width / 2).saturating_sub(House::WIDTH / 2) as i16;
        let house_y = horizon - House::HEIGHT as i16;
        // Карниз (тильды на 5-й строке арта): dx 4..32
        let eaves_y = house_y + 4;
        if eaves_y >= 0 && eaves_y < self.terminal_height as i16 {
            for dx in 4..33 {
                let x = house_x + dx;
                if x >= 0 && x < width && Self::covered(x, 3, self.coverage) {
                    renderer.render_char(x as u16, eaves_y as u16, '~', Color::White)?;
                }
            }
        }
        // Верхний скат: белеет при заметном покрове
        let ridge_y = house_y + 3;
        if self.coverage > 0.35 && ridge_y >= 0 && ridge_y < self.terminal_height as i16 {
            for dx in 4..31 {
                let x = house_x + dx;
                if x >= 0 && x < width && Self::covered(x, 4, (self.coverage - 0.35) * 1.4) {
                    renderer.render_char(x as u16, ridge_y as u16, '-', Color::White)?;
                }
            }
        }
        Ok(())
    }
}
