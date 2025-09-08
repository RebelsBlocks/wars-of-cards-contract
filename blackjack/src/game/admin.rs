use near_sdk::{env, log};
use crate::{CardsContract, events::{emit_event, log_error}};
use super::types::*;
use super::table::remove_table;

// ========================================
// ADMIN FUNCTIONS (Backend/Owner Only)
// ========================================

/// Advance game state (called by backend after processing)
pub fn advance_game_state(
    contract: &mut CardsContract, 
    table_id: String, 
    new_state: GameState
) -> bool {
    let timestamp = env::block_timestamp();
    
    // Get table
    let mut table = match contract.game_tables.get(&table_id) {
        Some(t) => t,
        None => {
            log_error("Table not found", &table_id, None);
            return false;
        }
    };

    let old_state = table.state.clone();
    table.state = new_state.clone();
    table.last_activity = timestamp;

    // Handle state-specific logic
    match new_state {
        GameState::Betting => {
            // Reset all players for new round
            for player in &mut table.players {
                player.burned_tokens = 0;
                player.pending_move = None;
                player.last_action_time = timestamp;
                
                // Activate observing players
                if player.state == PlayerState::Observing || 
                   player.state == PlayerState::WaitingForNextRound {
                    player.state = PlayerState::Active;
                }
            }
            
            // Set betting deadline
            table.betting_deadline = Some(
                timestamp + (contract.game_config.betting_timeout_ms * 1_000_000)
            );
            table.move_deadline = None;
            table.current_player_index = None;
            
            log!("Round {} started at table {}", table.round_number + 1, table_id);
        }

        GameState::PlayerTurn => {
            // Find first active player with bet
            let first_player_index = table.players.iter()
                .enumerate()
                .filter(|(_, p)| p.state == PlayerState::Active && p.burned_tokens > 0)
                .min_by_key(|(_, p)| p.seat_number)
                .map(|(i, _)| i as u8);

            table.current_player_index = first_player_index;
            table.betting_deadline = None;
            
            if first_player_index.is_some() {
                table.move_deadline = Some(
                    timestamp + (contract.game_config.move_timeout_ms * 1_000_000)
                );
            }
        }

        GameState::DealerTurn => {
            // Clear player-specific timers
            table.current_player_index = None;
            table.betting_deadline = None;
            table.move_deadline = None;
        }

        GameState::RoundEnded => {
            // Round completed
            table.round_number += 1;
            table.current_player_index = None;
            table.betting_deadline = None;
            table.move_deadline = None;
            
            // Update stats
            contract.blackjack_stats.total_games_played += 1;
            
            log!("Round {} completed at table {}", table.round_number, table_id);
        }

        GameState::WaitingForPlayers => {
            // Reset table for new players
            table.current_player_index = None;
            table.betting_deadline = None;
            table.move_deadline = None;
        }

        _ => {}
    }

    // Save updated table
    contract.game_tables.insert(&table_id, &table);

    // Emit state change event
    emit_event(BlackjackEvent::GameStateChanged {
        table_id: table_id.clone(),
        old_state,
        new_state,
        timestamp,
    });

    log!("Table {} state advanced from {:?} to {:?}", 
        table_id, old_state, new_state);

    true
}

/// Clear processed signals (called by backend after polling)
pub fn clear_signals(
    contract: &mut CardsContract, 
    table_id: String, 
    bet_count: u8, 
    move_count: u8
) {
    // Clear processed bet signals
    if bet_count > 0 {
        if let Some(mut pending_bets) = contract.pending_bets.get(&table_id) {
            if bet_count as usize >= pending_bets.len() {
                pending_bets.clear();
            } else {
                pending_bets.drain(0..bet_count as usize);
            }
            contract.pending_bets.insert(&table_id, &pending_bets);
        }
    }

    // Clear processed move signals
    if move_count > 0 {
        if let Some(mut pending_moves) = contract.pending_moves.get(&table_id) {
            if move_count as usize >= pending_moves.len() {
                pending_moves.clear();
            } else {
                pending_moves.drain(0..move_count as usize);
            }
            contract.pending_moves.insert(&table_id, &pending_moves);
        }
    }

    log!("Cleared {} bet signals and {} move signals from table {}", 
        bet_count, move_count, table_id);
}

