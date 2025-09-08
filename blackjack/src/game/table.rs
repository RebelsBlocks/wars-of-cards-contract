use near_sdk::{env, log, require};
use crate::{CardsContract, events::emit_event};
use super::types::*;
use super::player::{get_available_seats, count_active_players};

// ========================================
// TABLE MANAGEMENT FUNCTIONS
// ========================================

/// Create a new game table
pub fn create_table(contract: &mut CardsContract, table_id: Option<String>) -> String {
    let creator = env::predecessor_account_id();
    let timestamp = env::block_timestamp();
    
    // Generate table ID
    let final_table_id = table_id.unwrap_or_else(|| contract.generate_table_id());
    
    // Check if table already exists
    require!(
        contract.game_tables.get(&final_table_id).is_none(),
        format!("Table {} already exists", final_table_id)
    );
    
    // Create new table
    let table = GameTable {
        id: final_table_id.clone(),
        state: GameState::WaitingForPlayers,
        players: Vec::new(),
        current_player_index: None,
        round_number: 0,
        created_at: timestamp,
        last_activity: timestamp,
        betting_deadline: None,
        move_deadline: None,
        max_players: contract.game_config.max_players.unwrap_or(3),
        min_bet: contract.game_config.min_bet_amount,
        max_bet: contract.game_config.max_bet_amount,
        is_active: true,
    };
    
    // Save table
    contract.game_tables.insert(&final_table_id, &table);
    contract.blackjack_stats.active_tables += 1;
    
    // Initialize empty signal vectors
    contract.pending_bets.insert(&final_table_id, &Vec::new());
    contract.pending_moves.insert(&final_table_id, &Vec::new());
    
    // Emit event
    emit_event(BlackjackEvent::TableCreated {
        table_id: final_table_id.clone(),
        creator: creator.clone(),
        timestamp,
    });

    log!("Created game table: {} by {}", final_table_id, creator);
    
    final_table_id
}

/// Get game table view for client
pub fn get_table_view(contract: &CardsContract, table_id: &String) -> Option<GameTableView> {
    let table = contract.game_tables.get(table_id)?;
    let current_time = env::block_timestamp();
    
    // Convert players to view format
    let players: Vec<PlayerView> = table.players.iter().map(|player| {
        let time_since_action = if player.last_action_time > 0 {
            (current_time - player.last_action_time) / 1_000_000_000 // Convert to seconds
        } else {
            0
        };
        
        let is_current_player = table.current_player_index
            .map_or(false, |idx| {
                table.players.get(idx as usize)
                    .map_or(false, |p| p.account_id == player.account_id)
            });
        
        PlayerView {
            account_id: player.account_id.clone(),
            seat_number: player.seat_number,
            state: player.state.clone(),
            burned_tokens: player.burned_tokens,
            pending_move: player.pending_move.clone(),
            time_since_last_action: time_since_action,
            is_current_player,
        }
    }).collect();
    
    Some(GameTableView {
        id: table.id.clone(),
        state: table.state.clone(),
        players,
        current_player_index: table.current_player_index,
        round_number: table.round_number,
        betting_deadline: table.betting_deadline,
        move_deadline: table.move_deadline,
        available_seats: get_available_seats(contract, table_id),
        min_bet: table.min_bet,
        max_bet: table.max_bet,
        is_active: table.is_active,
    })
}

/// Get all active tables
pub fn get_active_tables(contract: &CardsContract) -> Vec<GameTableView> {
    let mut tables = Vec::new();
    
    for (table_id, _) in contract.game_tables.iter() {
        if let Some(table_view) = get_table_view(contract, &table_id) {
            if table_view.is_active {
                tables.push(table_view);
            }
        }
    }
    
    tables
}

