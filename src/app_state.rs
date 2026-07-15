use crate::config::LocationDisplay;
use crate::weather::{
    WeatherCondition, WeatherConditions, WeatherData, WeatherLocation, WeatherUnits,
    format_precipitation, format_temperature, format_wind_speed,
};
use crossterm::style::Color;
use std::time::Instant;

/// Румб по метеонаправлению ветра (откуда дует), 8 направлений
pub fn wind_rumb_ru(direction_deg: f64) -> &'static str {
    let deg = direction_deg.rem_euclid(360.0);
    match ((deg + 22.5) / 45.0) as usize % 8 {
        0 => "С",
        1 => "СВ",
        2 => "В",
        3 => "ЮВ",
        4 => "Ю",
        5 => "ЮЗ",
        6 => "З",
        _ => "СЗ",
    }
}

/// Стрелка, куда ветер несёт (направление в метео - откуда дует)
pub fn wind_arrow(direction_deg: f64) -> char {
    let to = (direction_deg + 180.0).rem_euclid(360.0);
    ['↑', '↗', '→', '↘', '↓', '↙', '←', '↖'][((to + 22.5) / 45.0) as usize % 8]
}

/// Узкий глиф погодного условия (только одноклеточные символы)
pub fn condition_glyph(condition: WeatherCondition, is_day: bool) -> char {
    match condition {
        WeatherCondition::Clear => {
            if is_day {
                '☀'
            } else {
                '☾'
            }
        }
        WeatherCondition::PartlyCloudy | WeatherCondition::Cloudy | WeatherCondition::Overcast => {
            '☁'
        }
        WeatherCondition::Fog => '≡',
        WeatherCondition::Drizzle
        | WeatherCondition::Rain
        | WeatherCondition::RainShowers
        | WeatherCondition::FreezingRain => '☂',
        WeatherCondition::Snow | WeatherCondition::SnowGrains | WeatherCondition::SnowShowers => {
            '❄'
        }
        WeatherCondition::Thunderstorm | WeatherCondition::ThunderstormHail => 'ϟ',
    }
}

/// Цвет условия для HUD
pub fn condition_color(condition: WeatherCondition, is_day: bool) -> Color {
    use crossterm::style::Color as C;
    match condition {
        WeatherCondition::Clear => {
            if is_day {
                C::Yellow
            } else {
                C::Rgb { r: 200, g: 205, b: 235 }
            }
        }
        WeatherCondition::PartlyCloudy => C::Grey,
        WeatherCondition::Cloudy | WeatherCondition::Overcast => C::DarkGrey,
        WeatherCondition::Fog => C::Grey,
        WeatherCondition::Drizzle
        | WeatherCondition::Rain
        | WeatherCondition::RainShowers
        | WeatherCondition::FreezingRain => C::Rgb { r: 110, g: 175, b: 255 },
        WeatherCondition::Snow | WeatherCondition::SnowGrains | WeatherCondition::SnowShowers => {
            C::White
        }
        WeatherCondition::Thunderstorm | WeatherCondition::ThunderstormHail => C::Magenta,
    }
}

/// Цвет температуры: от ледяного синего к жаркому оранжевому
pub fn temperature_color(celsius: f64) -> Color {
    use crossterm::style::Color as C;
    if celsius < 0.0 {
        C::Rgb { r: 150, g: 190, b: 255 }
    } else if celsius < 10.0 {
        C::Cyan
    } else if celsius < 18.0 {
        C::Rgb { r: 130, g: 220, b: 160 }
    } else if celsius < 25.0 {
        C::Rgb { r: 255, g: 205, b: 95 }
    } else {
        C::Rgb { r: 255, g: 140, b: 80 }
    }
}

/// Цветной кусок строки HUD-блока
pub type HudSpan = (String, Color);

/// Русские названия погоды для HUD (локальный патч скринсейвера)
pub fn condition_name_ru(condition: WeatherCondition) -> &'static str {
    match condition {
        WeatherCondition::Clear => "Ясно",
        WeatherCondition::Cloudy => "Облачно",
        WeatherCondition::PartlyCloudy => "Переменная облачность",
        WeatherCondition::Overcast => "Пасмурно",
        WeatherCondition::Fog => "Туман",
        WeatherCondition::Drizzle => "Морось",
        WeatherCondition::FreezingRain => "Ледяной дождь",
        WeatherCondition::Rain => "Дождь",
        WeatherCondition::Snow => "Снег",
        WeatherCondition::SnowGrains => "Снежная крупа",
        WeatherCondition::RainShowers => "Ливень",
        WeatherCondition::SnowShowers => "Снегопад",
        WeatherCondition::Thunderstorm => "Гроза",
        WeatherCondition::ThunderstormHail => "Гроза с градом",
    }
}

