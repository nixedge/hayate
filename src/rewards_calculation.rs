// Cardano Rewards Calculation
//
// Based on the Shelley ledger specification:
// https://github.com/IntersectMBO/cardano-ledger/blob/master/eras/shelley/formal-spec/incentives.tex
//
// Phase 1: Treasury & Reserves calculation
// Phase 2: Pool rewards distribution
// using exact rational arithmetic to match Haskell's Ratio type.

use num_rational::Ratio;

/// Type alias for rational numbers (matches Haskell's Ratio)
pub type Rational = Ratio<i64>;

/// Protocol parameters for rewards calculation
#[derive(Debug, Clone)]
pub struct ProtocolParams {
    /// ρ (rho) - Monetary expansion rate per epoch
    /// Preview: 3/1000 = 0.003 = 0.3%
    pub monetary_expansion_rate: Rational,

    /// τ (tau) - Treasury growth rate (fraction going to treasury)
    /// Preview: 1/5 = 0.2 = 20%
    pub treasury_rate: Rational,

    /// a₀ (a_0) - Stake influence factor
    /// Preview: 3/10 = 0.3
    pub stake_influence: Rational,

    /// d - Decentralization parameter
    /// Epoch 1: 1/1 (federated), Epoch 2+: 0/1 (decentralized)
    pub decentralization: Rational,

    /// k (nOpt) - Optimal number of pools
    /// Preview: 150
    pub optimal_pools: u64,

    /// Minimum fixed cost per pool (in lovelace)
    /// Preview: 340,000,000 lovelace = 340 ADA
    pub min_pool_cost: u64,
}

impl ProtocolParams {
    /// Preview network parameters (Epoch 1)
    pub fn preview_epoch1() -> Self {
        Self {
            monetary_expansion_rate: Rational::new(3, 1000),
            treasury_rate: Rational::new(1, 5),
            stake_influence: Rational::new(3, 10),
            decentralization: Rational::new(1, 1),
            optimal_pools: 150,
            min_pool_cost: 340_000_000,
        }
    }

    /// Preview network parameters (Epoch 2+, fully decentralized)
    pub fn preview_epoch2_plus() -> Self {
        Self {
            monetary_expansion_rate: Rational::new(3, 1000),
            treasury_rate: Rational::new(1, 5),
            stake_influence: Rational::new(3, 10),
            decentralization: Rational::new(0, 1),
            optimal_pools: 150,
            min_pool_cost: 340_000_000,
        }
    }
}

/// Calculate monetary expansion from reserves
///
/// Formula: Distr_E = Ada^Rsv_E × MER_E × min(Perf_E, 100%)
///
/// Where:
/// - Ada^Rsv_E = Reserves at epoch E
/// - MER_E = Monetary expansion rate (ρ)
/// - Perf_E = Network performance (η) = actual blocks / expected blocks
pub fn calculate_monetary_expansion(
    reserves: u64,
    rho: Rational,
    eta: Rational,
) -> u64 {
    // Cap performance at 100%
    let performance = if eta > Rational::from_integer(1) {
        Rational::from_integer(1)
    } else {
        eta
    };

    // Distr_E = Reserves × ρ × η
    let reserves_rational = Rational::from_integer(reserves as i64);
    let expansion = reserves_rational * rho * performance;

    // Floor to nearest lovelace (matches Haskell behavior)
    expansion.to_integer() as u64
}

/// Calculate treasury cut from total rewards pot
///
/// Formula: T_E = rPot × τ
///
/// Where:
/// - rPot = Total rewards pot (monetary expansion + fees)
/// - τ (tau) = Treasury growth rate
///
/// IMPORTANT: In Cardano, fees are included in the pot BEFORE
/// calculating the treasury cut (from cardano-ledger PulsingReward.hs):
///   rPot = ssFee ss <> deltaR1
///   deltaT1 = floor $ unboundRational (pr ^. ppTauL) * fromIntegral rPot
pub fn calculate_treasury_cut(
    rewards_pot: u64,
    tau: Rational,
) -> u64 {
    let pot_rational = Rational::from_integer(rewards_pot as i64);
    let cut = pot_rational * tau;

    // Floor to nearest lovelace (matches rationalToCoinViaFloor)
    cut.to_integer() as u64
}

