mod model;

use model::{Action, Map, Proposal, Request, Response};
use std::{cell::Cell, fs, time::Duration};
use tokio::time::Instant;

fn main() {
    let map_name = "Gothenburg";
    let map: Map =
        serde_json::from_str(&fs::read_to_string(format!("data/Map-{map_name}.json")).unwrap())
            .unwrap();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let api = Api::new();
        let request = Request {
            map_name: map_name.to_string(),
            proposals: map
                .customers
                .iter()
                .map(|customer| Proposal {
                    customer_name: customer.name.to_string(),
                    months_to_pay_back_loan: map.game_length_in_months,
                    yearly_interest_rate: 0.05,
                })
                .collect(),
            iterations: vec![
                map.customers
                    .iter()
                    .map(|customer| (
                        customer.name.clone(),
                        Action {
                            type_: "Skip".to_string(),
                            award: "None".to_string(),
                        }
                    ))
                    .collect();
                map.game_length_in_months
            ],
        };
        api.call(&request).await
    });
}

impl Request {
    fn validate(&self) {
        todo!()
    }
}

struct Api {
    api_key: &'static str,
    earliest_next_call: Cell<Instant>,
    client: reqwest::Client,
}
impl Api {
    const API_DELAY: Duration = Duration::from_millis(100);
    const ENDPOINT: &str = "https://api.considition.com/game";

    fn new() -> Self {
        let api_key = fs::read_to_string(".api-key")
            .expect("API KEY in `./.api-key`")
            .leak()
            .trim();
        dbg!(api_key);
        Self {
            api_key,
            earliest_next_call: Cell::new(Instant::now()),
            client: reqwest::Client::new(),
        }
    }
    async fn call(&self, request: &Request) {
        tokio::time::sleep_until(self.earliest_next_call.get()).await;
        self.earliest_next_call
            .set(Instant::now() + Self::API_DELAY);
        let resp = self
            .client
            .post(Self::ENDPOINT)
            .json(request)
            .header("x-api-key", self.api_key)
            .send()
            .await
            .unwrap();
        dbg!(&resp);
        let resp_body = resp.json::<Response>().await.unwrap();
        dbg!(&resp_body);
    }
}
