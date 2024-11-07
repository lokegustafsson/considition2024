use crate::{
    api::{CustomerSubmission, InputData},
    model::{Award, Customer, Personality},
};
use rayon::prelude::*;

const USE_VERY_SLOW_BUT_GOOD_DP: bool = false;
//const NUM_PARTICLES: usize = 20;
//const MAX_ITERS: u64 = 10000;
//const AWARD_CONF_TTL: usize = 1000;
const NUM_PARTICLES: usize = 200;
const MAX_ITERS: u64 = 1000; // 0
const AWARD_CONF_TTL: usize = 100_000;

#[derive(Clone)]
struct Param(Vec<f64>, usize);

impl argmin_math::ArgminAdd<Self, Self> for Param {
    fn add(&self, other: &Self) -> Self {
        Self(self.0.add(&other.0), self.1.clone())
    }
}
impl argmin_math::ArgminSub<Self, Self> for Param {
    fn sub(&self, other: &Self) -> Self {
        Self(self.0.sub(&other.0), self.1.clone())
    }
}
impl argmin_math::ArgminMinMax for Param {
    fn min(x: &Self, y: &Self) -> Self {
        Self(argmin_math::ArgminMinMax::min(&x.0, &y.0), x.1.clone())
    }
    fn max(x: &Self, y: &Self) -> Self {
        Self(argmin_math::ArgminMinMax::max(&x.0, &y.0), x.1.clone())
    }
}
impl argmin_math::ArgminMul<f64, Self> for Param {
    fn mul(&self, other: &f64) -> Self {
        Self(self.0.mul(other), self.1.clone())
    }
}
impl argmin_math::ArgminRandom for Param {
    fn rand_from_range<R: argmin_math::Rng>(min: &Self, max: &Self, rng: &mut R) -> Self {
        Self(
            Vec::<f64>::rand_from_range(&min.0, &max.0, rng),
            u64::rand_from_range(&u64::MIN, &u64::MAX, rng) as usize,
        )
    }
}
impl argmin_math::ArgminZeroLike for Param {
    fn zero_like(&self) -> Self {
        Self(vec![0.0; self.0.len()], self.1)
    }
}

