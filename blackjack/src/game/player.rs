use near_sdk::{env, log};
use crate::{CardsContract, events::{emit_event, log_error}};
use super::types::*;

// ========================================
// PLAYER MANAGEMENT FUNCTIONS
// ========================================

/// Join a game table at specific seat
pub fn join_table(contract: &mut CardsContract, table_id: String, seat_number: u8) -> bool {
    let player_account = env::predecessor_account_id();
    let timestamp = env::block_timestamp();

    // 1. Validate input parameters
    if seat_number < 1 || seat_number > 3 {
        log_error("Invalid seat number", &format!("Seat {}", seat_number), Some(player_account.clone()));
        return false;
    }

    // 2. Check if player has sufficient storage
    if !crate::storage::has_sufficient_blackjack_storage(
        contract.storage_deposits.get(&player_account).unwrap_or(near_sdk::NearToken::from_near(0)),
        &player_account
    ) {
        log_error("Insufficient storage for blackjack", "join_table", Some(player_account.clone()));
        return false;
    }

    // 3. Get or create table
    let mut table = match contract.game_tables.get(&table_id) {
        Some(t) => t,
        None => {
            log_error("Table not found", &table_id, Some(player_account.clone()));
            return false;
        }
    };

    // 4. Check if table is active and accepting players
    if !table.is_active {
        log_error("Table not active", &table_id, Some(player_account.clone()));
        return false;
    }

    if table.players.len() >= table.max_players as usize {
        log_error("Table full", &table_id, Some(player_account.clone()));
        return false;
    }

    // 5. Check if seat is occupied
    if table.players.iter().any(|p| p.seat_number == seat_number) {
        log_error("Seat occupied", &format!("Seat {}", seat_number), Some(player_account.clone()));
        return false;
    }

    // 6. Check if player is already at this table
    if table.players.iter().any(|p| p.account_id == player_account) {
        log_error("Player already at table", &table_id, Some(player_account.clone()));
        return false;
    }

    // 7. Determine player state based on game state
    let player_state = match table.state {
        GameState::WaitingForPlayers => PlayerState::Active,
        GameState::Betting => PlayerState::Active, // Can join during betting
        GameState::DealingInitialCards | 
        GameState::PlayerTurn | 
        GameState::DealerTurn => PlayerState::Observing, // Must wait for next round
        GameState::RoundEnded => PlayerState::WaitingForNextRound,
    };

    // 8. Create new blackjack player
    let blackjack_player = BlackjackPlayer {
        account_id: player_account.clone(),
        seat_number,
        state: player_state.clone(),
        burned_tokens: 0,
        joined_at: timestamp,
        last_action_time: timestamp,
        pending_move: None,
        total_burned_this_session: 0,
        rounds_played: 0,
    };

    // 9. Add player to table
    table.players.push(blackjack_player);
    table.last_activity = timestamp;

    // 10. Update contract state
    contract.game_tables.insert(&table_id, &table);
    contract.blackjack_stats.total_players_joined += 1;

    // 11. Emit event
    emit_event(BlackjackEvent::PlayerJoined {
        table_id: table_id.clone(),
        account_id: player_account.clone(),
        seat_number,
        timestamp,
    });

    log!("Player {} joined table {} at seat {} (state: {:?})", 
        player_account, table_id, seat_number, player_state);

    true
}

/// Leave a game table
pub fn leave_table(contract: &mut CardsContract, table_id: String) -> bool {
    let player_account = env::predecessor_account_id();
    let timestamp = env::block_timestamp();

    // 1. Get table
    let mut table = match contract.game_tables.get(&table_id) {
        Some(t) => t,
        None => {
            log_error("Table not found", &table_id, Some(player_account.clone()));
            return false;
        }
    };

    // 2. Find player at table
    let player_index = match table.players.iter().position(|p| p.account_id == player_account) {
        Some(idx) => idx,
        None => {
            log_error("Player not at table", &table_id, Some(player_account.clone()));
            return false;
        }
    };

    let player = &table.players[player_index];
    let seat_number = player.seat_number;
    let burned_tokens = player.burned_tokens;

    // 3. Handle refunds if player has active bet
    if burned_tokens > 0 && matches!(table.state, GameState::Betting | GameState::WaitingForPlayers) {
        // Refund burned tokens by minting them back
        if let Some(mut user_account) = contract.accounts.get(&player_account) {
            user_account.balance += burned_tokens;
            contract.accounts.insert(&player_account, &user_account);
            
            // Update contract stats
            contract.total_supply += burned_tokens;
            contract.blackjack_stats.total_tokens_burned_betting -= burned_tokens;
            
            log!("Refunded {} tokens to leaving player {}", burned_tokens, player_account);
        }
    }

    // 4. Adjust current player index if necessary
    if let Some(current_idx) = table.current_player_index {
        if current_idx as usize == player_index {
            // Current player is leaving - advance to next player or end turn
            table.current_player_index = find_next_active_player(&table, current_idx);
        } else if (current_idx as usize) > player_index {
            // Adjust index since we're removing a player before current
            table.current_player_index = Some(current_idx - 1);
        }
    }

    // 5. Remove player from table
    table.players.remove(player_index);
    table.last_activity = timestamp;

    // 6. Handle empty table cleanup
    if table.players.is_empty() {
        table.state = GameState::WaitingForPlayers;
        table.current_player_index = None;
        table.betting_deadline = None;
        table.move_deadline = None;
        
        // Clear pending signals for empty table
        contract.pending_bets.remove(&table_id);
        contract.pending_moves.remove(&table_id);
    }

    // 7. Update contract state
    contract.game_tables.insert(&table_id, &table);

    // 8. Emit event
    emit_event(BlackjackEvent::PlayerLeft {
        table_id: table_id.clone(),
        account_id: player_account.clone(),
        seat_number,
        timestamp,
    });

    log!("Player {} left table {} from seat {}", 
        player_account, table_id, seat_number);

    true
}

