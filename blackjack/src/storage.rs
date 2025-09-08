use near_sdk::{AccountId, NearToken};

/// Storage cost constants
pub const STORAGE_COST_PER_BYTE: u128 = 10_000_000_000_000_000_000; // 1e19 yoctoNEAR per byte

// For typical account names (20-30 chars), storage cost will be ~0.002-0.003 NEAR
pub const STORAGE_DEPOSIT_REQUIRED: u128 = 3_000_000_000_000_000_000_000; // ~0.003 NEAR minimum

/// Helper function to calculate storage cost for a UserAccount
pub fn calculate_user_storage_cost(account_id: &AccountId) -> NearToken {
    // Estimate bytes for UserAccount struct:
    let account_id_bytes = account_id.as_str().len() as u128;
    let balance_bytes = 16u128; // u128
    let last_claim_time_bytes = 8u128; // u64
    let storage_deposited_bytes = 1u128; // bool
    let total_claimed_bytes = 16u128; // u128
    let total_purchased_bytes = 16u128; // u128
    let total_burned_bytes = 16u128; // u128
    let registered_at_bytes = 8u128; // u64
    let borsh_overhead = 32u128; // Borsh serialization overhead
    let map_entry_overhead = 64u128; // UnorderedMap entry overhead
    
    let total_bytes = account_id_bytes + balance_bytes + last_claim_time_bytes + 
                     storage_deposited_bytes + total_claimed_bytes + total_purchased_bytes + 
                     total_burned_bytes + registered_at_bytes + borsh_overhead + map_entry_overhead;
    
    let cost_yocto = total_bytes * STORAGE_COST_PER_BYTE;
    
    // Add 20% safety margin for protocol changes
    let cost_with_margin = cost_yocto * 120 / 100;
    
    NearToken::from_yoctonear(cost_with_margin)
}

/// Calculate storage cost for BlackjackPlayer
pub fn calculate_blackjack_player_storage_cost(account_id: &AccountId) -> NearToken {
    // Estimate bytes for BlackjackPlayer struct:
    let account_id_bytes = account_id.as_str().len() as u128;
    let seat_number_bytes = 1u128; // u8
    let state_bytes = 4u128; // PlayerState enum
    let burned_tokens_bytes = 16u128; // u128
    let joined_at_bytes = 8u128; // u64
    let last_action_time_bytes = 8u128; // u64
    let pending_move_bytes = 8u128; // Option<PlayerMove>
    let total_burned_session_bytes = 16u128; // u128
    let rounds_played_bytes = 4u128; // u32
    let borsh_overhead = 32u128; // Borsh serialization overhead
    let vec_entry_overhead = 32u128; // Vec entry overhead
    
    let total_bytes = account_id_bytes + seat_number_bytes + state_bytes + 
                     burned_tokens_bytes + joined_at_bytes + last_action_time_bytes +
                     pending_move_bytes + total_burned_session_bytes + rounds_played_bytes +
                     borsh_overhead + vec_entry_overhead;
    
    let cost_yocto = total_bytes * STORAGE_COST_PER_BYTE;
    
    // Add 20% safety margin for protocol changes
    let cost_with_margin = cost_yocto * 120 / 100;
    
    NearToken::from_yoctonear(cost_with_margin)
}

/// Calculate storage cost for GameTable
pub fn calculate_game_table_storage_cost(table_id: &str, max_players: u8) -> NearToken {
    // Estimate bytes for GameTable struct:
    let table_id_bytes = table_id.len() as u128;
    let state_bytes = 4u128; // GameState enum
    let players_bytes = (max_players as u128) * 200u128; // Estimated bytes per player
    let current_player_index_bytes = 2u128; // Option<u8>
    let round_number_bytes = 8u128; // u64
    let created_at_bytes = 8u128; // u64
    let last_activity_bytes = 8u128; // u64
    let betting_deadline_bytes = 9u128; // Option<u64>
    let move_deadline_bytes = 9u128; // Option<u64>
    let max_players_bytes = 1u128; // u8
    let min_bet_bytes = 16u128; // u128
    let max_bet_bytes = 16u128; // u128
    let is_active_bytes = 1u128; // bool
    let borsh_overhead = 64u128; // Borsh serialization overhead
    let map_entry_overhead = 128u128; // UnorderedMap entry overhead
    
    let total_bytes = table_id_bytes + state_bytes + players_bytes + 
                     current_player_index_bytes + round_number_bytes + created_at_bytes +
                     last_activity_bytes + betting_deadline_bytes + move_deadline_bytes +
                     max_players_bytes + min_bet_bytes + max_bet_bytes + is_active_bytes +
                     borsh_overhead + map_entry_overhead;
    
    let cost_yocto = total_bytes * STORAGE_COST_PER_BYTE;
    
    // Add 30% safety margin for game complexity
    let cost_with_margin = cost_yocto * 130 / 100;
    
    NearToken::from_yoctonear(cost_with_margin)
}

