//! Расчёт текущей фазы луны без внешних API.
//! 0.0 = новолуние, 0.5 = полнолуние, по синодическому месяцу
//! от эталонного новолуния 6 января 2000, 18:14 UTC.

const SYNODIC_MONTH_DAYS: f64 = 29.530588853;
const REFERENCE_NEW_MOON_UNIX: i64 = 947_182_440;

pub fn current_moon_phase() -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(REFERENCE_NEW_MOON_UNIX);
    moon_phase_at(now)
}

pub fn moon_phase_at(unix_secs: i64) -> f64 {
    let days = (unix_secs - REFERENCE_NEW_MOON_UNIX) as f64 / 86_400.0;
    let phase = (days % SYNODIC_MONTH_DAYS + SYNODIC_MONTH_DAYS) % SYNODIC_MONTH_DAYS;
    phase / SYNODIC_MONTH_DAYS
}

/// Полнолуние с допуском примерно в сутки
pub fn is_full_moon(phase: f64) -> bool {
    (phase - 0.5).abs() < 0.017
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_new_moon_is_zero() {
        assert!(moon_phase_at(REFERENCE_NEW_MOON_UNIX) < 0.001);
    }

    #[test]
    fn test_full_moon_half_cycle_later() {
        let half_cycle = REFERENCE_NEW_MOON_UNIX + (SYNODIC_MONTH_DAYS * 43_200.0) as i64;
        let phase = moon_phase_at(half_cycle);
        assert!(is_full_moon(phase), "phase = {}", phase);
    }

    #[test]
    fn test_known_full_moon_2026() {
        // Полнолуние 3 января 2026 около 10:03 UTC
        let jan_3_2026 = 1_767_434_580;
        let phase = moon_phase_at(jan_3_2026);
        assert!(is_full_moon(phase), "phase = {}", phase);
    }
}
