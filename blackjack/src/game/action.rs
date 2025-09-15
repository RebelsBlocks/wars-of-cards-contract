use near_sdk::{env, log, require, AccountId};
use crate::{CardsContract, events::emit_event};
use super::types::*;

// ========================================
// HELPER FUNCTIONS
// ========================================

/// Burn tokens for a player (helper function)
fn burn_tokens_for_player(contract: &mut CardsContract, player_account: &AccountId, amount: u128) {
    // Burn tokens from user account
    let mut user_account = contract.accounts.get(player_account)
        .expect("User account not found");
    
    user_account.balance = user_account.balance.checked_sub(amount)
        .expect("Insufficient balance for burn");
    user_account.total_burned = user_account.total_burned.checked_add(amount)
        .expect("Total burned overflow");
        
    contract.accounts.insert(player_account, &user_account);

    // Update contract stats
    contract.total_supply = contract.total_supply.checked_sub(amount)
        .expect("Total supply underflow");
    contract.total_cards_burned = contract.total_cards_burned.checked_add(amount)
        .expect("Total cards burned overflow");
    contract.blackjack_stats.total_tokens_burned_betting = 
        contract.blackjack_stats.total_tokens_burned_betting.checked_add(amount)
            .expect("Betting burn stats overflow");
}

// ========================================
// SEAT-BASED BETTING AND MOVES
// ========================================

/// Place a bet by burning tokens (pure seat-based)
pub fn place_bet(contract: &mut CardsContract, amount: u128) -> bool {
    let player_account = env::predecessor_account_id();
    let timestamp = env::block_timestamp();

    // 1. Validate bet amount
    require!(
        contract.config.valid_burn_amounts.contains(&amount),
        "Invalid bet amount"
    );

    require!(
        crate::tokens::get_balance(contract, &player_account) >= amount,
        "Insufficient token balance"
    );

    // 2. Find player's seat
    let seat_number = match crate::game::player::is_player_seated(contract, &player_account) {
        Some(seat) => seat,
        None => {
            log!("Player {} not seated", player_account);
            return false;
        }
    };

    // 3. Validate game state
    require!(contract.game_state == GameState::Betting, "Game not in betting state");

    // 4. Get and validate player
    let mut player = match contract.seats.get(&seat_number) {
        Some(Some(p)) => p,
        _ => {
            log!("Player not found at seat {}", seat_number);
            return false;
        }
    };

    require!(player.state == PlayerState::Active, "Player not active");
    require!(player.total_burned_this_round == 0, "Player already bet this round");

    // 5. Burn tokens
    burn_tokens_for_player(contract, &player_account, amount);

    // 6. Create initial hand
    player.hands = vec![PlayerHand {
        hand_index: 1,
        bet_amount: amount,
        is_finished: false,
        has_doubled: false,
        has_split: false,
        can_hit: true,
        result: None,
    }];
    player.total_burned_this_round = amount;
    player.burns_tracking = vec![BurnRecord {
        burn_type: BurnType::Bet,
        amount,
        hand_index: 1,
        timestamp,
    }];
    player.last_action_time = timestamp;

    // 7. Update seat
    contract.seats.insert(&seat_number, &Some(player));

    // 8. Create bet signal
    let bet_signal = BetSignal {
        player_account: player_account.clone(),
        seat_number,
        amount,
        burn_type: BurnType::Bet,
        hand_index: 1,
        timestamp,
    };

    let mut pending_bets = contract.pending_bets.get(&seat_number).unwrap_or_default();
    pending_bets.push(bet_signal);
    contract.pending_bets.insert(&seat_number, &pending_bets);

    // 9. Update global state
    contract.last_activity = timestamp;

    // 10. Emit event
    emit_event(BlackjackEvent::BetPlaced {
        account_id: player_account.clone(),
        amount,
        seat_number,
        timestamp,
    });

    log!("Player {} placed bet of {} at seat {}", player_account, amount, seat_number);
    true
}

