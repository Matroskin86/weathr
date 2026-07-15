//! Кот, живущий в сцене. В плохую погоду сидит у дома под крышей,
//! в хорошую - гуляет, бегает, спит (потом потягивается), охотится
//! (днём бабочка, ночью светлячок, в листопад лист, изредка мышь;
//! ловит через раз - после удачи умывается), копает ямки у забора,
//! провожает взглядом самолёты и МКС, а в полнолуние смотрит на луну.

use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crate::scene::world::house::House;
use crate::weather::moonphase;
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
// Смотрит в небо на пролетающий борт/МКС/луну - звёзды в глазах
const CAT_SKYWATCH: [&str; 2] = ["  /\\_/\\", "~(=*.*=)"];
// Потягивается после сна
const CAT_STRETCH: [&str; 2] = ["  /\\_/\\", "\\(=>.<=)/"];
// Умывается лапкой после удачной охоты
const CAT_GROOM_A: [&str; 2] = ["  /\\_/\\", "~(=-.-=)c"];
const CAT_GROOM_B: [&str; 2] = ["  /\\_/\\", "~(=^.^=)"];

const CAT_HEIGHT: i16 = 2;
const CAT_WIDTH: i16 = 9;

/// Фазы охоты
#[derive(Clone, Copy, PartialEq)]
enum HuntPhase {
    Stalk,
    Pounce,
}

/// Кого ловим: воздушная добыча порхает, мышь убегает по земле
#[derive(Clone, Copy, PartialEq)]
enum Prey {
    Butterfly,
    Firefly,
    Leaf,
    Mouse,
}

#[derive(Clone, Copy, PartialEq)]
enum CatState {
    /// Плохая погода: сидит у стены дома
    Shelter,
    Sit,
    Walk,
    Sleep,
    /// Потягивание после сна
    Stretch,
    /// Умывание после удачной охоты
    Groom,
    Hunt(HuntPhase),
    Dig(u8),
    /// Провожает взглядом самолёт или МКС
    SkyWatch,
    /// Ясная ночь полнолуния: сидит и смотрит на луну
    MoonGaze,
}

pub struct CatSystem {
    state: CatState,
    x: f32,
    facing_right: bool,
    // Кадры до смены состояния
    timer: u32,
    // Счётчик кадров для анимации поз
    frame: u32,
    target_x: f32,
    running: bool,
    // Добыча
    prey: Prey,
    prey_x: f32,
    prey_y: f32,
    prey_phase: f32,
    // Кадры, пока в небе есть на что смотреть (самолёт/МКС)
    sky_watch_frames: u32,
    // Светлячок присел на хвост спящего кота (кадров осталось)
    firefly_rest: u32,
    // Демо-режим: короткие таймеры, частые события
    demo: bool,
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
            prey: Prey::Butterfly,
            prey_x: 0.0,
            prey_y: 0.0,
            prey_phase: 0.0,
            sky_watch_frames: 0,
            firefly_rest: 0,
            demo: false,
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
                (house_x as f32 + 20.0).min(self.terminal_width.saturating_sub(9) as f32)
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

    fn full_moon_night(ctx: &FrameContext<'_>) -> bool {
        if ctx.conditions.sun.is_day || ctx.conditions.is_cloudy {
            return false;
        }
        ctx.state
            .current_weather
            .as_ref()
            .and_then(|w| w.moon_phase)
            .map(moonphase::is_full_moon)
            .unwrap_or(false)
    }

    /// Делитель таймеров: в демо кошачья жизнь идёт в 4 раза быстрее
    fn pace(&self) -> u32 {
        if self.demo { 4 } else { 1 }
    }

    fn start_hunt(&mut self, ctx: &FrameContext<'_>, rng: &mut (impl Rng + ?Sized)) {
        let is_day = ctx.conditions.sun.is_day;
        self.prey = if is_day {
            if ctx.show_leaves {
                Prey::Leaf
            } else {
                Prey::Butterfly
            }
        } else {
            Prey::Firefly
        };
        self.state = CatState::Hunt(HuntPhase::Stalk);
        self.timer = 300;
        let side = if rng.random::<bool>() { 1.0 } else { -1.0 };
        self.prey_x = (self.x + side * (8.0 + rng.random::<f32>() * 8.0))
            .clamp(3.0, self.terminal_width.saturating_sub(4) as f32);
        self.prey_y = if self.prey == Prey::Leaf { -7.0 } else { -3.0 };
        self.prey_phase = 0.0;
    }

