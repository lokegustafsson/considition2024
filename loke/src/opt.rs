use crate::{
    api::{CustomerSubmission, InputData},
    model::{Award, Customer, Personality},
};
use rayon::prelude::*;

const HEAVY: bool = false;

pub fn blackbox_locally_optimized_submission(
    indata: &InputData,
) -> (f64, Vec<(&'static str, CustomerSubmission)>) {
    /*
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
    */
    //let best_award = Some("GiftCard");
    //let best_award = None;

    #[derive(Debug)]
    struct BlackboxOpt {
        customer: Customer,
        personality: Personality,
        game_length_in_months: usize,
        awards: Vec<Option<(Award, f64)>>,
    }
    fn param_to_rate_months(p: &Vec<f64>, personality: &Personality) -> (f64, usize) {
        let p0 = if personality.accepted_max_interest > 1.0 {
            p[0].powf(8.0)
        } else {
            p[0]
        };
        let rate =
            p0 * personality.accepted_max_interest + (1.0 - p0) * personality.accepted_min_interest;
        let months = p[1].max(0.0) as usize;
        (rate, months)
    }
    impl argmin::core::CostFunction for BlackboxOpt {
        type Param = Vec<f64>;
        type Output = f64;
        fn cost(&self, p: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
            let (rate, months) = param_to_rate_months(p, &self.personality);
            let (score, _budget_required) = crate::whitebox::simulate_simplified_kernel(
                &self.customer,
                &self.personality,
                rate,
                months,
                self.game_length_in_months,
                &self.awards,
            );
            Ok(-score)
        }
    }

    let ret = indata
        .map
        .customers
        .par_iter()
        .map(|customer| {
            // HACK: Jitter to workaround Slogger not locking output
            std::thread::sleep(std::time::Duration::from_millis(
                customer.capital as u64 % 123,
            ));

            let personality = &indata.personalities[&customer.personality];
            let opt = BlackboxOpt {
                customer: customer.clone(),
                personality: personality.clone(),
                game_length_in_months: indata.map.game_length_in_months,
                // TODO: BlackboxOpt assuming no awards
                awards: vec![None; indata.map.game_length_in_months],
            };
            let solver = argmin::solver::particleswarm::ParticleSwarm::<Vec<f64>, f64, _>::new(
                (
                    vec![0.0, 0.0],
                    vec![1.0, (4 * indata.map.game_length_in_months) as f64],
                ),
                if HEAVY { 1000 } else { 10 },
            );
            let res = argmin::core::Executor::new(opt, solver)
                .add_observer(
                    argmin_observer_slog::SlogLogger::term(),
                    argmin::core::observers::ObserverMode::NewBest,
                )
                .configure(|state| state.max_iters(if HEAVY { 100_000 } else { 10_000 }))
                .run()
                .unwrap();
            //dbg!(res.problem());
            //dbg!(&res.state().best_individual);
            let (rate, months) = param_to_rate_months(
                &res.state().best_individual.as_ref().unwrap().position,
                &personality.clone(),
            );

            let best_award = {
                let mk_score = |(an, a): (&'static str, Award)| -> f64 {
                    let interest_cost = match an {
                        "NoInterestRate" => 1.0,
                        "HalfInterestRate" => 0.5,
                        _ => 0.0,
                    } * customer.loan.amount
                        * rate
                        / 12.0;
                    a.base_happiness * personality.happiness_multiplier - a.cost - interest_cost
                };
                let best_award = indata
                    .awards
                    .iter()
                    .map(|(n, a)| (*n, *a))
                    .max_by(|a1, a2| {
                        let score1 = mk_score(*a1);
                        let score2 = mk_score(*a2);
                        PartialOrd::partial_cmp(&score1, &score2).unwrap()
                    })
                    .unwrap();
                if mk_score(best_award) <= 0.0 {
                    None
                } else {
                    Some(best_award)
                }
            };

            (0..((indata.map.game_length_in_months + 1) / 2 + 4)
                .min(indata.map.game_length_in_months))
                .map(|num_awards| {
                    let mut awards: Vec<Option<&str>> =
                        vec![None; indata.map.game_length_in_months];
                    let mut sim_awards: Vec<Option<(Award, f64)>> =
                        vec![None; indata.map.game_length_in_months];

                    let best_award_name = best_award.map(|a| a.0);
                    let best_award_sim = best_award.map(|a| {
                        (
                            a.1,
                            match a.0 {
                                "NoInterestRate" => 1.0,
                                "HalfInterestRate" => 0.5,
                                _ => 0.0,
                            },
                        )
                    });

                    let cutoff = (indata.map.game_length_in_months + 1) / 2;
                    for i in 0..num_awards.min(cutoff) {
                        awards[sim_awards.len() - 1 - 2 * i] = best_award_name;
                        sim_awards[awards.len() - 1 - 2 * i] = best_award_sim;
                    }
                    for i in 0..num_awards.saturating_sub(cutoff) {
                        awards[sim_awards.len() - 2 - 2 * i] = best_award_name;
                        sim_awards[awards.len() - 2 - 2 * i] = best_award_sim;
                    }
                    let (score, budget_required) = crate::whitebox::simulate_simplified_kernel(
                        &customer,
                        personality,
                        rate,
                        months,
                        indata.map.game_length_in_months,
                        &sim_awards,
                    );
                    tracing::info!(
                        customer.name,
                        rate,
                        months,
                        best_award_name,
                        score,
                        budget_required
                    );
                    (
                        (
                            customer.name,
                            CustomerSubmission {
                                months_to_pay_back_loan: months,
                                yearly_interest_rate: rate,
                                awards: awards.into(),
                            },
                        ),
                        score,
                        round_pre_knapsack(budget_required),
                    )
                })
                .collect::<Vec<(_, f64, usize)>>()
        })
        .collect();

    // NOTE: Incorrect if fractional loans
    fn round_pre_knapsack(x: f64) -> usize {
        // TODO: Maybe ceil
        x.round() as usize
    }

    let (ret, ret_score) = knapsack(ret, round_pre_knapsack(indata.map.budget));
    (ret_score, ret)
}

fn gcd(mut x: usize, mut y: usize) -> usize {
    while x != 0 {
        (x, y) = (y % x, x);
    }
    y
}

// O(budget * items.len())
// NOTE: At integer resolution
fn knapsack<T: Clone>(mut items: Vec<Vec<(T, f64, usize)>>, mut budget: usize) -> (Vec<T>, f64) {
    let mut d = budget;
    for candidates in &items {
        for &(_, _, c) in candidates {
            d = gcd(d, c);
        }
    }
    budget /= d;
    for candidates in &mut items {
        for it in candidates {
            it.2 /= d;
        }
    }

    // items of (opaque, score, cost)
    // dp[<=category][budget_spent] = (score, last chosen cat, last chosen variant)
    let mut dp: Vec<Vec<(f64, u32, u32)>> =
        vec![vec![(0.0, u32::MAX, u32::MAX); budget + 1]; items.len() + 1];
    for i in 0..items.len() {
        for b in 0..=budget {
            // dont buy
            if dp[i][b].0 > dp[i + 1][b].0 {
                dp[i + 1][b] = dp[i][b];
            }
        }
        for (variant, (_, score, cost)) in items[i].iter().enumerate() {
            for b in 0..=(budget - cost) {
                // buy additional
                let cand_score = dp[i][b].0 + score;
                if cand_score > dp[i + 1][b + cost].0 {
                    dp[i + 1][b + cost] = (cand_score, i as u32, variant as u32);
                }
            }
        }
    }
    let (winner_score, mut winner_last, mut winner_variant) = dp[items.len()][budget].clone();
    let mut winner_items = vec![];
    let mut winner_loop_budget = budget;
    while winner_last != u32::MAX {
        winner_items.push(
            items[winner_last as usize][winner_variant as usize]
                .0
                .clone(),
        );
        winner_loop_budget -= items[winner_last as usize][winner_variant as usize].2;

        (_, winner_last, winner_variant) = dp[winner_last as usize][winner_loop_budget];
    }
    (winner_items, winner_score)
}

#[test]
fn test_knapsack() {
    assert_eq!(knapsack(vec![vec![(1, 1.23, 1)]], 2), (vec![1], 1.23));
    assert_eq!(
        knapsack(vec![vec![(1, 1.23, 1)], vec![(2, 2.23, 1)]], 2),
        (vec![2, 1], 3.46)
    );
    assert_eq!(
        knapsack(
            vec![vec![(0, 5.0, 9)], vec![(1, 1.23, 10)], vec![(2, 2.23, 10)]],
            20
        ),
        (vec![2, 0], 7.23)
    );
    assert_eq!(
        knapsack(
            vec![vec![(1, 1.23, 10)], vec![(0, 5.0, 9)], vec![(2, 2.23, 1)]],
            2
        ),
        (vec![2, 0], 7.23)
    );
    assert_eq!(
        knapsack(
            vec![vec![(0, 5.0, 9)], vec![(2, 2.23, 10)], vec![(1, 1.23, 10)]],
            20
        ),
        (vec![2, 0], 7.23)
    );
}
