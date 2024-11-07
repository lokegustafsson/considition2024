use crate::{
    api::{CustomerSubmission, InputData},
    model::{Award, Personality, Score},
};
use std::{
    collections::{BTreeMap, VecDeque},
    num::{NonZeroU8, NonZeroUsize},
};

pub fn simulate_kernel_dp_optimal_awards(
    customer: &crate::model::Customer,
    personality: &Personality,
    yearly_interest_rate: f64,
    months_to_pay_back_loan: usize,
    months_game: usize,
    awards_available: &[(&'static str, Award, f64); 6],
) -> Vec<(f64, f64, Vec<Option<NonZeroU8>>)> {
    // Return pareto optimal (score, cost/max shortfall, award construction)
    assert!(personality.accepted_min_interest <= yearly_interest_rate);
    assert!(yearly_interest_rate <= personality.accepted_max_interest);
    assert!(months_to_pay_back_loan <= personality.months_limit_multiplier * months_game);

    let mut ctx = Ctx {
        customer,
        personality,
        yearly_interest_rate,
        months_to_pay_back_loan,
        months_game,
        awards_available,
        visited: (0..(2 * 6 * 2 * (months_game + 1)))
            .map(|_| BTreeMap::default())
            .collect(),
    };
    #[derive(Debug)]
    struct Ctx<'a> {
        customer: &'a crate::model::Customer,
        personality: &'a Personality,
        yearly_interest_rate: f64,
        months_to_pay_back_loan: usize,
        months_game: usize,
        awards_available: &'a [(&'static str, Award, f64); 6],
        visited: Box<[BTreeMap<TotalFloat, (f64, f64, u16)>]>,
    }
    impl<'a> Ctx<'a> {
        fn partition(
            &self,
            force_bankruptcy: bool,
            i: u16,
            awards_in_a_row: u8,
            last_2_awards: [u8; 2],
        ) -> usize {
            let mut state = 0;
            let mut consume = |i: usize, l: usize| {
                assert!(i < l, "{}", l);
                state = state * l + i;
                state
            };
            consume(force_bankruptcy as usize, 2);
            consume(awards_in_a_row as usize, 6);
            consume((last_2_awards[0] == last_2_awards[1]) as usize, 2);
            consume(i as usize, self.months_game + 1)
        }
    }
    let mut results: Vec<_> = [true, false]
        .into_iter()
        .flat_map(|force_bankruptcy| {
            dp(
                &mut ctx,
                force_bankruptcy,
                0,
                customer.capital,
                customer.loan.amount,
                0,
                0,
                customer.loan.amount,
                customer.loan.amount,
                customer.loan.environmental_impact,
                [0, 0],
                0,
            )
        })
        .collect();
    #[derive(PartialOrd, PartialEq, Debug)]
    struct TotalFloat(f64);
    impl Eq for TotalFloat {}
    impl Ord for TotalFloat {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            f64::total_cmp(&self.0, &other.0)
        }
    }
    fn dp<'a>(
        ctx: &mut Ctx<'a>,
        force_bankruptcy: bool,            // partition
        i: u16,                            // partition
        mut capital: f64,                  // ignore
        mut remaining_balance: f64,        // ignore
        mut marks: u8,                     // ignore
        awards_in_a_row: u8,               // partition
        mut budget_shortfall: f64,         // prefer lower
        mut budget_required: f64,          // prefer lower
        mut score: f64,                    // prefer higher
        last_2_awards: [u8; 2],            // partition
        months_without_awards_in_row: u16, // prefer lower
    ) -> Vec<(f64, f64, Vec<Option<NonZeroU8>>)> {
        {
            let p = ctx.partition(force_bankruptcy, i, awards_in_a_row, last_2_awards);
            if ctx.visited[p].range(TotalFloat(score)..).any(
                |(_, &(other_shortfall, other_required, other_months_without))| {
                    other_shortfall <= budget_shortfall
                        && other_required <= budget_required
                        && other_months_without <= months_without_awards_in_row
                },
            ) {
                // CULLED
                return Vec::new();
            } else {
                ctx.visited[p].insert(
                    TotalFloat(score),
                    (
                        budget_shortfall,
                        budget_required,
                        months_without_awards_in_row,
                    ),
                );
            }
        }
        if (i as usize) == ctx.months_game {
            if force_bankruptcy || score <= 0.0 {
                return vec![];
            } else {
                return vec![(score, budget_required, Vec::new())];
            }
        }
        // PAYMENTS
        budget_required = budget_required.max(budget_shortfall);
        let mut is_bankrupt = false;
        if (i as usize) < ctx.months_to_pay_back_loan {
            capital += ctx.customer.income;
            let cost_of_monthly_expense =
                ctx.customer.monthly_expenses * ctx.personality.living_standard_multiplier;
            let cost_of_student_loan = if ctx.customer.has_student_loan && (i % 3 == 0) {
                2000.0
            } else {
                0.0
            };
            let cost_of_kids = ctx.customer.number_of_kids * 2000.0;
            let cost_of_mortgage = ctx.customer.home_mortgage * 0.001;
            capital -=
                cost_of_monthly_expense + cost_of_student_loan + cost_of_kids + cost_of_mortgage;

            let amortization = ctx.customer.loan.amount / ctx.months_to_pay_back_loan as f64;
            let interest_payment = remaining_balance * ctx.yearly_interest_rate / 12.0;
            if interest_payment + amortization <= capital {
                capital -= interest_payment + amortization;
                remaining_balance = (remaining_balance - amortization).max(0.0);
                score += interest_payment;
                budget_shortfall -= interest_payment;
            } else {
                marks += 1;
                if marks >= 3 {
                    if force_bankruptcy {
                        score -= 500.0;
                        is_bankrupt = true;
                    } else {
                        return Vec::new();
                    }
                } else {
                    if force_bankruptcy {
                    } else {
                        score -= 50.0;
                    }
                }
            }
        }
        // AWARDS
        let mut ret = Vec::new();
        for award_idx in 0..7 {
            // NOTE: MUST NOT mutate shared vars here:
            if let Some((_, award, interest_rebate)) =
                NonZeroUsize::new(award_idx as usize).map(|i| ctx.awards_available[i.get() - 1])
            {
                let months_without_awards_in_row = 0;

                let happ_mult = if last_2_awards[0] == award_idx && last_2_awards[1] == award_idx {
                    -1.0
                } else {
                    1.0 - 0.2 * awards_in_a_row as f64
                };
                let last_2_awards = [award_idx, last_2_awards[0]];
                let score = if is_bankrupt || !force_bankruptcy {
                    score + award.base_happiness * ctx.personality.happiness_multiplier * happ_mult
                } else {
                    score
                };
                let interest_payment = remaining_balance * ctx.yearly_interest_rate / 12.0;
                let full_cost = award.cost + interest_rebate * interest_payment;
                let budget_shortfall = budget_shortfall + full_cost;
                let score = score - full_cost;
                let awards_in_a_row = (awards_in_a_row + 1).min(5);

                if is_bankrupt {
                    assert!(force_bankruptcy);
                    if score > 0.0 {
                        let mut path = vec![None; ctx.months_game - i as usize - 1];
                        path.push(NonZeroU8::new(award_idx));
                        ret.push((score, budget_required, path));
                    }
                } else {
                    let mut candidates = dp(
                        ctx,
                        force_bankruptcy,
                        i + 1,
                        capital,
                        remaining_balance,
                        marks,
                        awards_in_a_row,
                        budget_shortfall,
                        budget_required,
                        score,
                        last_2_awards,
                        months_without_awards_in_row,
                    );
                    for (_, _, ref mut path) in &mut candidates {
                        path.push(NonZeroU8::new(award_idx));
                    }
                    ret.extend(candidates);
                }
            } else {
                let months_without_awards_in_row = months_without_awards_in_row + 1;
                let score =
                    if (is_bankrupt || !force_bankruptcy) && months_without_awards_in_row > 3 {
                        score - 500.0 * months_without_awards_in_row as f64
                    } else {
                        score
                    };
                let awards_in_a_row = awards_in_a_row.saturating_sub(1);

                if is_bankrupt {
                    assert!(force_bankruptcy);
                    if score > 0.0 {
                        let mut path = vec![None; ctx.months_game - i as usize - 1];
                        path.push(NonZeroU8::new(award_idx));
                        ret.push((score, budget_required, path));
                    }
                } else {
                    let mut candidates = dp(
                        ctx,
                        force_bankruptcy,
                        i + 1,
                        capital,
                        remaining_balance,
                        marks,
                        awards_in_a_row,
                        budget_shortfall,
                        budget_required,
                        score,
                        last_2_awards,
                        months_without_awards_in_row,
                    );
                    for (_, _, ref mut path) in &mut candidates {
                        path.push(NonZeroU8::new(award_idx));
                    }
                    ret.extend(candidates);
                }
            }
        }
        // Highest score first
        ret.sort_by(|(s1, _, _), (s2, _, _)| f64::total_cmp(&s2, &s1));
        ret.retain({
            let mut lowest_budget_yet = f64::MAX;
            move |(_, c, _)| {
                if *c >= lowest_budget_yet {
                    false
                } else {
                    lowest_budget_yet = *c;
                    true
                }
            }
        });
        ret
    }

    // CHECKING ANSWER
    for (score, budget_required, path) in &mut results {
        path.reverse();
        if true {
            let (s, b, _) = simulate_simplified_kernel(
                customer,
                personality,
                yearly_interest_rate,
                months_to_pay_back_loan,
                months_game,
                &path
                    .into_iter()
                    .map(|i| {
                        i.map(|j| {
                            let t = awards_available[j.get() as usize - 1];
                            (t.1, t.2)
                        })
                    })
                    .collect::<Vec<_>>(),
            );
            if (*score - s).abs() > 1e-4 || (*budget_required - b).abs() > 1e-4 {
                dbg!(
                    score,
                    s,
                    budget_required,
                    b,
                    path,
                    customer,
                    yearly_interest_rate,
                    months_to_pay_back_loan
                );
                panic!();
            }
        }
    }

    results
}

