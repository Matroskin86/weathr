use crate::error::{NetworkError, WeatherError};
use crate::weather::provider::{WeatherProvider, WeatherProviderResponse};
use crate::weather::types::{
    CelestialEvents, PrecipitationUnit, TemperatureUnit, WeatherLocation, WeatherUnits,
    WindSpeedUnit,
};
use crate::weather::units::{normalize_precipitation, normalize_temperature, normalize_wind_speed};
use async_trait::async_trait;
use serde::Deserialize;
use serde::de::{self, Deserializer};
use std::time::Duration;

const OPEN_METEO_BASE_URL: &str = "https://api.open-meteo.com/v1/forecast";

pub struct OpenMeteoProvider {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    current: CurrentWeather,
    hourly: Option<HourlyForecast>,
    daily: Option<DailySun>,
}

/// Восход/закат на сегодня (первый элемент массивов, локальное время из timezone=auto)
#[derive(Debug, Deserialize)]
struct DailySun {
    sunrise: Vec<String>,
    sunset: Vec<String>,
}

/// Почасовые ряды open-meteo: первый элемент = текущий час (forecast_hours=13)
#[derive(Debug, Deserialize)]
struct HourlyForecast {
    temperature_2m: Vec<f64>,
    weather_code: Vec<f64>,
    precipitation_probability: Option<Vec<Option<i32>>>,
}

#[derive(Debug, Deserialize)]
struct CurrentWeather {
    time: String,
    temperature_2m: f64,
    apparent_temperature: f64,
    #[serde(deserialize_with = "deserialize_i32_from_number")]
    is_day: i32,
    precipitation: f64,
    #[serde(deserialize_with = "deserialize_i32_from_number")]
    weather_code: i32,
    wind_speed_10m: f64,
    wind_direction_10m: f64,
}

fn deserialize_i32_from_number<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Integer(i32),
        Float(f64),
    }

    match Number::deserialize(deserializer)? {
        Number::Integer(value) => Ok(value),
        Number::Float(value) => {
            if !value.is_finite() {
                return Err(de::Error::custom("expected a finite numeric value"));
            }
            Ok(value.round() as i32)
        }
    }
}

impl OpenMeteoProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|e| {
                eprintln!("Warning: Failed to create custom HTTP client: {}", e);
                eprintln!("Using default client with standard timeout settings.");
                reqwest::Client::new()
            });

        Self {
            client,
            base_url: OPEN_METEO_BASE_URL.to_string(),
        }
    }

    fn temperature_unit_param(unit: &TemperatureUnit) -> &'static str {
        match unit {
            TemperatureUnit::Celsius => "celsius",
            TemperatureUnit::Fahrenheit => "fahrenheit",
        }
    }

    fn wind_speed_unit_param(unit: &WindSpeedUnit) -> &'static str {
        match unit {
            WindSpeedUnit::Kmh => "kmh",
            WindSpeedUnit::Ms => "ms",
            WindSpeedUnit::Mph => "mph",
            WindSpeedUnit::Kn => "kn",
        }
    }

    fn precipitation_unit_param(unit: &PrecipitationUnit) -> &'static str {
        match unit {
            PrecipitationUnit::Mm => "mm",
            PrecipitationUnit::Inch => "inch",
        }
    }

    fn build_url(&self, location: &WeatherLocation, units: &WeatherUnits) -> String {
        format!(
            "{}?latitude={}&longitude={}&current=temperature_2m,apparent_temperature,is_day,precipitation,weather_code,wind_speed_10m,wind_direction_10m&hourly=temperature_2m,weather_code,precipitation_probability&forecast_hours=13&daily=sunrise,sunset&forecast_days=1&temperature_unit={}&wind_speed_unit={}&precipitation_unit={}&timezone=auto",
            self.base_url,
            location.latitude,
            location.longitude,
            Self::temperature_unit_param(&units.temperature),
            Self::wind_speed_unit_param(&units.wind_speed),
            Self::precipitation_unit_param(&units.precipitation)
        )
    }
}