pub struct AppState {
    pub current_weather: Option<WeatherData>,
    pub is_offline: bool,
    pub weather_conditions: WeatherConditions,
    pub loading_state: LoadingState,
    pub cached_weather_info: String,
    pub cached_forecast_info: String,
    /// HUD-блок: строки из цветных кусков, рисуется в рамке
    pub hud_block: Vec<Vec<HudSpan>>,
    pub weather_info_needs_update: bool,
    pub location: WeatherLocation,
    pub city_name: Option<String>,
    pub location_display: LocationDisplay,
    pub hide_location: bool,
    pub use_feels_like_temperature: bool,
    pub hide_quit_hint: bool,
    pub units: WeatherUnits,
    // Минута последнего форматирования HUD: смена минуты обновляет часы в HUD
    pub last_clock_minute: u32,
    // Unix-время данных, показываемых в оффлайне (для подписи возраста в HUD)
    pub offline_data_cached_at: Option<u64>,
    // Unix-время последнего успешного обновления погоды
    pub last_success_at: Option<u64>,
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Возраст данных для оффлайн-подписи: "N мин назад" / "N ч назад"
fn format_age(age_secs: u64) -> String {
    let minutes = age_secs / 60;
    if minutes < 120 {
        format!("{} мин назад", minutes.max(1))
    } else {
        format!("{} ч назад", minutes / 60)
    }
}

impl AppState {
    pub fn new(
        location: WeatherLocation,
        city_name: Option<String>,
        location_display: LocationDisplay,
        hide_location: bool,
        use_feels_like_temperature: bool,
        hide_quit_hint: bool,
        units: WeatherUnits,
    ) -> Self {
        Self {
            current_weather: None,
            is_offline: false,
            weather_conditions: WeatherConditions::default(),
            loading_state: LoadingState::new(),
            cached_weather_info: String::new(),
            cached_forecast_info: String::new(),
            hud_block: Vec::new(),
            weather_info_needs_update: true,
            location,
            city_name,
            location_display,
            hide_location,
            use_feels_like_temperature,
            hide_quit_hint,
            units,
            last_clock_minute: u32::MAX,
            offline_data_cached_at: None,
            last_success_at: None,
        }
    }

    /// Раз в минуту помечает HUD на перерисовку, чтобы часы не отставали
    pub fn tick_clock(&mut self) {
        use chrono::Timelike;
        let minute = chrono::Local::now().minute();
        if minute != self.last_clock_minute {
            self.last_clock_minute = minute;
            self.weather_info_needs_update = true;
        }
    }

    pub fn update_weather(&mut self, weather: WeatherData) {
        self.weather_conditions.is_thunderstorm = weather.condition.is_thunderstorm();
        self.weather_conditions.is_snowing = weather.condition.is_snowing();
        self.weather_conditions.is_raining =
            weather.condition.is_raining() && !self.weather_conditions.is_thunderstorm;
        self.weather_conditions.is_cloudy = weather.condition.is_cloudy();
        self.weather_conditions.is_foggy = weather.condition.is_foggy();
        self.weather_conditions.sun = weather.sun;

        self.current_weather = Some(weather);
        self.is_offline = false;
        self.offline_data_cached_at = None;
        self.last_success_at = Some(unix_now());
        self.weather_info_needs_update = true;
    }

    /// Оффлайн с сохранёнными данными: показываем их возраст в HUD
    pub fn set_offline_with_data_age(&mut self, cached_at: u64) {
        self.offline_data_cached_at = Some(cached_at);
        self.set_offline_mode(true);
    }

    pub fn set_offline_mode(&mut self, offline: bool) {
        self.is_offline = offline;
        self.weather_info_needs_update = true;
    }

    pub fn update_loading_animation(&mut self) {
        if self.loading_state.should_update() {
            self.loading_state.next_frame();
            self.weather_info_needs_update = true;
        }
    }

    pub fn get_condition_text(&self) -> &str {
        if let Some(ref weather) = self.current_weather {
            condition_name_ru(weather.condition)
        } else {
            "Загрузка"
        }
    }