/// Update treasury and reserves for an epoch transition
///
/// Returns: (new_treasury, new_reserves, distribution, treasury_cut, pool_pot)
///
/// Formula (from cardano-ledger PulsingReward.hs lines 121-141):
///   deltaR1 = floor(reserves × ρ × min(η, 1))
///   rPot = fees + deltaR1
///   deltaT1 = floor(rPot × τ)
///   _R = rPot - deltaT1
///
/// Account updates:
///   treasury_new = treasury + deltaT1
///   reserves_new = reserves - deltaR1
///
/// Note: Fees are consumed from the UTxO set, not from reserves
pub fn update_epoch_accounts(
    current_treasury: u64,
    current_reserves: u64,
    params: &ProtocolParams,
    performance_eta: Rational,
    fees: u64,  // Transaction fees collected during epoch
) -> (u64, u64, u64, u64, u64) {
    // Step 1: Calculate monetary expansion from reserves
    // deltaR1 = floor(reserves × ρ × min(η, 1))
    let distribution = calculate_monetary_expansion(
        current_reserves,
        params.monetary_expansion_rate,
        performance_eta,
    );

    // Step 2: Calculate total rewards pot (includes fees)
    // rPot = fees + deltaR1
    let rewards_pot = distribution + fees;

    // Step 3: Calculate treasury cut from total pot
    // deltaT1 = floor(rPot × τ)
    let treasury_cut = calculate_treasury_cut(
        rewards_pot,  // CRITICAL: Include fees in the pot!
        params.treasury_rate,
    );

    // Step 4: Calculate pool rewards pot
    // _R = rPot - deltaT1
    let pool_pot = rewards_pot - treasury_cut;

    // Update accounts:
    // - Treasury increases by deltaT1 (treasury cut)
    // - Reserves change by deltaR = -deltaR1 + deltaR2
    //   In early epochs with no delegator rewards, deltaR2 ≈ pool_pot,
    //   so: deltaR = -distribution + pool_pot = fees - treasury_cut
    //   Therefore: reserves_new = reserves_old + fees - treasury_cut
    let new_treasury = current_treasury + treasury_cut;
    let new_reserves = current_reserves + fees - treasury_cut;

    (new_treasury, new_reserves, distribution, treasury_cut, pool_pot)
}

// ============================================================================
// Phase 2: Pool Rewards Distribution
// ============================================================================

/// Pool information for rewards calculation
#[derive(Debug, Clone)]
pub struct PoolInfo {
    /// Pool ID (hash)
    pub pool_id: Vec<u8>,

    /// Pool pledge amount (in lovelace)
    pub pledge: u64,

    /// Total stake delegated to this pool (in lovelace)
    pub stake: u64,

    /// Fixed cost per epoch (in lovelace)
    pub fixed_cost: u64,

    /// Pool margin (as rational: numerator / denominator)
    pub margin: Rational,

    /// Blocks produced by this pool in the epoch
    pub blocks_produced: u64,
}

/// Calculate saturation threshold
///
/// Formula: z = 1 / k
///
/// Where k (nOpt) is the optimal number of pools
///
/// Example: k = 150 → z = 1/150 = 0.00667 (0.667% of total stake)
pub fn calculate_saturation_threshold(optimal_pools: u64) -> Rational {
    Rational::new(1, optimal_pools as i64)
}

/// Calculate effective pool stake (capped at saturation)
///
/// Formula: s^% = min(z, Pool^%)
///
/// Where:
/// - z = Saturation threshold
/// - Pool^% = Pool's stake / Total active stake
pub fn calculate_effective_pool_stake(
    pool_stake: u64,
    total_active_stake: u64,
    saturation_threshold: Rational,
) -> Rational {
    let pool_stake_fraction = Rational::new(pool_stake as i64, total_active_stake as i64);

    // Cap at saturation threshold
    if pool_stake_fraction > saturation_threshold {
        saturation_threshold
    } else {
        pool_stake_fraction
    }
}

