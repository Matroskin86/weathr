//! Поезд Starlink: реальные позиции спутников из TLE Celestrak,
//! пропагация SGP4 локально. Поезд - это плотная группа свежезапущенных
//! спутников (соседние NORAD-номера, летят цепочкой); когда такая группа
//! проходит над локацией - по небу ползёт вереница огоньков.

use std::time::Duration;
use tokio::sync::mpsc;

const TLE_URL: &str = "https://celestrak.org/NORAD/elements/gp.php?GROUP=starlink&FORMAT=tle";
// TLE свежее суток - достаточно для попадания в зону видимости
const TLE_MAX_AGE_SECS: u64 = 86_400;

/// Поезд над локацией: сколько спутников в цепочке
#[derive(Debug, Clone)]
pub struct StarlinkTrain {
    pub count: usize,
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Среднее звёздное время Гринвича в градусах (достаточно грубой формулы)
fn gmst_degrees(unix_secs: f64) -> f64 {
    let jd = unix_secs / 86_400.0 + 2_440_587.5;
    let d = jd - 2_451_545.0;
    (280.460_618_37 + 360.985_647_366_29 * d).rem_euclid(360.0)
}

/// TEME-координаты (км) в геоцентрические широту/долготу (градусы)
fn teme_to_lat_lon(position: &[f64; 3], unix_secs: f64) -> (f64, f64) {
    let (x, y, z) = (position[0], position[1], position[2]);
    let lat = z.atan2((x * x + y * y).sqrt()).to_degrees();
    let lon_inertial = y.atan2(x).to_degrees();
    let lon = (lon_inertial - gmst_degrees(unix_secs) + 540.0).rem_euclid(360.0) - 180.0;
    (lat, lon)
}

fn cache_path() -> Option<std::path::PathBuf> {
    dirs::cache_dir().map(|d| d.join("weathr").join("starlink.tle"))
}

async fn load_tle(client: &reqwest::Client) -> Option<String> {
    // Свежий кэш - читаем с диска, не дёргая Celestrak лишний раз
    if let Some(path) = cache_path()
        && let Ok(meta) = tokio::fs::metadata(&path).await
        && let Ok(modified) = meta.modified()
        && modified
            .elapsed()
            .map(|age| age.as_secs() < TLE_MAX_AGE_SECS)
            .unwrap_or(false)
        && let Ok(text) = tokio::fs::read_to_string(&path).await
    {
        return Some(text);
    }

    let text = client
        .get(TLE_URL)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    if let Some(path) = cache_path() {
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&path, &text).await;
    }
    Some(text)
}

/// NORAD-номера спутников, находящихся сейчас в зоне над точкой
fn overhead_ids(tle_text: &str, lat: f64, lon: f64) -> Vec<u64> {
    let now = unix_now() as f64;

    let mut overhead: Vec<u64> = Vec::new();
    for elements in sgp4::parse_3les(tle_text).unwrap_or_default() {
        let Ok(constants) = sgp4::Constants::from_elements(&elements) else {
            continue;
        };
        let Ok(minutes) = elements.datetime_to_minutes_since_epoch(
            &chrono::DateTime::from_timestamp(now as i64, 0)
                .map(|dt| dt.naive_utc())
                .unwrap_or_default(),
        ) else {
            continue;
        };
        let Ok(prediction) = constants.propagate(minutes) else {
            continue;
        };
        let (sat_lat, sat_lon) = teme_to_lat_lon(&prediction.position, now);
        let dlon = (sat_lon - lon + 540.0).rem_euclid(360.0) - 180.0;
        if (sat_lat - lat).abs() < 4.0 && dlon.abs() < 6.0 {
            overhead.push(elements.norad_id);
        }
    }
    overhead.sort_unstable();
    overhead
}

/// Ищет поезд над точкой: группа подряд идущих NORAD-номеров = один запуск,
/// летящий цепочкой. Возвращает размер самой большой группы.
fn find_train(tle_text: &str, lat: f64, lon: f64) -> usize {
    let overhead = overhead_ids(tle_text, lat, lon);
    if overhead.is_empty() {
        return 0;
    }
    let mut best = 1usize;
    let mut run = 1usize;
    for pair in overhead.windows(2) {
        if pair[1] - pair[0] <= 40 {
            run += 1;
        } else {
            best = best.max(run);
            run = 1;
        }
    }
    best.max(run)
}

/// Отладка: сколько спутников в зоне и каков максимальный поезд
pub fn debug_overhead(tle_text: &str, lat: f64, lon: f64) -> (usize, usize) {
    (
        overhead_ids(tle_text, lat, lon).len(),
        find_train(tle_text, lat, lon),
    )
}

/// Фоновый вотчер: раз в poll_secs пропагирует созвездие и шлёт событие,
/// когда над локацией проходит поезд из 4 и более спутников
pub fn spawn_starlink_watcher(lat: f64, lon: f64, poll_secs: u64) -> mpsc::Receiver<StarlinkTrain> {
    let (tx, rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("weathr-screensaver")
            .build()
            .unwrap_or_default();

        tokio::time::sleep(Duration::from_secs(45)).await;

        loop {
            if let Some(tle) = load_tle(&client).await {
                // Пропагация ~10 тысяч спутников - тяжёлая для async-потока,
                // уводим в блокирующий пул
                let count = tokio::task::spawn_blocking(move || find_train(&tle, lat, lon))
                    .await
                    .unwrap_or(0);
                if count >= 4 {
                    if tx.send(StarlinkTrain { count }).await.is_err() {
                        break;
                    }
                    // Поезд показан - до следующего часа не дёргаемся
                    tokio::time::sleep(Duration::from_secs(3_600)).await;
                    continue;
                }
            }
            tokio::time::sleep(Duration::from_secs(poll_secs.max(300))).await;
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gmst_reasonable() {
        // GMST всегда в [0, 360)
        let g = gmst_degrees(1_784_000_000.0);
        assert!((0.0..360.0).contains(&g));
    }

    #[test]
    fn test_teme_conversion_equator() {
        // Точка на экваторе в плоскости XY даёт нулевую широту
        let (lat, _lon) = teme_to_lat_lon(&[7000.0, 0.0, 0.0], 1_784_000_000.0);
        assert!(lat.abs() < 0.001);
    }
}
