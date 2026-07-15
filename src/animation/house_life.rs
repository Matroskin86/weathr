//! Жизнь дома: тёмным вечером окна ярко горят, глубокой ночью светятся
//! приглушённо, а в дневную жару распахиваются створки.

use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crate::scene::world::house::House;
use crossterm::style::Color;

use chrono::Timelike;
use rand::Rng;
use std::io;

// Тот же арт, что рисует сцена: из него берём позиции окон
const HOUSE_ART: &str = include_str!("../scene/world/assets/house.txt");

/// Окно в арте дома: смещение и ширина рамки
struct Window {
    dx: i16,
    dy: i16,
    len: i16,
}

fn find_windows() -> Vec<Window> {
    let mut windows = Vec::new();
    for (dy, line) in HOUSE_ART.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '[' {
                if let Some(close) = chars[i..].iter().position(|&c| c == ']') {
                    windows.push(Window {
                        dx: i as i16,
                        dy: dy as i16,
                        len: (close + 1) as i16,
                    });
                    i += close + 1;
                    continue;
                }
            }
            i += 1;
        }
    }
    windows
}

enum HouseMood {
    /// День, ничего не подсвечиваем
    Plain,
    /// Дневная жара: створки распахнуты
    WindowsOpen,
    /// Тёмный вечер: окна ярко горят
    EveningLight,
    /// Глубокая ночь: приглушённый свет
    NightLight,
}

pub struct HouseLifeSystem {
    windows: Vec<Window>,
    terminal_width: u16,
    terminal_height: u16,
}

impl HouseLifeSystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            windows: find_windows(),
            terminal_width,
            terminal_height,
        }
    }

    fn mood(ctx: &FrameContext<'_>) -> HouseMood {
        let is_day = ctx.conditions.sun.is_day;
        let temperature = ctx
            .state
            .current_weather
            .as_ref()
            .map(|w| w.temperature)
            .unwrap_or(15.0);

        if is_day {
            if temperature > 24.0 {
                HouseMood::WindowsOpen
            } else {
                HouseMood::Plain
            }
        } else {
            // Вечер до 23:00 - яркий свет, дальше до утра - приглушённый
            let hour = chrono::Local::now().hour();
            if (6..23).contains(&hour) {
                HouseMood::EveningLight
            } else {
                HouseMood::NightLight
            }
        }
    }
}

impl AnimationSystem for HouseLifeSystem {
    fn id(&self) -> &'static str {
        "house_life"
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::PostScene
    }

    fn on_resize(&mut self, size: TerminalSize) {
        self.terminal_width = size.width;
        self.terminal_height = size.height;
    }

    fn update(&mut self, ctx: &FrameContext<'_>, _rng: &mut dyn Rng, _commands: &mut FrameCommands) {
        self.terminal_width = ctx.size.width;
        self.terminal_height = ctx.size.height;
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        let mood = Self::mood(ctx);
        let color = match mood {
            HouseMood::Plain => return Ok(()),
            HouseMood::WindowsOpen => None,
            HouseMood::EveningLight => Some(Color::Rgb { r: 255, g: 205, b: 95 }),
            HouseMood::NightLight => Some(Color::Rgb { r: 140, g: 100, b: 45 }),
        };

        let house_x = (self.terminal_width / 2).saturating_sub(House::WIDTH / 2) as i16;
        let house_y = ctx.horizon_y as i16 - House::HEIGHT as i16;
        let art_lines: Vec<&str> = HOUSE_ART.lines().collect();

        for window in &self.windows {
            let y = house_y + window.dy;
            if y < 0 || y >= self.terminal_height as i16 {
                continue;
            }
            let Some(line) = art_lines.get(window.dy as usize) else {
                continue;
            };
            let chars: Vec<char> = line.chars().collect();

            for i in 0..window.len {
                let x = house_x + window.dx + i;
                if x < 0 || x >= self.terminal_width as i16 {
                    continue;
                }
                let src = chars
                    .get((window.dx + i) as usize)
                    .copied()
                    .unwrap_or(' ');
                match color {
                    Some(light) => {
                        // Светящееся окно: рамка того же начертания тёплым цветом
                        renderer.render_char(x as u16, y as u16, src, light)?;
                    }
                    None => {
                        // Жара: правая створка распахнута
                        let ch = if i == window.len - 1 { '/' } else { src };
                        renderer.render_char(x as u16, y as u16, ch, Color::Yellow)?;
                    }
                }
            }
        }
        Ok(())
    }
}