    fn start_mouse_hunt(&mut self, rng: &mut (impl Rng + ?Sized)) {
||        self.prey = Prey::Mouse;
        self.state = CatState::Hunt(HuntPhase::Stalk);
        self.timer = 450;
        // Мышь выскакивает рядом и удирает от кота к ближайшему краю
        let side = if rng.random::<bool>() { 1.0 } else { -1.0 };
        self.prey_x = (self.x + side * 10.0).clamp(3.0, self.terminal_width as f32 - 6.0);
        self.prey_y = 0.0;
        self.prey_phase = 0.0;
    }

    fn pick_next_state(&mut self, ctx: &FrameContext<'_>, rng: &mut (impl Rng + ?Sized)) {
        let is_day = ctx.conditions.sun.is_day;
        let roll = rng.random::<f32>();

        // Полнолуние ясной ночью: кот заворожён луной
        let moon_p = if Self::full_moon_night(ctx) {
            if self.demo { 0.2 } else { 0.15 }
        } else {
            0.0
        };
        // Ночью кот больше спит; мыши активнее в сумерках.
        // Мышь и охота идут в начале очереди: хвост распределения их не съедает
        let (walk_p, sleep_p, hunt_p, mouse_p, dig_p) = if is_day {
            (0.33, 0.13, 0.20, 0.10, 0.10)
        } else {
            (0.18, 0.30, 0.12, 0.16, 0.05)
        };
        // В демо мышь выскакивает почти через раз, охота чаще
        let (hunt_p, mouse_p) = if self.demo {
            (hunt_p * 1.5, 0.4)
        } else {
            (hunt_p, mouse_p)
        };

        let mut threshold = moon_p;
        if roll < threshold {
            self.state = CatState::MoonGaze;
            self.timer = (600 + (rng.random::<u32>() % 450)) / self.pace();
            // Луна рисуется в правой четверти неба
            self.facing_right = self.x < self.terminal_width as f32 * 0.75;
            return;
        }
        threshold += mouse_p;
        if roll < threshold {
            self.start_mouse_hunt(rng);
            return;
        }
        threshold += hunt_p;
        if roll < threshold {
            self.start_hunt(ctx, rng);
            return;
        }
        threshold += walk_p;
        if roll < threshold {
            self.state = CatState::Walk;
            self.running = rng.random::<f32>() < 0.3;
            self.target_x =
                2.0 + rng.random::<f32>() * (self.terminal_width.saturating_sub(12) as f32);
            return;
        }
        threshold += sleep_p;
        if roll < threshold {
            self.state = CatState::Sleep;
            self.timer = (450 + (rng.random::<u32>() % 900)) / self.pace();
            return;
        }
        threshold += dig_p;
        if roll < threshold {
            self.state = CatState::Walk;
            self.running = false;
            self.target_x = (self.terminal_width as f32 * 0.85)
                .min(self.terminal_width.saturating_sub(10) as f32);
            return;
        }
        self.state = CatState::Sit;
        self.timer = (90 + (rng.random::<u32>() % 240)) / self.pace();
    }

    /// Исход прыжка: через раз добыча поймана (умывание), иначе упущена
    fn resolve_pounce(&mut self, rng: &mut (impl Rng + ?Sized)) {
        if rng.random::<bool>() {
            self.state = CatState::Groom;
            self.timer = 45;
        } else {
            self.state = CatState::Sit;
            self.timer = 30;
        }
    }

