use crate::{
    api::{CustomerSubmission, InputData},
    model::{Customer, Personality},
};

pub fn blackbox_locally_optimized_submission(
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
        customer: Customer,
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
            Ok(-crate::whitebox::simulate_simplified_kernel(
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
                100,
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
                monthly_payment = crate::whitebox::compute_total_monthly_payment(
                    rate,
                    months,
                    customer.loan.amount
                )
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
