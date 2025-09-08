use near_sdk::{env, log, require};
use crate::CardsContract;
use super::types::*;

// ========================================
// BET PLACEMENT (Token Burning)
// ========================================

/// Place a bet by burning tokens
pub fn place_bet(contract: &mut CardsContract, table_id: String, amount: u128) -> bool {
    let player_account = env::predecessor_account_id();
    let timestamp = env::block_timestamp();

    // 1. Early validation to save gas
    require!(
        contract.config.valid_burn_amounts.contains(&amount),
        "Invalid bet amount"
    );

    require!(
        contract.has_sufficient_balance(&player_account, amount),
        "Insufficient token balance"
    );

    // 2. Validate table exists and is in betting state
    let mut table = match contract.game_tables.get(&table_id) {
        Some(t) => t,
        None => {
            log!("Table {} not found", table_id);
            return false;
        }
    };

    require!(table.state == GameState::Betting, "Table not in betting state");
    require!(table.is_active, "Table is not active");

    // 3. Find player at table
    let player_index = table.players.iter().position(|p| p.account_id == player_account);
    let player_index = match player_index {
        Some(idx) => idx,
        None => {
            log!("Player {} not found at table {}", player_account, table_id);
            return false;
        }
    };

    let player = &mut table.players[player_index];

    // 4. Validate bet
    require!(player.state == PlayerState::Active, "Player not active");
    require!(player.burned_tokens == 0, "Player already placed bet");
    require!(amount >= table.min_bet, "Bet below minimum");
    require!(amount <= table.max_bet, "Bet above maximum");

    // CRITICAL FIX: Burn tokens FIRST (state change before external effects)
    let mut user_account = contract.accounts.get(&player_account)
        .expect("User account not found");
    
    // Use checked arithmetic for safety
    user_account.balance = user_account.balance.checked_sub(amount)
        .expect("Insufficient balance for bet");
    user_account.total_burned = user_account.total_burned.checked_add(amount)
        .expect("Total burned overflow");
        
    contract.accounts.insert(&player_account, &user_account);

    // Update contract stats
    contract.total_supply = contract.total_supply.checked_sub(amount)
        .expect("Total supply underflow");
    contract.total_cards_burned = contract.total_cards_burned.checked_add(amount)
        .expect("Total cards burned overflow");
    contract.blackjack_stats.total_tokens_burned_betting = 
        contract.blackjack_stats.total_tokens_burned_betting.checked_add(amount)
        .expect("Blackjack tokens burned overflow");

    // Store seat number before updating player state
    let seat_number = player.seat_number;

    // Update player state
    player.burned_tokens = amount;
    player.total_burned_this_session = player.total_burned_this_session.checked_add(amount)
        .expect("Session burned overflow");
    player.last_action_time = timestamp;

    // Create bet signal for backend processing
    let bet_signal = BetSignal {
        player_account: player_account.clone(),
        table_id: table_id.clone(),
        amount,
        timestamp,
        seat_number,
    };

    // Add to pending bets
    let mut pending_bets = contract.pending_bets.get(&table_id).unwrap_or_default();
    pending_bets.push(bet_signal);
    contract.pending_bets.insert(&table_id, &pending_bets);

    // Update table
    table.last_activity = timestamp;
    contract.game_tables.insert(&table_id, &table);

    // Emit event
    contract.emit_event(BlackjackEvent::BetPlaced {
        table_id: table_id.clone(),
        account_id: player_account.clone(),
        amount,
        seat_number,
        timestamp,
    });

    log!("Bet placed: {} burned {} tokens at table {}", 
        player_account, amount, table_id);

    true
}

// ========================================
// MOVE SIGNALING (Player Actions)
// ========================================

