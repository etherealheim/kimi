use color_eyre::Result;
use reqwest::blocking::Client;
use serde::Deserialize;

#[allow(dead_code)]
const OPEN_METEO_URL: &str = "https://api.open-meteo.com/v1/forecast";
#[allow(dead_code)]
const PRAGUE_LAT: f32 = 50.0755;
#[allow(dead_code)]
const PRAGUE_LON: f32 = 14.4378;

#[allow(dead_code)]
pub struct WeatherService {
    client: Client,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WeatherResponse {
    current_weather: CurrentWeather,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CurrentWeather {
    temperature: f32,
    windspeed: f32,
    weathercode: i32,
    time: String,
}

#[allow(dead_code)]
impl WeatherService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub fn fetch_current_weather_json(&self) -> Result<String> {
        let url = format!(
            "{OPEN_METEO_URL}?latitude={PRAGUE_LAT}&longitude={PRAGUE_LON}&current_weather=true"
        );
        let response = self.client.get(url).send()?.error_for_status()?;
        let payload: WeatherResponse = response.json()?;

        let summary = serde_json::json!({
            "location": "Prague",
            "time": payload.current_weather.time,
            "temperature_c": payload.current_weather.temperature,
            "wind_kph": payload.current_weather.windspeed,
            "weather_code": payload.current_weather.weathercode
        });

        Ok(summary.to_string())
    }

    pub fn weather_system_prompt(&self) -> Result<String> {
        let weather_json = self.fetch_current_weather_json()?;
        Ok(format!("Current weather (Prague): {}", weather_json))
    }
}
