use near_sdk::{env, log, AccountId};
use crate::{CardsContract, events::emit_event};
use super::types::*;

// ========================================
// SEAT-BASED ADMIN FUNCTIONS
// ========================================

/// Advance game state
pub fn advance_game_state(contract: &mut CardsContract, new_state: GameState) -> bool {
    let timestamp = env::block_timestamp();
    
    let old_state = contract.game_state.clone();
    contract.game_state = new_state.clone();
    contract.last_activity = timestamp;

    // Handle state-specific logic
    match new_state {
        GameState::Betting => {
            // Reset all players for new round
            for seat in 1..=3 {
                if let Some(Some(mut player)) = contract.seats.get(&seat) {
                    player.total_burned_this_round = 0;
                    player.hands.clear();
                    player.current_hand_index = 1;
                    player.burns_tracking.clear();
                    player.last_action_time = timestamp;
                    
                    // Activate observing players
                    if player.state == PlayerState::Observing || 
                       player.state == PlayerState::WaitingForNextRound {
                        player.state = PlayerState::Active;
                    }
                    
                    contract.seats.insert(&seat, &Some(player));
                }
            }
            
            contract.current_player_seat = None;
        }

        GameState::PlayerTurn => {
            // Find first active player with bet
            let first_player_seat = (1..=3)
                .find(|&seat| {
                    contract.seats.get(&seat)
                        .flatten()
                        .map(|p| p.state == PlayerState::Active && p.total_burned_this_round > 0)
                        .unwrap_or(false)
                })
                .map(|seat| seat);

            contract.current_player_seat = first_player_seat;
        }

        GameState::DealerTurn => {
            // Round completed
            contract.round_number += 1;
            contract.current_player_seat = None;
        }

        GameState::WaitingForPlayers => {
            // Reset for next round
            contract.current_player_seat = None;
        }

        _ => {}
    }

    // Emit event
    emit_event(BlackjackEvent::GameStateChanged {
        old_state,
        new_state: new_state.clone(),
        timestamp,
    });

    log!("Game state changed to {:?}", new_state);
    true
}

/// Kick player by account ID
pub fn kick_player(contract: &mut CardsContract, account_id: AccountId, reason: String) -> bool {
    let timestamp = env::block_timestamp();
    
    // Find player's seat
    let seat_number = match crate::game::player::is_player_seated(contract, &account_id) {
        Some(seat) => seat,
        None => {
            log!("Player {} not found for kicking", account_id);
            return false;
        }
    };

    let player = match contract.seats.get(&seat_number) {
        Some(Some(p)) => p,
        _ => return false,
    };

    // Handle refunds
    if player.total_burned_this_round > 0 {
        if let Some(mut user_account) = contract.accounts.get(&account_id) {
            user_account.balance += player.total_burned_this_round;
            contract.accounts.insert(&account_id, &user_account);
            
            contract.total_supply += player.total_burned_this_round;
            contract.blackjack_stats.total_tokens_burned_betting -= player.total_burned_this_round;
            
            log!("Refunded {} tokens to kicked player {}", player.total_burned_this_round, account_id);
        }
    }

    // Adjust current player if necessary
    if contract.current_player_seat == Some(seat_number) {
        contract.current_player_seat = crate::game::player::find_next_active_player(contract, seat_number);
    }

    // Remove player
    contract.seats.insert(&seat_number, &None);
    contract.last_activity = timestamp;

    // Clear signals
    contract.pending_bets.insert(&seat_number, &Vec::new());
    contract.pending_moves.insert(&seat_number, &Vec::new());

    emit_event(BlackjackEvent::PlayerLeft {
        account_id: account_id.clone(),
        seat_number,
        timestamp,
    });

    log!("Player {} kicked from seat {} - {}", account_id, seat_number, reason);
    true
}

/// Get detailed admin statistics
pub fn get_admin_stats(contract: &CardsContract) -> AdminStats {
    let mut total_active_bets = 0u128;
    let mut total_pending_signals = 0u32;
    
    // Count active bets and signals across all seats
    for seat_num in 1..=3 {
        if let Some(Some(player)) = contract.seats.get(&seat_num) {
            total_active_bets += player.total_burned_this_round;
        }
        
        total_pending_signals += contract.pending_bets.get(&seat_num).map_or(0, |v| v.len()) as u32;
        total_pending_signals += contract.pending_moves.get(&seat_num).map_or(0, |v| v.len()) as u32;
    }
    
    AdminStats {
        contract_stats: crate::tokens::get_contract_stats(contract),
        blackjack_stats: contract.blackjack_stats.clone(),
        total_active_bets,
        total_pending_signals,
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
    pub timestamp: u64,
}
