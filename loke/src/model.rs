#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Map {
    pub name: &'static str,
    pub budget: f64,
    pub game_length_in_months: usize,
    pub customers: Vec<Customer>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Customer {
    pub name: &'static str,
    pub loan: Loan,
    pub personality: String,
    pub capital: f64,
    pub income: f64,
    pub monthly_expenses: f64,
    pub number_of_kids: f64,
    #[serde(alias = "mortgage")]
    pub home_mortgage: f64,
    #[serde(alias = "hasStudentLoans")]
    pub has_student_loan: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Loan {
    pub product: String,
    pub environmental_impact: f64,
    pub amount: f64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[serde(deny_unknown_fields)]
pub struct Personalities {
    pub personalities: BTreeMap<String, Personality>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Personality {
    pub happiness_multiplier: f64,
    pub accepted_min_interest: f64,
    pub accepted_max_interest: f64,
    pub living_standard_multiplier: f64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[serde(deny_unknown_fields)]
pub struct Awards {
    pub awards: BTreeMap<String, Award>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Award {
    pub cost: f64,
    pub base_happiness: f64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub customer_name: &'static str,
    pub months_to_pay_back_loan: usize,
    pub yearly_interest_rate: f64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
#[serde(deny_unknown_fields)]
pub struct Action {
    #[serde(rename = "Type")]
    pub type_: &'static str,
    pub award: &'static str,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub map_name: &'static str,
    pub proposals: Vec<Proposal>,
    pub iterations: Vec<BTreeMap<&'static str, Action>>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub achievements_unlocked: Vec<serde_json::Value>,
    pub game_id: String,
    pub message: (),
    pub score: Score,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Score {
    pub map_name: String,

    pub environmental_impact: f64,
    pub happiness_score: f64,
    pub total_profit: f64,

    /// Sum of the 3 sub-scores
    pub total_score: f64,
}
