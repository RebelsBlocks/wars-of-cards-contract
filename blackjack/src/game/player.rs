use near_sdk::{env, log, AccountId};
use crate::{CardsContract, events::{emit_event, log_error}};
use super::types::*;

// ========================================
// PLAYER FUNCTIONS
// ========================================

/// Take a seat (1, 2, or 3)
pub fn take_seat(contract: &mut CardsContract, seat_number: u8) -> bool {
    let player_account = env::predecessor_account_id();
    let timestamp = env::block_timestamp();

    // 1. Validate seat number
    if seat_number < 1 || seat_number > 3 {
        log_error("Invalid seat number", &format!("Seat {}", seat_number), Some(player_account.clone()));
        return false;
    }

    // 2. Check if joining is allowed in current game state
    if contract.game_state != GameState::WaitingForPlayers {
        log_error("Cannot join seat", "Can only join seats during WaitingForPlayers state", Some(player_account.clone()));
        return false;
    }

    // 3. Check if seat is available
    if contract.seats.get(&seat_number).is_some() {
        log_error("Seat occupied", &format!("Seat {}", seat_number), Some(player_account.clone()));
        return false;
    }

    // 4. Check if player is already seated somewhere
    for seat in 1..=3 {
        if let Some(Some(existing_player)) = contract.seats.get(&seat) {
            if existing_player.account_id == player_account {
                log_error("Player already seated", &format!("Seat {}", seat), Some(player_account.clone()));
                return false;
            }
        }
    }

    // 5. Check storage
    if !crate::storage::has_sufficient_blackjack_storage(
        contract.storage_deposits.get(&player_account).unwrap_or(near_sdk::NearToken::from_near(0)),
        &player_account
    ) {
        log_error("Insufficient storage for blackjack", "take_seat", Some(player_account.clone()));
        return false;
    }

    // 6. Create seat player
    let seat_player = SeatPlayer {
        account_id: player_account.clone(),
        seat_number,
        state: match contract.game_state {
            GameState::WaitingForPlayers => PlayerState::Active,
            GameState::Betting => PlayerState::Active,
            _ => PlayerState::Observing, // Must wait for next round
        },
        current_hand_index: 1,
        hands: Vec::new(),
        total_burned_this_round: 0,
        burns_tracking: Vec::new(),
        joined_at: timestamp,
        last_action_time: timestamp,
        rounds_played: 0,
    };

    // 7. Place player in seat
    contract.seats.insert(&seat_number, &Some(seat_player));
    contract.last_activity = timestamp;
    contract.blackjack_stats.total_players_joined += 1;

    // 8. Emit event
    emit_event(BlackjackEvent::PlayerJoined {
        account_id: player_account.clone(),
        seat_number,
        timestamp,
    });

    log!("Player {} took seat {}", player_account, seat_number);
    true
}

/// Leave your current seat
pub fn leave_seat(contract: &mut CardsContract) -> bool {
    let player_account = env::predecessor_account_id();
    let timestamp = env::block_timestamp();

    // 1. Find player's current seat
    let mut player_seat = None;
    for seat in 1..=3 {
        if let Some(Some(existing_player)) = contract.seats.get(&seat) {
            if existing_player.account_id == player_account {
                player_seat = Some((seat, existing_player));
                break;
            }
        }
    }

    let (seat_number, player) = match player_seat {
        Some((seat, player)) => (seat, player),
        None => {
            log_error("Player not seated", "leave_seat", Some(player_account.clone()));
            return false;
        }
    };

    // 2. Handle refunds if player has active bet
    if player.total_burned_this_round > 0 && matches!(contract.game_state, GameState::Betting | GameState::WaitingForPlayers) {
        // Refund burned tokens by minting them back
        if let Some(mut user_account) = contract.accounts.get(&player_account) {
            user_account.balance += player.total_burned_this_round;
            contract.accounts.insert(&player_account, &user_account);
            
            // Update contract stats
            contract.total_supply += player.total_burned_this_round;
            contract.blackjack_stats.total_tokens_burned_betting -= player.total_burned_this_round;
            
            log!("Refunded {} tokens to leaving player {}", player.total_burned_this_round, player_account);
        }
    }

    // 3. Adjust current player if necessary
    if contract.current_player_seat == Some(seat_number) {
        contract.current_player_seat = find_next_active_player(contract, seat_number);
    }

    // 4. Remove player from seat (clear the entry entirely)
    contract.seats.remove(&seat_number);
    contract.last_activity = timestamp;

    // 5. Clear pending signals for this seat
    contract.pending_bets.insert(&seat_number, &Vec::new());
    contract.pending_moves.insert(&seat_number, &Vec::new());

    // 6. Emit event
    emit_event(BlackjackEvent::PlayerLeft {
        account_id: player_account.clone(),
        seat_number,
        timestamp,
    });

    log!("Player {} left seat {}", player_account, seat_number);
    true
}

// ========================================
// HELPER FUNCTIONS
// ========================================

/// Find next active player after given seat
pub fn find_next_active_player(contract: &CardsContract, start_seat: u8) -> Option<u8> {
    for i in 1..=3 {
        let next_seat = ((start_seat - 1 + i) % 3) + 1;
        if let Some(Some(player)) = contract.seats.get(&next_seat) {
            if player.state == PlayerState::Active && player.total_burned_this_round > 0 {
                return Some(next_seat);
            }
        }
    }
    None
}

/// Get player at specific seat
pub fn get_player_at_seat(contract: &CardsContract, seat_number: u8) -> Option<SeatPlayer> {
    contract.seats.get(&seat_number).flatten()
}

/// Check if player is seated
pub fn is_player_seated(contract: &CardsContract, player_account: &AccountId) -> Option<u8> {
    for seat in 1..=3 {
        if let Some(Some(player)) = contract.seats.get(&seat) {
            if player.account_id == *player_account {
                return Some(seat);
            }
        }
    }
    None
}

/// Count active players
pub fn count_active_players(contract: &CardsContract) -> u8 {
    (1..=3)
        .filter(|&seat| {
            contract.seats.get(&seat)
                .flatten()
                .map(|p| p.state == PlayerState::Active)
                .unwrap_or(false)
        })
        .count() as u8
}