/// Signal a move 
pub fn signal_move(contract: &mut CardsContract, move_type: PlayerMove, hand_index: u8) -> bool {
    let player_account = env::predecessor_account_id();
    let timestamp = env::block_timestamp();

    // 1. Find player's seat
    let seat_number = match crate::game::player::is_player_seated(contract, &player_account) {
        Some(seat) => seat,
        None => {
            log!("Player {} not seated", player_account);
            return false;
        }
    };

    // 2. Validate game state - must be the specific seat's turn
    let expected_state = match seat_number {
        1 => GameState::Seat1Turn,
        2 => GameState::Seat2Turn,
        3 => GameState::Seat3Turn,
        _ => {
            log!("Invalid seat number for turn validation: {}", seat_number);
            return false;
        }
    };
    require!(contract.game_state == expected_state, "Not your turn");

    // 3. Check if it's this player's turn (redundant but kept for safety)
    require!(contract.current_player_seat == Some(seat_number), "Not your turn");

    // 4. Get and validate player
    let mut player = match contract.seats.get(&seat_number) {
        Some(Some(p)) => p,
        _ => {
            log!("Player not found at seat {}", seat_number);
            return false;
        }
    };

    // 5. Validate hand index
    require!(hand_index >= 1 && hand_index <= 2, "Invalid hand index (must be 1 or 2)");
    require!(hand_index == player.current_hand_index, "Must play current hand index");

    let hand_idx = (hand_index - 1) as usize;
    require!(hand_idx < player.hands.len(), "Hand does not exist");
    require!(!player.hands[hand_idx].is_finished, "Hand is already finished");

    // 6. Process move
    match move_type {
        PlayerMove::Hit => {
            require!(player.hands[hand_idx].can_hit, "Cannot hit on this hand");
        }
        PlayerMove::Stand => {
            let hand = &mut player.hands[hand_idx];
            hand.is_finished = true;
            hand.can_hit = false;
        }
        PlayerMove::Double => {
            require!(!player.hands[hand_idx].has_doubled, "Cannot double twice on same hand");
            require!(player.hands[hand_idx].can_hit, "Cannot double on finished hand");
            
            let double_amount = player.hands[hand_idx].bet_amount;
            require!(
                crate::tokens::get_balance(contract, &player_account) >= double_amount,
                "Insufficient tokens for double"
            );
            
            burn_tokens_for_player(contract, &player_account, double_amount);
            
            let hand = &mut player.hands[hand_idx];
            hand.has_doubled = true;
            hand.is_finished = true;
            hand.can_hit = false;
            hand.bet_amount += double_amount;
            
            player.total_burned_this_round += double_amount;
            player.burns_tracking.push(BurnRecord {
                burn_type: BurnType::Double,
                amount: double_amount,
                hand_index,
                timestamp,
            });
        }
        PlayerMove::Split => {
            require!(hand_index == 1, "Can only split on hand 1");
            require!(!player.hands[hand_idx].has_split, "Cannot split twice");
            require!(player.hands.len() == 1, "Cannot split when already have multiple hands");
            
            let split_amount = player.hands[hand_idx].bet_amount;
            require!(
                crate::tokens::get_balance(contract, &player_account) >= split_amount,
                "Insufficient tokens for split"
            );
            
            burn_tokens_for_player(contract, &player_account, split_amount);
            
            player.hands[hand_idx].has_split = true;
            
            let hand2 = PlayerHand {
                hand_index: 2,
                bet_amount: split_amount,
                is_finished: false,
                has_doubled: false,
                has_split: false,
                can_hit: true,
                result: None,
            };
            player.hands.push(hand2);
            
            player.current_hand_index = 2;
            player.total_burned_this_round += split_amount;
            player.burns_tracking.push(BurnRecord {
                burn_type: BurnType::Split,
                amount: split_amount,
                hand_index: 2,
                timestamp,
            });
        }
    }

    // 7. Handle hand completion logic
    if player.hands[hand_idx].is_finished {
        if hand_index == 2 && player.hands.len() > 1 && !player.hands[0].is_finished {
            player.current_hand_index = 1;
        }
    }

    // 8. Update seat
    player.last_action_time = timestamp;
    contract.seats.insert(&seat_number, &Some(player));

    // 9. Create move signal
    let move_signal = MoveSignal {
        player_account: player_account.clone(),
        seat_number,
        move_type: move_type.clone(),
        hand_index,
        timestamp,
    };

    let mut pending_moves = contract.pending_moves.get(&seat_number).unwrap_or_default();
    pending_moves.push(move_signal);
    contract.pending_moves.insert(&seat_number, &pending_moves);

    // 10. Update global state
    contract.last_activity = timestamp;

    // 11. Emit event
    emit_event(BlackjackEvent::MoveSignaled {
        account_id: player_account.clone(),
        move_type,
        timestamp,
    });

    log!("Player {} made move {:?} on hand {} at seat {}", player_account, move_type, hand_index, seat_number);
    true
}

/// Distribute winnings by minting tokens (admin only)
pub fn distribute_winnings(
    contract: &mut CardsContract, 
    distribution: WinningsDistribution
) -> bool {
    let timestamp = env::block_timestamp();

    // 1. Update round number (safety check)
    require!(
        distribution.round_number >= contract.round_number,
        "Cannot distribute winnings for past rounds"
    );

    // 2. Process each player's winnings
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

    // 3. Update contract stats
    contract.total_supply += total_minted;
    contract.blackjack_stats.total_winnings_distributed += total_minted;
    contract.blackjack_stats.total_hands_dealt += distribution.distributions.len() as u64;

    // 4. Reset all players for next round
    for seat in 1..=3 {
        if let Some(Some(mut player)) = contract.seats.get(&seat) {
            // Reset to clean state for next round
            player.current_hand_index = 1;
            player.hands.clear();
            player.total_burned_this_round = 0;
            player.burns_tracking.clear();
            player.last_action_time = timestamp;
            player.rounds_played += 1;
            
            // Keep player active if they want to continue
            if player.state == PlayerState::Active {
                player.state = PlayerState::Active; // Ready for next round
            }
            
            contract.seats.insert(&seat, &Some(player));
        }
    }

    // 5. Update global game state
    contract.round_number += 1;
    contract.last_activity = timestamp;
    contract.game_state = GameState::WaitingForPlayers; // Ready for next round
    contract.current_player_seat = None;

    // 6. Auto-clear all signals since round is complete 
    for seat_number in 1..=3 {
        contract.pending_bets.insert(&seat_number, &Vec::new());
        contract.pending_moves.insert(&seat_number, &Vec::new());
    }

    // 7. Emit event
    emit_event(BlackjackEvent::WinningsDistributed {
        round_number: distribution.round_number,
        total_minted,
        players_count: distribution.distributions.len() as u8,
        timestamp,
    });

    log!("Winnings distribution completed: {} tokens minted across {} players", 
        total_minted, distribution.distributions.len());
    log!("Round {} ended, game reset to WaitingForPlayers state", distribution.round_number);

    true
}