impl Default for OpenMeteoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WeatherProvider for OpenMeteoProvider {
    fn get_attribution(&self) -> &'static str {
        ""
    }

    async fn get_current_weather(
        &self,
        location: &WeatherLocation,
        units: &WeatherUnits,
    ) -> Result<WeatherProviderResponse, WeatherError> {
        let url = self.build_url(location, units);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .and_then(|resp| resp.error_for_status())
            .map_err(|e| WeatherError::Network(NetworkError::from_reqwest(e, &url, 30)))?;

        let data: OpenMeteoResponse = response
            .json()
            .await
            .map_err(|e| WeatherError::Network(NetworkError::from_reqwest(e, &url, 30)))?;

        let moon_phase = Some(crate::weather::moonphase::current_moon_phase());

        // Восход/закат: время после 'T' из ISO-строк daily (уже локальное, timezone=auto)
        let parse_sun_time = |raw: Option<&String>| -> Option<chrono::NaiveTime> {
            let time_part = raw?.split('T').nth(1)?;
            chrono::NaiveTime::parse_from_str(time_part, "%H:%M").ok()
        };
        let (sun_rise, sun_set) = match data.daily.as_ref() {
            Some(daily) => (
                parse_sun_time(daily.sunrise.first()),
                parse_sun_time(daily.sunset.first()),
            ),
            None => (None, None),
        };
        let mut sun = CelestialEvents::only_day(data.current.is_day);
        sun.rise = sun_rise;
        sun.set = sun_set;
        // Дуге солнца нужны границы и зенит: рассвет/закат и середина дня
        sun.begin_twilight = sun_rise;
        sun.end_twilight = sun_set;
        if let (Some(rise), Some(set)) = (sun_rise, sun_set) {
            let day_secs = (set - rise).num_seconds().max(0);
            sun.upper_transit = rise.overflowing_add_signed(
                chrono::Duration::seconds(day_secs / 2),
            ).0.into();
        }

        // Точки прогноза +3/+6/+12 часов из почасовых рядов (индекс 0 = текущий час)
        let forecast = data
            .hourly
            .as_ref()
            .map(|hourly| {
                [3usize, 6, 12]
                    .iter()
                    .filter_map(|&idx| {
                        let temperature = *hourly.temperature_2m.get(idx)?;
                        let weather_code = *hourly.weather_code.get(idx)? as i32;
                        let precipitation_probability = hourly
                            .precipitation_probability
                            .as_ref()
                            .and_then(|probs| probs.get(idx).copied().flatten());
                        Some(crate::weather::provider::RawForecastPoint {
                            hours_ahead: idx as u8,
                            temperature: normalize_temperature(temperature, units.temperature),
                            weather_code,
                            precipitation_probability,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(WeatherProviderResponse {
            forecast,
            weather_code: data.current.weather_code,
            temperature: normalize_temperature(data.current.temperature_2m, units.temperature),
            feels_like_temperature: normalize_temperature(
                data.current.apparent_temperature,
                units.temperature,
            ),
            precipitation: normalize_precipitation(data.current.precipitation, units.precipitation),
            wind_speed: normalize_wind_speed(data.current.wind_speed_10m, units.wind_speed),
            wind_direction: data.current.wind_direction_10m,
            sun,
            moon_phase,
            timestamp: data.current.time,
            attribution: self.get_attribution().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_conversion_params() {
        assert_eq!(
            OpenMeteoProvider::temperature_unit_param(&TemperatureUnit::Celsius),
            "celsius"
        );
        assert_eq!(
            OpenMeteoProvider::temperature_unit_param(&TemperatureUnit::Fahrenheit),
            "fahrenheit"
        );
        assert_eq!(
            OpenMeteoProvider::wind_speed_unit_param(&WindSpeedUnit::Kmh),
            "kmh"
        );
        assert_eq!(
            OpenMeteoProvider::wind_speed_unit_param(&WindSpeedUnit::Ms),
            "ms"
        );
        assert_eq!(
            OpenMeteoProvider::precipitation_unit_param(&PrecipitationUnit::Mm),
            "mm"
        );
    }
}