/// Find available table with open seats
pub fn find_available_table(contract: &CardsContract) -> Option<GameTableView> {
    for (table_id, _) in contract.game_tables.iter() {
        if let Some(table_view) = get_table_view(contract, &table_id) {
            if table_view.is_active && 
               !table_view.available_seats.is_empty() &&
               matches!(table_view.state, GameState::WaitingForPlayers | GameState::Betting) {
                return Some(table_view);
            }
        }
    }
    None
}

/// Update table state
pub fn set_table_state(
    contract: &mut CardsContract, 
    table_id: String, 
    new_state: GameState
) -> bool {
    if let Some(mut table) = contract.game_tables.get(&table_id) {
        let old_state = table.state.clone();
        table.state = new_state.clone();
        table.last_activity = env::block_timestamp();
        
        // Clear deadlines when appropriate
        match new_state {
            GameState::WaitingForPlayers => {
                table.betting_deadline = None;
                table.move_deadline = None;
                table.current_player_index = None;
            }
            GameState::Betting => {
                table.move_deadline = None;
                table.betting_deadline = Some(
                    env::block_timestamp() + (contract.game_config.betting_timeout_ms * 1_000_000)
                );
            }
            GameState::PlayerTurn => {
                table.betting_deadline = None;
                table.move_deadline = Some(
                    env::block_timestamp() + (contract.game_config.move_timeout_ms * 1_000_000)
                );
            }
            GameState::DealerTurn => {
                table.betting_deadline = None;
                table.move_deadline = None;
            }
            GameState::RoundEnded => {
                table.betting_deadline = None;
                table.move_deadline = None;
            }
            _ => {}
        }
        
        contract.game_tables.insert(&table_id, &table);
        
        // Emit event
        emit_event(BlackjackEvent::GameStateChanged {
            table_id: table_id.clone(),
            old_state,
            new_state,
            timestamp: env::block_timestamp(),
        });
        
        log!("Table {} state changed to {:?}", table_id, new_state);
        return true;
    }
    false
}

/// Set current player at table
pub fn set_current_player(
    contract: &mut CardsContract, 
    table_id: String, 
    player_account: near_sdk::AccountId
) -> bool {
    if let Some(mut table) = contract.game_tables.get(&table_id) {
        // Find player index
        let player_index = table.players.iter()
            .position(|p| p.account_id == player_account);
        
        if let Some(index) = player_index {
            table.current_player_index = Some(index as u8);
            table.last_activity = env::block_timestamp();
            
            // Set move deadline
            table.move_deadline = Some(
                env::block_timestamp() + (contract.game_config.move_timeout_ms * 1_000_000)
            );
            
            contract.game_tables.insert(&table_id, &table);
            
            log!("Current player set to {} at table {}", player_account, table_id);
            return true;
        }
    }
    false
}

/// Clear current player (end turn)
pub fn clear_current_player(contract: &mut CardsContract, table_id: String) -> bool {
    if let Some(mut table) = contract.game_tables.get(&table_id) {
        table.current_player_index = None;
        table.move_deadline = None;
        table.last_activity = env::block_timestamp();
        
        contract.game_tables.insert(&table_id, &table);
        
        log!("Cleared current player at table {}", table_id);
        return true;
    }
    false
}

/// Check if table can start a round
pub fn can_start_round(contract: &CardsContract, table_id: &String) -> bool {
    if let Some(table) = contract.game_tables.get(table_id) {
        let active_players = count_active_players(contract, table_id);
        
        return table.is_active &&
               matches!(table.state, GameState::WaitingForPlayers | GameState::RoundEnded) &&
               active_players > 0;
    }
    false
}

/// Check if all players have placed bets
pub fn all_players_bet(contract: &CardsContract, table_id: &String) -> bool {
    if let Some(table) = contract.game_tables.get(table_id) {
        let active_players: Vec<_> = table.players.iter()
            .filter(|p| p.state == PlayerState::Active)
            .collect();
        
        if active_players.is_empty() {
            return false;
        }
        
        return active_players.iter().all(|p| p.burned_tokens > 0);
    }
    false
}

