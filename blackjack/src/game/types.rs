use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    AccountId,
};
use schemars::JsonSchema;

// ======================================
// GAME STATE ENUMS
// ======================================

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Copy, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum GameState {
    WaitingForPlayers,
    Betting,
    DealingInitialCards,
    PlayerTurn,
    DealerTurn,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Copy, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum PlayerState {
    WaitingForNextRound,
    Active,
    SittingOut,
    Observing,
    AwaitingBuyIn,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Copy, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum PlayerMove {
    Hit,
    Stand,
    Double,
    Split,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Copy, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum HandResult {
    Blackjack,
    Win,
    Push,
    Bust,
    Lose,
}

// ======================================
// PLAYER STRUCTURES
// ======================================

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct SeatPlayer {
    #[schemars(with = "String")]
    pub account_id: AccountId,
    pub seat_number: u8, // 1, 2, or 3
    pub state: PlayerState,
    pub current_hand_index: u8, // 1 or 2 (2 only after split)
    pub hands: Vec<PlayerHand>, // Max 2 hands (index 0=hand1, 1=hand2)
    pub total_burned_this_round: u128, // All burns: bet + double + split
    pub burns_tracking: Vec<BurnRecord>, // Detailed burn history
    pub joined_at: u64,
    pub last_action_time: u64,
    pub rounds_played: u32,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PlayerHand {
    pub hand_index: u8, // 1 or 2
    pub bet_amount: u128,
    pub is_finished: bool, // true after stand/double/bust
    pub has_doubled: bool,
    pub has_split: bool,
    pub can_hit: bool, // false after stand/double
    pub result: Option<HandResult>,
}

// ======================================
// BURN TRACKING STRUCTURES
// ======================================

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct BurnRecord {
    pub burn_type: BurnType, // Bet, Double, Split
    pub amount: u128,
    pub hand_index: u8,
    pub timestamp: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Copy, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum BurnType {
    Bet,    // Initial bet
    Double, // Double down
    Split,  // Split hand
}


// ======================================
// GAME ACTIONS & SIGNALS
// ======================================

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct BetSignal {
    #[schemars(with = "String")]
    pub player_account: AccountId,
    pub seat_number: u8,
    pub amount: u128,
    pub burn_type: BurnType, // Bet, Double, Split
    pub hand_index: u8,
    pub timestamp: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct MoveSignal {
    #[schemars(with = "String")]
    pub player_account: AccountId,
    pub seat_number: u8,
    pub move_type: PlayerMove,
    pub hand_index: u8, // Always required now
    pub timestamp: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct WinningsDistribution {
    pub round_number: u64,
    pub distributions: Vec<PlayerWinning>,
    pub timestamp: u64,
    pub total_minted: u128,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PlayerWinning {
    #[schemars(with = "String")]
    pub account_id: AccountId,
    pub seat_number: u8,
    pub bet_amount: u128,
    pub winnings: u128, // Amount to mint (includes bet return)
    pub result: HandResult,
    pub hand_index: u8,
}

// ======================================
// VIEW STRUCTURES
// ======================================

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PlayerView {
    #[schemars(with = "String")]
    pub account_id: AccountId,
    pub seat_number: u8,
    pub state: PlayerState,
    pub current_hand_index: u8,
    pub hands: Vec<PlayerHand>,
    pub total_burned_this_round: u128,
    pub time_since_last_action: u64, // seconds
    pub is_current_player: bool,
}

// ======================================
// ADMIN STRUCTURES
// ======================================

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct GameConfig {
    pub betting_timeout_ms: u64, // How long players have to bet
    pub move_timeout_ms: u64, // How long for each move
    pub round_break_ms: u64, // Break between rounds
    pub max_inactive_time_ms: u64, // Before kicking player
    pub min_bet_amount: u128,
    pub max_bet_amount: u128,
    pub auto_start_delay_ms: u64, // Delay before auto-starting with 1 player
    pub max_players: Option<u8>, // Maximum players (3 seats)
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            betting_timeout_ms: 45_000,  // 45 seconds
            move_timeout_ms: 30_000,     // 30 seconds  
            round_break_ms: 5_000,       // 5 seconds
            max_inactive_time_ms: 180_000, // 3 minutes
            min_bet_amount: 10,
            max_bet_amount: 1000,
            auto_start_delay_ms: 20_000, // 20 seconds
            max_players: Some(3), // Default 3 players
        }
    }
}

// ======================================
// EVENTS
// ======================================

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum BlackjackEvent {
    PlayerJoined {
        account_id: AccountId,
        seat_number: u8,
        timestamp: u64,
    },
    PlayerLeft {
        account_id: AccountId,
        seat_number: u8,
        timestamp: u64,
    },
    BetPlaced {
        account_id: AccountId,
        amount: u128,
        seat_number: u8,
        timestamp: u64,
    },
    MoveSignaled {
        account_id: AccountId,
        move_type: PlayerMove,
        timestamp: u64,
    },
    GameStateChanged {
        old_state: GameState,
        new_state: GameState,
        timestamp: u64,
    },
    WinningsDistributed {
        round_number: u64,
        total_minted: u128,
        players_count: u8,
        timestamp: u64,
    },
    SignalsCleared {
        bet_signals_cleared: u8,
        move_signals_cleared: u8,
        timestamp: u64,
    },
    EmergencyRefund {
        reason: String,
        players_refunded: u8,
        timestamp: u64,
    },
    GlobalPause {
        reason: String,
        timestamp: u64,
    },
    GlobalResume {
        timestamp: u64,
    },
}

// ======================================
// ERROR TYPES
// ======================================

#[derive(Debug)]
pub enum GameError {
    SeatOccupied,
    InvalidSeatNumber,
    PlayerNotFound,
    InvalidGameState,
    InvalidMove,
    InsufficientTokens,
    BetTooLow,
    BetTooHigh,
    NotPlayerTurn,
    AlreadyBet,
    TimeoutExpired,
    NotAuthorized,
}

impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            GameError::SeatOccupied => write!(f, "Seat is already occupied"),
            GameError::InvalidSeatNumber => write!(f, "Invalid seat number (must be 1-3)"),
            GameError::PlayerNotFound => write!(f, "Player not found at seat"),
            GameError::InvalidGameState => write!(f, "Invalid game state for this action"),
            GameError::InvalidMove => write!(f, "Invalid move for current situation"),
            GameError::InsufficientTokens => write!(f, "Insufficient token balance"),
            GameError::BetTooLow => write!(f, "Bet amount too low"),
            GameError::BetTooHigh => write!(f, "Bet amount too high"),
            GameError::NotPlayerTurn => write!(f, "Not your turn"),
            GameError::AlreadyBet => write!(f, "Already placed bet this round"),
            GameError::TimeoutExpired => write!(f, "Action timeout expired"),
            GameError::NotAuthorized => write!(f, "Not authorized for this action"),
        }
    }
}
