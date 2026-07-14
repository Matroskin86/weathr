//! Реальные самолёты над локацией: позиции из OpenSky Network,
//! модель/борт/маршрут из adsbdb.com. Оба API бесплатные и без ключей.
//!
//! Лимит анонимного OpenSky - около 100 запросов в сутки, поэтому опрос
//! редкий (по умолчанию раз в 20 минут) и результаты adsbdb кэшируются.

use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

const OPENSKY_URL: &str = "https://opensky-network.org/api/states/all";
const ADSBDB_URL: &str = "https://api.adsbdb.com/v0";

/// Готовый к показу борт: подпись для экрана и направление полёта
#[derive(Debug, Clone)]
pub struct FlightLabel {
    pub label: String,
    pub eastbound: bool,
}

#[derive(Debug, Deserialize)]
struct OpenSkyResponse {
    states: Option<Vec<Vec<serde_json::Value>>>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbAircraftResponse {
    response: Option<AdsbdbAircraftInner>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbAircraftInner {
    aircraft: Option<AdsbdbAircraft>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct AdsbdbAircraft {
    icao_type: Option<String>,
    registration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbCallsignResponse {
    response: Option<AdsbdbCallsignInner>,
}

#[derive(Debug, Deserialize)]
struct AdsbdbCallsignInner {
    flightroute: Option<AdsbdbFlightRoute>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct AdsbdbFlightRoute {
    callsign_iata: Option<String>,
    origin: Option<AdsbdbAirport>,
    destination: Option<AdsbdbAirport>,
}

#[derive(Debug, Deserialize, Clone)]
struct AdsbdbAirport {
    iata_code: Option<String>,
}

/// Кандидат из OpenSky: борт в воздухе с позывным
struct Candidate {
    icao24: String,
    callsign: String,
    eastbound: bool,
}

fn pick_candidate(states: &[Vec<serde_json::Value>]) -> Option<Candidate> {
    // Поля state-вектора OpenSky: 0 icao24, 1 callsign, 8 on_ground, 10 true_track
    states.iter().find_map(|s| {
        let icao24 = s.first()?.as_str()?.trim().to_string();
        let callsign = s.get(1)?.as_str()?.trim().to_string();
        let on_ground = s.get(8)?.as_bool()?;
        let track = s.get(10).and_then(|v| v.as_f64()).unwrap_or(90.0);

        if icao24.is_empty() || callsign.is_empty() || on_ground {
            return None;
        }

        // Курс 0-180 = восточная составляющая, летит по экрану слева направо
        Some(Candidate {
            icao24,
            callsign,
            eastbound: (0.0..180.0).contains(&track),
        })
    })
}

fn build_label(
    callsign: &str,
    aircraft: &AdsbdbAircraft,
    route: &AdsbdbFlightRoute,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    let model_reg: Vec<&str> = [
        aircraft.icao_type.as_deref(),
        aircraft.registration.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !model_reg.is_empty() {
        parts.push(model_reg.join(" "));
    }

    let flight_number = route
        .callsign_iata
        .clone()
        .unwrap_or_else(|| callsign.to_string());
    let leg = match (&route.origin, &route.destination) {
        (Some(origin), Some(dest)) => match (&origin.iata_code, &dest.iata_code) {
            (Some(from), Some(to)) => format!("{} {}-{}", flight_number, from, to),
            _ => flight_number,
        },
        _ => flight_number,
    };
    parts.push(leg);

    parts.join(" | ")
}

async fn fetch_flight(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
    radius: f64,
    aircraft_cache: &mut HashMap<String, AdsbdbAircraft>,
    route_cache: &mut HashMap<String, AdsbdbFlightRoute>,
) -> Option<FlightLabel> {
    let url = format!(
        "{}?lamin={}&lomin={}&lamax={}&lomax={}",
        OPENSKY_URL,
        lat - radius,
        lon - radius * 1.6,
        lat + radius,
        lon + radius * 1.6,
    );

    let data: OpenSkyResponse = client.get(&url).send().await.ok()?.json().await.ok()?;
    let candidate = pick_candidate(&data.states?)?;

    let aircraft = match aircraft_cache.get(&candidate.icao24) {
        Some(cached) => cached.clone(),
        None => {
            let url = format!("{}/aircraft/{}", ADSBDB_URL, candidate.icao24);
            let info = match client.get(&url).send().await {
                Ok(resp) => resp
                    .json::<AdsbdbAircraftResponse>()
                    .await
                    .ok()
                    .and_then(|r| r.response)
                    .and_then(|r| r.aircraft)
                    .unwrap_or_default(),
                Err(_) => AdsbdbAircraft::default(),
            };
            aircraft_cache.insert(candidate.icao24.clone(), info.clone());
            info
        }
    };

    let route = match route_cache.get(&candidate.callsign) {
        Some(cached) => cached.clone(),
        None => {
            let url = format!("{}/callsign/{}", ADSBDB_URL, candidate.callsign);
            let info = match client.get(&url).send().await {
                Ok(resp) => resp
                    .json::<AdsbdbCallsignResponse>()
                    .await
                    .ok()
                    .and_then(|r| r.response)
                    .and_then(|r| r.flightroute)
                    .unwrap_or_default(),
                Err(_) => AdsbdbFlightRoute::default(),
            };
            route_cache.insert(candidate.callsign.clone(), info.clone());
            info
        }
    };

    Some(FlightLabel {
        label: build_label(&candidate.callsign, &aircraft, &route),
        eastbound: candidate.eastbound,
    })
}

/// Фоновый опрос неба: раз в poll_secs шлёт в канал реальный борт из радиуса
pub fn spawn_flight_watcher(
    lat: f64,
    lon: f64,
    radius_deg: f64,
    poll_secs: u64,
) -> mpsc::Receiver<FlightLabel> {
    let (tx, rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("weathr-screensaver")
            .build()
            .unwrap_or_default();

        let mut aircraft_cache: HashMap<String, AdsbdbAircraft> = HashMap::new();
        let mut route_cache: HashMap<String, AdsbdbFlightRoute> = HashMap::new();

        // Первый борт через полминуты после старта, дальше по расписанию
        tokio::time::sleep(Duration::from_secs(30)).await;

        loop {
            if let Some(flight) = fetch_flight(
                &client,
                lat,
                lon,
                radius_deg,
                &mut aircraft_cache,
                &mut route_cache,
            )
            .await
                && tx.send(flight).await.is_err()
            {
                break;
            }
            tokio::time::sleep(Duration::from_secs(poll_secs.max(300))).await;
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(icao: &str, callsign: &str, on_ground: bool, track: f64) -> Vec<serde_json::Value> {
        let mut s = vec![serde_json::Value::Null; 12];
        s[0] = serde_json::json!(icao);
        s[1] = serde_json::json!(callsign);
        s[8] = serde_json::json!(on_ground);
        s[10] = serde_json::json!(track);
        s
    }

    #[test]
    fn test_pick_candidate_skips_ground_and_empty() {
        let states = vec![
            state("aaa", "", false, 90.0),
            state("bbb", "AFL123", true, 90.0),
            state("ccc", "AFL456", false, 275.0),
        ];
        let c = pick_candidate(&states).unwrap();
        assert_eq!(c.icao24, "ccc");
        assert_eq!(c.callsign, "AFL456");
        assert!(!c.eastbound);
    }

    #[test]
    fn test_build_label_full() {
        let aircraft = AdsbdbAircraft {
            icao_type: Some("A320".to_string()),
            registration: Some("RA-73756".to_string()),
        };
        let route = AdsbdbFlightRoute {
            callsign_iata: Some("SU1234".to_string()),
            origin: Some(AdsbdbAirport {
                iata_code: Some("SVO".to_string()),
            }),
            destination: Some(AdsbdbAirport {
                iata_code: Some("LED".to_string()),
            }),
        };
        assert_eq!(
            build_label("AFL1234", &aircraft, &route),
            "A320 RA-73756 | SU1234 SVO-LED"
        );
    }

    #[test]
    fn test_build_label_no_route_falls_back_to_callsign() {
        let aircraft = AdsbdbAircraft::default();
        let route = AdsbdbFlightRoute::default();
        assert_eq!(build_label("DRU544", &aircraft, &route), "DRU544");
    }
}