/// Get next player in turn order
pub fn get_next_player_in_turn(
    contract: &CardsContract, 
    table_id: &String, 
    current_player: Option<near_sdk::AccountId>
) -> Option<near_sdk::AccountId> {
    if let Some(table) = contract.game_tables.get(table_id) {
        // Get active players with bets in seat order (1, 2, 3)
        let mut active_players: Vec<_> = table.players.iter()
            .filter(|p| p.state == PlayerState::Active && p.burned_tokens > 0)
            .collect();
        
        active_players.sort_by(|a, b| a.seat_number.cmp(&b.seat_number));
        
        if active_players.is_empty() {
            return None;
        }
        
        // If no current player, return first active player
        let current = match current_player {
            Some(account) => account,
            None => return Some(active_players[0].account_id.clone()),
        };
        
        // Find current player and return next
        for (i, player) in active_players.iter().enumerate() {
            if player.account_id == current {
                let next_index = (i + 1) % active_players.len();
                return Some(active_players[next_index].account_id.clone());
            }
        }
        
        // If current player not found, return first
        Some(active_players[0].account_id.clone())
    } else {
        None
    }
}

/// Remove table (cleanup)
pub fn remove_table(contract: &mut CardsContract, table_id: String, reason: String) {
    if contract.game_tables.get(&table_id).is_some() {
        // Clean up signals
        contract.pending_bets.remove(&table_id);
        contract.pending_moves.remove(&table_id);
        
        // Remove table
        contract.game_tables.remove(&table_id);
        contract.blackjack_stats.active_tables = 
            contract.blackjack_stats.active_tables.saturating_sub(1);
        
        // Emit event
        emit_event(BlackjackEvent::TableClosed {
            table_id: table_id.clone(),
            reason: reason.clone(),
            timestamp: env::block_timestamp(),
        });
        
        log!("Removed table {}: {}", table_id, reason);
    }
}

/// Update table activity timestamp
pub fn update_table_activity(contract: &mut CardsContract, table_id: &String) {
    if let Some(mut table) = contract.game_tables.get(table_id) {
        table.last_activity = env::block_timestamp();
        contract.game_tables.insert(table_id, &table);
    }
}

/// Check if table is expired (no activity for too long)
pub fn is_table_expired(contract: &CardsContract, table_id: &String, timeout_ms: u64) -> bool {
    if let Some(table) = contract.game_tables.get(table_id) {
        let timeout_ns = timeout_ms * 1_000_000;
        let time_since_activity = env::block_timestamp() - table.last_activity;
        
        return time_since_activity > timeout_ns;
    }
    true // Missing table is considered expired
}

/// Get table statistics
pub fn get_table_stats(contract: &CardsContract, table_id: &String) -> Option<TableStats> {
    let table = contract.game_tables.get(table_id)?;
    
    let total_players = table.players.len() as u8;
    let active_players = count_active_players(contract, table_id);
    let total_pot = table.players.iter().map(|p| p.burned_tokens).sum();
    let average_bet = if active_players > 0 {
        total_pot / active_players as u128
    } else {
        0
    };
    
    Some(TableStats {
        table_id: table_id.clone(),
        state: table.state.clone(),
        round_number: table.round_number,
        total_players,
        active_players,
        total_pot,
        average_bet,
        created_at: table.created_at,
        last_activity: table.last_activity,
        uptime_seconds: (env::block_timestamp() - table.created_at) / 1_000_000_000,
    })
}

/// Helper struct for table statistics
#[derive(near_sdk::serde::Serialize, near_sdk::serde::Deserialize, schemars::JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct TableStats {
    pub table_id: String,
    pub state: GameState,
    pub round_number: u64,
    pub total_players: u8,
    pub active_players: u8,
    pub total_pot: u128,
    pub average_bet: u128,
    pub created_at: u64,
    pub last_activity: u64,
    pub uptime_seconds: u64,
}

