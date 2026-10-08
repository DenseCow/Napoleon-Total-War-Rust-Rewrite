//! The campaign-battle weather pick (BATTLE_FIDELITY.md §52 (3)).

use crate::BattleClimateWeather;

/// The weather a campaign battle gets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimateWeatherPick<'a> {
    /// The picked `battle_climate_weather_descriptions` row (its `weather` is the battle's weather key).
    pub row: &'a BattleClimateWeather,
    /// `rand ≤ flag_chance × 0.01` (picker output `+4`; meaning UNKNOWN, INFERRED: precipitation falls).
    pub flag: bool,
}

/// The campaign-battle weather pick `0x00F5B4D0` (CONFIRMED static): four filters in turn (`0x00F5B2A0`;
/// a filter that keeps nothing falls back): weight > 0 (else all rows); the climate (else all); the season
/// (else the `season_summer` rows); the desired weather if any (else unchanged). Then a draw by weight
/// with the setup's LCG: `r = rand16 / 65535 × Σ weight`, the row whose span `[before, after)` holds `r`,
/// and a second draw for the flag. `unit_float` is that LCG's `rand16 / 65535` (`CaRng::unit_float`).
/// No row picked (no rows, or `r` at the very end) → the table's first row with the flag off. Not wired
/// yet: campaign battles do not start from the campaign map.
pub fn pick_climate_weather<'a>(
    rows: &'a [BattleClimateWeather],
    climate: &str,
    season: &str,
    desired: Option<&str>,
    mut unit_float: impl FnMut() -> f32,
) -> Option<ClimateWeatherPick<'a>> {
    fn filter(
        list: Vec<&BattleClimateWeather>,
        keep: impl Fn(&BattleClimateWeather) -> bool,
        fallback: impl Fn(&BattleClimateWeather) -> bool,
    ) -> Vec<&BattleClimateWeather> {
        let (hit, miss): (Vec<_>, Vec<_>) = list.into_iter().partition(|r| keep(r));
        if hit.is_empty() { miss.into_iter().filter(|r| fallback(r)).collect() } else { hit }
    }
    let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let mut list: Vec<_> = rows.iter().collect();
    list = filter(list, |r| r.weight > 0, |_| true);
    list = filter(list, |r| eq(&r.climate, climate), |_| true);
    list = filter(list, |r| eq(&r.season, season), |r| eq(&r.season, "season_summer"));
    if let Some(w) = desired {
        list = filter(list, |r| eq(&r.weather, w), |_| true);
    }
    let total: f32 = list.iter().map(|r| r.weight as f32).sum();
    let r = unit_float() * total;
    let mut acc = 0.0f32;
    for row in list {
        let before = acc;
        acc += row.weight as f32;
        if before <= r && r < acc {
            let flag = unit_float() <= row.flag_chance as f32 * 0.01;
            return Some(ClimateWeatherPick { row, flag });
        }
    }
    rows.first().map(|row| ClimateWeatherPick { row, flag: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(climate: &str, season: &str, weather: &str, weight: i32, chance: i32) -> BattleClimateWeather {
        BattleClimateWeather {
            key: format!("{climate}_{weather}_{season}"),
            climate: climate.into(),
            season: season.into(),
            weather: weather.into(),
            weight,
            flag_chance: chance,
            unknown_38: 0,
            heat: 0,
            cold: 0,
            unknown_44: 0,
            unknown_48: 0,
        }
    }

    #[test]
    fn filters_fall_back_and_draw_by_weight() {
        let rows = vec![
            row("temperate", "season_summer", "dry", 80, 0),
            row("temperate", "season_summer", "heavy_rain", 20, 50),
            row("temperate", "season_winter", "light_snow", 0, 0),
            row("desert", "season_summer", "dry", 100, 0),
        ];
        // r = 0.9 × 100 = 90 → the rain row [80, 100); flag draw 0.4 ≤ 0.5.
        let mut d = [0.9f32, 0.4].into_iter();
        let p = pick_climate_weather(&rows, "temperate", "season_summer", None, || d.next().unwrap()).unwrap();
        assert_eq!((p.row.weather.as_str(), p.flag), ("heavy_rain", true));
        // Winter has no weighted row: falls back to the summer rows of the climate.
        let mut d = [0.1f32, 0.9].into_iter();
        let p = pick_climate_weather(&rows, "temperate", "season_winter", None, || d.next().unwrap()).unwrap();
        assert_eq!((p.row.weather.as_str(), p.flag), ("dry", false));
        // A desired weather narrows the draw; r at the very end picks nothing → the first row.
        let mut d = [1.0f32].into_iter();
        let p = pick_climate_weather(&rows, "temperate", "season_summer", Some("heavy_rain"), || d.next().unwrap()).unwrap();
        assert_eq!((p.row.key.as_str(), p.flag), ("temperate_dry_season_summer", false));
    }
}