/// Update player's last action time (for timeout management)
pub fn update_player_activity(contract: &mut CardsContract, table_id: &String, player_account: &near_sdk::AccountId) {
    if let Some(mut table) = contract.game_tables.get(table_id) {
        if let Some(player) = table.players.iter_mut().find(|p| p.account_id == *player_account) {
            player.last_action_time = env::block_timestamp();
            contract.game_tables.insert(table_id, &table);
        }
    }
}

/// Check for inactive players and remove them
pub fn cleanup_inactive_players(contract: &mut CardsContract, table_id: String, timeout_ms: u64) -> u8 {
    let mut removed_count = 0;
    let current_time = env::block_timestamp();
    let timeout_ns = timeout_ms * 1_000_000; // Convert ms to nanoseconds

    if let Some(mut table) = contract.game_tables.get(&table_id) {
        let initial_player_count = table.players.len();
        
        // Find inactive players
        let mut players_to_remove = Vec::new();
        for (index, player) in table.players.iter().enumerate() {
            let time_since_action = current_time - player.last_action_time;
            if time_since_action > timeout_ns {
                players_to_remove.push((index, player.account_id.clone(), player.seat_number));
            }
        }

        // Remove inactive players (in reverse order to maintain indices)
        for (index, account_id, seat_number) in players_to_remove.into_iter().rev() {
            let player = &table.players[index];
            
            // Refund any active bets
            if player.burned_tokens > 0 {
                if let Some(mut user_account) = contract.accounts.get(&account_id) {
                    user_account.balance += player.burned_tokens;
                    contract.accounts.insert(&account_id, &user_account);
                    
                    contract.total_supply += player.burned_tokens;
                    contract.blackjack_stats.total_tokens_burned_betting -= player.burned_tokens;
                }
            }

            table.players.remove(index);
            removed_count += 1;

            emit_event(BlackjackEvent::PlayerLeft {
                table_id: table_id.clone(),
                account_id,
                seat_number,
                timestamp: current_time,
            });
        }

        // Update table state if players were removed
        if removed_count > 0 {
            table.last_activity = current_time;
            
            // Adjust current player index
            if let Some(current_idx) = table.current_player_index {
                if (current_idx as usize) >= table.players.len() {
                    table.current_player_index = find_next_active_player(&table, 0);
                }
            }

            // Handle empty table
            if table.players.is_empty() {
                table.state = GameState::WaitingForPlayers;
                table.current_player_index = None;
                table.betting_deadline = None;
                table.move_deadline = None;
                
                contract.pending_bets.remove(&table_id);
                contract.pending_moves.remove(&table_id);
            }

            contract.game_tables.insert(&table_id, &table);
            
            log!("Removed {} inactive players from table {} (was {}, now {})", 
                removed_count, table_id, initial_player_count, table.players.len());
        }
    }

    removed_count
}

/// Change player state
pub fn set_player_state(
    contract: &mut CardsContract, 
    table_id: String, 
    player_account: near_sdk::AccountId, 
    new_state: PlayerState
) -> bool {
    if let Some(mut table) = contract.game_tables.get(&table_id) {
        if let Some(player) = table.players.iter_mut().find(|p| p.account_id == player_account) {
            let old_state = player.state.clone();
            player.state = new_state.clone();
            player.last_action_time = env::block_timestamp();
            
            contract.game_tables.insert(&table_id, &table);
            
            log!("Player {} state changed from {:?} to {:?} at table {}", 
                player_account, old_state, new_state, table_id);
            
            return true;
        }
    }
    false
}

