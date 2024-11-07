mod api;
mod model;
mod opt;
mod whitebox;

use api::{Api, CustomerSubmission, InputData};
use itertools::Itertools;
use model::Score;
use std::iter;
use tokio::time::Instant;

fn main() {
    use tracing_subscriber::Layer;
    tracing::subscriber::set_global_default(
        tracing_subscriber::filter::targets::Targets::new()
            .with_target("hyper_util", tracing::Level::INFO)
            .with_target("considition2024::api", tracing::Level::INFO)
            .with_default(tracing::Level::TRACE)
            .with_subscriber(
                tracing_subscriber::FmtSubscriber::builder()
                    .with_max_level(tracing::Level::TRACE)
                    .finish(),
            ),
    )
    .expect("enabling global logger");

    //let indata = InputData::load("Gothenburg");
    //let indata = InputData::load("Nottingham");
    let indata = InputData::load("Almhult");
    dbg!(&indata.awards);

    let api = Api::new();

    let start = Instant::now();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        run(&api, &indata).await;
    });
    tracing::info!(num_calls = ?api.num_calls(), elapsed = ?start.elapsed());
}

async fn run(api: &Api, indata: &InputData) {
    const WHICH: &str = "localopt";
    match WHICH {
        "sweep" => {
            let results = sweep(api, indata).await;
            println!();
            let mut best_tot_score = 0.0;
            for (rate, award, score, whitebox_score) in results {
                let record = if score.total_score >= best_tot_score {
                    best_tot_score = score.total_score;
                    " <=============== RECORD!"
                } else {
                    ""
                };
                println!("{score} @ rate={rate:.3} award={award:?}{record}");
                if score.environmental_impact != whitebox_score.environmental_impact
                    || (score.happiness_score - whitebox_score.happiness_score).abs() > 0.4
                    || (score.total_score - whitebox_score.total_score).abs() > 1e-5
                {
                    eprintln!("mismatch\n    real={score:?}\nwhitebox={whitebox_score:?}\n");
                }
            }
        }
        "localopt" => {
            let (expected_score, submission) = opt::blackbox_locally_optimized_submission(indata);
            let score = api.evaluate(&indata, &submission).await;
            let whitebox_score = whitebox::simulate(&indata, &submission);
            dbg!(&expected_score, &whitebox_score, &score);
        }
        "remoteopt" => {
            todo!()
        }
        _ => unimplemented!(),
    };
}

async fn sweep(api: &Api, indata: &InputData) -> Vec<(f64, Option<&'static str>, Score, Score)> {
    let rates = linspace(0.0, 6.0, 121);
    //let awards = iter::once(None).chain(indata.awards.keys().copied().map(Some));
    let awards = iter::once(None);
    let parameters = rates.cartesian_product(awards);

    return futures::future::join_all(parameters.map(|(rate, award)| async move {
        let submission = parameterized(indata, rate, award);
        let score = api.evaluate(&indata, &submission).await;
        let whitebox_score = whitebox::simulate(&indata, &submission);
        (rate, award, score, whitebox_score)
    }))
    .await;

    fn parameterized(
        indata: &InputData,
        rate: f64,
        award: Option<&'static str>,
    ) -> Vec<(&'static str, CustomerSubmission)> {
        indata
            .map
            .customers
            .iter()
            .filter(|c| c.name == "Gary Kreiger")
            .map(|customer| {
                let personality = indata.personalities.get(&customer.personality).unwrap();
                (
                    customer.name,
                    CustomerSubmission {
                        months_to_pay_back_loan: //personality.months_limit_multiplier
                            1* indata.map.game_length_in_months,
                        yearly_interest_rate: rate.clamp(
                            personality.accepted_min_interest,
                            personality.accepted_max_interest,
                        ),
                        awards: (0..(indata.map.game_length_in_months))
                            .map(|_| award)
                            .collect(),
                    },
                )
            })
            .collect()
    }
}

fn linspace(a: f64, b: f64, num: usize) -> impl Clone + Iterator<Item = f64> {
    (0..num).map(move |i| {
        let frac = (i as f64) / ((num - 1) as f64);
        frac * (b - a) + a
    })
}