/// Signal a move (hit, stand, double, split)
pub fn signal_move(
    contract: &mut CardsContract, 
    table_id: String, 
    move_type: PlayerMove, 
    hand_index: Option<u8>
) -> bool {
    let player_account = env::predecessor_account_id();
    let timestamp = env::block_timestamp();

    // 1. Validate table exists and is in player turn state
    let mut table = match contract.game_tables.get(&table_id) {
        Some(t) => t,
        None => {
            log!("Table {} not found", table_id);
            return false;
        }
    };

    require!(table.state == GameState::PlayerTurn, "Not in player turn state");
    require!(table.is_active, "Table is not active");

    // 2. Find player and validate it's their turn
    let player_index = table.players.iter().position(|p| p.account_id == player_account);
    let player_index = match player_index {
        Some(idx) => idx,
        None => {
            log!("Player {} not found at table {}", player_account, table_id);
            return false;
        }
    };

    // Check if it's this player's turn
    let current_player_index = table.current_player_index
        .expect("No current player set") as usize;
    require!(player_index == current_player_index, "Not your turn");

    let player = &mut table.players[player_index];
    require!(player.state == PlayerState::Active, "Player not active");

    // 3. Validate move (basic validation - detailed validation in backend)
    match move_type {
        PlayerMove::Double => {
            // For double, player needs sufficient tokens for second bet
            require!(
                contract.has_sufficient_balance(&player_account, player.burned_tokens),
                "Insufficient tokens for double"
            );
        }
        _ => {} // Hit, Stand, Split validated in backend with card context
    }

    // 4. Special handling for double (burn additional tokens immediately)
    if move_type == PlayerMove::Double {
        let double_amount = player.burned_tokens;
        
        // Burn additional tokens
        let mut user_account = contract.accounts.get(&player_account)
            .expect("User account not found");
        
        user_account.balance -= double_amount;
        user_account.total_burned += double_amount;
        contract.accounts.insert(&player_account, &user_account);

        // Update contract stats
        contract.total_supply = contract.total_supply.saturating_sub(double_amount);
        contract.total_cards_burned += double_amount;
        contract.blackjack_stats.total_tokens_burned_betting += double_amount;

        // Update player
        player.burned_tokens += double_amount; // Now 2x original bet
        player.total_burned_this_session += double_amount;
    }

    // 5. Update player state
    player.pending_move = Some(move_type.clone());
    player.last_action_time = timestamp;

    // 6. Create move signal for backend processing
    let move_signal = MoveSignal {
        player_account: player_account.clone(),
        table_id: table_id.clone(),
        move_type: move_type.clone(),
        timestamp,
        hand_index,
    };

    // Add to pending moves
    let mut pending_moves = contract.pending_moves.get(&table_id).unwrap_or_default();
    pending_moves.push(move_signal);
    contract.pending_moves.insert(&table_id, &pending_moves);

    // 7. Update table
    table.last_activity = timestamp;
    contract.game_tables.insert(&table_id, &table);

    // 8. Emit event
    contract.emit_event(BlackjackEvent::MoveSignaled {
        table_id: table_id.clone(),
        account_id: player_account.clone(),
        move_type,
        timestamp,
    });

    log!("Move signaled: {} used {:?} at table {}", 
        player_account, move_type, table_id);

    true
}

// ========================================
// WINNINGS DISTRIBUTION (Token Minting)
// ========================================

