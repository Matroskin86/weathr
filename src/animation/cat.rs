//! Кот, живущий в сцене. В плохую погоду сидит у дома под крышей,
//! в хорошую - гуляет, бегает, спит, ловит бабочек (ночью светлячков)
//! и иногда ходит копать ямку к забору.

use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crate::scene::world::house::House;
use crossterm::style::Color;

use rand::{Rng, RngExt};
use std::io;

// Позы кота (смотрит вправо; для левого направления зеркалятся).
// Компактный кот с усами =^.^= и хвостом; хвост машет при ходьбе.
const CAT_SIT: [&str; 2] = ["  /\\_/\\", "~(=^.^=)"];
const CAT_SIT_BLINK: [&str; 2] = ["  /\\_/\\", "~(=-.-=)"];
const CAT_WALK_A: [&str; 2] = ["/ /\\_/\\", " (=o.o=)"];
const CAT_WALK_B: [&str; 2] = ["\\ /\\_/\\", " (=o.o=)"];
const CAT_SLEEP_A: [&str; 2] = ["   z    ", "~(=-.-=)"];
const CAT_SLEEP_B: [&str; 2] = ["   Z    ", "_(=-.-=)"];
const CAT_POUNCE: [&str; 2] = ["| /\\_/\\", " (=>o<=)"];
const CAT_DIG: [&str; 2] = ["  /\\_/\\", "~(=-.o=)"];

const CAT_HEIGHT: i16 = 2;
const CAT_WIDTH: i16 = 8;

/// Фазы охоты на бабочку
#[derive(Clone, Copy, PartialEq)]
enum HuntPhase {
    Stalk,
    Pounce,
}

#[derive(Clone, Copy, PartialEq)]
enum CatState {
    /// Плохая погода: сидит у стены дома
    Shelter,
    Sit,
    Walk,
    Sleep,
    Hunt(HuntPhase),
    Dig(u8),
}

pub struct CatSystem {
    state: CatState,
    x: f32,
    // Смотрит вправо?
    facing_right: bool,
    // Кадры до смены состояния
    timer: u32,
    // Счётчик кадров для анимации поз
    frame: u32,
    // Куда идём в Walk
    target_x: f32,
    // Бег вместо шага
    running: bool,
    // Бабочка (или ночной светлячок) для охоты
    butterfly_x: f32,
    butterfly_y: f32,
    butterfly_phase: f32,
    terminal_width: u16,
    terminal_height: u16,
}

