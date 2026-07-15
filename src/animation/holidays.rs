//! Сезонные пасхалки: новогодняя гирлянда на ёлке (31 декабря - 7 января),
//! ракета в День космонавтики (12 апреля) и салют вечером 9 мая.
//! В демо-режиме все праздники активны одновременно.

use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crate::scene::world::house::House;
use crossterm::style::Color;

use chrono::Datelike;
use rand::{Rng, RngExt};
use std::io;

// Лампочки гирлянды: смещения по звёздам ёлки (арт pine_tree 9x5, ствол не считаем)
const GARLAND_BULBS: [(i16, i16); 10] = [
    (4, 0),
    (3, 1),
    (5, 1),
    (2, 2),
    (4, 2),
    (6, 2),
    (1, 3),
    (3, 3),
    (5, 3),
    (7, 3),
];
const GARLAND_COLORS: [Color; 5] = [
    Color::Red,
    Color::Yellow,
    Color::Cyan,
    Color::Magenta,
    Color::Green,
];

// Ракета Востока: летит вертикально вверх
const ROCKET_ART: [&str; 4] = [" /\\ ", " || ", " || ", "/||\\"];

/// Один залп салюта: взлёт снаряда, потом разлёт искр кольцом
struct Firework {
    x: f32,
    y: f32,
    burst_y: f32,
    radius: f32,
    color: Color,
    bursting: bool,
}

pub struct HolidaySystem {
    demo: bool,
    frame: u32,
    // Ракета 12 апреля: y-позиция, None = не летит
    rocket_y: Option<f32>,
    rocket_x: i16,
    rocket_cooldown: u32,
    fireworks: Vec<Firework>,
    firework_cooldown: u32,
    terminal_width: u16,
    terminal_height: u16,
}

impl HolidaySystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            demo: false,
            frame: 0,
            rocket_y: None,
            rocket_x: 0,
            rocket_cooldown: 300,
            fireworks: Vec::new(),
            firework_cooldown: 60,
            terminal_width,
            terminal_height,
        }
    }

    fn garland_active(&self) -> bool {
        if self.demo {
            return true;
        }
        let now = chrono::Local::now();
        (now.month() == 12 && now.day() >= 31) || (now.month() == 1 && now.day() <= 7)
    }

    fn cosmonautics_active(&self) -> bool {
        if self.demo {
            return true;
        }
        let now = chrono::Local::now();
        now.month() == 4 && now.day() == 12
    }

    fn victory_active(&self, is_day: bool) -> bool {
        if self.demo {
            return true;
        }
        let now = chrono::Local::now();
        // Салют вечером и ночью
        now.month() == 5 && now.day() == 9 && !is_day
    }

    /// Позиция ёлки: повторяет расчёт декораций сцены
    fn pine_position(&self, horizon_y: u16) -> Option<(i16, i16)> {
        let house_x = (self.terminal_width / 2).saturating_sub(House::WIDTH / 2);
        let pine_x = house_x + House::WIDTH + 18;
        if pine_x + 9 > self.terminal_width {
            return None;
        }
        Some((pine_x as i16, horizon_y as i16 - 5))
    }
}

