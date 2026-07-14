use crate::config::LocationDisplay;
use crate::weather::{
    WeatherCondition, WeatherConditions, WeatherData, WeatherLocation, WeatherUnits,
    format_precipitation, format_temperature, format_wind_speed,
};
use std::time::Instant;

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
            format!(" | Место: {}", label)
        };

        // "Ощущается как": добавка к температуре, если включено в конфиге
        let apparent_temp_str = if let Some(ref weather) = self.current_weather {
            if self.use_feels_like_temperature {
                let (apparent_temp, apparent_temp_unit) =
                    format_temperature(weather.feels_like_temperature, self.units.temperature);
                format!(" (ощущается {:.1}{})", apparent_temp, apparent_temp_unit)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let clock = chrono::Local::now().format("%H:%M");
        let quit_hint = if self.hide_quit_hint {
            ""
        } else {
            " | Выход: 'q'"
        };

        self.cached_weather_info = if let Some(ref weather) = self.current_weather {
            let (temp, temp_unit) = format_temperature(weather.temperature, self.units.temperature);
            let (wind, wind_unit) = format_wind_speed(weather.wind_speed, self.units.wind_speed);
            let (precip, precip_unit) =
                format_precipitation(weather.precipitation, self.units.precipitation);

            let offline_indicator = if self.is_offline {
                match self.offline_data_cached_at {
                    Some(cached_at) => {
                        let age = unix_now().saturating_sub(cached_at);
                        format!("ОФФЛАЙН (данные {}) | ", format_age(age))
                    }
                    None => "ОФФЛАЙН (симуляция) | ".to_string(),
                }
            } else {
                String::new()
            };

            format!(
                "{} | {}Погода: {} | Темп: {:.1}{}{} | Ветер: {:.1}{} | Осадки: {:.1}{}{}{}",
                clock,
                offline_indicator,
                self.get_condition_text(),
                temp,
                temp_unit,
                apparent_temp_str,
                wind,
                wind_unit,
                precip,
                precip_unit,
                location_str,
                quit_hint
            )
        } else {
            format!(
                "{} | Погода: загрузка... {}",
                clock,
                self.loading_state.current_char()
            )
        };

        // Вторая строка HUD: прогноз на +3/+6/+12 часов
        self.cached_forecast_info = if let Some(ref weather) = self.current_weather {
            if weather.forecast.is_empty() {
                String::new()
            } else {
                let parts: Vec<String> = weather
                    .forecast
                    .iter()
                    .map(|point| {
                        let (temp, unit) =
                            format_temperature(point.temperature, self.units.temperature);
                        let prob = match point.precipitation_probability {
                            Some(p) if p >= 20 => format!(", осадки {}%", p),
                            _ => String::new(),
                        };
                        format!(
                            "+{}ч {:.0}{} {}{}",
                            point.hours_ahead,
                            temp,
                            unit,
                            condition_name_ru(point.condition),
                            prob
                        )
                    })
                    .collect();
                format!("Прогноз: {}", parts.join(" | "))
            }
        } else {
            String::new()
        };

        self.weather_info_needs_update = false;
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
                .contains("Место: 34.08°N, 84.29°W")
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

        assert!(app.cached_weather_info.contains("Место: Alpharetta"));
        assert!(!app.cached_weather_info.contains("34.08°N"));
    }

    #[test]
    fn test_display_city_mode_without_city_falls_back() {
        let mut app = create_app_state_full(34.0754, -84.2941, None, LocationDisplay::City);
        app.update_cached_info();

        assert!(
            app.cached_weather_info
                .contains("Место: 34.08°N, 84.29°W")
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
                .contains("Место: Alpharetta (34.08°N, 84.29°W)")
        );
    }

    #[test]
    fn test_display_mixed_mode_without_city_falls_back() {
        let mut app = create_app_state_full(34.0754, -84.2941, None, LocationDisplay::Mixed);
        app.update_cached_info();

        assert!(
            app.cached_weather_info
                .contains("Место: 34.08°N, 84.29°W")
        );
        assert!(!app.cached_weather_info.contains("("));
    }
}