/// Force advance to next player (admin override)
pub fn force_next_player(
    contract: &mut CardsContract, 
    table_id: String, 
    reason: String
) -> bool {
    let mut table = match contract.game_tables.get(&table_id) {
        Some(t) => t,
        None => return false,
    };

    if table.state != GameState::PlayerTurn {
        return false;
    }

    // Find next active player
    let current_index = table.current_player_index.unwrap_or(0) as usize;
    let mut next_index = None;

    for i in 1..table.players.len() {
        let check_index = (current_index + i) % table.players.len();
        let player = &table.players[check_index];
        
        if player.state == PlayerState::Active && player.burned_tokens > 0 {
            next_index = Some(check_index as u8);
            break;
        }
    }

    match next_index {
        Some(index) => {
            table.current_player_index = Some(index);
            table.move_deadline = Some(
                env::block_timestamp() + (contract.game_config.move_timeout_ms * 1_000_000)
            );
            
            contract.game_tables.insert(&table_id, &table);
            
            log!("Forced next player at table {}: {} (reason: {})", 
                table_id, table.players[index as usize].account_id, reason);
            
            true
        }
        None => {
            // No more players - advance to dealer turn
            advance_game_state(contract, table_id, GameState::DealerTurn)
        }
    }
}

/// Set specific player as current (admin override)
pub fn set_current_player_admin(
    contract: &mut CardsContract, 
    table_id: String, 
    player_account: near_sdk::AccountId
) -> bool {
    let mut table = match contract.game_tables.get(&table_id) {
        Some(t) => t,
        None => return false,
    };

    // Find player
    let player_index = table.players.iter()
        .position(|p| p.account_id == player_account);

    if let Some(index) = player_index {
        table.current_player_index = Some(index as u8);
        table.move_deadline = Some(
            env::block_timestamp() + (contract.game_config.move_timeout_ms * 1_000_000)
        );
        table.last_activity = env::block_timestamp();
        
        contract.game_tables.insert(&table_id, &table);
        
        log!("Admin set current player to {} at table {}", player_account, table_id);
        return true;
    }

    false
}

/// Close table (emergency admin function)
pub fn close_table(contract: &mut CardsContract, table_id: String, reason: String) {
    // Refund all active bets before closing
    if let Some(table) = contract.game_tables.get(&table_id) {
        for player in &table.players {
            if player.burned_tokens > 0 {
                // Refund by minting tokens back
                if let Some(mut user_account) = contract.accounts.get(&player.account_id) {
                    user_account.balance += player.burned_tokens;
                    contract.accounts.insert(&player.account_id, &user_account);
                    
                    // Update contract stats
                    contract.total_supply += player.burned_tokens;
                    contract.blackjack_stats.total_tokens_burned_betting -= player.burned_tokens;
                    
                    log!("Refunded {} tokens to {} (table closure)", 
                        player.burned_tokens, player.account_id);
                }
            }
        }
    }

    // Remove table
    remove_table(contract, table_id, reason);
}

/// Pause/unpause table
pub fn set_table_active(
    contract: &mut CardsContract, 
    table_id: String, 
    is_active: bool
) -> bool {
    if let Some(mut table) = contract.game_tables.get(&table_id) {
        table.is_active = is_active;
        table.last_activity = env::block_timestamp();
        
        contract.game_tables.insert(&table_id, &table);
        
        log!("Table {} set to {}", table_id, if is_active { "active" } else { "paused" });
        return true;
    }
    false
}

/// Update table betting limits
pub fn update_table_limits(
    contract: &mut CardsContract, 
    table_id: String, 
    min_bet: Option<u128>, 
    max_bet: Option<u128>
) -> bool {
    if let Some(mut table) = contract.game_tables.get(&table_id) {
        if let Some(min) = min_bet {
            table.min_bet = min;
        }
        if let Some(max) = max_bet {
            table.max_bet = max;
        }
        
        table.last_activity = env::block_timestamp();
        contract.game_tables.insert(&table_id, &table);
        
        log!("Updated table {} limits: min={}, max={}", 
            table_id, table.min_bet, table.max_bet);
        return true;
    }
    false
}