// ========================================
// HELPER FUNCTIONS
// ========================================

/// Find next active player after given index
fn find_next_active_player(table: &GameTable, start_index: u8) -> Option<u8> {
    let player_count = table.players.len();
    if player_count == 0 {
        return None;
    }

    for i in 1..=player_count {
        let next_index = (start_index as usize + i) % player_count;
        let player = &table.players[next_index];
        
        if player.state == PlayerState::Active && player.burned_tokens > 0 {
            return Some(next_index as u8);
        }
    }
    
    None
}

/// Get player at table
pub fn get_player_at_table(
    contract: &CardsContract, 
    table_id: &String, 
    player_account: &near_sdk::AccountId
) -> Option<BlackjackPlayer> {
    contract.game_tables.get(table_id)
        .and_then(|table| {
            table.players.iter()
                .find(|p| p.account_id == *player_account)
                .cloned()
        })
}

/// Check if player is at table
pub fn is_player_at_table(
    contract: &CardsContract, 
    table_id: &String, 
    player_account: &near_sdk::AccountId
) -> bool {
    get_player_at_table(contract, table_id, player_account).is_some()
}

/// Get player's current seat at table
pub fn get_player_seat(
    contract: &CardsContract, 
    table_id: &String, 
    player_account: &near_sdk::AccountId
) -> Option<u8> {
    get_player_at_table(contract, table_id, player_account)
        .map(|player| player.seat_number)
}

/// Count active players at table
pub fn count_active_players(contract: &CardsContract, table_id: &String) -> u8 {
    contract.game_tables.get(table_id)
        .map(|table| {
            table.players.iter()
                .filter(|p| p.state == PlayerState::Active)
                .count() as u8
        })
        .unwrap_or(0)
}

/// Get all available seats at table
pub fn get_available_seats(contract: &CardsContract, table_id: &String) -> Vec<u8> {
    let occupied_seats: Vec<u8> = contract.game_tables.get(table_id)
        .map(|table| table.players.iter().map(|p| p.seat_number).collect())
        .unwrap_or_default();

    (1..=3).filter(|seat| !occupied_seats.contains(seat)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    #[test]
    fn test_available_seats() {
        let mut contract = crate::CardsContract::new(accounts(0));
        let table_id = "test-table".to_string();
        
        // Create a test table
        let table = GameTable {
            id: table_id.clone(),
            state: GameState::WaitingForPlayers,
            players: vec![
                BlackjackPlayer {
                    account_id: accounts(1),
                    seat_number: 1,
                    state: PlayerState::Active,
                    burned_tokens: 0,
                    joined_at: 0,
                    last_action_time: 0,
                    pending_move: None,
                    total_burned_this_session: 0,
                    rounds_played: 0,
                },
                BlackjackPlayer {
                    account_id: accounts(2),
                    seat_number: 3,
                    state: PlayerState::Active,
                    burned_tokens: 0,
                    joined_at: 0,
                    last_action_time: 0,
                    pending_move: None,
                    total_burned_this_session: 0,
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
        
        contract.game_tables.insert(&table_id, &table);
        
        let available = get_available_seats(&contract, &table_id);
        assert_eq!(available, vec![2]); // Only seat 2 should be available
    }

    #[test]
    fn test_count_active_players() {
        let mut contract = crate::CardsContract::new(accounts(0));
        let table_id = "test-table".to_string();
        
        // Create a test table with mixed player states
        let table = GameTable {
            id: table_id.clone(),
            state: GameState::WaitingForPlayers,
            players: vec![
                BlackjackPlayer {
                    account_id: accounts(1),
                    seat_number: 1,
                    state: PlayerState::Active,
                    burned_tokens: 0,
                    joined_at: 0,
                    last_action_time: 0,
                    pending_move: None,
                    total_burned_this_session: 0,
                    rounds_played: 0,
                },
                BlackjackPlayer {
                    account_id: accounts(2),
                    seat_number: 2,
                    state: PlayerState::SittingOut,
                    burned_tokens: 0,
                    joined_at: 0,
                    last_action_time: 0,
                    pending_move: None,
                    total_burned_this_session: 0,
                    rounds_played: 0,
                },
                BlackjackPlayer {
                    account_id: accounts(3),
                    seat_number: 3,
                    state: PlayerState::Active,
                    burned_tokens: 0,
                    joined_at: 0,
                    last_action_time: 0,
                    pending_move: None,
                    total_burned_this_session: 0,
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
        
        contract.game_tables.insert(&table_id, &table);
        
        let active_count = count_active_players(&contract, &table_id);
        assert_eq!(active_count, 2); // Only players 1 and 3 are active
    }
}