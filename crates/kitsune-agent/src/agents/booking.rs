use crate::ai_client::{AgentAiClient, ModelTier};
use crate::error::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightOffer {
    pub price_minor: i64,
    pub currency: String,
    pub duration_mins: u32,
    pub stops: u8,
    pub airline: String,
    pub booking_url: String,
}

impl FlightOffer {
    pub fn price_display(&self) -> String {
        format!("{:.2} {}", self.price_minor as f64 / 100.0, self.currency)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BookingPriority {
    Cheapest,
    Fastest,
    Earliest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingCriteria {
    pub primary: BookingPriority,
    pub max_stops: Option<u8>,
    pub max_price_minor: Option<i64>,
}

impl BookingCriteria {
    pub fn rank<'a>(&self, offers: &'a [FlightOffer]) -> Option<&'a FlightOffer> {
        let filtered: Vec<&FlightOffer> = offers
            .iter()
            .filter(|o| {
                self.max_stops.map_or(true, |m| o.stops <= m)
                    && self.max_price_minor.map_or(true, |m| o.price_minor <= m)
            })
            .collect();
        filtered.into_iter().min_by(|a, b| match self.primary {
            BookingPriority::Cheapest => a.price_minor.cmp(&b.price_minor),
            BookingPriority::Fastest => a.duration_mins.cmp(&b.duration_mins),
            BookingPriority::Earliest => a.stops.cmp(&b.stops),
        })
    }
}

pub struct BookingAgent {
    ai: Arc<AgentAiClient>,
    http: reqwest::Client,
}

impl BookingAgent {
    pub fn new(ai: Arc<AgentAiClient>) -> AgentResult<Self> {
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AgentError::Internal(e.to_string()))?;
        Ok(Self { ai, http })
    }

    pub async fn fetch_offers(
        &self,
        origin: &str,
        destination: &str,
        date: &str,
    ) -> AgentResult<Vec<FlightOffer>> {
        let url = format!(
            "https://www.google.com/travel/flights?q=Flights+from+{}+to+{}+on+{}",
            urlencoding::encode(origin),
            urlencoding::encode(destination),
            urlencoding::encode(date)
        );

        let html = match self
            .http
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .send()
            .await
        {
            Ok(r) => r.text().await.unwrap_or_default(),
            Err(e) => {
                tracing::warn!("Flights fetch failed: {e}");
                return Ok(vec![]);
            }
        };

        let prompt = format!(
            r#"Extract flight offers from this HTML. Return ONLY valid JSON (no markdown):
[{{"price_minor":12345,"currency":"EUR","duration_mins":120,"stops":0,"airline":"LH","booking_url":"https://..."}}]

HTML (first 3000 chars):
{}

If no flights found, return: []"#,
            &html[..html.len().min(3000)]
        );

        let response = self.ai.complete(&prompt, ModelTier::Fast).await?;
        let json_str = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();
        let offers: Vec<FlightOffer> = serde_json::from_str(json_str).unwrap_or_default();
        info!(count = offers.len(), %origin, %destination, %date, "BookingAgent fetched offers");
        Ok(offers)
    }
}