/// Emergency refund all bets at table
pub fn emergency_refund_table(contract: &mut CardsContract, table_id: String) -> u8 {
    let mut refunded_count = 0;
    
    if let Some(mut table) = contract.game_tables.get(&table_id) {
        for player in &mut table.players {
            if player.burned_tokens > 0 {
                // Refund by minting tokens back
                if let Some(mut user_account) = contract.accounts.get(&player.account_id) {
                    user_account.balance += player.burned_tokens;
                    contract.accounts.insert(&player.account_id, &user_account);
                    
                    // Update contract stats
                    contract.total_supply += player.burned_tokens;
                    contract.blackjack_stats.total_tokens_burned_betting -= player.burned_tokens;
                    
                    log!("Emergency refunded {} tokens to {}", 
                        player.burned_tokens, player.account_id);
                    
                    player.burned_tokens = 0;
                    refunded_count += 1;
                }
            }
        }
        
        // Reset table state
        table.state = GameState::WaitingForPlayers;
        table.current_player_index = None;
        table.betting_deadline = None;
        table.move_deadline = None;
        table.last_activity = env::block_timestamp();
        
        contract.game_tables.insert(&table_id, &table);
        
        // Clear signals
        contract.pending_bets.insert(&table_id, &Vec::new());
        contract.pending_moves.insert(&table_id, &Vec::new());
    }
    
    if refunded_count > 0 {
        log!("Emergency refund completed at table {}: {} players refunded", 
            table_id, refunded_count);
    }
    
    refunded_count
}

/// Kick player from table (admin function)
pub fn kick_player(
    contract: &mut CardsContract, 
    table_id: String, 
    player_account: near_sdk::AccountId, 
    reason: String
) -> bool {
    let mut table = match contract.game_tables.get(&table_id) {
        Some(t) => t,
        None => return false,
    };

    // Find and remove player
    let player_index = table.players.iter().position(|p| p.account_id == player_account);
    
    if let Some(index) = player_index {
        let player = &table.players[index];
        let seat_number = player.seat_number;
        let burned_tokens = player.burned_tokens;
        
        // Refund active bet
        if burned_tokens > 0 {
            if let Some(mut user_account) = contract.accounts.get(&player_account) {
                user_account.balance += burned_tokens;
                contract.accounts.insert(&player_account, &user_account);
                
                contract.total_supply += burned_tokens;
                contract.blackjack_stats.total_tokens_burned_betting -= burned_tokens;
            }
        }
        
        // Adjust current player index if necessary
        if let Some(current_idx) = table.current_player_index {
            if current_idx as usize == index {
                table.current_player_index = find_next_active_player_index(&table, current_idx);
            } else if (current_idx as usize) > index {
                table.current_player_index = Some(current_idx - 1);
            }
        }
        
        // Remove player
        table.players.remove(index);
        table.last_activity = env::block_timestamp();
        
        // Handle empty table
        if table.players.is_empty() {
            table.state = GameState::WaitingForPlayers;
            table.current_player_index = None;
            table.betting_deadline = None;
            table.move_deadline = None;
        }
        
        contract.game_tables.insert(&table_id, &table);
        
        // Emit event
        emit_event(BlackjackEvent::PlayerLeft {
            table_id: table_id.clone(),
            account_id: player_account.clone(),
            seat_number,
            timestamp: env::block_timestamp(),
        });
        
        log!("Admin kicked player {} from table {} (reason: {})", 
            player_account, table_id, reason);
        
        return true;
    }
    
    false
}

/// Get detailed admin statistics
pub fn get_admin_stats(contract: &CardsContract) -> AdminStats {
    let mut total_active_bets = 0u128;
    let mut total_pending_signals = 0u32;
    let mut tables_by_state = std::collections::HashMap::new();
    
    for (table_id, table) in contract.game_tables.iter() {
        // Count active bets
        for player in &table.players {
            total_active_bets += player.burned_tokens;
        }
        
        // Count pending signals
        total_pending_signals += contract.pending_bets.get(&table_id).map_or(0, |v| v.len()) as u32;
        total_pending_signals += contract.pending_moves.get(&table_id).map_or(0, |v| v.len()) as u32;
        
        // Count tables by state
        *tables_by_state.entry(table.state.clone()).or_insert(0u32) += 1;
    }
    
    AdminStats {
        contract_stats: crate::tokens::get_contract_stats(contract),
        blackjack_stats: contract.blackjack_stats.clone(),
        total_active_bets,
        total_pending_signals,
        tables_by_state,
        timestamp: env::block_timestamp(),
    }
}

/// Admin statistics structure
#[derive(near_sdk::serde::Serialize, near_sdk::serde::Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct AdminStats {
    pub contract_stats: crate::tokens::ContractStats,
    pub blackjack_stats: crate::BlackjackStats,
    pub total_active_bets: u128,
    pub total_pending_signals: u32,
    pub tables_by_state: std::collections::HashMap<GameState, u32>,
    pub timestamp: u64,
}

// ========================================
// HELPER FUNCTIONS
// ========================================

