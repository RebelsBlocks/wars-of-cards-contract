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
    RoundEnded,
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
pub struct BlackjackPlayer {
    #[schemars(with = "String")]
    pub account_id: AccountId,
    pub seat_number: u8, // 1, 2, or 3
    pub state: PlayerState,
    pub burned_tokens: u128, // Current bet amount
    pub joined_at: u64,
    pub last_action_time: u64,
    pub pending_move: Option<PlayerMove>, // Signals from contract
    pub total_burned_this_session: u128,
    pub rounds_played: u32,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PlayerHand {
    pub bet_amount: u128,
    pub is_finished: bool,
    pub has_doubled: bool,
    pub has_split: bool,
    pub result: Option<HandResult>,
    pub hand_index: u8, // For split hands (0 = main, 1+ = split)
}

// ======================================
// GAME TABLE STRUCTURE
// ======================================

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct GameTable {
    pub id: String,
    pub state: GameState,
    pub players: Vec<BlackjackPlayer>, // Max 3 players
    pub current_player_index: Option<u8>,
    pub round_number: u64,
    pub created_at: u64,
    pub last_activity: u64,
    pub betting_deadline: Option<u64>, // When betting phase ends
    pub move_deadline: Option<u64>, // When current player must move
    pub max_players: u8, // Usually 3
    pub min_bet: u128,
    pub max_bet: u128,
    pub is_active: bool, // Can be paused by admin
}

// ======================================
// GAME ACTIONS & SIGNALS
// ======================================

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct BetSignal {
    #[schemars(with = "String")]
    pub player_account: AccountId,
    pub table_id: String,
    pub amount: u128,
    pub timestamp: u64,
    pub seat_number: u8,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct MoveSignal {
    #[schemars(with = "String")]
    pub player_account: AccountId,
    pub table_id: String,
    pub move_type: PlayerMove,
    pub timestamp: u64,
    pub hand_index: Option<u8>, // For split hands
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct WinningsDistribution {
    pub table_id: String,
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
// VIEW STRUCTURES (for querying)
// ======================================

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct GameTableView {
    pub id: String,
    pub state: GameState,
    pub players: Vec<PlayerView>,
    pub current_player_index: Option<u8>,
    pub round_number: u64,
    pub betting_deadline: Option<u64>,
    pub move_deadline: Option<u64>,
    pub available_seats: Vec<u8>, // [1, 2, 3] minus occupied
    pub min_bet: u128,
    pub max_bet: u128,
    pub is_active: bool,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PlayerView {
    #[schemars(with = "String")]
    pub account_id: AccountId,
    pub seat_number: u8,
    pub state: PlayerState,
    pub burned_tokens: u128,
    pub pending_move: Option<PlayerMove>,
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
    pub max_players: Option<u8>, // Maximum players per table
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
// EVENTS (for backend polling)
// ======================================

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum BlackjackEvent {
    PlayerJoined {
        table_id: String,
        account_id: AccountId,
        seat_number: u8,
        timestamp: u64,
    },
    PlayerLeft {
        table_id: String,
        account_id: AccountId,
        seat_number: u8,
        timestamp: u64,
    },
    BetPlaced {
        table_id: String,
        account_id: AccountId,
        amount: u128,
        seat_number: u8,
        timestamp: u64,
    },
    MoveSignaled {
        table_id: String,
        account_id: AccountId,
        move_type: PlayerMove,
        timestamp: u64,
    },
    GameStateChanged {
        table_id: String,
        old_state: GameState,
        new_state: GameState,
        timestamp: u64,
    },
    WinningsDistributed {
        table_id: String,
        round_number: u64,
        total_minted: u128,
        players_count: u8,
        timestamp: u64,
    },
    TableCreated {
        table_id: String,
        creator: AccountId,
        timestamp: u64,
    },
    TableClosed {
        table_id: String,
        reason: String,
        timestamp: u64,
    },
    SignalsCleared {
        table_id: String,
        bet_signals_cleared: u8,
        move_signals_cleared: u8,
        timestamp: u64,
    },
    EmergencyRefund {
        table_id: String,
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
    TableNotFound,
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
    TableFull,
    NotAuthorized,
}

impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            GameError::TableNotFound => write!(f, "Game table not found"),
            GameError::SeatOccupied => write!(f, "Seat is already occupied"),
            GameError::InvalidSeatNumber => write!(f, "Invalid seat number (must be 1-3)"),
            GameError::PlayerNotFound => write!(f, "Player not found at table"),
            GameError::InvalidGameState => write!(f, "Invalid game state for this action"),
            GameError::InvalidMove => write!(f, "Invalid move for current situation"),
            GameError::InsufficientTokens => write!(f, "Insufficient token balance"),
            GameError::BetTooLow => write!(f, "Bet amount too low"),
            GameError::BetTooHigh => write!(f, "Bet amount too high"),
            GameError::NotPlayerTurn => write!(f, "Not your turn"),
            GameError::AlreadyBet => write!(f, "Already placed bet this round"),
            GameError::TimeoutExpired => write!(f, "Action timeout expired"),
            GameError::TableFull => write!(f, "Table is full"),
            GameError::NotAuthorized => write!(f, "Not authorized for this action"),
        }
    }
}