pub fn blackbox_locally_optimized_submission(
    indata: &InputData,
) -> (f64, Vec<(&'static str, CustomerSubmission)>) {
    #[derive(Debug)]
    struct BlackboxOpt {
        customer: Customer,
        personality: Personality,
        game_length_in_months: usize,
        award_available: [(&'static str, Award, f64); 6],
        id_to_awards_ttl: dashmap::DashMap<usize, (Vec<Option<(Award, f64)>>, usize)>,
    }
    fn param_to_rate_months(p: &Vec<f64>, personality: &Personality) -> (f64, usize) {
        let p0 = if personality.accepted_max_interest > 1.0 {
            p[0].powf(8.0)
        } else {
            p[0]
        };
        let rate =
            p0 * personality.accepted_max_interest + (1.0 - p0) * personality.accepted_min_interest;
        let months = (p[1] * personality.months_limit_multiplier as f64).round() as usize;
        (rate, months)
    }
    impl argmin::core::CostFunction for BlackboxOpt {
        type Param = Param;
        type Output = f64;
        fn cost(&self, p: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
            let (rate, months) = param_to_rate_months(&p.0, &self.personality);
            let mut entry = self
                .id_to_awards_ttl
                .entry(p.1)
                .or_insert_with(|| (vec![None; self.game_length_in_months], AWARD_CONF_TTL));
            let (awards, ttl) = entry.value_mut();
            if *ttl == 0 {
                let aws = crate::whitebox::simulate_kernel_dp_optimal_awards(
                    &self.customer,
                    &self.personality,
                    rate,
                    months,
                    self.game_length_in_months,
                    &self.award_available,
                )
                .into_iter()
                .max_by(|(s1, _, _), (s2, _, _)| f64::total_cmp(&s1, &s2))
                .unwrap_or_else(|| (0.0, 0.0, vec![None; self.game_length_in_months]));

                *awards = aws
                    .2
                    .into_iter()
                    .map(|a| {
                        a.map(|aa| {
                            let t = self.award_available[(aa.get() as usize) - 1];
                            (t.1, t.2)
                        })
                    })
                    .collect();
                *ttl = AWARD_CONF_TTL;
            } else {
                *ttl -= 1;
            }
            let (score, _budget_required, _bankrupt) = crate::whitebox::simulate_simplified_kernel(
                &self.customer,
                &self.personality,
                rate,
                months,
                self.game_length_in_months,
                awards,
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
            let award_available = {
                let entry = indata.awards.first_key_value().unwrap();
                let mut ret = [(*entry.0, *entry.1, 0.0); 6];
                for (&n, &a) in indata.awards.iter() {
                    let d = match n {
                        "NoInterestRate" => 1.0,
                        "HalfInterestRate" => 0.5,
                        _ => 0.0,
                    };
                    ret[a.id.get() as usize - 1] = (n, a, d);
                }
                ret
            };
            let opt = BlackboxOpt {
                customer: customer.clone(),
                personality: personality.clone(),
                game_length_in_months: indata.map.game_length_in_months,
                award_available: award_available.clone(),
                id_to_awards_ttl: dashmap::DashMap::new(),
            };
            let award_available = opt.award_available.clone();
            let solver = argmin::solver::particleswarm::ParticleSwarm::<Param, f64, _>::new(
                (
                    Param(vec![0.0, 0.0], 0),
                    Param(
                        vec![1.0, indata.map.game_length_in_months as f64],
                        usize::MAX,
                    ),
                ),
                NUM_PARTICLES,
            );
            let res = argmin::core::Executor::new(opt, solver)
                .add_observer(
                    argmin_observer_slog::SlogLogger::term(),
                    argmin::core::observers::ObserverMode::NewBest,
                )
                .configure(|state| state.max_iters(MAX_ITERS))
                .run()
                .unwrap();
            //dbg!(res.problem());
            //dbg!(&res.state().best_individual);
            let (rate, months) = param_to_rate_months(
                &res.state().best_individual.as_ref().unwrap().position.0,
                &personality.clone(),
            );
            if USE_VERY_SLOW_BUT_GOOD_DP {
                crate::whitebox::simulate_kernel_dp_optimal_awards(
                    &customer,
                    &personality,
                    rate,
                    months,
                    indata.map.game_length_in_months,
                    &award_available,
                )
                .into_iter()
                .map(|(score, cost, awards)| {
                    tracing::info!(customer.name, rate, months, ?awards, score, cost);
                    // TODO: Cost rounding could be incorrect
                    (
                        (
                            customer.name,
                            CustomerSubmission {
                                months_to_pay_back_loan: months,
                                yearly_interest_rate: rate,
                                awards: awards
                                    .into_iter()
                                    .map(|a| a.map(|aa| award_available[(aa.get() as usize) - 1].0))
                                    .collect(),
                            },
                        ),
                        score,
                        cost.round() as usize,
                    )
                })
                .collect()
            } else {
                (0..36)
                    .filter_map(|idx| {
                        let a = idx % 6;
                        let b = idx / 6;
                        if a == b {
                            return None;
                        }
                        let mut awards: Vec<Option<&str>> =
                            vec![None; indata.map.game_length_in_months];
                        let mut sim_awards: Vec<Option<(Award, f64)>> =
                            vec![None; indata.map.game_length_in_months];
                        let mut lasta = true;
                        for i in 0..indata.map.game_length_in_months {
                            if i % 4 != 3 {
                                continue;
                            }
                            let xx = if !lasta {
                                lasta = true;
                                award_available[a]
                            } else {
                                lasta = false;
                                award_available[b]
                            };
                            awards[i] = Some(xx.0);
                            sim_awards[i] = Some((
                                xx.1,
                                match xx.0 {
                                    "NoInterestRate" => 1.0,
                                    "HalfInterestRate" => 0.5,
                                    _ => 0.0,
                                },
                            ));
                        }

                        let (score, budget_required, bankruptcy_at) =
                            crate::whitebox::simulate_simplified_kernel(
                                &customer,
                                personality,
                                rate,
                                months,
                                indata.map.game_length_in_months,
                                &sim_awards,
                            );
                        let bankruptcy_at = if bankruptcy_at == -1 {
                            String::new()
                        } else {
                            format!("bankrupt_at={}", bankruptcy_at)
                        };
                        tracing::info!(
                            customer.name,
                            rate,
                            months,
                            score,
                            budget_required,
                            "{}",
                            bankruptcy_at,
                        );
                        Some((
                            (
                                customer.name,
                                CustomerSubmission {
                                    months_to_pay_back_loan: months,
                                    yearly_interest_rate: rate,
                                    awards: awards.into(),
                                },
                            ),
                            score,
                            round_pre_knapsack(budget_required, true),
                        ))
                    })
                    .filter(|(_, score, cost)| *score > 0.0 && *cost < indata.map.budget as usize)
                    .collect::<Vec<(_, f64, usize)>>()
            }
        })
        .collect();

    // NOTE: Incorrect if fractional loans
    fn round_pre_knapsack(x: f64, cceil: bool) -> usize {
        // TODO: Maybe ceil
        if cceil {
            (x.ceil() / 10.0) as usize
        } else {
            (x.floor() / 10.0) as usize
        }
    }

    let (ret, ret_score) = knapsack(ret, round_pre_knapsack(indata.map.budget, false));
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
            if budget < *cost {
                continue;
            }
            for b in 0..=budget.saturating_sub(*cost) {
                // buy additional
                let cand_score = dp[i][b].0 + score;
                if cand_score > dp[i + 1][b + cost].0 {
                    dp[i + 1][b + cost] = (cand_score, i as u32, variant as u32);
                }
            }
        }
    }
    let (winner_score, mut winner_last, mut winner_variant) = dp[items.len()][budget];
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