    pub fn update_cached_info(&mut self) {
        if !self.weather_info_needs_update {
            return;
        }

        let location_str = if self.hide_location {
            String::new()
        } else {
            let (lat_value, lat_dir) = if self.location.latitude >= 0.0 {
                (self.location.latitude, "N")
            } else {
                (-self.location.latitude, "S")
            };
            let (lon_value, lon_dir) = if self.location.longitude >= 0.0 {
                (self.location.longitude, "E")
            } else {
                (-self.location.longitude, "W")
            };
            let coords = format!("{:.2}°{}, {:.2}°{}", lat_value, lat_dir, lon_value, lon_dir);
            let label = match self.location_display {
                LocationDisplay::Coordinates => coords,
                LocationDisplay::City => match &self.city_name {
                    Some(city) => city.clone(),
                    None => coords,
                },
                LocationDisplay::Mixed => match &self.city_name {
                    Some(city) => format!("{} ({})", city, coords),
                    None => coords,
                },
            };
            label
        };

        let clock = chrono::Local::now().format("%H:%M").to_string();

        // HUD-блок: аккуратная сводка цветными кусками, рисуется в рамке.
        // Дизайн: время и место, температура цветом по шкале, ветер со
        // стрелкой, прогноз погодными глифами, закат/восход со своим знаком
        let dim = Color::DarkGrey;
        let sep = (" · ".to_string(), dim);
        let mut block: Vec<Vec<HudSpan>> = Vec::new();

        if let Some(ref weather) = self.current_weather {
            let is_day = weather.sun.is_day;
            let (temp, temp_unit) = format_temperature(weather.temperature, self.units.temperature);
            let (wind, wind_unit) = format_wind_speed(weather.wind_speed, self.units.wind_speed);
            let (precip, precip_unit) =
                format_precipitation(weather.precipitation, self.units.precipitation);

            // Строка 1: время · место · условие с глифом
            let mut line = vec![(clock.clone(), Color::Cyan)];
            if !location_str.is_empty() {
                line.push(sep.clone());
                line.push((location_str.clone(), Color::White));
            }
            line.push(sep.clone());
            line.push((
                format!(
                    "{} {}",
                    condition_glyph(weather.condition, is_day),
                    condition_name_ru(weather.condition)
                ),
                condition_color(weather.condition, is_day),
            ));
            block.push(line);

            // Строка 2: температура (цвет по шкале) и "ощущается"
            let mut line = vec![(
                format!("{:.1}{}", temp, temp_unit),
                temperature_color(weather.temperature),
            )];
            if self.use_feels_like_temperature {
                let (apparent, unit) =
                    format_temperature(weather.feels_like_temperature, self.units.temperature);
                line.push((format!("  ощущается {:.1}{}", apparent, unit), dim));
            }
            block.push(line);

            // Строка 3: ветер со стрелкой + осадки
            let precip_color = if weather.precipitation > 0.05 {
                Color::Rgb { r: 110, g: 175, b: 255 }
            } else {
                dim
            };
            block.push(vec![
                (
                    format!(
                        "ветер {:.1}{} {}{}",
                        wind,
                        wind_unit,
                        wind_rumb_ru(weather.wind_direction),
                        wind_arrow(weather.wind_direction)
                    ),
                    Color::Grey,
                ),
                sep.clone(),
                (format!("☂ {:.1}{}", precip, precip_unit), precip_color),
            ]);

            // Строка 4: прогноз глифами + закат/восход
            let mut line: Vec<HudSpan> = Vec::new();
            for (i, point) in weather.forecast.iter().enumerate() {
                if i > 0 {
                    line.push((" ".to_string(), dim));
                }
                let (ptemp, _) = format_temperature(point.temperature, self.units.temperature);
                line.push((format!("+{}ч", point.hours_ahead), dim));
                line.push((
                    format!(" {:.0}°", ptemp),
                    temperature_color(point.temperature),
                ));
                line.push((
                    condition_glyph(point.condition, true).to_string(),
                    condition_color(point.condition, true),
                ));
            }
            let sun_event = if is_day {
                weather
                    .sun
                    .set
                    .map(|t| (format!("☾ {}", t.format("%H:%M")), Color::Rgb {
                        r: 255,
                        g: 150,
                        b: 80,
                    }))
            } else {
                weather
                    .sun
                    .rise
                    .map(|t| (format!("☀ {}", t.format("%H:%M")), Color::Yellow))
            };
            if let Some(event) = sun_event {
                if !line.is_empty() {
                    line.push(sep.clone());
                }
                line.push(event);
            }
            if !line.is_empty() {
                block.push(line);
            }

            // Оффлайн-статус отдельной строкой
            if self.is_offline {
                let text = match self.offline_data_cached_at {
                    Some(cached_at) => {
                        let age = unix_now().saturating_sub(cached_at);
                        format!("ОФФЛАЙН · данные {}", format_age(age))
                    }
                    None => "ОФФЛАЙН · симуляция".to_string(),
                };
                block.push(vec![(text, Color::Rgb { r: 255, g: 110, b: 110 })]);
            }
        } else {
            block.push(vec![
                (clock.clone(), Color::Cyan),
                (
                    format!("  загрузка погоды {}", self.loading_state.current_char()),
                    dim,
                ),
            ]);
        }

        // Плоский текст блока: для тестов и совместимости
        self.cached_weather_info = block
            .first()
            .map(|line| line.iter().map(|(s, _)| s.as_str()).collect::<String>())
            .unwrap_or_default();
        self.cached_forecast_info = block
            .iter()
            .skip(1)
            .map(|line| line.iter().map(|(s, _)| s.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        self.hud_block = block;

        self.weather_info_needs_update = false;
    }

    /// Весь текст HUD-блока одной строкой (для тестов)
    pub fn hud_text(&self) -> String {
        self.hud_block
            .iter()
            .map(|line| line.iter().map(|(s, _)| s.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn should_show_sun(&self) -> bool {
        if !self.weather_conditions.sun.is_day {
            return false;
        }

        if let Some(ref weather) = self.current_weather {
            matches!(
                weather.condition,
                WeatherCondition::Clear | WeatherCondition::PartlyCloudy | WeatherCondition::Cloudy
            )
        } else {
            false
        }
    }

    pub fn should_show_fireflies(&self) -> bool {
        if self.weather_conditions.sun.is_day {
            return false;
        }

        if let Some(ref weather) = self.current_weather {
            let is_warm = weather.temperature > 15.0;
            let is_clear_night = matches!(
                weather.condition,
                WeatherCondition::Clear | WeatherCondition::PartlyCloudy
            );
            is_warm
                && is_clear_night
                && !self.weather_conditions.is_raining
                && !self.weather_conditions.is_thunderstorm
                && !self.weather_conditions.is_snowing
        } else {
            false
        }
    }
}

pub struct LoadingState {
    pub frame: usize,
    pub last_update: Instant,
    pub loading_chars: [char; 4],
}

impl LoadingState {
    pub fn new() -> Self {
        Self {
            frame: 0,
            last_update: Instant::now(),
            loading_chars: ['|', '/', '-', '\\'],
        }
    }

    pub fn should_update(&self) -> bool {
        self.last_update.elapsed() >= std::time::Duration::from_millis(100)
    }

    pub fn next_frame(&mut self) {
        self.frame = (self.frame + 1) % self.loading_chars.len();
        self.last_update = Instant::now();
    }

    pub fn current_char(&self) -> char {
        self.loading_chars[self.frame]
    }
}

impl Default for LoadingState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LocationDisplay;
    use crate::weather::types::{
        CelestialEvents, PrecipitationUnit, TemperatureUnit, WindSpeedUnit,
    };

    fn create_app_state(lat: f64, lon: f64) -> AppState {
        create_app_state_full(lat, lon, None, LocationDisplay::Coordinates)
    }

    fn create_app_state_full(
        lat: f64,
        lon: f64,
        city: Option<String>,
        display: LocationDisplay,
    ) -> AppState {
        let location = WeatherLocation {
            latitude: lat,
            longitude: lon,
            elevation: None,
        };
        let units = WeatherUnits {
            temperature: TemperatureUnit::Celsius,
            wind_speed: WindSpeedUnit::Kmh,
            precipitation: PrecipitationUnit::Mm,
        };
        let mut app = AppState::new(location, city, display, false, false, false, units);

        let weather = WeatherData {
            condition: WeatherCondition::Clear,
            temperature: 20.0,
            feels_like_temperature: 20.0,
            precipitation: 0.0,
            wind_speed: 10.0,
            wind_direction: 0.0,
            moon_phase: Some(0.5),
            timestamp: "2024-01-01T12:00:00Z".to_string(),
            attribution: "".to_string(),
            forecast: Vec::new(),
            sun: CelestialEvents::from_bool(true),
        };
        app.update_weather(weather);

        app
    }

    #[test]
    fn test_new_york_coordinates() {
        // New York: 40.7128°N, 74.0060°W (positive lat, negative lon)
        let mut app = create_app_state(40.7128, -74.0060);
        app.update_cached_info();

        println!("NYC: {}", app.cached_weather_info);
        assert!(app.cached_weather_info.contains("40.71°N"));
        assert!(app.cached_weather_info.contains("74.01°W"));
    }

    #[test]
    fn test_sydney_coordinates() {
        // Sydney: 33.8688°S, 151.2093°E (negative lat, positive lon)
        let mut app = create_app_state(-33.8688, 151.2093);
        app.update_cached_info();

        println!("Sydney: {}", app.cached_weather_info);
        assert!(app.cached_weather_info.contains("33.87°S"));
        assert!(app.cached_weather_info.contains("151.21°E"));
    }

    #[test]
    fn test_london_coordinates() {
        // London: 51.5074°N, 0.1278°W (positive lat, negative lon near 0)
        let mut app = create_app_state(51.5074, -0.1278);
        app.update_cached_info();

        println!("London: {}", app.cached_weather_info);
        assert!(app.cached_weather_info.contains("51.51°N"));
        assert!(app.cached_weather_info.contains("0.13°W"));
    }

    #[test]
    fn test_sao_paulo_coordinates() {
        // São Paulo: 23.5505°S, 46.6333°W (negative lat, negative lon)
        let mut app = create_app_state(-23.5505, -46.6333);
        app.update_cached_info();

        println!("São Paulo: {}", app.cached_weather_info);
        assert!(app.cached_weather_info.contains("23.55°S"));
        assert!(app.cached_weather_info.contains("46.63°W"));
    }

    #[test]
    fn test_tokyo_coordinates() {
        // Tokyo: 35.6762°N, 139.6503°E (positive lat, positive lon)
        let mut app = create_app_state(35.6762, 139.6503);
        app.update_cached_info();

        println!("Tokyo: {}", app.cached_weather_info);
        assert!(app.cached_weather_info.contains("35.68°N"));
        assert!(app.cached_weather_info.contains("139.65°E"));
    }

    #[test]
    fn test_equator_prime_meridian() {
        // Null Island: 0°, 0° (exactly at equator and prime meridian)
        let mut app = create_app_state(0.0, 0.0);
        app.update_cached_info();

        println!("Null Island: {}", app.cached_weather_info);
        assert!(app.cached_weather_info.contains("0.00°N"));
        assert!(app.cached_weather_info.contains("0.00°E"));
    }

    #[test]
    fn test_display_coordinates_mode() {
        let mut app = create_app_state_full(
            34.0754,
            -84.2941,
            Some("Alpharetta".to_string()),
            LocationDisplay::Coordinates,
        );
        app.update_cached_info();

        assert!(
            app.cached_weather_info
                .contains("34.08°N, 84.29°W")
        );
        assert!(!app.cached_weather_info.contains("Alpharetta"));
    }

    #[test]
    fn test_display_city_mode_with_city() {
        let mut app = create_app_state_full(
            34.0754,
            -84.2941,
            Some("Alpharetta".to_string()),
            LocationDisplay::City,
        );
        app.update_cached_info();

        assert!(app.cached_weather_info.contains("Alpharetta"));
        assert!(!app.cached_weather_info.contains("34.08°N"));
    }

    #[test]
    fn test_display_city_mode_without_city_falls_back() {
        let mut app = create_app_state_full(34.0754, -84.2941, None, LocationDisplay::City);
        app.update_cached_info();

        assert!(
            app.cached_weather_info
                .contains("34.08°N, 84.29°W")
        );
    }

    #[test]
    fn test_display_mixed_mode_with_city() {
        let mut app = create_app_state_full(
            34.0754,
            -84.2941,
            Some("Alpharetta".to_string()),
            LocationDisplay::Mixed,
        );
        app.update_cached_info();

        assert!(
            app.cached_weather_info
                .contains("Alpharetta (34.08°N, 84.29°W)")
        );
    }

    #[test]
    fn test_display_mixed_mode_without_city_falls_back() {
        let mut app = create_app_state_full(34.0754, -84.2941, None, LocationDisplay::Mixed);
        app.update_cached_info();

        assert!(
            app.cached_weather_info
                .contains("34.08°N, 84.29°W")
        );
        assert!(!app.cached_weather_info.contains("("));
    }
}
