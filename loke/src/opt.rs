use crate::{
    api::{CustomerSubmission, InputData},
    model::{Customer, Personality},
};
use rayon::prelude::*;

const HEAVY: bool = false;

pub fn blackbox_locally_optimized_submission(
    indata: &InputData,
) -> (
    f64,
    Option<&'static str>,
    Vec<(&'static str, CustomerSubmission)>,
) {
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
    let best_award = None;

    #[derive(Debug)]
    struct BlackboxOpt {
        customer: Customer,
        personality: Personality,
        game_length_in_months: usize,
    }
    fn param_to_rate_months(p: &Vec<f64>, personality: &Personality) -> (f64, usize) {
        let rate = p[0] * personality.accepted_max_interest
            + (1.0 - p[0]) * personality.accepted_min_interest;
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
                self.personality.living_standard_multiplier,
                rate,
                months,
                self.game_length_in_months,
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
            };
            let solver = argmin::solver::particleswarm::ParticleSwarm::<Vec<f64>, f64, _>::new(
                (vec![0.0, 0.0], vec![1.0, 2e9]),
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
            let (score, budget_required) = crate::whitebox::simulate_simplified_kernel(
                &customer,
                personality.living_standard_multiplier,
                rate,
                months,
                indata.map.game_length_in_months,
            );

            tracing::info!(customer.name, rate, months, score, budget_required,);
            (
                (
                    customer.name,
                    CustomerSubmission {
                        months_to_pay_back_loan: months,
                        yearly_interest_rate: rate,
                        awards: (0..(indata.map.game_length_in_months))
                            .map(|_| best_award)
                            .collect(),
                    },
                ),
                score,
                budget_required,
            )
        })
        .collect();

    let (ret, ret_score) = knapsack(ret, indata.map.budget);
    (ret_score, best_award, ret)
}

fn gcd(mut x: usize, mut y: usize) -> usize {
    while x != 0 {
        (x, y) = (y % x, x);
    }
    y
}

// O(budget * items.len())
// NOTE: At integer resolution
fn knapsack<T: Clone>(items: Vec<(T, f64, f64)>, budget: f64) -> (Vec<T>, f64) {
    let mut budget = budget as usize;

    let mut d = budget;
    let mut items: Vec<(T, f64, usize)> = items
        .into_iter()
        .map(|(it, s, c)| {
            let c = c.round() as usize;
            d = gcd(d, c);
            (it, s, c)
        })
        .collect();
    budget /= d;
    for it in &mut items {
        it.2 /= d;
    }
    // items of (opaque, score, cost)
    // dp[<=item][budget_spent] = (score, last chosen)
    let mut dp: Vec<Vec<(f64, usize)>> = vec![vec![(0.0, usize::MAX); budget + 1]; items.len() + 1];
    for i in 0..items.len() {
        let (_, score, cost) = items[i];
        for b in 0..=budget {
            // dont buy
            if dp[i][b].0 > dp[i + 1][b].0 {
                dp[i + 1][b] = dp[i][b];
            }
        }
        for b in 0..=(budget - cost) {
            // buy additional
            let cand_score = dp[i][b].0 + score;
            if cand_score > dp[i + 1][b + cost].0 {
                dp[i + 1][b + cost] = (cand_score, i);
            }
        }
    }
    let (winner_score, mut winner_last) = dp[items.len()][budget].clone();
    let mut winner_items = vec![];
    let mut winner_loop_budget = budget;
    while winner_last != usize::MAX {
        winner_items.push(items[winner_last].0.clone());
        winner_loop_budget -= items[winner_last].2;

        winner_last = dp[winner_last][winner_loop_budget].1;
    }
    (winner_items, winner_score)
}

#[test]
fn test_knapsack() {
    assert_eq!(knapsack(vec![(1, 1.23, 1.0)], 2.0), (vec![1], 1.23));
    assert_eq!(
        knapsack(vec![(1, 1.23, 1.0), (2, 2.23, 1.0)], 2.0),
        (vec![2, 1], 3.46)
    );
    assert_eq!(
        knapsack(vec![(0, 5.0, 0.9), (1, 1.23, 1.0), (2, 2.23, 1.0)], 2.0),
        (vec![2, 0], 7.23)
    );
    assert_eq!(
        knapsack(vec![(1, 1.23, 1.0), (0, 5.0, 0.9), (2, 2.23, 1.0)], 2.0),
        (vec![2, 0], 7.23)
    );
    assert_eq!(
        knapsack(vec![(0, 5.0, 0.9), (2, 2.23, 1.0), (1, 1.23, 1.0),], 2.0),
        (vec![2, 0], 7.23)
    );
}
