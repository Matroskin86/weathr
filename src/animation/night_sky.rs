//! Ночные гости неба: падающие звёзды, кометы, НЛО,
//! Дед Мороз на санях в новогоднюю неделю и Баба Яга в ступе.
//! Всё спавнится только ясной ночью; в демо-режиме - гораздо чаще.

use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crossterm::style::Color;

use chrono::Datelike;
use rand::{Rng, RngExt};
use std::io;

// Дед Мороз: три оленя тянут сани (кириллица в спрайте намеренно)
const SLEIGH_ROW_0: &str = r"  \,    \,    \,   o/";
const SLEIGH_ROW_1: &str = r" (o>===(o>===(o>==[__]";
const SLEIGH_LABEL: &str = "С Новым годом!";

// Баба Яга: ступа с метлой
const YAGA_ROW_0: &str = r"  }:{";
const YAGA_ROW_1: &str = r" \(_)/--*";
const YAGA_LABEL: &str = "Баба Яга";

// НЛО
const UFO_ROW_0: &str = r"  _._";
const UFO_ROW_1: &str = r"<(_o_)>";

/// Падающая звезда: короткий росчерк со следом
struct Meteor {
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    life: u32,
}

/// Летун с горизонтальным курсом и покачиванием
struct Flyer {
    x: f32,
    y: f32,
    speed: f32,
    phase: f32,
    // НЛО периодически зависает
    hover: u32,
}

pub struct NightSkySystem {
    demo: bool,
    frame: u32,
    meteors: Vec<Meteor>,
    meteor_cooldown: u32,
    comet: Option<Flyer>,
    comet_cooldown: u32,
    ufo: Option<Flyer>,
    ufo_cooldown: u32,
    sleigh: Option<Flyer>,
    sleigh_cooldown: u32,
    yaga: Option<Flyer>,
    yaga_cooldown: u32,
    // Болид: очень редкая яркая падающая звезда с цветным следом
    bolide: Option<Meteor>,
    bolide_cooldown: u32,
    // Северное сияние: кадры до конца показа и время для волн
    aurora_frames: u32,
    aurora_cooldown: u32,
    aurora_t: f32,
    // Поезд Starlink: x головы и число огоньков (None = не летит)
    starlink_x: Option<f32>,
    starlink_count: usize,
    terminal_width: u16,
    terminal_height: u16,
}

impl NightSkySystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            demo: false,
            frame: 0,
            meteors: Vec::new(),
            meteor_cooldown: 300,
            comet: None,
            comet_cooldown: 4_000,
            ufo: None,
            ufo_cooldown: 9_000,
            sleigh: None,
            sleigh_cooldown: 2_000,
            yaga: None,
            yaga_cooldown: 14_000,
            bolide: None,
            bolide_cooldown: 40_000,
            aurora_frames: 0,
            aurora_cooldown: 20_000,
            aurora_t: 0.0,
            starlink_x: None,
            starlink_count: 0,
            terminal_width,
            terminal_height,
        }
    }

    /// Зимние месяцы - сезон северного сияния
    fn winter_season(&self) -> bool {
        if self.demo {
            return true;
        }
        matches!(chrono::Local::now().month(), 12 | 1 | 2)
    }

    fn new_year_season(&self) -> bool {
        if self.demo {
            return true;
        }
        let now = chrono::Local::now();
        (now.month() == 12 && now.day() >= 31) || (now.month() == 1 && now.day() <= 7)
    }

    /// Верхняя треть неба для маршрутов летунов
    fn sky_band(&self, rng: &mut (impl Rng + ?Sized)) -> f32 {
        1.0 + rng.random::<f32>() * (self.terminal_height as f32 / 4.0).max(2.0)
    }

    fn spawn_flyer(&self, rng: &mut (impl Rng + ?Sized), speed: f32, width: f32) -> Flyer {
        let leftward = rng.random::<bool>();
        Flyer {
            x: if leftward {
                self.terminal_width as f32
            } else {
                -width
            },
            y: self.sky_band(rng),
            speed: if leftward { -speed } else { speed },
            phase: 0.0,
            hover: 0,
        }
    }

    fn update_flyer(flyer: &mut Flyer, sway: f32) {
        flyer.phase += 0.08;
        if flyer.hover == 0 {
            flyer.x += flyer.speed;
        } else {
            flyer.hover -= 1;
        }
        let _ = sway;
    }

    fn flyer_gone(&self, flyer: &Flyer, width: f32) -> bool {
        flyer.x < -(width + 16.0) || flyer.x > self.terminal_width as f32 + 16.0
    }

    fn render_two_row_sprite(
        &self,
        renderer: &mut TerminalRenderer,
        x: i16,
        y: i16,
        rows: [&str; 2],
        flip: bool,
        color_for: impl Fn(char) -> Color,
    ) -> io::Result<()> {
        let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
        for (row_idx, row) in rows.iter().enumerate() {
            let render_y = y + row_idx as i16;
            if render_y < 0 || render_y >= self.terminal_height as i16 {
                continue;
            }
            let chars: Vec<char> = if flip {
                let mut padded: Vec<char> = row.chars().collect();
                padded.resize(width, ' ');
                padded
                    .iter()
                    .rev()
                    .map(|ch| match ch {
                        '(' => ')',
                        ')' => '(',
                        '/' => '\\',
                        '\\' => '/',
                        '<' => '>',
                        '>' => '<',
                        '[' => ']',
                        ']' => '[',
                        '{' => '}',
                        '}' => '{',
                        c => *c,
                    })
                    .collect()
            } else {
                row.chars().collect()
            };
            for (col, ch) in chars.iter().enumerate() {
                if *ch == ' ' {
                    continue;
                }
                let render_x = x + col as i16;
                if render_x < 0 || render_x >= self.terminal_width as i16 {
                    continue;
                }
                renderer.render_char(render_x as u16, render_y as u16, *ch, color_for(*ch))?;
            }
        }
        Ok(())
    }

    fn render_label(
        &self,
        renderer: &mut TerminalRenderer,
        x: i16,
        y: i16,
        label: &str,
    ) -> io::Result<()> {
        if y < 0 || y >= self.terminal_height as i16 {
            return Ok(());
        }
        for (i, ch) in label.chars().enumerate() {
            let render_x = x + i as i16;
            if render_x >= 0 && render_x < self.terminal_width as i16 {
                renderer.render_char(render_x as u16, y as u16, ch, Color::DarkGrey)?;
            }
        }
        Ok(())
    }
}

