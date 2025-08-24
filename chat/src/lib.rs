use near_sdk::json_types::{U64, U128};
use near_sdk::store::{LookupMap, Vector, IterableSet}; 
use near_sdk::{env, near, AccountId, NearToken, require}; 
use near_sdk::Promise;

// Storage cost calculation based on actual bytes used
// NEAR storage staking: 1E19 yoctoNEAR per byte (100KB per 1 NEAR)
const STORAGE_COST_PER_BYTE: u128 = 10_000_000_000_000_000_000; // 1E19 yoctoNEAR

// Helper function to calculate storage cost for a message
fn calculate_storage_cost(account_id: &AccountId, message: &String) -> NearToken {
    // Estimate bytes for this specific message:
    let account_id_bytes = account_id.as_str().len() as u128;
    let message_bytes = message.len() as u128;
    let timestamp_bytes = 8u128; // U64
    let storage_paid_bytes = 32u128; // U128 as string
    let struct_overhead = 50u128; // Borsh serialization + Vector overhead
    
    let total_bytes = account_id_bytes + message_bytes + timestamp_bytes + storage_paid_bytes + struct_overhead;
    let cost_yocto = total_bytes * STORAGE_COST_PER_BYTE;
    
    // Add 20% safety margin for protocol changes and indexing overhead
    let cost_with_margin = cost_yocto * 120 / 100;
    
    // Return actual calculated cost
    NearToken::from_yoctonear(cost_with_margin)
}