impl AnimationSystem for HolidaySystem {
    fn id(&self) -> &'static str {
        "holidays"
    }

    fn layer(&self) -> RenderLayer {
        // Поверх сцены: гирлянда ложится на ёлку, салют ярче облаков
        RenderLayer::PostScene
    }

    fn on_resize(&mut self, size: TerminalSize) {
        self.terminal_width = size.width;
        self.terminal_height = size.height;
    }

    fn on_demo_mode(&mut self, demo: bool) {
        self.demo = demo;
        if demo {
            self.rocket_cooldown = 90;
            self.firework_cooldown = 20;
        }
    }

    fn update(&mut self, ctx: &FrameContext<'_>, rng: &mut dyn Rng, _commands: &mut FrameCommands) {
        self.terminal_width = ctx.size.width;
        self.terminal_height = ctx.size.height;
        self.frame = self.frame.wrapping_add(1);

        // Ракета: раз в час в праздник (в демо - раз в полторы минуты)
        if self.cosmonautics_active() {
            match self.rocket_y.as_mut() {
                Some(y) => {
                    *y -= 0.8;
                    if *y < -(ROCKET_ART.len() as f32) - 2.0 {
                        self.rocket_y = None;
                        self.rocket_cooldown = if self.demo { 900 } else { 54_000 };
                    }
                }
                None => {
                    self.rocket_cooldown = self.rocket_cooldown.saturating_sub(1);
                    if self.rocket_cooldown == 0 {
                        // Стартовая площадка справа за домом, не из крыши
                        let house_x = (self.terminal_width / 2).saturating_sub(House::WIDTH / 2);
                        let house_right = house_x + House::WIDTH;
                        self.rocket_x = if house_right + 10 < self.terminal_width {
                            (house_right + 8) as i16
                        } else {
                            self.terminal_width.saturating_sub(6) as i16
                        };
                        self.rocket_y = Some(ctx.horizon_y as f32 - 1.0);
                    }
                }
            }
        }

        // Салют: новые залпы и физика существующих
        if self.victory_active(ctx.conditions.sun.is_day) {
            self.firework_cooldown = self.firework_cooldown.saturating_sub(1);
            if self.firework_cooldown == 0 && self.fireworks.len() < 4 {
                let x = 6.0 + rng.random::<f32>() * (self.terminal_width.saturating_sub(12) as f32);
                let burst_y = 3.0 + rng.random::<f32>() * (ctx.horizon_y as f32 * 0.4);
                self.fireworks.push(Firework {
                    x,
                    y: ctx.horizon_y as f32,
                    burst_y,
                    radius: 0.0,
                    color: GARLAND_COLORS[(rng.random::<u32>() % 5) as usize],
                    bursting: false,
                });
                self.firework_cooldown = if self.demo {
                    30 + rng.random::<u32>() % 30
                } else {
                    45 + rng.random::<u32>() % 90
                };
            }
            for fw in &mut self.fireworks {
                if fw.bursting {
                    fw.radius += 0.35;
                } else {
                    fw.y -= 0.9;
                    if fw.y <= fw.burst_y {
                        fw.bursting = true;
                    }
                }
            }
            self.fireworks.retain(|fw| !(fw.bursting && fw.radius > 7.0));
        } else {
            self.fireworks.clear();
        }
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        // Гирлянда на ёлке
        if self.garland_active()
            && let Some((pine_x, pine_y)) = self.pine_position(ctx.horizon_y)
        {
            for (i, (dx, dy)) in GARLAND_BULBS.iter().enumerate() {
                // Лампочки переливаются: каждая мигает со своим сдвигом
                if (self.frame / 6 + i as u32) % 3 == 0 {
                    continue;
                }
                let x = pine_x + dx;
                let y = pine_y + dy;
                if x >= 0
                    && x < self.terminal_width as i16
                    && y >= 0
                    && y < self.terminal_height as i16
                {
                    renderer.render_char(
                        x as u16,
                        y as u16,
                        'o',
                        GARLAND_COLORS[i % GARLAND_COLORS.len()],
                    )?;
                }
            }
        }

        // Ракета с пламенем и подписью
        if let Some(rocket_y) = self.rocket_y {
            let base_y = rocket_y.floor() as i16;
            for (row, line) in ROCKET_ART.iter().enumerate() {
                let y = base_y + row as i16;
                if y < 0 || y >= self.terminal_height as i16 {
                    continue;
                }
                for (col, ch) in line.chars().enumerate() {
                    if ch == ' ' {
                        continue;
                    }
                    let x = self.rocket_x + col as i16;
                    if x >= 0 && x < self.terminal_width as i16 {
                        renderer.render_char(x as u16, y as u16, ch, Color::White)?;
                    }
                }
            }
            // Пламя под соплом
            let flame_y = base_y + ROCKET_ART.len() as i16;
            if flame_y >= 0 && flame_y < self.terminal_height as i16 {
                let flame = if (self.frame / 2) % 2 == 0 { "vVv" } else { "VvV" };
                for (i, ch) in flame.chars().enumerate() {
                    let x = self.rocket_x + i as i16;
                    if x >= 0 && x < self.terminal_width as i16 {
                        let color = if ch == 'V' { Color::Red } else { Color::Yellow };
                        renderer.render_char(x as u16, flame_y as u16, ch, color)?;
                    }
                }
            }
            // Подпись слева от ракеты (справа может не хватить экрана)
            let label = "Поехали!";
            let label_y = base_y + 1;
            let label_x = self.rocket_x - label.chars().count() as i16 - 2;
            if label_y >= 0 && label_y < self.terminal_height as i16 {
                for (i, ch) in label.chars().enumerate() {
                    let x = label_x + i as i16;
                    if x >= 0 && x < self.terminal_width as i16 {
                        renderer.render_char(x as u16, label_y as u16, ch, Color::DarkGrey)?;
                    }
                }
            }
        }

        // Салют
        for fw in &self.fireworks {
            if !fw.bursting {
                let x = fw.x.floor() as i16;
                let y = fw.y.floor() as i16;
                if x >= 0
                    && x < self.terminal_width as i16
                    && y >= 0
                    && y < self.terminal_height as i16
                {
                    renderer.render_char(x as u16, y as u16, '|', Color::DarkGrey)?;
                }
            } else {
                // Кольцо искр, сплюснутое по вертикали под пропорции клеток
                let spark = if fw.radius < 4.0 { '*' } else { '.' };
                for k in 0..12 {
                    let angle = (k as f32) * std::f32::consts::TAU / 12.0;
                    let x = (fw.x + angle.cos() * fw.radius).floor() as i16;
                    let y = (fw.y + angle.sin() * fw.radius * 0.5).floor() as i16;
                    if x >= 0
                        && x < self.terminal_width as i16
                        && y >= 0
                        && y < self.terminal_height as i16
                    {
                        renderer.render_char(x as u16, y as u16, spark, fw.color)?;
                    }
                }
            }
        }

        Ok(())
    }
}
