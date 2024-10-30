use crate::model::{self, Action, Award, Map, Personality, Proposal, Request, Response, Score};
use reqwest::StatusCode;
use std::{cell::Cell, collections::BTreeMap, fmt, fs, time::Duration};
use tokio::time::Instant;

pub struct CustomerSubmission {
    pub months_to_pay_back_loan: usize,
    pub yearly_interest_rate: f64,
    pub awards: Box<[Option<&'static str>]>,
}

pub struct InputData {
    pub awards: BTreeMap<&'static str, Award>,
    pub personalities: BTreeMap<String, Personality>,
    pub map: Map,
}

impl InputData {
    pub fn load(map_name: &str) -> Self {
        let awards: &'static str = fs::read_to_string(format!("data/Awards.json"))
            .unwrap()
            .leak();
        let map: &'static str = fs::read_to_string(format!("data/Map-{map_name}.json"))
            .unwrap()
            .leak();
        let personalities: &'static str = fs::read_to_string(format!("data/personalities.json"))
            .unwrap()
            .leak();
        Self {
            awards: serde_json::from_str::<model::Awards>(awards)
                .unwrap()
                .awards
                .into_iter()
                .map(|(k, v)| (&*k.leak(), v))
                .collect(),
            personalities: serde_json::from_str::<model::Personalities>(personalities)
                .unwrap()
                .personalities
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v))
                .collect(),
            map: {
                let mut map = serde_json::from_str::<model::Map>(&map).unwrap();
                for customer in &mut map.customers {
                    customer.personality = customer.personality.to_lowercase();
                }
                map
            },
        }
    }
}

impl Request {
    fn create_of_per_customer(
        indata: &InputData,
        submission: &[(&'static str, CustomerSubmission)],
    ) -> Self {
        Self {
            map_name: indata.map.name,
            proposals: submission
                .iter()
                .map(|(customer_name, sub)| Proposal {
                    customer_name,
                    months_to_pay_back_loan: sub.months_to_pay_back_loan,
                    yearly_interest_rate: sub.yearly_interest_rate,
                })
                .collect(),
            iterations: (0..indata.map.game_length_in_months)
                .map(|i| {
                    submission
                        .iter()
                        .map(|(customer_name, sub)| {
                            (
                                *customer_name,
                                match sub.awards.get(i).copied().flatten() {
                                    None => Action {
                                        type_: "Skip",
                                        award: "None",
                                    },
                                    Some(award) => Action {
                                        type_: "Award",
                                        award,
                                    },
                                },
                            )
                        })
                        .collect()
                })
                .collect(),
        }
    }
}

pub struct Api {
    api_key: &'static str,
    earliest_next_call: Cell<Instant>,
    num_calls: Cell<usize>,
    client: reqwest::Client,
}
impl Api {
    const API_DELAY: Duration = Duration::from_millis(100);
    const ENDPOINT: &str = "https://api.considition.com/game";

    pub fn new() -> Self {
        let api_key = fs::read_to_string(".api-key")
            .expect("API KEY in `./.api-key`")
            .leak()
            .trim();
        tracing::info!(api_key);
        Self {
            api_key,
            earliest_next_call: Cell::new(Instant::now()),
            num_calls: Cell::new(0),
            client: reqwest::Client::new(),
        }
    }
    pub fn num_calls(&self) -> usize {
        self.num_calls.get()
    }
    fn acquire_slot(&self) -> Instant {
        let old_slot = self.earliest_next_call.get();
        let next_slot = (old_slot + Self::API_DELAY).max(Instant::now());
        self.earliest_next_call.set(next_slot);
        next_slot
    }
    async fn call(&self, request: &Request) -> Response {
        tokio::time::sleep_until(self.acquire_slot()).await;

        self.num_calls.set(self.num_calls.get() + 1);

        let resp = loop {
            let response = self
                .client
                .post(Self::ENDPOINT)
                .json(request)
                .header("x-api-key", self.api_key)
                .send()
                .await
                .unwrap();
            match response.status() {
                StatusCode::TOO_MANY_REQUESTS => {
                    eprint!("429");
                    tokio::time::sleep_until(self.acquire_slot()).await;
                    continue;
                }
                _ => {
                    eprint!(".");
                    break response
                        .error_for_status()
                        .unwrap()
                        .json::<Response>()
                        .await
                        .unwrap();
                }
            }
        };
        tracing::trace!(?resp);
        resp
    }
    pub async fn evaluate(
        &self,
        indata: &InputData,
        submission: &[(&'static str, CustomerSubmission)],
    ) -> crate::model::Score {
        let request = Request::create_of_per_customer(indata, submission);
        let response = self.call(&request).await;
        response.score
    }
}

impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Score {
            map_name,
            environmental_impact,
            happiness_score,
            total_profit,
            total_score,
        } = self;
        write!(
            f,
            "{}: env={:.2} hap={:.2} pro={:.2} tot={:.2}",
            map_name, environmental_impact, happiness_score, total_profit, total_score
        )
    }
}