impl AnimationSystem for NightSkySystem {
    fn id(&self) -> &'static str {
        "night_sky"
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Background
    }

    fn is_active(&self, ctx: &FrameContext<'_>) -> bool {
        !ctx.conditions.is_raining
            && !ctx.conditions.is_thunderstorm
            && !ctx.conditions.is_snowing
            && !ctx.conditions.is_foggy
    }

    fn on_resize(&mut self, size: TerminalSize) {
        self.terminal_width = size.width;
        self.terminal_height = size.height;
    }

    fn on_starlink_train(&mut self, count: usize) {
        if self.starlink_x.is_none() {
            self.starlink_count = count.clamp(4, 10);
            self.starlink_x = Some(-4.0);
        }
    }

    fn on_demo_mode(&mut self, demo: bool) {
        self.demo = demo;
        if demo {
            self.meteor_cooldown = 60;
            self.comet_cooldown = 250;
            self.ufo_cooldown = 500;
            self.sleigh_cooldown = 150;
            self.yaga_cooldown = 800;
            self.bolide_cooldown = 1_050;
            self.aurora_cooldown = 400;
        }
    }

    fn update(&mut self, ctx: &FrameContext<'_>, rng: &mut dyn Rng, _commands: &mut FrameCommands) {
        self.terminal_width = ctx.size.width;
        self.terminal_height = ctx.size.height;
        self.frame = self.frame.wrapping_add(1);

        let night = !ctx.conditions.sun.is_day;
        // Сквозь плотные тучи звездопад и комету не видно (переменная облачность не мешает)
        let sky_visible = !ctx
            .state
            .current_weather
            .as_ref()
            .map(|w| {
                matches!(
                    w.condition,
                    crate::weather::WeatherCondition::Cloudy
                        | crate::weather::WeatherCondition::Overcast
                )
            })
            .unwrap_or(false);

        // Метеоры: частые ночные росчерки
        for meteor in &mut self.meteors {
            meteor.x += meteor.dx;
            meteor.y += meteor.dy;
            meteor.life = meteor.life.saturating_sub(1);
        }
        let height_limit = ctx.horizon_y as f32 - 4.0;
        self.meteors
            .retain(|m| m.life > 0 && m.y < height_limit && m.x > -4.0);
        if night && sky_visible {
            self.meteor_cooldown = self.meteor_cooldown.saturating_sub(1);
            if self.meteor_cooldown == 0 {
                let leftward = rng.random::<bool>();
                self.meteors.push(Meteor {
                    x: rng.random::<f32>() * self.terminal_width as f32,
                    y: 1.0 + rng.random::<f32>() * 4.0,
                    dx: if leftward { -1.3 } else { 1.3 },
                    dy: 0.45,
                    life: 22,
                });
                self.meteor_cooldown = if self.demo {
                    90 + rng.random::<u32>() % 120
                } else {
                    900 + rng.random::<u32>() % 1800
                };
            }
        }

        // Болид: очень редкий, крупный и цветной
        if let Some(bolide) = self.bolide.as_mut() {
            bolide.x += bolide.dx;
            bolide.y += bolide.dy;
            bolide.life = bolide.life.saturating_sub(1);
            let dead = bolide.life == 0 || bolide.y > ctx.horizon_y as f32 - 5.0;
            if dead {
                self.bolide = None;
            }
        } else if night && sky_visible {
            self.bolide_cooldown = self.bolide_cooldown.saturating_sub(1);
            if self.bolide_cooldown == 0 {
                let x = 10.0 + rng.random::<f32>() * (self.terminal_width as f32 - 20.0);
                // Летит от края к центру: весь росчерк остаётся на экране
                let leftward = x > self.terminal_width as f32 / 2.0;
                self.bolide = Some(Meteor {
                    x,
                    y: 1.0,
                    dx: if leftward { -1.7 } else { 1.7 },
                    dy: 0.55,
                    life: 44,
                });
                // Раз в 2-4 часа: настоящая редкость
                self.bolide_cooldown = if self.demo {
                    1_300
                } else {
                    108_000 + rng.random::<u32>() % 108_000
                };
            }
        }

        // Северное сияние: зимними ясными ночами, идёт несколько минут
        if self.aurora_frames > 0 {
            self.aurora_frames -= 1;
            self.aurora_t += 0.02;
        } else if night && sky_visible && self.winter_season() {
            self.aurora_cooldown = self.aurora_cooldown.saturating_sub(1);
            if self.aurora_cooldown == 0 {
                self.aurora_frames = 1_800 + rng.random::<u32>() % 1_800;
                self.aurora_cooldown = if self.demo {
                    1_200
                } else {
                    54_000 + rng.random::<u32>() % 54_000
                };
            }
        }

        // Комета: редкая, падает по пологой косой к горизонту
        if let Some(comet) = self.comet.as_mut() {
            comet.x += comet.speed;
            // Снижение: phase хранит вертикальную скорость
            comet.y += comet.phase;
            let too_low = comet.y > ctx.horizon_y as f32 * 0.55;
            if too_low || self.flyer_gone(self.comet.as_ref().unwrap(), 10.0) {
                self.comet = None;
            }
        } else if night && sky_visible {
            self.comet_cooldown = self.comet_cooldown.saturating_sub(1);
            if self.comet_cooldown == 0 {
                let mut comet = self.spawn_flyer(rng, 0.34, 10.0);
                comet.y = 1.0 + rng.random::<f32>() * 2.0;
                comet.phase = 0.09;
                self.comet = Some(comet);
                self.comet_cooldown = if self.demo {
                    600
                } else {
                    13_500 + rng.random::<u32>() % 9_000
                };
            }
        }

        // НЛО: зигзаги и зависания
        if let Some(ufo) = self.ufo.as_mut() {
            ufo.phase += 0.1;
            if ufo.hover > 0 {
                ufo.hover -= 1;
            } else {
                ufo.x += ufo.speed;
                // Иногда замирает на пару секунд
                if rng.random::<f32>() < 0.008 {
                    ufo.hover = 25 + rng.random::<u32>() % 20;
                }
            }
            if self.flyer_gone(self.ufo.as_ref().unwrap(), 8.0) {
                self.ufo = None;
            }
        } else if night {
            self.ufo_cooldown = self.ufo_cooldown.saturating_sub(1);
            if self.ufo_cooldown == 0 {
                self.ufo = Some(self.spawn_flyer(rng, 0.5, 8.0));
                self.ufo_cooldown = if self.demo {
                    900
                } else {
                    18_000 + rng.random::<u32>() % 18_000
                };
            }
        }

        // Дед Мороз: только в новогоднюю неделю
        if let Some(sleigh) = self.sleigh.as_mut() {
            Self::update_flyer(sleigh, 0.0);
            if self.flyer_gone(self.sleigh.as_ref().unwrap(), 22.0) {
                self.sleigh = None;
            }
        } else if night && self.new_year_season() {
            self.sleigh_cooldown = self.sleigh_cooldown.saturating_sub(1);
            if self.sleigh_cooldown == 0 {
                self.sleigh = Some(self.spawn_flyer(rng, 0.4, 22.0));
                self.sleigh_cooldown = if self.demo {
                    1_100
                } else {
                    9_000 + rng.random::<u32>() % 9_000
                };
            }
        }

        // Поезд Starlink: вереница ползёт слева направо (орбита восточная)
        if let Some(x) = self.starlink_x.as_mut() {
            *x += 0.45;
            let tail = self.starlink_count as f32 * 3.0;
            if *x > self.terminal_width as f32 + tail + 14.0 {
                self.starlink_x = None;
            }
        }

        // Баба Яга: редкая гостья с пьяной траекторией
        if let Some(yaga) = self.yaga.as_mut() {
            yaga.phase += 0.12;
            yaga.x += yaga.speed;
            if self.flyer_gone(self.yaga.as_ref().unwrap(), 9.0) {
                self.yaga = None;
            }
        } else if night {
            self.yaga_cooldown = self.yaga_cooldown.saturating_sub(1);
            if self.yaga_cooldown == 0 {
                self.yaga = Some(self.spawn_flyer(rng, 0.45, 9.0));
                self.yaga_cooldown = if self.demo {
                    1_400
                } else {
                    27_000 + rng.random::<u32>() % 27_000
                };
            }
        }
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        _ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        // Северное сияние: колышущиеся занавеси, фиолетовый градиент сверху
        // к зелёному снизу (труколор с деградацией до White на бедных терминалах)
        if self.aurora_frames > 0 {
            let t = self.aurora_t;
            for x in 0..self.terminal_width as i16 {
                let fx = x as f32;
                let wave = (fx * 0.13 + t * 1.7).sin() + (fx * 0.07 - t).sin();
                let height = (2.5 + wave * 1.6).max(1.0) as i16;
                for y in 1..=height {
                    // Мерцающие прорехи в занавеси
                    if (x + y * 3 + (t * 9.0) as i16) % 4 == 0 {
                        continue;
                    }
                    if y >= self.terminal_height as i16 {
                        break;
                    }
                    let (ch, color) = match y {
                        1 => ('.', Color::Rgb { r: 205, g: 120, b: 255 }),
                        2 => (':', Color::Rgb { r: 150, g: 140, b: 255 }),
                        3 => (':', Color::Rgb { r: 95, g: 215, b: 205 }),
                        _ => ('|', Color::Rgb { r: 110, g: 240, b: 145 }),
                    };
                    renderer.render_char(x as u16, y as u16, ch, color)?;
                }
            }
        }

        // Болид: крупная голова и сиренево-розовый след
        if let Some(bolide) = &self.bolide {
            let trail: [(char, Color, f32); 6] = [
                ('@', Color::Rgb { r: 255, g: 245, b: 190 }, 0.0),
                ('O', Color::Rgb { r: 215, g: 150, b: 255 }, 1.0),
                ('o', Color::Rgb { r: 190, g: 120, b: 255 }, 2.0),
                ('*', Color::Rgb { r: 255, g: 140, b: 225 }, 3.0),
                ('+', Color::Rgb { r: 225, g: 120, b: 200 }, 4.5),
                ('.', Color::DarkGrey, 6.0),
            ];
            for (ch, color, back) in trail {
                let x = (bolide.x - bolide.dx * back).floor() as i16;
                let y = (bolide.y - bolide.dy * back).floor() as i16;
                if x >= 0
                    && x < self.terminal_width as i16
                    && y >= 0
                    && y < self.terminal_height as i16
                {
                    renderer.render_char(x as u16, y as u16, ch, color)?;
                }
            }
        }

        // Метеоры со следом
        for meteor in &self.meteors {
            let trail = [
                ('*', Color::White, 0.0),
                ('+', Color::White, 1.0),
                ('.', Color::Grey, 2.0),
                ('.', Color::DarkGrey, 3.5),
            ];
            for (ch, color, back) in trail {
                let x = (meteor.x - meteor.dx * back).floor() as i16;
                let y = (meteor.y - meteor.dy * back).floor() as i16;
                if x >= 0
                    && x < self.terminal_width as i16
                    && y >= 0
                    && y < self.terminal_height as i16
                {
                    renderer.render_char(x as u16, y as u16, ch, color)?;
                }
            }
        }

        // Комета: яркая голова, хвост тянется по диагонали вверх-назад
        if let Some(comet) = &self.comet {
            let dir = comet.speed.signum();
            let head_x = comet.x.floor() as i16;
            let head_y = comet.y.floor() as i16;
            let tail = ['o', '*', '*', '+', '+', '.', '.', '.'];
            if head_x >= 0
                && head_x < self.terminal_width as i16
                && head_y >= 0
                && head_y < self.terminal_height as i16
            {
                renderer.render_char(head_x as u16, head_y as u16, 'O', Color::Cyan)?;
            }
            for (i, ch) in tail.iter().enumerate() {
                // Хвост строго против вектора движения: назад по x и вверх по y
                let x = head_x - (dir * (i as f32 + 1.0)) as i16;
                let y = head_y - ((i as f32 + 1.0) * 0.3).round() as i16;
                if x >= 0
                    && x < self.terminal_width as i16
                    && y >= 0
                    && y < self.terminal_height as i16
                {
                    let color = if i < 3 { Color::Cyan } else { Color::DarkGrey };
                    renderer.render_char(x as u16, y as u16, *ch, color)?;
                }
            }
        }

        // Поезд Starlink: цепочка огоньков с подписью
        if let Some(head_x) = self.starlink_x {
            let y: i16 = 6;
            if y < self.terminal_height as i16 {
                for k in 0..self.starlink_count {
                    let x = (head_x - (k as f32) * 3.0).floor() as i16;
                    if x >= 0 && x < self.terminal_width as i16 {
                        // Лёгкое мерцание вереницы
                        let bright = (self.frame / 4 + k as u32) % 5 != 0;
                        let (ch, color) = if bright {
                            ('•', Color::Rgb { r: 220, g: 230, b: 255 })
                        } else {
                            ('·', Color::Grey)
                        };
                        renderer.render_char(x as u16, y as u16, ch, color)?;
                    }
                }
                let label = format!("Starlink x{}", self.starlink_count);
                let label_x = (head_x + 2.0) as i16;
                let label_y = y + 1;
                if label_y < self.terminal_height as i16 {
                    for (i, ch) in label.chars().enumerate() {
                        let x = label_x + i as i16;
                        if x >= 0 && x < self.terminal_width as i16 {
                            renderer.render_char(x as u16, label_y as u16, ch, Color::DarkGrey)?;
                        }
                    }
                }
            }
        }

        // НЛО с мигающим огоньком
        if let Some(ufo) = &self.ufo {
            let x = ufo.x.floor() as i16;
            let y = (ufo.y + ufo.phase.sin() * 1.2).floor() as i16;
            let light = [Color::Red, Color::Green, Color::Cyan][((self.frame / 5) % 3) as usize];
            self.render_two_row_sprite(
                renderer,
                x,
                y,
                [UFO_ROW_0, UFO_ROW_1],
                false,
                |ch| match ch {
                    'o' => light,
                    '_' | '.' => Color::Grey,
                    _ => Color::DarkGrey,
                },
            )?;
        }

        // Дед Мороз на санях
        if let Some(sleigh) = &self.sleigh {
            let x = sleigh.x.floor() as i16;
            let y = (sleigh.y + sleigh.phase.sin() * 0.8).floor() as i16;
            let flip = sleigh.speed < 0.0;
            self.render_two_row_sprite(
                renderer,
                x,
                y,
                [SLEIGH_ROW_0, SLEIGH_ROW_1],
                flip,
                |ch| match ch {
                    'o' => Color::DarkYellow,
                    '[' | ']' | '_' => Color::Red,
                    '/' => Color::Red,
                    _ => Color::Grey,
                },
            )?;
            self.render_label(renderer, x + 4, y + 2, SLEIGH_LABEL)?;
        }

        // Баба Яга в ступе
        if let Some(yaga) = &self.yaga {
            let x = yaga.x.floor() as i16;
            let y = (yaga.y + yaga.phase.sin() * 1.6).floor() as i16;
            let flip = yaga.speed > 0.0; // метла должна быть сзади по ходу
            self.render_two_row_sprite(
                renderer,
                x,
                y,
                [YAGA_ROW_0, YAGA_ROW_1],
                flip,
                |ch| match ch {
                    '}' | '{' | ':' => Color::Grey,
                    '*' => Color::DarkYellow,
                    _ => Color::DarkYellow,
                },
            )?;
            self.render_label(renderer, x, y + 2, YAGA_LABEL)?;
        }

        Ok(())
    }
}
