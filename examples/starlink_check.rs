//! Проверка поиска поезда Starlink: пропагирует реальный TLE-файл
//! и печатает, что сейчас над точкой. Запуск:
//! cargo run --release --example starlink_check <путь-к-tle> [lat] [lon]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("путь к TLE-файлу");
    let lat: f64 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(55.7558);
    let lon: f64 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(37.6173);
    let tle = std::fs::read_to_string(path).expect("читаем TLE");
    let t0 = std::time::Instant::now();
    let (overhead, train) = weathr::starlink::debug_overhead(&tle, lat, lon);
    println!(
        "спутников в зоне: {}, максимальный поезд: {}, расчёт занял {:?}",
        overhead,
        train,
        t0.elapsed()
    );
}