/// Cleanup expired tables
pub fn cleanup_expired_tables(contract: &mut CardsContract, timeout_ms: u64) -> u8 {
    let mut removed_count = 0;
    let mut tables_to_remove = Vec::new();
    
    // Find expired tables
    for (table_id, _) in contract.game_tables.iter() {
        if is_table_expired(contract, &table_id, timeout_ms) {
            tables_to_remove.push(table_id);
        }
    }
    
    // Remove expired tables
    for table_id in tables_to_remove {
        remove_table(contract, table_id, "Table expired due to inactivity".to_string());
        removed_count += 1;
    }
    
    if removed_count > 0 {
        log!("Cleaned up {} expired tables", removed_count);
    }
    
    removed_count
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
    fn test_create_table() {
        let context = get_context(accounts(1));
        testing_env!(context);
        
        let mut contract = crate::CardsContract::new(accounts(0));
        let table_id = create_table(&mut contract, Some("test-table".to_string()));
        
        assert_eq!(table_id, "test-table");
        assert!(contract.game_tables.get(&table_id).is_some());
        
        let table_view = get_table_view(&contract, &table_id).unwrap();
        assert_eq!(table_view.state, GameState::WaitingForPlayers);
        assert_eq!(table_view.available_seats, vec![1, 2, 3]);
        assert!(table_view.is_active);
    }

    #[test]
    fn test_table_state_transitions() {
        let context = get_context(accounts(1));
        testing_env!(context);
        
        let mut contract = crate::CardsContract::new(accounts(0));
        let table_id = create_table(&mut contract, Some("test-table".to_string()));
        
        // Test state transitions
        assert!(set_table_state(&mut contract, table_id.clone(), GameState::Betting));
        let table_view = get_table_view(&contract, &table_id).unwrap();
        assert_eq!(table_view.state, GameState::Betting);
        assert!(table_view.betting_deadline.is_some());
        
        assert!(set_table_state(&mut contract, table_id.clone(), GameState::PlayerTurn));
        let table_view = get_table_view(&contract, &table_id).unwrap();
        assert_eq!(table_view.state, GameState::PlayerTurn);
        assert!(table_view.move_deadline.is_some());
        assert!(table_view.betting_deadline.is_none());
    }

    #[test]
    fn test_find_available_table() {
        let context = get_context(accounts(1));
        testing_env!(context);
        
        let mut contract = crate::CardsContract::new(accounts(0));
        
        // No tables initially
        assert!(find_available_table(&contract).is_none());
        
        // Create available table
        let table_id = create_table(&mut contract, Some("available-table".to_string()));
        let available = find_available_table(&contract);
        assert!(available.is_some());
        assert_eq!(available.unwrap().id, table_id);
        
        // Make table unavailable (no open seats scenario would require more setup)
        set_table_state(&mut contract, table_id.clone(), GameState::PlayerTurn);
        let available = find_available_table(&contract);
        assert!(available.is_none()); // PlayerTurn state makes it unavailable for joining
    }

    #[test]
    fn test_table_cleanup() {
        let mut context = get_context(accounts(1));
        testing_env!(context.clone());
        
        let mut contract = crate::CardsContract::new(accounts(0));
        let table_id = create_table(&mut contract, Some("cleanup-test".to_string()));
        
        // Table should exist
        assert!(contract.game_tables.get(&table_id).is_some());
        
        // Simulate time passing
        context.block_timestamp = 1000 * 60 * 60 * 1_000_000_000; // 1000 hours later
        testing_env!(context);
        
        // Cleanup expired tables (1 hour timeout)
        let removed = cleanup_expired_tables(&mut contract, 60 * 60 * 1000); // 1 hour in ms
        assert_eq!(removed, 1);
        assert!(contract.game_tables.get(&table_id).is_none());
    }
}