/// Find next active player index after given position
fn find_next_active_player_index(table: &GameTable, start_index: u8) -> Option<u8> {
    let player_count = table.players.len();
    if player_count == 0 {
        return None;
    }

    for i in 1..=player_count {
        let check_index = (start_index as usize + i) % player_count;
        let player = &table.players[check_index];
        
        if player.state == PlayerState::Active && player.burned_tokens > 0 {
            return Some(check_index as u8);
        }
    }
    
    None
}

/// Validate admin permissions for table operations
pub fn validate_admin_table_access(
    contract: &CardsContract, 
    table_id: &String
) -> Result<(), String> {
    if contract.game_tables.get(table_id).is_none() {
        return Err("Table not found".to_string());
    }
    
    let caller = env::predecessor_account_id();
    if caller != contract.owner_id && !contract.game_admins.get(&caller).unwrap_or(false) {
        return Err("Insufficient admin privileges".to_string());
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    fn get_context(predecessor: near_sdk::AccountId) -> near_sdk::VMContext {
        VMContextBuilder::new()
            .current_account_id(accounts(0))
            .predecessor_account_id(predecessor)
            .build()
    }

    #[test]
    fn test_advance_game_state() {
        let context = get_context(accounts(1));
        testing_env!(context);
        
        let mut contract = crate::CardsContract::new(accounts(1));
        let table_id = crate::game::table::create_table(&mut contract, Some("test-table".to_string()));
        
        // Test state advancement
        assert!(advance_game_state(&mut contract, table_id.clone(), GameState::Betting));
        
        let table = contract.game_tables.get(&table_id).unwrap();
        assert_eq!(table.state, GameState::Betting);
        assert!(table.betting_deadline.is_some());
    }

    #[test]
    fn test_clear_signals() {
        let context = get_context(accounts(1));
        testing_env!(context);
        
        let mut contract = crate::CardsContract::new(accounts(1));
        let table_id = "test-table".to_string();
        
        // Add some test signals
        let bet_signals = vec![
            BetSignal {
                player_account: accounts(2),
                table_id: table_id.clone(),
                amount: 50,
                timestamp: 0,
                seat_number: 1,
            },
            BetSignal {
                player_account: accounts(3),
                table_id: table_id.clone(),
                amount: 100,
                timestamp: 0,
                seat_number: 2,
            },
        ];
        
        contract.pending_bets.insert(&table_id, &bet_signals);
        
        // Clear first signal
        clear_signals(&mut contract, table_id.clone(), 1, 0);
        
        let remaining = contract.pending_bets.get(&table_id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].player_account, accounts(3));
        
        // Clear all remaining
        clear_signals(&mut contract, table_id.clone(), 1, 0);
        let remaining = contract.pending_bets.get(&table_id).unwrap();
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn test_emergency_refund() {
        let context = get_context(accounts(1));
        testing_env!(context);
        
        let mut contract = crate::CardsContract::new(accounts(1));
        
        // Setup user with tokens
        let mut user = crate::tokens::UserAccount::default();
        user.balance = 1000;
        user.storage_deposited = true;
        contract.accounts.insert(&accounts(2), &user);
        contract.total_supply = 1000;
        
        // Create table with player who has active bet
        let table = GameTable {
            id: "test-table".to_string(),
            state: GameState::Betting,
            players: vec![
                BlackjackPlayer {
                    account_id: accounts(2),
                    seat_number: 1,
                    state: PlayerState::Active,
                    burned_tokens: 50, // Active bet
                    joined_at: 0,
                    last_action_time: 0,
                    pending_move: None,
                    total_burned_this_session: 50,
                    rounds_played: 0,
                },
            ],
            current_player_index: None,
            round_number: 0,
            created_at: 0,
            last_activity: 0,
            betting_deadline: None,
            move_deadline: None,
            max_players: 3,
            min_bet: 10,
            max_bet: 1000,
            is_active: true,
        };
        
        contract.game_tables.insert(&"test-table".to_string(), &table);
        contract.blackjack_stats.total_tokens_burned_betting = 50;
        
        // Emergency refund
        let refunded = emergency_refund_table(&mut contract, "test-table".to_string());
        assert_eq!(refunded, 1);
        
        // Check refund was processed
        let user_after = contract.accounts.get(&accounts(2)).unwrap();
        assert_eq!(user_after.balance, 1050); // Original 1000 + refunded 50
        
        let table_after = contract.game_tables.get(&"test-table".to_string()).unwrap();
        assert_eq!(table_after.players[0].burned_tokens, 0);
        assert_eq!(table_after.state, GameState::WaitingForPlayers);
    }
}