    fn update_impl(&mut self, ctx: &FrameContext<'_>, rng: &mut (impl Rng + ?Sized)) {
        self.terminal_width = ctx.size.width;
        self.terminal_height = ctx.size.height;
        self.frame = self.frame.wrapping_add(1);
        self.sky_watch_frames = self.sky_watch_frames.saturating_sub(1);

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

        // В небе самолёт или МКС: спокойные занятия прерываются - кот смотрит
        if self.sky_watch_frames > 0
            && matches!(
                self.state,
                CatState::Sit | CatState::Walk | CatState::Groom | CatState::Stretch
            )
        {
            self.state = CatState::SkyWatch;
        }

        match self.state {
            CatState::Shelter => {}
            CatState::SkyWatch => {
                if self.sky_watch_frames == 0 {
                    self.state = CatState::Sit;
                    self.timer = 45;
                }
            }
            CatState::MoonGaze => {
                if self.timer == 0 || !Self::full_moon_night(ctx) {
                    self.state = CatState::Sit;
                    self.timer = 60;
                } else {
                    self.timer -= 1;
                }
            }
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
                    if self.target_x >= self.terminal_width as f32 * 0.8 {
                        self.state = CatState::Dig(0);
                        self.timer = 90;
                    } else {
                        self.state = CatState::Sit;
                        self.timer = (90 + (rng.random::<u32>() % 240)) / self.pace();
                    }
                }
            }
            CatState::Sleep => {
                if self.timer == 0 {
                    // Проснулся - потягивается
                    self.state = CatState::Stretch;
                    self.timer = 35;
                } else {
                    self.timer -= 1;
                }
            }
            CatState::Stretch => {
                if self.timer == 0 {
                    self.state = CatState::Sit;
                    self.timer = 60;
                } else {
                    self.timer -= 1;
                }
            }
            CatState::Groom => {
                if self.timer == 0 {
                    self.state = CatState::Sit;
                    self.timer = 90;
                } else {
                    self.timer -= 1;
                }
            }
            CatState::Hunt(phase) => {
                match self.prey {
                    Prey::Mouse => {
                        // Мышь удирает по земле к ближайшему краю
                        let dir = if self.prey_x >= self.x { 1.0 } else { -1.0 };
                        self.prey_x += dir * 0.34;
                    }
                    Prey::Leaf => {
                        // Лист планирует вниз, покачиваясь
                        self.prey_phase += 0.1;
                        self.prey_x += self.prey_phase.sin() * 0.4;
                        if self.prey_y < -1.0 {
                            self.prey_y += 0.06;
                        }
                    }
                    _ => {
                        // Бабочка/светлячок порхает синусоидой
                        self.prey_phase += 0.15;
                        self.prey_x += self.prey_phase.sin() * 0.3;
                    }
                }

                match phase {
                    HuntPhase::Stalk => {
                        let chase_speed = if self.prey == Prey::Mouse { 0.4 } else { 0.07 };
                        self.facing_right = self.prey_x > self.x;
                        self.x += if self.facing_right {
                            chase_speed
                        } else {
                            -chase_speed
                        };

                        let mouse_escaped = self.prey == Prey::Mouse
                            && (self.prey_x <= 1.0
                                || self.prey_x >= self.terminal_width as f32 - 2.0);

                        if mouse_escaped || self.timer == 0 {
                            // Добыча ушла
                            self.state = CatState::Sit;
                            self.timer = 45;
                        } else if (self.x + CAT_WIDTH as f32 / 2.0 - self.prey_x).abs() < 3.0 {
                            self.state = CatState::Hunt(HuntPhase::Pounce);
                            self.timer = 12;
                        } else {
                            self.timer -= 1;
                        }
                    }
                    HuntPhase::Pounce => {
                        if self.timer == 0 {
                            self.resolve_pounce(rng);
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

        // Тёплой летней ночью светлячок может присесть на хвост спящего кота
        if self.state == CatState::Sleep {
            let warm_night = !ctx.conditions.sun.is_day
                && ctx
                    .state
                    .current_weather
                    .as_ref()
                    .map(|w| w.temperature > 15.0)
                    .unwrap_or(false);
            if self.firefly_rest > 0 {
                self.firefly_rest -= 1;
            } else if warm_night {
                let chance = if self.demo { 0.02 } else { 0.002 };
                if rng.random::<f32>() < chance {
                    self.firefly_rest = 120 + rng.random::<u32>() % 150;
                }
            }
        } else {
            self.firefly_rest = 0;
        }

        self.x = self
            .x
            .clamp(1.0, self.terminal_width.saturating_sub(9) as f32);
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
            CatState::SkyWatch | CatState::MoonGaze => CAT_SKYWATCH,
            CatState::Stretch => CAT_STRETCH,
            CatState::Groom => {
                if (self.frame / 8) % 2 == 0 {
                    CAT_GROOM_A
                } else {
                    CAT_GROOM_B
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
        let is_day = ctx.conditions.sun.is_day;

        for (row, line) in pose.iter().enumerate() {
            let render_y = cat_y + row as i16;
            if render_y < 0 || render_y >= self.terminal_height as i16 {
                continue;
            }
            // Строки позы разной длины: перед зеркалированием выравниваем
            // до общей ширины, иначе уши съезжают на символ (баг с хвостом вправо)
            let pose_width = pose.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let chars: Vec<char> = if self.facing_right {
                line.chars().collect()
            } else {
                let mut padded: Vec<char> = line.chars().collect();
                padded.resize(pose_width, ' ');
                padded
                    .iter()
                    .rev()
                    .map(|ch| match ch {
                        '(' => ')',
                        ')' => '(',
                        '/' => '\\',
                        '\\' => '/',
                        'c' => 'ɔ',
                        c => *c,
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
                    // Глаза: на охоте горят красным, днём зелёные, в темноте жёлтые
                    'o' | '^' | '*' | '>' | '<' => {
                        if matches!(self.state, CatState::Hunt(_)) {
                            Color::Rgb { r: 255, g: 70, b: 70 }
                        } else if is_day {
                            Color::Green
                        } else {
                            Color::Yellow
                        }
                    }
                    'z' | 'Z' => Color::Cyan,
                    _ => Color::DarkYellow,
                };
                renderer.render_char(render_x as u16, render_y as u16, *ch, color)?;
            }
        }

        // Добыча
        if let CatState::Hunt(_) = self.state {
            match self.prey {
                Prey::Mouse => {
                    // Мышь бежит по земле, мордой от кота
                    let my = cat_y + CAT_HEIGHT - 1;
                    let sprite = if self.prey_x >= self.x { "~~E:>" } else { "<:3~~" };
                    let mx = self.prey_x.floor() as i16 - 2;
                    for (i, ch) in sprite.chars().enumerate() {
                        let render_x = mx + i as i16;
                        if render_x >= 0
                            && render_x < self.terminal_width as i16
                            && my >= 0
                            && my < self.terminal_height as i16
                        {
                            renderer.render_char(render_x as u16, my as u16, ch, Color::Grey)?;
                        }
                    }
                }
                _ => {
                    let bx = self.prey_x.floor() as i16;
                    let wave = if self.prey == Prey::Leaf {
                        0
                    } else {
                        (self.prey_phase.cos() * 1.2) as i16
                    };
                    let by = cat_y + self.prey_y as i16 + wave;
                    if bx >= 0
                        && bx < self.terminal_width as i16
                        && by >= 0
                        && by < self.terminal_height as i16
                    {
                        let (ch, color) = match self.prey {
                            Prey::Butterfly => {
                                if (self.frame / 5) % 2 == 0 {
                                    ('Ж', Color::Magenta)
                                } else {
                                    ('ж', Color::Magenta)
                                }
                            }
                            Prey::Firefly => ('*', Color::Yellow),
                            Prey::Leaf => ('*', Color::DarkYellow),
                            Prey::Mouse => unreachable!(),
                        };
                        renderer.render_char(bx as u16, by as u16, ch, color)?;
                    }
                }
            }
        }

        // Светлячок мигает на хвосте спящего кота
        if self.firefly_rest > 0 && (self.frame / 7) % 2 == 0 {
            let tail_x = if self.facing_right {
                x
            } else {
                x + CAT_WIDTH - 2
            };
            let tail_y = cat_y + 1;
            if tail_x >= 0
                && tail_x < self.terminal_width as i16
                && tail_y >= 0
                && tail_y < self.terminal_height as i16
            {
                renderer.render_char(
                    tail_x as u16,
                    tail_y as u16,
                    '*',
                    Color::Rgb { r: 255, g: 250, b: 130 },
                )?;
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
        self.x = self.x.clamp(1.0, size.width.saturating_sub(9) as f32);
    }

    fn on_real_flight(&mut self, _label: &str, _eastbound: bool) {
        // Самолёт летит через экран около 20 секунд - кот провожает взглядом
        self.sky_watch_frames = 300;
    }

    fn on_iss_pass(&mut self, _label: &str) {
        self.sky_watch_frames = 220;
    }

    fn on_demo_mode(&mut self, demo: bool) {
        self.demo = demo;
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
