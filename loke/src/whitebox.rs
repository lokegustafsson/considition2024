use crate::api::{CustomerSubmission, InputData};

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
                None
            }
        })
        .collect();

    let mut ret = crate::model::Score {
        map_name: indata.map.name.to_string(),
        environmental_impact: accepted_customers
            .iter()
            .map(|(c, _)| c.loan.environmental_impact)
            .sum(),
        happiness_score: 0.0,
        total_profit: 0.0,
        total_score: 0.0,
    };
    // TODO: transposed order?? not read?
    let mut _budget = indata.map.budget
        - accepted_customers
            .iter()
            .map(|(c, __)| c.loan.amount)
            .sum::<f64>();

    for (customer, customer_submission) in accepted_customers {
        let mut customer_capital = customer.capital;
        let mut customer_remaining_balance = customer.loan.amount;
        let mut customer_marks = 0;
        let mut _successful_payment_streak = 0; // NOTE: Never read
        let mut customer_happiness = 0.0;
        let mut _is_bankrupt = false; // NOTE: Never read

        let personality = &indata.personalities[&customer.personality];
        for i in 0..customer_submission.months_to_pay_back_loan {
            // Payday
            customer_capital += customer.income;

            // PayBills
            let cost_of_monthly_expense =
                customer.monthly_expenses * personality.living_standard_multiplier;
            const BUG_STUDENT_LOAN_ALWAYS_FALSE: bool = false;
            let cost_of_student_loan = if customer.has_student_loan && (i % 3 == 0) && BUG_STUDENT_LOAN_ALWAYS_FALSE {
                2000.0
            } else {
                0.0
            };
            let cost_of_kids = customer.number_of_kids * 2000.0;
            let cost_of_mortgage = customer.home_mortgage * 0.01;
            // NOTE: This is a sign error in their code
            customer_capital -=
                cost_of_monthly_expense - cost_of_student_loan - cost_of_kids - cost_of_mortgage;

            // CanPayLoan
            let monthly_payment = compute_total_monthly_payment(
                customer_submission.yearly_interest_rate,
                customer_submission.months_to_pay_back_loan,
                customer.loan.amount,
            );
            if customer_capital >= monthly_payment {
                // PayLoan
                customer_capital -= monthly_payment;
                let interest_payment =
                    customer_remaining_balance * customer_submission.yearly_interest_rate / 12.0;
                let _principal_payment = monthly_payment - interest_payment;

                // NOTE: DEAD CODE DUE TO SIGN ERROR IN JUDGE CODE
                //customer_remaining_balance = (customer_remaining_balance + principal_payment)
                //    .clamp(0.0, customer_remaining_balance);

                _successful_payment_streak += 1;
                // NOTE: This is an error, profit should use old interest payment.
                let new_interest_payment =
                    customer_remaining_balance * customer_submission.yearly_interest_rate / 12.0;
                ret.total_profit += new_interest_payment;
                _budget += new_interest_payment;
            } else {
                // IncrementMark
                customer_marks += 1;
                _successful_payment_streak = 0;
                const MARKS_LIMIT: usize = 3;
                if customer_marks >= MARKS_LIMIT {
                    _is_bankrupt = true;
                    customer_happiness -= 500.0;
                } else {
                    customer_happiness -= 50.0;
                }
            }

            // Award
            if let Some(award) = customer_submission.awards[i] {
                let crate::model::Award {
                    cost,
                    base_happiness,
                } = &indata.awards[award];
                customer_happiness += base_happiness * personality.happiness_multiplier;
                _budget -= match award {
                    "NoInterestRate" => {
                        assert_eq!(*base_happiness, 500.0);
                        customer_remaining_balance * customer_submission.yearly_interest_rate / 12.0
                    }
                    "HalfInterestRate" => {
                        assert_eq!(*base_happiness, 150.0);
                        customer_remaining_balance * customer_submission.yearly_interest_rate / 24.0
                    }
                    _ => *cost,
                }
            }
        }
        ret.happiness_score += customer_happiness;
    }

    ret.total_score = ret.environmental_impact + ret.happiness_score + ret.total_profit;
    ret
}
fn compute_total_monthly_payment(yearly_rate: f64, months_to_pay_back: usize, amount: f64) -> f64 {
    let months_to_pay_back = months_to_pay_back as i32;
    let mut monthly_rate = yearly_rate / 12.0;
    if monthly_rate < 1e-5 {
        monthly_rate = 0.0001;
    }
    let interest_rate = 1.0 + monthly_rate;
    let upper = monthly_rate * interest_rate.powi(months_to_pay_back);
    let mut lower = interest_rate.powi(months_to_pay_back) - 1.0;
    if lower < 1e-5 {
        lower = 0.0001;
    }
    amount * (upper / lower)
}
