mod api;
mod model;
mod whitebox;

use api::{Api, CustomerSubmission, InputData};
use itertools::Itertools;
use model::Personality;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
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

    let indata = InputData::load("Gothenburg");

    let api = Api::new();

    let start = Instant::now();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        run(&api, &indata).await;
    });
    tracing::info!(num_calls = ?api.num_calls(), elapsed = ?start.elapsed());
}

async fn run(api: &Api, indata: &InputData) {
    let rates = linspace(0.0, 6.0, 121);
    //let awards = iter::once(None).chain(indata.awards.keys().map(|&x| Some(x)));
    let awards = iter::once(Some(
        *indata
            .awards
            .iter()
            .max_by(|(_, v1), (_, v2)| {
                PartialOrd::partial_cmp(&v1.base_happiness, &v2.base_happiness).unwrap()
            })
            .unwrap()
            .0,
    ));

    let parameters = rates.cartesian_product(awards);

    let results = if false {
        futures::future::join_all(parameters.map(|(rate, award)| async move {
            let submission = parameterized_submission(indata, rate, award);
            let score = api.evaluate(&indata, &submission).await;
            let whitebox_score = whitebox::simulate(&indata, &submission);
            assert_eq!(
                whitebox_score,
                whitebox::simulate_simplified(&indata, &submission)
            );
            (rate, award, score, whitebox_score)
        }))
        .await
    } else {
        let (award, submission) = if false {
            locally_optimized_submission(indata)
        } else {
            blackbox_locally_optimized_submission(indata)
        };
        let score = api.evaluate(&indata, &submission).await;
        let whitebox_score = whitebox::simulate(&indata, &submission);
        assert_eq!(
            whitebox_score,
            whitebox::simulate_simplified(&indata, &submission)
        );
        vec![(0.0, Some(award), score, whitebox_score)]
    };

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
            || score.happiness_score != whitebox_score.happiness_score
            || (score.total_score - whitebox_score.total_score).abs() > 1e-5
        {
            eprintln!("mismatch\n    real={score:?}\nwhitebox={whitebox_score:?}\n");
        }
    }
}

fn parameterized_submission(
    indata: &InputData,
    rate: f64,
    award: Option<&'static str>,
) -> Vec<(&'static str, CustomerSubmission)> {
    indata
        .map
        .customers
        .iter()
        .map(|customer| {
            let personality = indata.personalities.get(&customer.personality);
            (
                customer.name,
                CustomerSubmission {
                    months_to_pay_back_loan: indata.map.game_length_in_months,
                    yearly_interest_rate: match personality {
                        Some(&Personality {
                            accepted_min_interest,
                            accepted_max_interest,
                            ..
                        }) => rate.clamp(accepted_min_interest, accepted_max_interest),
                        None => {
                            tracing::warn!(?customer.personality, "Unknown");
                            rate
                        }
                    },
                    awards: (0..(indata.map.game_length_in_months))
                        .map(|_| award)
                        .collect(),
                },
            )
        })
        .collect()
}

fn locally_optimized_submission(
    indata: &InputData,
) -> (&'static str, Vec<(&'static str, CustomerSubmission)>) {
    let best_award = indata
        .awards
        .iter()
        .max_by(|(_, v1), (_, v2)| {
            PartialOrd::partial_cmp(&v1.base_happiness, &v2.base_happiness).unwrap()
        })
        .unwrap();
    assert!(
        best_award.1.base_happiness >= 0.0,
        "TODO Implement support for skipping award"
    );
    let best_award = *best_award.0;

    let ret = indata
        .map
        .customers
        .iter()
        .map(|customer| {
            let personality = &indata.personalities[&customer.personality];
            let rates = linspace_par(
                personality.accepted_min_interest,
                personality.accepted_max_interest,
                if customer.name == "Glenn" {
                    1_000_000_000
                } else {
                    10_000_000
                } + 1,
            );
            let (rate, month, _) = rates
                .flat_map_iter(|rate| {
                    //let months = iter::once(indata.map.game_length_in_months);
                    //let months = 0..(1000*indata.map.game_length_in_months + 1);
                    //let months = iter::once(1000 * indata.map.game_length_in_months);
                    let months = (0..(1000 * indata.map.game_length_in_months + 1))
                        .step_by(10 * indata.map.game_length_in_months);
                    months.map(move |month| (rate, month))
                })
                .map(|(rate, month)| {
                    (
                        rate,
                        month,
                        whitebox::simulate_simplified_kernel(
                            customer,
                            personality.living_standard_multiplier,
                            rate,
                            month,
                            indata.map.game_length_in_months,
                        ),
                    )
                })
                .max_by(|(_, _, score1), (_, _, score2)| score1.partial_cmp(score2).unwrap())
                .unwrap();

            tracing::info!(
                customer.name,
                rate,
                month,
                monthly_payment =
                    whitebox::compute_total_monthly_payment(rate, month, customer.loan.amount)
            );
            (
                customer.name,
                CustomerSubmission {
                    months_to_pay_back_loan: month,
                    yearly_interest_rate: rate,
                    awards: (0..(indata.map.game_length_in_months))
                        .map(|_| Some(best_award))
                        .collect(),
                },
            )
        })
        .collect();

    (best_award, ret)
}