// Maximizing score is equivalent to maximizing this per customer
pub fn simulate_simplified_kernel(
    customer: &crate::model::Customer,
    personality: &Personality,
    yearly_interest_rate: f64,
    months_to_pay_back_loan: usize,
    months_game: usize,
    awards_with_rebates: &[Option<(Award, f64)>],
) -> (f64, f64, i32) {
    assert!(personality.accepted_min_interest <= yearly_interest_rate);
    assert!(yearly_interest_rate <= personality.accepted_max_interest);
    assert_eq!(awards_with_rebates.len(), months_game);
    assert!(months_to_pay_back_loan <= personality.months_limit_multiplier * months_game);
    let mut capital: f64 = customer.capital;
    let mut remaining_balance = customer.loan.amount;
    let mut marks = 0;
    let mut awards_in_a_row: i32 = 0;

    let mut budget_shortfall = customer.loan.amount;
    let mut budget_required = budget_shortfall;

    let mut score = customer.loan.environmental_impact;
    let mut happiness = 0.0;
    let mut last_3_awards: VecDeque<NonZeroU8> = VecDeque::new();
    let mut months_without_awards_in_row = 0;
    let mut bankruptcy_at = -1;
    'bankruptcy: for i in 0..months_game {
        budget_required = budget_required.max(budget_shortfall);
        let mut is_bankrupt = false;
        if i < months_to_pay_back_loan {
            capital += customer.income;
            let cost_of_monthly_expense =
                customer.monthly_expenses * personality.living_standard_multiplier;
            let cost_of_student_loan = if customer.has_student_loan && (i % 3 == 0) {
                2000.0
            } else {
                0.0
            };
            let cost_of_kids = customer.number_of_kids * 2000.0;
            let cost_of_mortgage = customer.home_mortgage * 0.001;
            capital -=
                cost_of_monthly_expense + cost_of_student_loan + cost_of_kids + cost_of_mortgage;

            let amortization = customer.loan.amount / months_to_pay_back_loan as f64;
            let interest_payment = remaining_balance * yearly_interest_rate / 12.0;
            if interest_payment + amortization <= capital {
                capital -= interest_payment + amortization;
                remaining_balance = (remaining_balance - amortization).max(0.0);
                score += interest_payment;
                budget_shortfall -= interest_payment;
            } else {
                marks += 1;
                if marks >= 3 {
                    happiness = -500.0;
                    is_bankrupt = true;
                } else {
                    happiness -= 50.0;
                }
            }
        }
        if let Some((award, interest_rebate)) = awards_with_rebates[i] {
            while last_3_awards.len() >= 3 {
                last_3_awards.pop_front();
            }
            last_3_awards.push_back(award.id);
            months_without_awards_in_row = 0;

            let happ_mult = if last_3_awards.len() == 3
                && last_3_awards[0] == last_3_awards[1]
                && last_3_awards[1] == last_3_awards[2]
            {
                -1.0
            } else {
                1.0 - 0.2 * awards_in_a_row as f64
            };
            happiness += award.base_happiness * personality.happiness_multiplier * happ_mult;
            let interest_payment = remaining_balance * yearly_interest_rate / 12.0;
            let full_cost = award.cost + interest_rebate * interest_payment;
            budget_shortfall += full_cost;
            score -= full_cost;
            awards_in_a_row = (awards_in_a_row + 1).min(5);
        } else {
            months_without_awards_in_row += 1;
            if months_without_awards_in_row > 3 {
                happiness -= 500.0 * months_without_awards_in_row as f64;
            }
            awards_in_a_row = (awards_in_a_row - 1).max(0);
        }
        if is_bankrupt {
            bankruptcy_at = i as i32;
            break 'bankruptcy;
        }
    }
    score += happiness;
    (score, budget_required, bankruptcy_at)
}

