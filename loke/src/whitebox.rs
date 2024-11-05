use crate::{
    api::{CustomerSubmission, InputData},
    model::Score,
};

// Maximizing score is equivalent to maximizing this per customer
pub fn simulate_simplified_kernel(
    customer: &crate::model::Customer,
    living_standard_multiplier: f64,
    yearly_interest_rate: f64,
    months_to_pay_back_loan: usize,
    months_game: usize,
) -> (f64, f64) {
    let mut capital: f64 = customer.capital;
    let mut remaining_balance = customer.loan.amount;
    let mut marks = 0;
    //let mut awards_in_a_row = 0;

    let mut budget_shortfall = customer.loan.amount;
    let mut budget_required = budget_shortfall;

    let mut score = customer.loan.environmental_impact;
    let mut happiness = 0.0;
    'bankruptcy: for i in 0..months_game {
        if i < months_to_pay_back_loan {
            capital += customer.income;
            let cost_of_monthly_expense = customer.monthly_expenses * living_standard_multiplier;
            const BUG_STUDENT_LOAN_ALWAYS_FALSE: bool = true;
            let cost_of_student_loan =
                if customer.has_student_loan && (i % 3 == 0) && BUG_STUDENT_LOAN_ALWAYS_FALSE {
                    2000.0
                } else {
                    0.0
                };
            let cost_of_kids = customer.number_of_kids * 2000.0;
            let cost_of_mortgage = customer.home_mortgage * 0.01;
            // NOTE: This is a sign error in their code
            capital -=
                cost_of_monthly_expense - cost_of_student_loan - cost_of_kids - cost_of_mortgage;

            let interest_payment = remaining_balance * yearly_interest_rate / 12.0;
            let amortization = customer.loan.amount / months_to_pay_back_loan as f64;
            if interest_payment + amortization <= capital {
                capital -= interest_payment + amortization;
                remaining_balance = (remaining_balance - amortization).max(0.0);
                score += interest_payment;
                budget_shortfall -= interest_payment;
            } else {
                marks += 1;
                capital = 0.0;
                if marks >= 3 {
                    //happiness -= 500.0;
                    happiness = -500.0;
                    break 'bankruptcy;
                } else {
                    happiness -= 50.0;
                }
            }
        }
        // FIXME:
        //budget_shortfall += 4.0; // Assume buying cheap award
        budget_required = budget_required.max(budget_shortfall);
    }
    score += happiness;
    (score, budget_required)
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

    // NOTE: Yes, zero is legal
    assert!(
        submission.iter().all(|(_, s)| {
            #[allow(unused_comparisons)]
            let ret = s.months_to_pay_back_loan >= 0;
            ret
        }),
        "Customers need at least one month to pay back loan"
    );

    assert!(
        submission
            .iter()
            .all(|(n, _)| indata.map.customers.iter().any(|c| c.name == *n)),
        "All requested customers must exist on the chosen map!"
    );

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
            // TODO: Also initialize loan object
            if personality.accepted_min_interest <= sub.yearly_interest_rate
                && sub.yearly_interest_rate <= personality.accepted_max_interest
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
            const BUG_STUDENT_LOAN_ALWAYS_FALSE: bool = true;
            let cost_of_student_loan =
                if customer.has_student_loan && (i % 3 == 0) && BUG_STUDENT_LOAN_ALWAYS_FALSE {
                    2000.0
                } else {
                    0.0
                };
            let cost_of_kids = customer.number_of_kids * 2000.0;
            let cost_of_mortgage = customer.home_mortgage * 0.01;
            // NOTE: This is a sign error in their code
            customer_state.capital -=
                cost_of_monthly_expense - cost_of_student_loan - cost_of_kids - cost_of_mortgage;

            // CanPayLoan
            if i < customer_submission.months_to_pay_back_loan {
                let interest_payment = customer_state.remaining_balance
                    * customer_submission.yearly_interest_rate
                    / 12.0;
                let amortization =
                    customer.loan.amount / customer_submission.months_to_pay_back_loan as f64;
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
                    customer_state.capital = 0.0;
                    const MARKS_LIMIT: usize = 3;
                    if customer_state.marks >= MARKS_LIMIT {
                        customer_state.is_bankrupt = true;
                        //customer_state.happiness -= 500.0;
                        customer_state.happiness = -500.0;
                    } else {
                        customer_state.happiness -= 50.0;
                    }
                }
            }

            // Award
            if let Some(award) = customer_submission.awards[i] {
                let crate::model::Award {
                    cost,
                    base_happiness,
                } = &indata.awards[award];
                let happiness_multiplier = 1.0 - 0.2 * (customer_state.awards_in_a_row as f64);
                customer_state.happiness +=
                    base_happiness * personality.happiness_multiplier * happiness_multiplier;
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
                customer_state.awards_in_a_row = customer_state.awards_in_a_row.saturating_sub(1);
            }
        }
    }
    for customer_state in customer_state {
        ret.happiness_score += customer_state.happiness;
    }

    ret.total_score = (ret.environmental_impact + ret.happiness_score + ret.total_profit).trunc();
    ret
}
