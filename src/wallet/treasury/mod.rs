// SundaeSwap treasury contract interactions
//
// Provides CLI-facing operations for:
// - fund: move ADA from the treasury script to a vendor UTxO with a milestone schedule
// - block-payment: pause/resume individual milestone payouts (Adjudicate)
// - withdraw: claim matured payouts from a vendor UTxO (Withdraw)

pub mod cli;
pub mod config;
pub mod types;