/// Distribute winnings by minting tokens (admin only - called by backend)
pub fn distribute_winnings(
    contract: &mut CardsContract, 
    distribution: WinningsDistribution
) -> bool {
    let timestamp = env::block_timestamp();
    let table_id = &distribution.table_id;

    // 1. Validate table exists
    let mut table = match contract.game_tables.get(table_id) {
        Some(t) => t,
        None => {
            log!("Table {} not found for winnings distribution", table_id);
            return false;
        }
    };

    // 2. Update table round number (safety check)
    require!(
        distribution.round_number >= table.round_number,
        "Cannot distribute winnings for past rounds"
    );

    // 3. Process each player's winnings
    let mut total_minted = 0u128;
    
    for winning in &distribution.distributions {
        // Find player account
        if let Some(mut user_account) = contract.accounts.get(&winning.account_id) {
            // Mint winnings (add to balance)
            user_account.balance += winning.winnings;
            contract.accounts.insert(&winning.account_id, &user_account);
            
            total_minted += winning.winnings;
            
            log!("Winnings distributed: {} received {} tokens (result: {:?})", 
                winning.account_id, winning.winnings, winning.result);
        } else {
            log!("Warning: Player {} not found for winnings distribution", 
                winning.account_id);
        }
    }

    // 4. Update contract stats
    contract.total_supply += total_minted;
    contract.blackjack_stats.total_winnings_distributed += total_minted;
    contract.blackjack_stats.total_hands_dealt += distribution.distributions.len() as u64;

    // 5. Reset players for next round
    for player in &mut table.players {
        player.burned_tokens = 0;
        player.pending_move = None;
        player.last_action_time = timestamp;
        
        // Set state based on whether they want to continue
        if player.state == PlayerState::Active {
            player.state = PlayerState::Active; // Ready for next round
        }
    }

    // 6. Update table state
    table.round_number += 1;
    table.last_activity = timestamp;
    table.state = GameState::RoundEnded; // Backend will advance to Betting or WaitingForPlayers
    contract.game_tables.insert(table_id, &table);

    // 7. Auto-clear all signals since round is complete
    contract.pending_bets.insert(table_id, &Vec::new());
    contract.pending_moves.insert(table_id, &Vec::new());

    // 8. Emit event
    contract.emit_event(BlackjackEvent::WinningsDistributed {
        table_id: table_id.clone(),
        round_number: distribution.round_number,
        total_minted,
        players_count: distribution.distributions.len() as u8,
        timestamp,
    });

    log!("Winnings distribution completed: {} tokens minted across {} players at table {}", 
        total_minted, distribution.distributions.len(), table_id);
    log!("Auto-cleared all pending signals for table {} after round completion", table_id);

    true
}

// ========================================
// HELPER FUNCTIONS
// ========================================

/// Check if all players at table have placed bets
pub fn all_bets_placed(contract: &CardsContract, table_id: &String) -> bool {
    if let Some(table) = contract.game_tables.get(table_id) {
        let active_players: Vec<_> = table.players.iter()
            .filter(|p| p.state == PlayerState::Active)
            .collect();

        if active_players.is_empty() {
            return false;
        }

        active_players.iter().all(|p| p.burned_tokens > 0)
    } else {
        false
    }
}

/// Get total pot (all burned tokens) for a table
pub fn get_table_pot(contract: &CardsContract, table_id: &String) -> u128 {
    contract.game_tables.get(table_id)
        .map(|table| {
            table.players.iter()
                .map(|p| p.burned_tokens)
                .sum()
        })
        .unwrap_or(0)
}

/// Check if player can make a specific move (basic validation)
pub fn can_make_move(
    contract: &CardsContract, 
    table_id: &String, 
    player_account: &near_sdk::AccountId,
    move_type: &PlayerMove
) -> Result<(), GameError> {
    let table = contract.game_tables.get(table_id)
        .ok_or(GameError::TableNotFound)?;

    if table.state != GameState::PlayerTurn {
        return Err(GameError::InvalidGameState);
    }

    let player = table.players.iter()
        .find(|p| p.account_id == *player_account)
        .ok_or(GameError::PlayerNotFound)?;

    if player.state != PlayerState::Active {
        return Err(GameError::InvalidGameState);
    }

    // Check if it's player's turn
    let current_index = table.current_player_index
        .ok_or(GameError::InvalidGameState)? as usize;
    let player_index = table.players.iter()
        .position(|p| p.account_id == *player_account)
        .ok_or(GameError::PlayerNotFound)?;

    if current_index != player_index {
        return Err(GameError::NotPlayerTurn);
    }

    // Move-specific validation
    match move_type {
        PlayerMove::Double => {
            if !contract.has_sufficient_balance(player_account, player.burned_tokens) {
                return Err(GameError::InsufficientTokens);
            }
        }
        _ => {} // Other moves validated in backend with card context
    }

    Ok(())
}