/// Calculate storage cost for pending signals (bets/moves)
pub fn calculate_signals_storage_cost(table_id: &str, max_signals: u16) -> NearToken {
    // Estimate bytes for Vec<BetSignal> or Vec<MoveSignal>:
    let table_id_bytes = table_id.len() as u128;
    let signal_size_bytes = 128u128; // Estimated bytes per signal
    let signals_bytes = (max_signals as u128) * signal_size_bytes;
    let vec_overhead = 24u128; // Vec overhead
    let map_entry_overhead = 64u128; // LookupMap entry overhead
    
    let total_bytes = table_id_bytes + signals_bytes + vec_overhead + map_entry_overhead;
    
    let cost_yocto = total_bytes * STORAGE_COST_PER_BYTE;
    
    // Add 25% safety margin
    let cost_with_margin = cost_yocto * 125 / 100;
    
    NearToken::from_yoctonear(cost_with_margin)
}

/// Estimate total storage cost for creating a game table
pub fn estimate_table_creation_cost(table_id: &str, max_players: u8, max_signals_per_type: u16) -> NearToken {
    let table_cost = calculate_game_table_storage_cost(table_id, max_players);
    let bet_signals_cost = calculate_signals_storage_cost(table_id, max_signals_per_type);
    let move_signals_cost = calculate_signals_storage_cost(table_id, max_signals_per_type);
    
    let total_yocto = table_cost.as_yoctonear() + 
                     bet_signals_cost.as_yoctonear() + 
                     move_signals_cost.as_yoctonear();
    
    NearToken::from_yoctonear(total_yocto)
}

/// Check if user has sufficient storage for blackjack operations
pub fn has_sufficient_blackjack_storage(
    user_deposit: NearToken, 
    account_id: &AccountId
) -> bool {
    let user_cost = calculate_user_storage_cost(account_id);
    let player_cost = calculate_blackjack_player_storage_cost(account_id);
    let total_required = NearToken::from_yoctonear(
        user_cost.as_yoctonear() + player_cost.as_yoctonear()
    );
    
    user_deposit >= total_required
}

/// Get recommended storage deposit for full blackjack functionality
pub fn recommended_storage_deposit(account_id: &AccountId) -> NearToken {
    let user_cost = calculate_user_storage_cost(account_id);
    let player_cost = calculate_blackjack_player_storage_cost(account_id);
    
    // Add extra buffer for potential future features
    let total_yocto = (user_cost.as_yoctonear() + player_cost.as_yoctonear()) * 150 / 100;
    
    NearToken::from_yoctonear(total_yocto)
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::accounts;

    #[test]
    fn test_storage_calculations() {
        let account = accounts(1);
        let user_cost = calculate_user_storage_cost(&account);
        let player_cost = calculate_blackjack_player_storage_cost(&account);
        
        // Basic sanity checks
        assert!(user_cost.as_yoctonear() > 0);
        assert!(player_cost.as_yoctonear() > 0);
        assert!(player_cost.as_yoctonear() >= user_cost.as_yoctonear() / 2); // Player should be substantial
        
        println!("User storage cost: {} NEAR", user_cost.as_near());
        println!("Player storage cost: {} NEAR", player_cost.as_near());
    }

    #[test]
    fn test_table_storage_calculation() {
        let table_cost = calculate_game_table_storage_cost("table-123", 3);
        let signals_cost = calculate_signals_storage_cost("table-123", 50);
        
        assert!(table_cost.as_yoctonear() > 0);
        assert!(signals_cost.as_yoctonear() > 0);
        
        println!("Table storage cost: {} NEAR", table_cost.as_near());
        println!("Signals storage cost: {} NEAR", signals_cost.as_near());
    }

    #[test]
    fn test_storage_sufficiency() {
        let account = accounts(1);
        let recommended = recommended_storage_deposit(&account);
        
        assert!(has_sufficient_blackjack_storage(recommended, &account));
        
        let insufficient = NearToken::from_yoctonear(recommended.as_yoctonear() / 2);
        assert!(!has_sufficient_blackjack_storage(insufficient, &account));
        
        println!("Recommended storage: {} NEAR", recommended.as_near());
    }
}