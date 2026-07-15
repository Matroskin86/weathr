use crate::animation::{AnimationSystem, FrameCommands, FrameContext, RenderLayer, TerminalSize};
use crate::render::TerminalRenderer;
use crossterm::style::Color;

use rand::{Rng, RngExt};
use std::io;
use std::sync::OnceLock;

const AIRPLANE_ART: &str = include_str!("assets/airplane.txt");

/// Зеркалит ASCII-арт по горизонтали для полёта в обратную сторону:
/// строки разворачиваются, парные символы меняются местами
fn mirror_art(art: &str) -> Vec<String> {
    let width = art.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    art.lines()
        .map(|line| {
            let mut padded: Vec<char> = line.chars().collect();
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
                    '`' => '\'',
                    '\'' => '`',
                    c => *c,
                })
                .collect()
        })
        .collect()
}

// Нос самолёта в ассете смотрит вправо (летит слева направо)
fn art_eastbound() -> &'static Vec<String> {
    static ART: OnceLock<Vec<String>> = OnceLock::new();
    ART.get_or_init(|| AIRPLANE_ART.lines().map(|l| l.to_string()).collect())
}

fn art_westbound() -> &'static Vec<String> {
    static ART: OnceLock<Vec<String>> = OnceLock::new();
    ART.get_or_init(|| mirror_art(AIRPLANE_ART))
}

#[derive(Clone)]
struct Airplane {
    x: f32,
    y: f32,
    // Знак скорости задаёт направление: >0 слева направо
    speed: f32,
    // Подпись реального борта: "A320 RA-73756 | SU1234 SVO-LED"
    label: Option<String>,
}

pub struct AirplaneSystem {
    planes: Vec<Airplane>,
    terminal_width: u16,
    terminal_height: u16,
    spawn_cooldown: u16,
    // true = только реальные борта, случайные не спавнятся
    real_only: bool,
}

impl AirplaneSystem {
    pub fn new(terminal_width: u16, terminal_height: u16) -> Self {
        Self {
            planes: Vec::with_capacity(2),
            terminal_width,
            terminal_height,
            spawn_cooldown: 0,
            real_only: false,
        }
    }

    fn art_width(art: &[String]) -> f32 {
        art.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32
    }

    pub fn update(
        &mut self,
        terminal_width: u16,
        terminal_height: u16,
        rng: &mut (impl Rng + ?Sized),
    ) {
        self.terminal_width = terminal_width;
        self.terminal_height = terminal_height;

        for plane in &mut self.planes {
            plane.x += plane.speed;
        }

        let width_f = terminal_width as f32;
        self.planes.retain(|p| {
            // Запас на ширину арта и подписи, чтобы борт полностью уходил за край
            let margin = Self::art_width(art_eastbound())
                .max(p.label.as_ref().map_or(0.0, |l| l.chars().count() as f32));
            if p.speed >= 0.0 {
                p.x < width_f
            } else {
                p.x + margin > 0.0
            }
        });

        self.spawn_cooldown = self.spawn_cooldown.saturating_sub(1);
        if !self.real_only && self.spawn_cooldown == 0 && rng.random::<f32>() < 0.001 {
            self.spawn_plane(rng);
            self.spawn_cooldown = 600 + (rng.random::<u16>() % 300);
        }
    }

    /// Эшелон самолётов: ниже МКС (строки 7-11 её), чтобы борта не наезжали на станцию
    fn flight_level(terminal_height: u16, rng: &mut (impl Rng + ?Sized)) -> f32 {
        let band = (terminal_height / 7).max(2);
        (12 + (rng.random::<u16>() % band)) as f32
    }

    fn spawn_plane(&mut self, rng: &mut (impl Rng + ?Sized)) {
        let y = Self::flight_level(self.terminal_height, rng);
        let speed = 0.3 + (rng.random::<f32>() * 0.2);

        self.planes.push(Airplane {
            x: 0.0,
            y,
            speed,
            label: None,
        });
    }

    /// Спавн реального борта с подписью; направление по курсу из OpenSky
    pub fn spawn_labeled(&mut self, label: &str, eastbound: bool) {
        // Один реальный борт на экране за раз - не устраиваем аэрошоу
        if self.planes.iter().any(|p| p.label.is_some()) {
            return;
        }

        let y = 12.0 + (self.terminal_height / 14) as f32;
        let art_w = Self::art_width(art_eastbound());

        let (x, speed) = if eastbound {
            (-art_w, 0.35)
        } else {
            (self.terminal_width as f32, -0.35)
        };

        self.planes.push(Airplane {
            x,
            y,
            speed,
            label: Some(label.to_string()),
        });
    }

    pub fn render(&self, renderer: &mut TerminalRenderer) -> io::Result<()> {
        for plane in &self.planes {
            let x = plane.x.floor() as i16;
            let y = plane.y as i16;

            let art = if plane.speed >= 0.0 {
                art_eastbound()
            } else {
                art_westbound()
            };

            for (line_offset, line) in art.iter().enumerate() {
                let render_y = y + line_offset as i16;
                if render_y < 0 || render_y >= self.terminal_height as i16 {
                    continue;
                }

                for (char_offset, ch) in line.chars().enumerate() {
                    let render_x = x + char_offset as i16;
                    if render_x < 0 || render_x >= self.terminal_width as i16 {
                        continue;
                    }

                    if ch != ' ' {
                        let color = match ch {
                            '"' => Color::Cyan,
                            '\\' | '/' => Color::Blue,
                            '_' => Color::DarkGrey,
                            '~' => Color::Grey,
                            _ => Color::White,
                        };
                        renderer.render_char(render_x as u16, render_y as u16, ch, color)?;
                    }
                }
            }

            // Подпись реального борта под самолётом
            if let Some(ref label) = plane.label {
                let label_y = y + art.len() as i16;
                if label_y >= 0 && label_y < self.terminal_height as i16 {
                    for (char_offset, ch) in label.chars().enumerate() {
                        let render_x = x + char_offset as i16;
                        if render_x < 0 || render_x >= self.terminal_width as i16 {
                            continue;
                        }
                        renderer.render_char(
                            render_x as u16,
                            label_y as u16,
                            ch,
                            Color::DarkGrey,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl AnimationSystem for AirplaneSystem {
    fn id(&self) -> &'static str {
        "airplanes"
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
        self.planes
            .retain(|p| p.x < size.width as f32 && p.y < size.height as f32);
    }

    fn on_real_flight(&mut self, label: &str, eastbound: bool) {
        self.spawn_labeled(label, eastbound);
    }

    fn on_flights_mode(&mut self, real_only: bool) {
        self.real_only = real_only;
    }

    fn update(&mut self, ctx: &FrameContext<'_>, rng: &mut dyn Rng, _commands: &mut FrameCommands) {
        self.update(ctx.size.width, ctx.size.height, rng);
    }

    fn render(
        &mut self,
        renderer: &mut TerminalRenderer,
        _ctx: &FrameContext<'_>,
    ) -> io::Result<()> {
        AirplaneSystem::render(self, renderer)
    }
}
