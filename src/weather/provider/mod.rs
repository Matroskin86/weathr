use crate::error::WeatherError;
use crate::weather::types::{CelestialEvents, WeatherLocation, WeatherUnits};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod met_office;
pub mod open_meteo;
pub mod supplementary;

/// Сырая точка прогноза от провайдера (код погоды ещё не превращён в условие)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawForecastPoint {
    pub hours_ahead: u8,
    pub temperature: f64,
    pub weather_code: i32,
    pub precipitation_probability: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherProviderResponse {
    pub weather_code: i32,
    pub temperature: f64,
    pub feels_like_temperature: f64,
    pub precipitation: f64,
    pub wind_speed: f64,
    pub wind_direction: f64,
    pub sun: CelestialEvents,
    pub moon_phase: Option<f64>,
    pub timestamp: String,
    pub attribution: String,
    #[serde(default)]
    pub forecast: Vec<RawForecastPoint>,
}

#[async_trait]
pub trait WeatherProvider: Send + Sync {
    async fn get_current_weather(
        &self,
        location: &WeatherLocation,
        units: &WeatherUnits,
    ) -> Result<WeatherProviderResponse, WeatherError>;

    fn get_attribution(&self) -> &'static str;
}