pub fn simulate(
    indata: &InputData,
    submission: &[(&'static str, CustomerSubmission)],
) -> crate::model::Score {
    assert!(
        submission.len() > 0,
        "You must choose at least one customer to play!"
    );

    assert!(
        submission
            .iter()
            .all(|(_, s)| s.awards.len() <= indata.map.game_length_in_months),
        "You can not exceed amount of months in 'iterations' then described in map config"
    );

    assert!(
        submission
            .iter()
            .all(|(_, s)| s.awards.len() == indata.map.game_length_in_months),
        "You must provide customer actions for each month of the designated game length!"
    );

    assert!(
        submission
            .iter()
            .all(|(n, _)| indata.map.customers.iter().any(|c| c.name == *n)),
        "All requested customers must exist on the chosen map!"
    );
    for (name, s) in submission {
        let customer = indata
            .map
            .customers
            .iter()
            .find(|c| c.name == *name)
            .unwrap();
        let personality = &indata.personalities[&customer.personality];
        assert!(
            s.months_to_pay_back_loan
                <= personality.months_limit_multiplier * indata.map.game_length_in_months
        );
    }

    let accepted_customers: Vec<_> = submission
        .iter()
        .filter_map(|(customer_name, sub)| {
            let customer = indata
                .map
                .customers
                .iter()
                .find(|c| c.name == *customer_name)
                .unwrap();
            let personality = &indata.personalities[&customer.personality];

            if personality.accepted_min_interest <= sub.yearly_interest_rate
                && sub.yearly_interest_rate <= personality.accepted_max_interest
                && sub.months_to_pay_back_loan
                    <= personality.months_limit_multiplier * indata.map.game_length_in_months
            {
                Some((customer, sub))
            } else {
                eprintln!(
                    "never printed, because local_opt will never suggest out-of-bounds rates"
                );
                None
            }
        })
        .collect();

    let mut ret = Score {
        map_name: indata.map.name.to_string(),
        environmental_impact: accepted_customers
            .iter()
            .map(|(c, _)| c.loan.environmental_impact)
            .sum(),
        happiness_score: 0.0,
        total_profit: 0.0,
        total_score: 0.0,
    };
    let mut budget = indata.map.budget
        - accepted_customers
            .iter()
            .map(|(c, __)| c.loan.amount)
            .sum::<f64>();

    struct CustomerState {
        capital: f64,
        remaining_balance: f64,
        marks: usize,
        happiness: f64,
        is_bankrupt: bool,
        awards_in_a_row: usize,
        last_3_awards: VecDeque<NonZeroU8>,
        months_without_awards_in_row: usize,
    }
    let mut customer_state: Vec<CustomerState> = accepted_customers
        .iter()
        .map(|(customer, _customer_submission)| CustomerState {
            capital: customer.capital,
            remaining_balance: customer.loan.amount,
            marks: 0,
            happiness: 0.0,
            is_bankrupt: false,
            awards_in_a_row: 0,
            last_3_awards: VecDeque::new(),
            months_without_awards_in_row: 0,
        })
        .collect();
    for i in 0..indata.map.game_length_in_months {
        for ((customer, customer_submission), customer_state) in
            accepted_customers.iter().zip(customer_state.iter_mut())
        {
            if budget <= 0.0 {
                return Score {
                    map_name: indata.map.name.to_string(),

                    environmental_impact: f64::NEG_INFINITY,
                    happiness_score: f64::NEG_INFINITY,
                    total_profit: f64::NEG_INFINITY,

                    total_score: f64::NEG_INFINITY,
                };
            }
            if customer_state.is_bankrupt {
                continue;
            }
            let personality = &indata.personalities[&customer.personality];

            // Payday
            customer_state.capital += customer.income;

            // PayBills
            let cost_of_monthly_expense =
                customer.monthly_expenses * personality.living_standard_multiplier;
            let cost_of_student_loan = if customer.has_student_loan && (i % 3 == 0) {
                2000.0
            } else {
                0.0
            };
            let cost_of_kids = customer.number_of_kids * 2000.0;
            let cost_of_mortgage = customer.home_mortgage * 0.001;
            customer_state.capital -=
                cost_of_monthly_expense + cost_of_student_loan + cost_of_kids + cost_of_mortgage;

            // CanPayLoan
            if i < customer_submission.months_to_pay_back_loan {
                let interest_payment = customer_state.remaining_balance
                    * customer_submission.yearly_interest_rate
                    / 12.0;
                let amortization =
                    customer.loan.amount / customer_submission.months_to_pay_back_loan as f64;
                /*
                dbg!(
                    customer.name,
                    customer_submission.yearly_interest_rate,
                    customer_submission.months_to_pay_back_loan,
                    customer_state.capital,
                    interest_payment,
                    amortization,
                    customer_state.marks,
                    ret.total_profit
                );
                */
                if interest_payment + amortization <= customer_state.capital {
                    // PayLoan
                    customer_state.capital -= interest_payment + amortization;
                    let profit = interest_payment;
                    customer_state.remaining_balance =
                        (customer_state.remaining_balance - amortization).max(0.0);
                    ret.total_profit += profit;
                    budget += profit; // NOTE: their bug
                } else {
                    // IncrementMark
                    customer_state.marks += 1;
                    const MARKS_LIMIT: usize = 3;
                    if customer_state.marks >= MARKS_LIMIT {
                        customer_state.is_bankrupt = true;
                        customer_state.happiness = -500.0;
                    } else {
                        customer_state.happiness -= 50.0;
                    }
                }
            }

            // Award
            // NOTE: They have bug where customer is not paid back interest.
            if let Some(award) = customer_submission.awards[i] {
                let crate::model::Award {
                    id,
                    cost,
                    base_happiness,
                } = &indata.awards[award];

                while customer_state.last_3_awards.len() >= 3 {
                    customer_state.last_3_awards.pop_front();
                }
                customer_state.last_3_awards.push_back(*id);
                customer_state.months_without_awards_in_row = 0;

                let happ_mult = if customer_state.last_3_awards.len() == 3
                    && customer_state.last_3_awards[0] == customer_state.last_3_awards[1]
                    && customer_state.last_3_awards[1] == customer_state.last_3_awards[2]
                {
                    -1.0
                } else {
                    1.0 - 0.2 * customer_state.awards_in_a_row as f64
                };

                customer_state.happiness +=
                    base_happiness * personality.happiness_multiplier * happ_mult;
                customer_state.awards_in_a_row = (customer_state.awards_in_a_row + 1).min(5);
                let cost = *cost
                    + match award {
                        "NoInterestRate" => {
                            customer_state.remaining_balance
                                * customer_submission.yearly_interest_rate
                                / 12.0
                        }
                        "HalfInterestRate" => {
                            customer_state.remaining_balance
                                * customer_submission.yearly_interest_rate
                                / 24.0
                        }
                        _ => 0.0,
                    };
                ret.total_profit -= cost;
                budget -= cost;
            } else {
                customer_state.months_without_awards_in_row += 1;
                if customer_state.months_without_awards_in_row > 3 {
                    customer_state.happiness -=
                        500.0 * customer_state.months_without_awards_in_row as f64;
                }
                customer_state.awards_in_a_row = customer_state.awards_in_a_row.saturating_sub(1);
            }
        }
    }
    for customer_state in customer_state {
        ret.happiness_score += customer_state.happiness;
    }

    ret.total_profit = ret.total_profit.trunc();
    ret.total_score = (ret.environmental_impact + ret.happiness_score + ret.total_profit).trunc();
    ret
}