fn blackbox_locally_optimized_submission(
    indata: &InputData,
) -> (&'static str, Vec<(&'static str, CustomerSubmission)>) {
    let best_award = indata
        .awards
        .iter()
        .max_by(|(_, v1), (_, v2)| {
            PartialOrd::partial_cmp(&v1.base_happiness, &v2.base_happiness).unwrap()
        })
        .unwrap();
    assert!(
        best_award.1.base_happiness >= 0.0,
        "TODO Implement support for skipping award"
    );
    let best_award = *best_award.0;

    #[derive(Debug)]
    struct BlackboxOpt {
        customer: model::Customer,
        personality: Personality,
        game_length_in_months: usize,
    }
    fn param_to_rate_months(p: &Vec<f64>, personality: &Personality) -> (f64, usize) {
        let rate = p[0].clamp(
            personality.accepted_min_interest,
            personality.accepted_max_interest,
        );
        let months = p[1].max(0.0) as usize;
        (rate, months)
    }
    impl argmin::core::CostFunction for BlackboxOpt {
        type Param = Vec<f64>;
        type Output = f64;
        fn cost(&self, p: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
            let (rate, months) = param_to_rate_months(p, &self.personality);
            Ok(-whitebox::simulate_simplified_kernel(
                &self.customer,
                self.personality.living_standard_multiplier,
                rate,
                months,
                self.game_length_in_months,
            ))
        }
    }

    let ret = indata
        .map
        .customers
        .iter()
        .map(|customer| {
            let personality = &indata.personalities[&customer.personality];
            let opt = BlackboxOpt {
                customer: customer.clone(),
                personality: personality.clone(),
                game_length_in_months: indata.map.game_length_in_months,
            };
            let solver = argmin::solver::particleswarm::ParticleSwarm::<Vec<f64>, f64, _>::new(
                (vec![0.0, 0.0], vec![1.0, 1_000_000.0]),
                10,
            );
            let res = argmin::core::Executor::new(opt, solver)
                .add_observer(
                    argmin_observer_slog::SlogLogger::term(),
                    argmin::core::observers::ObserverMode::NewBest,
                )
                .configure(|state| state.max_iters(100000))
                .run()
                .unwrap();
            dbg!(res.problem());
            dbg!(&res.state().best_individual);
            let (rate, months) = param_to_rate_months(
                &res.state().best_individual.as_ref().unwrap().position,
                &personality.clone(),
            );

            tracing::info!(
                customer.name,
                rate,
                months,
                monthly_payment =
                    whitebox::compute_total_monthly_payment(rate, months, customer.loan.amount)
            );
            (
                customer.name,
                CustomerSubmission {
                    months_to_pay_back_loan: months,
                    yearly_interest_rate: rate,
                    awards: (0..(indata.map.game_length_in_months))
                        .map(|_| Some(best_award))
                        .collect(),
                },
            )
        })
        .collect();

    (best_award, ret)
}

fn linspace(a: f64, b: f64, num: usize) -> impl Clone + Iterator<Item = f64> {
    (0..num).map(move |i| {
        let frac = (i as f64) / ((num - 1) as f64);
        frac * (b - a) + a
    })
}
fn linspace_par(
    a: f64,
    b: f64,
    num: usize,
) -> impl rayon::iter::IndexedParallelIterator + rayon::iter::ParallelIterator<Item = f64> {
    (0..num).into_par_iter().map(move |i| {
        let frac = (i as f64) / ((num - 1) as f64);
        frac * (b - a) + a
    })
}
