//! AfroBIM 5D: a bill of quantities and cost estimate built from the model's quantities.
//!
//! ```text
//!  model elements --collectors--> QuantityLines --mapping rules--> bill items
//!       (one per crate)          (neutral record)  (user's bill)       |
//!                                                                      v
//!  rate library + exchange rates (user's) --------------------> priced items, sections,
//!                                                               additions, grand total
//!                                                                      |
//!                       phasing windows --> cash flow / S-curve        +--> CSV outputs
//!                       percent complete --> interim payment           +--> comparison
//! ```
//!
//! This crate contains no rates, prices, exchange rates, percentages (preliminaries,
//! contingency, VAT, retention...), productivity figures, item numbers or standard bills. Every
//! one is entered by the user or loaded from the user's file. Money and quantities are exact
//! decimals ([`Decimal`]), never `f64`. See the README for the data flow and every formula.

mod bill;
mod cashflow;
mod collect;
mod csv;
pub mod date;
pub mod decimal;
mod error;
mod estimate;
mod lines;
mod module;
mod output;
mod payment;
mod rates;

pub use bill::{
    Adjustment, AmbiguousLine, AmountRounding, Bill, Condition, Contribution, Item, ItemKind, MappingRule, Match, MeasuredBill, MeasuredItem,
    RoundingRules, Section, UnmappedLine, UnmappedReason,
};
pub use cashflow::{cash_flow_with, spread_linear, CashFlow, Distribution, MonthRow, Phasing, PhasingTarget, SCurvePoint};
pub use collect::{
    collect_all, collect_corridors, collect_drainage, collect_earthworks, collect_gantries, collect_lighting, collect_markings, collect_pavement,
    collect_restraint, collect_signs, collect_supports, CollectOptions, SIGN_CATEGORY,
};
pub use date::Date;
pub use decimal::{Decimal, Rounding, RoundingMode};
pub use error::{CostError, Result};
pub use estimate::{
    compare, Addition, AdditionBase, AdditionChange, AdditionKind, AdditionResult, ChangeStatus, Comparison, Estimate, EstimateResult, ItemChange,
    PricedItem, SectionTotal, Unpriced,
};
pub use lines::{lines_to_csv, QuantityLine, QuantitySet, SkippedSource, COLLECTED_DECIMALS, LINES_CSV_HEADER};
pub use module::{
    add_estimate, estimate_of, estimates, refresh_estimate, set_estimate, EstimateElement, EstimateModule, EstimateSummary, SectionSummary,
    ESTIMATE_CATEGORY, ESTIMATE_LAYER, SUMMARY_BAR_MAX_MM,
};
pub use output::{audit_csv, cash_flow_csv, certificate_csv, comparison_csv, estimate_summary_csv, priced_boq_csv, unmapped_csv, unpriced_boq_csv};
pub use payment::{
    certify, quantities_from_progress, Certificate, CertificateLine, Effect, GrossBasis, PaymentRule, PaymentRuleKind, PaymentState, PaymentTerms,
    RuleOutcome,
};
pub use rates::{
    BuildUp, BuildUpKind, BuildUpResult, Component, Currency, CurrencyTable, Entry, ExchangeRate, Money, PercentBase, Rate, RateLibrary, RATES_CSV_HEADER,
};