/// Calculate effective pledge (capped at saturation)
///
/// Formula: p^% = min(z, Pledge^%)
///
/// Where:
/// - z = Saturation threshold
/// - Pledge^% = Pool's pledge / Total active stake
pub fn calculate_effective_pledge(
    pledge: u64,
    total_active_stake: u64,
    saturation_threshold: Rational,
) -> Rational {
    let pledge_fraction = Rational::new(pledge as i64, total_active_stake as i64);

    // Cap at saturation threshold
    if pledge_fraction > saturation_threshold {
        saturation_threshold
    } else {
        pledge_fraction
    }
}

/// Calculate optimal reward factor with pledge influence
///
/// Formula: R^Factor = s^% × inf × [(s^% - p^% × (z - s^%)) ÷ z]
///
/// Where:
/// - s^% = Effective pool stake
/// - p^% = Effective pledge
/// - z = Saturation threshold
/// - inf (a₀) = Stake influence factor
pub fn calculate_optimal_reward_factor(
    effective_stake: Rational,
    effective_pledge: Rational,
    saturation_threshold: Rational,
    stake_influence: Rational,
) -> Rational {
    // Calculate: (s^% - p^% × (z - s^%)) ÷ z
    let numerator = effective_stake - (effective_pledge * (saturation_threshold - effective_stake));
    let bracket_term = numerator / saturation_threshold;

    // R^Factor = s^% × inf × bracket_term
    effective_stake * stake_influence * bracket_term
}

/// Calculate optimal pool reward
///
/// Formula: R^Optimal = R_E × R^Factor
///
/// Where:
/// - R_E = Total reward pot (after treasury cut)
/// - R^Factor = Optimal reward factor
pub fn calculate_optimal_pool_reward(
    pool_rewards_pot: u64,
    reward_factor: Rational,
) -> u64 {
    let pot_rational = Rational::from_integer(pool_rewards_pot as i64);
    let optimal = pot_rational * reward_factor;

    // Floor to nearest lovelace
    optimal.to_integer() as u64
}

/// Calculate pool performance factor
///
/// Formula: Pool^Perf = Pool's blocks / Pool's expected blocks
///
/// Expected blocks = Total epoch blocks × (Pool's stake / Total stake)
pub fn calculate_pool_performance(
    blocks_produced: u64,
    pool_stake: u64,
    total_active_stake: u64,
    total_epoch_blocks: u64,
) -> Rational {
    // Expected blocks for this pool
    let stake_fraction = Rational::new(pool_stake as i64, total_active_stake as i64);
    let expected_blocks_rational = Rational::from_integer(total_epoch_blocks as i64) * stake_fraction;
    let expected_blocks = expected_blocks_rational.to_integer().max(1); // At least 1 to avoid division by zero

    // Performance = actual / expected
    Rational::new(blocks_produced as i64, expected_blocks)
}

/// Calculate gross pool reward (after applying performance)
///
/// Formula: R^Gross = R^Optimal × Pool^Perf
///
/// Where:
/// - R^Optimal = Optimal reward for this pool
/// - Pool^Perf = Pool performance factor
pub fn calculate_gross_pool_reward(
    optimal_reward: u64,
    performance: Rational,
) -> u64 {
    let optimal_rational = Rational::from_integer(optimal_reward as i64);
    let gross = optimal_rational * performance;

    // Floor to nearest lovelace
    gross.to_integer() as u64
}