impl CatSystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            state: CatState::Sit,
            x: (terminal_width as f32 / 3.0).max(2.0),
            facing_right: true,
            timer: 60,
            frame: 0,
            target_x: 0.0,
            running: false,
            butterfly_x: 0.0,
            butterfly_y: 0.0,
            butterfly_phase: 0.0,
            terminal_width,
            terminal_height,
        }
    }

    /// Земля кота: ноги на одной линии с основанием дома
    fn ground_row(&self, horizon_y: u16) -> i16 {
        horizon_y as i16 - CAT_HEIGHT
    }

    /// Точка у стены дома, где кот прячется от непогоды (под скатом крыши)
    fn shelter_x(&self, chimney_x: Option<u16>) -> f32 {
        match chimney_x {
            Some(cx) => {
                let house_x = cx.saturating_sub(House::CHIMNEY_X_OFFSET);
                (house_x as f32 + 20.0).min(self.terminal_width.saturating_sub(7) as f32)
            }
            None => 2.0,
        }
    }

    fn bad_weather(ctx: &FrameContext<'_>) -> bool {
        let freezing = ctx
            .state
            .current_weather
            .as_ref()
            .map(|w| w.temperature < -8.0)
            .unwrap_or(false);
        ctx.conditions.is_raining
            || ctx.conditions.is_thunderstorm
            || ctx.conditions.is_snowing
            || ctx.conditions.is_foggy
            || freezing
    }

    fn pick_next_state(&mut self, ctx: &FrameContext<'_>, rng: &mut (impl Rng + ?Sized)) {
        let is_day = ctx.conditions.sun.is_day;
        let roll = rng.random::<f32>();

        // Ночью кот больше спит, охотится на светлячков
        let (walk_p, sleep_p, hunt_p, dig_p) = if is_day {
            (0.40, 0.15, 0.20, 0.10)
        } else {
            (0.25, 0.40, 0.15, 0.05)
        };

        if roll < walk_p {
            self.state = CatState::Walk;
            self.running = rng.random::<f32>() < 0.3;
            self.target_x =
                2.0 + rng.random::<f32>() * (self.terminal_width.saturating_sub(10) as f32);
        } else if roll < walk_p + sleep_p {
            self.state = CatState::Sleep;
            self.timer = 450 + (rng.random::<u32>() % 900);
        } else if roll < walk_p + sleep_p + hunt_p {
            self.state = CatState::Hunt(HuntPhase::Stalk);
            self.timer = 300;
            let side = if rng.random::<bool>() { 1.0 } else { -1.0 };
            self.butterfly_x = (self.x + side * (8.0 + rng.random::<f32>() * 8.0))
                .clamp(3.0, self.terminal_width.saturating_sub(4) as f32);
            self.butterfly_y = -3.0;
            self.butterfly_phase = 0.0;
        } else if roll < walk_p + sleep_p + hunt_p + dig_p {
            // Ямка копается у правого края, у забора
            self.state = CatState::Walk;
            self.running = false;
            self.target_x = (self.terminal_width as f32 * 0.85)
                .min(self.terminal_width.saturating_sub(8) as f32);
        } else {
            self.state = CatState::Sit;
            self.timer = 90 + (rng.random::<u32>() % 240);
        }
    }

    fn update_impl(&mut self, ctx: &FrameContext<'_>, rng: &mut (impl Rng + ?Sized)) {
        self.terminal_width = ctx.size.width;
        self.terminal_height = ctx.size.height;
        self.frame = self.frame.wrapping_add(1);

        let bad = Self::bad_weather(ctx);
        let shelter_x = self.shelter_x(ctx.chimney.map(|c| c.x));

        // Непогода: всё бросаем и бежим под крышу
        if bad {
            if (self.x - shelter_x).abs() > 1.5 {
                self.facing_right = shelter_x > self.x;
                self.x += if self.facing_right { 0.4 } else { -0.4 };
                self.state = CatState::Walk;
                self.running = true;
            } else {
                self.state = CatState::Shelter;
            }
            return;
        }

        // Погода наладилась - выходим из укрытия
        if self.state == CatState::Shelter {
            self.state = CatState::Sit;
            self.timer = 45;
        }

        match self.state {
            CatState::Shelter => {}
            CatState::Sit => {
                if self.timer == 0 {
                    self.pick_next_state(ctx, rng);
                } else {
                    self.timer -= 1;
                }
            }
            CatState::Walk => {
                let speed = if self.running { 0.35 } else { 0.12 };
                self.facing_right = self.target_x > self.x;
                self.x += if self.facing_right { speed } else { -speed };
                if (self.x - self.target_x).abs() < 1.0 {
                    // Дошли до забора - копаем; иначе просто садимся
                    if self.target_x >= self.terminal_width as f32 * 0.8 {
                        self.state = CatState::Dig(0);
                        self.timer = 90;
                    } else {
                        self.state = CatState::Sit;
                        self.timer = 90 + (rng.random::<u32>() % 240);
                    }
                }
            }
            CatState::Sleep => {
                if self.timer == 0 {
                    self.state = CatState::Sit;
                    self.timer = 60;
                } else {
                    self.timer -= 1;
                }
            }
            CatState::Hunt(phase) => {
                // Бабочка порхает синусоидой
                self.butterfly_phase += 0.15;
                self.butterfly_x += self.butterfly_phase.sin() * 0.3;
                match phase {
                    HuntPhase::Stalk => {
                        self.facing_right = self.butterfly_x > self.x;
                        self.x += if self.facing_right { 0.07 } else { -0.07 };
                        if (self.x + CAT_WIDTH as f32 / 2.0 - self.butterfly_x).abs() < 3.0 {
                            self.state = CatState::Hunt(HuntPhase::Pounce);
                            self.timer = 12;
                        } else if self.timer == 0 {
                            // Бабочка улетела
                            self.state = CatState::Sit;
                            self.timer = 60;
                        } else {
                            self.timer -= 1;
                        }
                    }
                    HuntPhase::Pounce => {
                        if self.timer == 0 {
                            self.state = CatState::Sit;
                            self.timer = 90;
                        } else {
                            self.timer -= 1;
                        }
                    }
                }
            }
            CatState::Dig(step) => {
                if self.timer == 0 {
                    if step >= 2 {
                        self.state = CatState::Sit;
                        self.timer = 120;
                    } else {
                        self.state = CatState::Dig(step + 1);
                        self.timer = 60;
                    }
                } else {
                    self.timer -= 1;
                }
            }
        }

        self.x = self
            .x
            .clamp(1.0, self.terminal_width.saturating_sub(7) as f32);
    }

    fn current_pose(&self) -> [&'static str; 2] {
        match self.state {
            CatState::Shelter => {
                if self.frame % 90 < 8 {
                    CAT_SIT_BLINK
                } else {
                    CAT_SIT
                }
            }
            CatState::Sit => {
                if self.frame % 75 < 6 {
                    CAT_SIT_BLINK
                } else {
                    CAT_SIT
                }
            }
            CatState::Walk => {
                if (self.frame / 4) % 2 == 0 {
                    CAT_WALK_A
                } else {
                    CAT_WALK_B
                }
            }
            CatState::Sleep => {
                if (self.frame / 12) % 2 == 0 {
                    CAT_SLEEP_A
                } else {
                    CAT_SLEEP_B
                }
            }
            CatState::Hunt(HuntPhase::Stalk) => {
                if (self.frame / 6) % 2 == 0 {
                    CAT_WALK_A
                } else {
                    CAT_WALK_B
                }
            }
            CatState::Hunt(HuntPhase::Pounce) => CAT_POUNCE,
            CatState::Dig(_) => CAT_DIG,
        }
    }

    fn render_impl(
        &self,
        renderer: &mut TerminalRenderer,
        ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        let cat_y = self.ground_row(ctx.horizon_y);
        let x = self.x.floor() as i16;
        let pose = self.current_pose();

        for (row, line) in pose.iter().enumerate() {
            let render_y = cat_y + row as i16;
            if render_y < 0 || render_y >= self.terminal_height as i16 {
                continue;
            }
            let chars: Vec<char> = if self.facing_right {
                line.chars().collect()
            } else {
                line.chars()
                    .rev()
                    .map(|ch| match ch {
                        '(' => ')',
                        ')' => '(',
                        '/' => '\\',
                        '\\' => '/',
                        c => c,
                    })
                    .collect()
            };
            for (col, ch) in chars.iter().enumerate() {
                if *ch == ' ' {
                    continue;
                }
                let render_x = x + col as i16;
                if render_x < 0 || render_x >= self.terminal_width as i16 {
                    continue;
                }
                let color = match *ch {
                    'o' | '>' | '<' => Color::Green,
                    'z' | 'Z' => Color::Cyan,
                    _ => Color::DarkYellow,
                };
                renderer.render_char(render_x as u16, render_y as u16, *ch, color)?;
            }
        }

        // Бабочка днём / светлячок ночью
        if let CatState::Hunt(_) = self.state {
            let bx = self.butterfly_x.floor() as i16;
            let by = cat_y + self.butterfly_y as i16 + (self.butterfly_phase.cos() * 1.2) as i16;
            if bx >= 0
                && bx < self.terminal_width as i16
                && by >= 0
                && by < self.terminal_height as i16
            {
                let (ch, color) = if ctx.conditions.sun.is_day {
                    // Ж машет крыльями
                    if (self.frame / 5) % 2 == 0 {
                        ('Ж', Color::Magenta)
                    } else {
                        ('ж', Color::Magenta)
                    }
                } else {
                    ('*', Color::Yellow)
                };
                renderer.render_char(bx as u16, by as u16, ch, color)?;
            }
        }

        // Следы копания
        if let CatState::Dig(step) = self.state {
            let dot_x = x + if self.facing_right { CAT_WIDTH + 1 } else { -2 };
            let dot_y = cat_y + CAT_HEIGHT - 1;
            if dot_x >= 0
                && dot_x < self.terminal_width as i16
                && dot_y >= 0
                && dot_y < self.terminal_height as i16
            {
                let ch = match step {
                    0 => '.',
                    1 => 'o',
                    _ => 'O',
                };
                renderer.render_char(dot_x as u16, dot_y as u16, ch, Color::DarkGrey)?;
            }
        }

        Ok(())
    }
}

impl AnimationSystem for CatSystem {
    fn id(&self) -> &'static str {
        "cat"
    }

    fn layer(&self) -> RenderLayer {
        // Поверх сцены: кот ходит перед домом
        RenderLayer::PostScene
    }

    fn on_resize(&mut self, size: TerminalSize) {
        self.terminal_width = size.width;
        self.terminal_height = size.height;
        self.x = self.x.clamp(1.0, size.width.saturating_sub(7) as f32);
    }

    fn update(&mut self, ctx: &FrameContext<'_>, rng: &mut dyn Rng, _commands: &mut FrameCommands) {
        self.update_impl(ctx, rng);
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        self.render_impl(renderer, ctx)
    }
}
