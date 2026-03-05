// Plutus cost models for script execution
//
// These are the standard cost models used on mainnet and testnets.
// Cost models define the execution costs (in ExUnits) for Plutus operations.

use crate::wallet::plutus::PlutusVersion;

/// Get the default cost model for a given Plutus version
///
/// These are the cost models currently used on mainnet (as of Conway era).
pub fn default_cost_model(version: PlutusVersion) -> Vec<i64> {
    match version {
        PlutusVersion::V1 => plutus_v1_cost_model(),
        PlutusVersion::V2 => plutus_v2_cost_model(),
        PlutusVersion::V3 => plutus_v3_cost_model(),
    }
}

/// Plutus V1 cost model
///
/// Source: Cardano mainnet protocol parameters
pub fn plutus_v1_cost_model() -> Vec<i64> {
    vec![
        100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4,
        16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100,
        16000, 100, 94375, 32, 132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769,
        4, 2, 85848, 228465, 122, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148,
        27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32,
        76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541,
        1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32, 85848, 228465, 122,
        0, 1, 1, 90434, 519, 0, 1, 74433, 32, 85848, 228465, 122, 0, 1, 1, 85848, 228465, 122,
        0, 1, 1, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0, 141992, 32, 100788, 420,
        1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933, 32, 24623, 32,
        53384111, 14333, 10,
    ]
}

/// Plutus V2 cost model
///
/// Source: Cardano mainnet protocol parameters
pub fn plutus_v2_cost_model() -> Vec<i64> {
    vec![
        100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4,
        16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100,
        16000, 100, 94375, 32, 132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769,
        4, 2, 85848, 228465, 122, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148,
        27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32,
        76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541,
        1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32, 85848, 228465, 122,
        0, 1, 1, 90434, 519, 0, 1, 74433, 32, 85848, 228465, 122, 0, 1, 1, 85848, 228465, 122,
        0, 1, 1, 955506, 213312, 0, 2, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0,
        141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32,
        25933, 32, 24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10,
    ]
}

/// Plutus V3 cost model
///
/// Note: V3 is still in development. This is a placeholder using V2 costs.
/// Update this when V3 cost model is finalized.
pub fn plutus_v3_cost_model() -> Vec<i64> {
    // For now, use V2 cost model as a conservative estimate
    // This should be updated once V3 cost model is officially defined
    plutus_v2_cost_model()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plutus_v1_cost_model_length() {
        let model = plutus_v1_cost_model();
        assert_eq!(model.len(), 166);
    }

    #[test]
    fn test_plutus_v2_cost_model_length() {
        let model = plutus_v2_cost_model();
        assert_eq!(model.len(), 175);
    }

    #[test]
    fn test_default_cost_model_v1() {
        let model = default_cost_model(PlutusVersion::V1);
        assert_eq!(model.len(), 166);
    }

    #[test]
    fn test_default_cost_model_v2() {
        let model = default_cost_model(PlutusVersion::V2);
        assert_eq!(model.len(), 175);
    }

    #[test]
    fn test_default_cost_model_v3() {
        let model = default_cost_model(PlutusVersion::V3);
        // V3 uses V2 for now
        assert_eq!(model.len(), 175);
    }

    #[test]
    fn test_cost_models_not_empty() {
        assert!(!plutus_v1_cost_model().is_empty());
        assert!(!plutus_v2_cost_model().is_empty());
        assert!(!plutus_v3_cost_model().is_empty());
    }
}