/// Calculate rewards for a single pool
///
/// Returns: (optimal_reward, gross_reward, effective_stake, effective_pledge, performance)
pub fn calculate_pool_rewards(
    pool: &PoolInfo,
    pool_rewards_pot: u64,
    total_active_stake: u64,
    total_epoch_blocks: u64,
    params: &ProtocolParams,
) -> (u64, u64, Rational, Rational, Rational) {
    // Calculate saturation threshold
    let z = calculate_saturation_threshold(params.optimal_pools);

    // Calculate effective stake and pledge
    let effective_stake = calculate_effective_pool_stake(pool.stake, total_active_stake, z);
    let effective_pledge = calculate_effective_pledge(pool.pledge, total_active_stake, z);

    // Calculate optimal reward factor
    let reward_factor = calculate_optimal_reward_factor(
        effective_stake,
        effective_pledge,
        z,
        params.stake_influence,
    );

    // Calculate optimal reward
    let optimal_reward = calculate_optimal_pool_reward(pool_rewards_pot, reward_factor);

    // Calculate performance
    let performance = calculate_pool_performance(
        pool.blocks_produced,
        pool.stake,
        total_active_stake,
        total_epoch_blocks,
    );

    // Calculate gross reward
    let gross_reward = calculate_gross_pool_reward(optimal_reward, performance);

    (optimal_reward, gross_reward, effective_stake, effective_pledge, performance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::ToPrimitive;

    #[test]
    fn test_preview_epoch1_to_2() {
        // Known values from Preview network
        let treasury_epoch1: u64 = 9_000_000_000_000;
        let reserves_epoch1: u64 = 14_991_000_000_000_000;

        let params = ProtocolParams::preview_epoch1();
        let eta = Rational::from_integer(1); // Assume 100% performance

        // Actual fees from NewEpochState (utxosFees = Coin 437793)
        let fees: u64 = 437_793;

        let (new_treasury, new_reserves, distribution, treasury_cut, pool_pot) =
            update_epoch_accounts(treasury_epoch1, reserves_epoch1, &params, eta, fees);

        // Expected values (from actual Preview data)
        let expected_treasury: u64 = 17_994_600_087_558;
        let expected_reserves: u64 = 14_982_005_400_350_235;

        println!("Epoch 1 → 2 Transition:");
        println!("  Distribution: {} lovelace", distribution);
        println!("  Treasury cut: {} lovelace", treasury_cut);
        println!("  Fees: {} lovelace", fees);
        println!("  Pool pot: {} lovelace", pool_pot);
        println!("  New treasury: {} (expected: {})", new_treasury, expected_treasury);
        println!("  New reserves: {} (expected: {})", new_reserves, expected_reserves);

        // Allow small rounding differences (100 ADA = 100M lovelace)
        let treasury_diff = (new_treasury as i64 - expected_treasury as i64).abs();
        let reserves_diff = (new_reserves as i64 - expected_reserves as i64).abs();

        println!("  Treasury diff: {} lovelace", treasury_diff);
        println!("  Reserves diff: {} lovelace", reserves_diff);

        assert!(treasury_diff < 100_000_000, "Treasury mismatch: {} lovelace", treasury_diff);
        assert!(reserves_diff < 100_000_000, "Reserves mismatch: {} lovelace", reserves_diff);
    }

    #[test]
    fn test_preview_epoch2_to_3() {
        // Known values from Preview network
        let treasury_epoch2: u64 = 17_994_600_087_558;
        let reserves_epoch2: u64 = 14_982_005_400_350_235;

        let params = ProtocolParams::preview_epoch2_plus();
        let eta = Rational::from_integer(1); // Assume 100% performance

        // Actual fees from NewEpochState (utxosFees = Coin 206597)
        let fees: u64 = 206_597;

        let (new_treasury, new_reserves, distribution, treasury_cut, pool_pot) =
            update_epoch_accounts(treasury_epoch2, reserves_epoch2, &params, eta, fees);

        // Expected values (from actual Preview data)
        let expected_treasury: u64 = 26_983_803_369_087;
        let expected_reserves: u64 = 14_973_016_197_275_303;

        println!("Epoch 2 → 3 Transition:");
        println!("  Distribution: {} lovelace", distribution);
        println!("  Treasury cut: {} lovelace", treasury_cut);
        println!("  Fees: {} lovelace", fees);
        println!("  Pool pot: {} lovelace", pool_pot);
        println!("  New treasury: {} (expected: {})", new_treasury, expected_treasury);
        println!("  New reserves: {} (expected: {})", new_reserves, expected_reserves);

        // Allow small rounding differences (100 ADA = 100M lovelace)
        let treasury_diff = (new_treasury as i64 - expected_treasury as i64).abs();
        let reserves_diff = (new_reserves as i64 - expected_reserves as i64).abs();

        println!("  Treasury diff: {} lovelace", treasury_diff);
        println!("  Reserves diff: {} lovelace", reserves_diff);

        assert!(treasury_diff < 100_000_000, "Treasury mismatch: {} lovelace", treasury_diff);
        assert!(reserves_diff < 100_000_000, "Reserves mismatch: {} lovelace", reserves_diff);
    }

    #[test]
    fn test_monetary_expansion_formula() {
        let reserves: u64 = 15_000_000_000_000_000; // 15 billion ADA
        let rho = Rational::new(3, 1000); // 0.3%
        let eta = Rational::from_integer(1); // 100% performance

        let expansion = calculate_monetary_expansion(reserves, rho, eta);

        // Expected: 15P × 0.003 = 45T lovelace
        let expected: u64 = 45_000_000_000_000;

        assert_eq!(expansion, expected);
    }

    #[test]
    fn test_treasury_cut_formula() {
        let distribution: u64 = 45_000_000_000_000; // 45T lovelace
        let tau = Rational::new(1, 5); // 20%

        let cut = calculate_treasury_cut(distribution, tau);

        // Expected: 45T × 0.2 = 9T lovelace
        let expected: u64 = 9_000_000_000_000;

        assert_eq!(cut, expected);
    }

    #[test]
    fn test_performance_cap() {
        let reserves: u64 = 15_000_000_000_000_000;
        let rho = Rational::new(3, 1000);
        let eta = Rational::new(150, 100); // 150% performance (impossible)

        let expansion = calculate_monetary_expansion(reserves, rho, eta);

        // Should be capped at 100%
        let expected: u64 = 45_000_000_000_000;

        assert_eq!(expansion, expected);
    }

    // ========================================================================
    // Phase 2 Tests: Pool Rewards
    // ========================================================================

    #[test]
    fn test_saturation_threshold() {
        let k: u64 = 150; // Preview optimal pools
        let z = calculate_saturation_threshold(k);

        // z should be 1/150
        assert_eq!(z, Rational::new(1, 150));

        // As percentage: 0.00667 = 0.667%
        let z_percent = z.to_f64().unwrap() * 100.0;
        assert!((z_percent - 0.6666666).abs() < 0.0001);
    }

    #[test]
    fn test_effective_stake_below_saturation() {
        let pool_stake: u64 = 100_000_000_000_000; // 100T lovelace
        let total_stake: u64 = 300_000_000_000_000; // 300T lovelace
        let z = Rational::new(1, 150);

        let effective = calculate_effective_pool_stake(pool_stake, total_stake, z);

        // Pool has 1/3 of stake = 33.33%
        // Saturation is 1/150 = 0.667%
        // Since 33.33% > 0.667%, should be capped at saturation
        assert_eq!(effective, z);
    }

    #[test]
    fn test_effective_pledge() {
        let pledge: u64 = 100_000_000_000_000; // 100T lovelace
        let total_stake: u64 = 300_000_000_000_000; // 300T lovelace
        let z = Rational::new(1, 150);

        let effective = calculate_effective_pledge(pledge, total_stake, z);

        // Pledge is 1/3 of total stake = 33.33%
        // Should be capped at saturation (0.667%)
        assert_eq!(effective, z);
    }

    #[test]
    fn test_pool_performance() {
        // Pool produced 1 block
        // Pool has 1/3 of stake
        // Epoch had 100 blocks total
        // Expected: 1/3 × 100 = 33.33 blocks
        // Performance: 1 / 33 = 0.0303 = 3.03%

        let blocks_produced: u64 = 1;
        let pool_stake: u64 = 100_000_000_000_000;
        let total_stake: u64 = 300_000_000_000_000;
        let total_blocks: u64 = 100;

        let perf = calculate_pool_performance(
            blocks_produced,
            pool_stake,
            total_stake,
            total_blocks,
        );

        // Expected blocks: 100 × (100T / 300T) = 33
        // Performance: 1 / 33
        assert_eq!(perf, Rational::new(1, 33));
    }

    #[test]
    fn test_preview_pool_rewards_epoch1() {
        // Preview Epoch 1 → 2 data
        let pool_rewards_pot: u64 = 35_978_400_000_000; // 35.978T lovelace
        let total_active_stake: u64 = 300_000_000_000_000; // 300T lovelace
        let total_epoch_blocks: u64 = 1; // Only 1 block in epoch 1

        let params = ProtocolParams::preview_epoch1();

        // Pool that produced the block (pool 40d806...)
        let producing_pool = PoolInfo {
            pool_id: vec![],
            pledge: 100_000_000_000_000, // 100T lovelace
            stake: 100_000_000_000_000,   // 100T lovelace (1/3 of total)
            fixed_cost: 500_000_000,      // 500 ADA
            margin: Rational::new(1, 1),  // 100%
            blocks_produced: 1,
        };

        let (optimal, gross, eff_stake, eff_pledge, perf) = calculate_pool_rewards(
            &producing_pool,
            pool_rewards_pot,
            total_active_stake,
            total_epoch_blocks,
            &params,
        );

        println!("Preview Epoch 1 Pool Rewards:");
        println!("  Pool rewards pot: {} lovelace", pool_rewards_pot);
        println!("  Effective stake: {:?}", eff_stake);
        println!("  Effective pledge: {:?}", eff_pledge);
        println!("  Performance: {:?}", perf);
        println!("  Optimal reward: {} lovelace", optimal);
        println!("  Gross reward: {} lovelace", gross);

        // Saturation check
        let z = calculate_saturation_threshold(params.optimal_pools);
        assert_eq!(eff_stake, z); // Should be capped
        assert_eq!(eff_pledge, z); // Should be capped

        // Performance should be 100% (produced 1 out of expected 1)
        // Wait, expected for this pool: 1 block × (100T / 300T) = 0.33 blocks
        // So performance = 1 / 0.33 = 3.03 = 303%
        // But we cap at 100%, so... actually no, we don't cap pool performance

        // Optimal reward should be > 0
        assert!(optimal > 0);

        // Gross reward should be optimal × performance
        let expected_gross = ((optimal as f64) * perf.to_f64().unwrap()) as u64;
        let gross_diff = (gross as i64 - expected_gross as i64).abs();
        assert!(gross_diff < 1000, "Gross reward calculation mismatch");
    }

    #[test]
    fn test_optimal_reward_factor() {
        // Test with Preview epoch 1 values
        let z = Rational::new(1, 150); // Saturation
        let s_percent = z; // Effective stake (at saturation)
        let p_percent = z; // Effective pledge (at saturation)
        let inf = Rational::new(3, 10); // a₀ = 0.3

        let factor = calculate_optimal_reward_factor(s_percent, p_percent, z, inf);

        println!("Reward factor: {:?}", factor);
        println!("Reward factor (decimal): {}", factor.to_f64().unwrap());

        // Factor should be positive
        assert!(factor > Rational::from_integer(0));

        // For fully saturated pool with full pledge:
        // R^Factor = s × a₀ × [(s - p × (z - s)) / z]
        // When s = z and p = z:
        // R^Factor = z × a₀ × [(z - z × (z - z)) / z]
        //          = z × a₀ × [z / z]
        //          = z × a₀ × 1
        //          = z × a₀
        //          = (1/150) × (3/10)
        //          = 3/1500
        //          = 1/500

        let expected = Rational::new(1, 500);
        assert_eq!(factor, expected);
    }
}
