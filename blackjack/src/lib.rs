use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    collections::{UnorderedMap, LookupMap},
    env, log, near_bindgen, require,
    serde::{Deserialize, Serialize},
    AccountId, NearToken, PanicOnDefault,
};
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractMetadata {
    pub version: String,
    pub link: Option<String>,
    pub build_info: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct GameStateView {
    pub state: GameState,
    pub round_number: u64,
    pub current_player_seat: Option<u8>,
    pub available_seats: Vec<u8>,
    pub occupied_seats: Vec<u8>,
}

// Import modules
mod tokens;
mod storage;
mod events;
mod game;

// Re-export key types for convenience
pub use tokens::*;
pub use storage::*;
pub use events::*;
pub use game::*;

/// Main contract structure combining tokens and blackjack
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct CardsContract {
    // ========================================
    // TOKEN SYSTEM 
    // ========================================
    /// Total supply of cards ever created
    pub total_supply: u128,
    /// Total cards claimed through free daily claims
    pub total_cards_claimed: u128,
    /// Total cards purchased
    pub total_cards_purchased: u128,
    /// Total cards burned
    pub total_cards_burned: u128,
    /// Map of account_id -> UserAccount
    pub accounts: UnorderedMap<AccountId, UserAccount>,
    /// Storage deposits by account
    pub storage_deposits: UnorderedMap<AccountId, NearToken>,
    /// Contract settings for tokens
    pub config: ContractConfig,
    
    // ========================================
    // BLACKJACK SYSTEM (Seat-Based)
    // ========================================
    /// Fixed 3 seats (1, 2, 3) - None means empty, Some means occupied
    pub seats: LookupMap<u8, Option<SeatPlayer>>,
    /// Player signals pending backend processing (seat_number -> Vec<signals>)
    pub pending_bets: LookupMap<u8, Vec<BetSignal>>,
    pub pending_moves: LookupMap<u8, Vec<MoveSignal>>,
    /// Global game state
    pub game_state: GameState,
    /// Current round number
    pub round_number: u64,
    /// Current player turn (seat number)
    pub current_player_seat: Option<u8>,
    /// Game creation time
    pub game_created_at: u64,
    pub last_activity: u64,
    /// Game configuration
    pub game_config: GameConfig,
    /// Statistics for blackjack
    pub blackjack_stats: BlackjackStats,
    
    // ========================================
    // SHARED
    // ========================================
    /// Contract owner (admin functions)
    pub owner_id: AccountId,
    /// Admin accounts that can manage games
    pub game_admins: UnorderedMap<AccountId, bool>,
    
    // ========================================
    // GLOBAL PAUSE SYSTEM
    // ========================================
    /// Global pause state for upgrades/emergencies
    pub is_globally_paused: Option<bool>,
    pub pause_reason: Option<String>,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct BlackjackStats {
    pub total_games_played: u64,
    pub total_hands_dealt: u64,
    pub total_tokens_burned_betting: u128,
    pub total_winnings_distributed: u128,
    pub total_players_joined: u64,
}

impl Default for BlackjackStats {
    fn default() -> Self {
        Self {
            total_games_played: 0,
            total_hands_dealt: 0,
            total_tokens_burned_betting: 0,
            total_winnings_distributed: 0,
            total_players_joined: 0,
        }
    }
}

#[near_bindgen]
impl CardsContract {
    /// Initialize the contract
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        require!(!env::state_exists(), "Contract already initialized");
        
        let mut game_admins = UnorderedMap::new(b"g");
        game_admins.insert(&owner_id, &true); // Owner is automatically an admin
        
        Self {
            // Token system
            total_supply: 0,
            total_cards_claimed: 0,
            total_cards_purchased: 0,
            total_cards_burned: 0,
            accounts: UnorderedMap::new(b"a"),
            storage_deposits: UnorderedMap::new(b"d"),
            config: ContractConfig::default(),
            
            // Blackjack system (Pure Seat-Based)
            seats: LookupMap::new(b"s"),
            pending_bets: LookupMap::new(b"p"),
            pending_moves: LookupMap::new(b"m"),
            game_state: GameState::WaitingForPlayers,
            round_number: 0,
            current_player_seat: None,
            game_created_at: env::block_timestamp(),
            last_activity: env::block_timestamp(),
            game_config: GameConfig::default(),
            blackjack_stats: BlackjackStats::default(),
            
            // Shared
            owner_id: owner_id.clone(),
            game_admins,
            
            // Global pause system
            is_globally_paused: Some(false),
            pause_reason: None,
        }
    }

    // ========================================
    // TOKEN FUNCTIONS
    // ========================================
    
    /// Deposit storage for user account
    #[payable]
    pub fn storage_deposit(&mut self, account_id: Option<AccountId>) -> StorageBalance {
        self.assert_not_paused();
        tokens::storage_deposit(self, account_id)
    }

    /// Withdraw unused storage deposit
    pub fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        tokens::storage_withdraw(self, amount)
    }

    /// Get storage balance for account
    pub fn storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        tokens::storage_balance_of(self, account_id)
    }

    /// Get storage bounds
    pub fn storage_balance_bounds(&self) -> StorageBounds {
        tokens::storage_balance_bounds(self)
    }


    /// Claim daily cards
    pub fn claim(&mut self) -> u128 {
        self.assert_not_paused();
        tokens::claim_daily_cards(self)
    }

    /// Purchase cards with NEAR
    #[payable]
    pub fn purchase(&mut self, tier_index: u8) -> u128 {
        self.assert_not_paused();
        tokens::purchase_cards(self, tier_index)
    }


    /// Check if user can claim cards (gas-free)
    pub fn check_claim_eligibility(&self, account_id: &AccountId) -> ClaimEligibility {
        tokens::check_claim_eligibility(self, account_id)
    }

    /// Get user card balance
    pub fn get_balance(&self, account_id: &AccountId) -> u128 {
        tokens::get_balance(self, account_id)
    }

    /// Get detailed user statistics
    pub fn get_user_stats(&self, account_id: &AccountId) -> Option<UserStats> {
        tokens::get_user_stats(self, account_id)
    }

    /// Get contract statistics
    pub fn get_contract_stats(&self) -> ContractStats {
        tokens::get_contract_stats(self)
    }

    /// Get purchase tiers
    pub fn get_purchase_tiers(&self) -> &Vec<PurchaseTier> {
        tokens::get_purchase_tiers(self)
    }

    /// Get tier info by index (0-3)
    pub fn get_tier_info(&self, tier_index: u8) -> Option<&PurchaseTier> {
        tokens::get_tier_info(self, tier_index)
    }

    /// Get valid burn amounts
    pub fn get_valid_burn_amounts(&self) -> &Vec<u128> {
        tokens::get_valid_burn_amounts(self)
    }

    /// Get contract configuration
    pub fn get_token_config(&self) -> &ContractConfig {
        tokens::get_config(self)
    }

    /// Update contract configuration (Owner only)
    pub fn update_token_config(&mut self, update: AdminConfigUpdate) {
        tokens::update_config(self, update)
    }

    // ========================================
    // BLACKJACK FUNCTIONS 
    // ========================================

    /// Take a seat (1, 2, or 3)
    pub fn take_seat(&mut self, seat_number: u8) -> bool {
        self.assert_not_paused();
        game::player::take_seat(self, seat_number)
    }

    /// Leave your current seat
    pub fn leave_seat(&mut self) -> bool {
        self.assert_not_paused();
        game::player::leave_seat(self)
    }

    /// Place a bet (burns tokens)
    pub fn bet(&mut self, amount: u128) -> bool {
        self.assert_not_paused();
        game::action::place_bet(self, amount)
    }

    /// Signal a move (hit, stand, double, split)
    pub fn make_move(&mut self, move_type: PlayerMove, hand_index: u8) -> bool {
        self.assert_not_paused();
        game::action::signal_move(self, move_type, hand_index)
    }

    /// Distribute winnings (admin/backend only)
    pub fn distribute_winnings(&mut self, distribution: WinningsDistribution) -> bool {
        self.assert_admin();
        game::action::distribute_winnings(self, distribution)
    }

    /// Advance game state (backend trigger)
    pub fn game_mode(&mut self, new_state: GameState) -> bool {
        self.assert_admin();
        game::admin::advance_game_state(self, new_state)
    }

    // ========================================
    // VIEW FUNCTIONS 
    // ========================================

    /// Get current game state and seat information
    pub fn get_game_state(&self) -> GameStateView {
        GameStateView {
            state: self.game_state.clone(),
            round_number: self.round_number,
            current_player_seat: self.current_player_seat,
            available_seats: self.get_available_seats(),
            occupied_seats: self.get_occupied_seats(),
        }
    }

    /// Get player information for a specific seat
    pub fn get_seat_player(&self, seat_number: u8) -> Option<PlayerView> {
        if seat_number < 1 || seat_number > 3 {
            return None;
        }
        self.seats.get(&seat_number).flatten().map(|player| {
            PlayerView {
                account_id: player.account_id.clone(),
                seat_number: player.seat_number,
                state: player.state.clone(),
                current_hand_index: player.current_hand_index,
                hands: player.hands.clone(),
                total_burned_this_round: player.total_burned_this_round,
                time_since_last_action: (env::block_timestamp() - player.last_action_time) / 1_000_000_000,
                is_current_player: self.current_player_seat == Some(seat_number),
            }
        })
    }

    /// Get all occupied seats
    pub fn get_all_players(&self) -> Vec<PlayerView> {
        (1..=3).filter_map(|seat| self.get_seat_player(seat)).collect()
    }

    /// Get pending bet signals (for backend polling)
    pub fn get_bets_signals(&self, seat_number: u8) -> Vec<BetSignal> {
        self.pending_bets.get(&seat_number).unwrap_or_default()
    }

    /// Get pending move signals (for backend polling)
    pub fn get_moves_signals(&self, seat_number: u8) -> Vec<MoveSignal> {
        self.pending_moves.get(&seat_number).unwrap_or_default()
    }

    /// Get blackjack statistics
    pub fn get_blackjack_stats(&self) -> &BlackjackStats {
        &self.blackjack_stats
    }

    /// Get available seats (1, 2, 3)
    pub fn get_available_seats(&self) -> Vec<u8> {
        (1..=3).filter(|&seat| self.seats.get(&seat).is_none()).collect()
    }

    /// Get occupied seats
    pub fn get_occupied_seats(&self) -> Vec<u8> {
        (1..=3).filter(|&seat| self.seats.get(&seat).is_some()).collect()
    }

    // ========================================
    // ADMIN FUNCTIONS
    // ========================================


    /// Kick specific player by account ID (admin only)
    pub fn kick_player_by_account(&mut self, account_id: AccountId, reason: String) -> bool {
        self.assert_admin();
        game::admin::kick_player(self, account_id, reason)
    }
    
    /// Auto-clear processed signals after round completion
    /// Called by backend after each round
    pub fn cleanup_round_signals(&mut self, seat_number: u8, round_number: u64) {
        self.assert_admin();
        
        // Verify this is for current/completed round
        require!(
            round_number >= self.round_number, 
            "Cannot clear signals for future rounds"
        );
        
        // Clear signals for specific seat since round is complete
        self.pending_bets.insert(&seat_number, &Vec::new());
        self.pending_moves.insert(&seat_number, &Vec::new());
        
        log!("Cleaned up signals for seat {} after round {}", seat_number, round_number);
    }
    
    /// Global pause for contract upgrades (owner only)
    pub fn global_pause(&mut self, reason: String) {
        self.assert_owner();
        
        self.is_globally_paused = Some(true);
        self.pause_reason = Some(reason.clone());
        
        self.emit_event(BlackjackEvent::GlobalPause {
            reason: reason.clone(),
            timestamp: env::block_timestamp(),
        });
        
        log!("CONTRACT GLOBALLY PAUSED: {}", reason);
    }
    
    /// Resume operations after pause
    pub fn global_resume(&mut self) {
        self.assert_owner();
        
        self.is_globally_paused = Some(false);
        self.pause_reason = None;
        
        self.emit_event(BlackjackEvent::GlobalResume {
            timestamp: env::block_timestamp(),
        });
        
        log!("Global pause lifted - operations resumed");
    }
    
    /// Check if any operation should be blocked
    pub fn assert_not_paused(&self) {
        require!(!self.is_globally_paused.unwrap_or(false), 
                format!("Contract paused: {}", 
                       self.pause_reason.as_ref().unwrap_or(&"No reason given".to_string())));
    }

    // ========================================
    // INTERNAL HELPER FUNCTIONS
    // ========================================

    /// Check if caller is contract owner
    pub fn assert_owner(&self) {
        let caller = env::predecessor_account_id();
        require!(caller == self.owner_id, "Only contract owner can call this method");
    }

    /// Check if caller is admin (owner or game admin)
    pub fn assert_admin(&self) {
        let caller = env::predecessor_account_id();
        require!(
            caller == self.owner_id || self.game_admins.get(&caller).unwrap_or(false),
            "Only contract admin can call this method"
        );
    }

    /// Check if user has sufficient token balance
    pub fn has_sufficient_balance(&self, account_id: &AccountId, amount: u128) -> bool {
        self.get_balance(account_id) >= amount
    }


    /// Emit event for logging (internal only)
    fn emit_event<T: Serialize>(&self, event: T) {
        events::emit_event(event)
    }

    /// Get contract metadata
    pub fn get_contract_metadata(&self) -> ContractMetadata {
        ContractMetadata {
            version: "0.1.3".to_string(),
            link: Some("https://warsofcards.online/".to_string()),
            build_info: Some("Rebels Blocks".to_string()),
        }
    }

}

// ========================================
// IMPROVED ERROR HANDLING
// ========================================

#[derive(Debug)]
pub enum ContractError {
    InsufficientStorage,
    InsufficientBalance,
    UserNotFound,
    InvalidAmount,
    OperationPaused,
    Overflow,
    Underflow,
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ContractError::InsufficientStorage => write!(f, "Insufficient storage deposit"),
            ContractError::InsufficientBalance => write!(f, "Insufficient token balance"),
            ContractError::UserNotFound => write!(f, "User account not found"),
            ContractError::InvalidAmount => write!(f, "Invalid amount specified"),
            ContractError::OperationPaused => write!(f, "Contract operations are paused"),
            ContractError::Overflow => write!(f, "Arithmetic overflow"),
            ContractError::Underflow => write!(f, "Arithmetic underflow"),
        }
    }
}

// Tests would be split across modules
#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, VMContext, NearToken};
    use crate::storage::STORAGE_DEPOSIT_REQUIRED;
    use crate::game::types::*;

    fn get_context(predecessor_account_id: AccountId, attached_deposit: NearToken, block_timestamp: u64) -> VMContext {
        VMContextBuilder::new()
            .predecessor_account_id(predecessor_account_id)
            .current_account_id(accounts(0))
            .attached_deposit(attached_deposit)
            .block_timestamp(block_timestamp)
            .build()
    }

    #[test]
    fn test_blackjack_contract_flow() {
        // Similar to test_guestbook_flow
        let mut context = get_context(accounts(1), NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED), 0);
        testing_env!(context.clone());
        
        let mut contract = CardsContract::new(accounts(0));
        
        // 1. Contract initialization ✅
        assert_eq!(contract.owner_id, accounts(0));
        assert_eq!(contract.total_supply, 0);
        assert!(contract.game_admins.get(&accounts(0)).unwrap_or(false));
        
        // 2. Storage deposit ✅
        let balance = contract.storage_deposit(None);
        assert!(balance.total.as_yoctonear() >= STORAGE_DEPOSIT_REQUIRED);
        
        // 3. Claim tokens ✅
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context.clone());
        
        let claimed = contract.claim();
        assert_eq!(claimed, 1000);
        assert_eq!(contract.get_balance(&accounts(1)), 1000);
        
        // 4. Take seat ✅
        let joined = contract.take_seat(1);
        assert!(joined);
        
        // 5. Check game state ✅
        let game_state = contract.get_game_state();
        assert_eq!(game_state.state, GameState::WaitingForPlayers);
        assert_eq!(game_state.occupied_seats, vec![1]);
        assert_eq!(game_state.available_seats, vec![2, 3]);
        
        // 7. Check contract stats ✅
        let stats = contract.get_contract_stats();
        assert_eq!(stats.total_supply, 1000);
        assert_eq!(stats.total_claimed, 1000);
        assert_eq!(stats.active_users, 1);
        
        let blackjack_stats = contract.get_blackjack_stats();
        assert_eq!(blackjack_stats.total_players_joined, 1);
    }

    #[test]
    fn test_multiple_players_blackjack() {
        // Similar to test_multiple_users
        let mut contract = CardsContract::new(accounts(0));
        
        // Player 1 setup
        let mut context = get_context(accounts(1), NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED), 0);
        testing_env!(context.clone());
        contract.storage_deposit(Some(accounts(1)));
        
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context.clone());
        contract.claim(); // Get 1000 tokens
        
        // Player 2 setup
        context.predecessor_account_id = accounts(2);
        context.attached_deposit = NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED);
        testing_env!(context.clone());
        contract.storage_deposit(Some(accounts(2)));
        
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context.clone());
        contract.claim(); // Get 1000 tokens
        
        // Player 1 takes seat
        context.predecessor_account_id = accounts(1);
        testing_env!(context.clone());
        let joined1 = contract.take_seat(1);
        assert!(joined1);
        
        // Player 2 takes seat
        context.predecessor_account_id = accounts(2);
        testing_env!(context.clone());
        let joined2 = contract.take_seat(2);
        assert!(joined2);
        
        // Check game state
        let game_state = contract.get_game_state();
        assert_eq!(game_state.occupied_seats, vec![1, 2]);
        assert_eq!(game_state.available_seats, vec![3]); // Only seat 3 left
        
        // Check contract stats
        let stats = contract.get_contract_stats();
        assert_eq!(stats.total_supply, 2000); // Both players claimed
        assert_eq!(stats.active_users, 2);
        
        let blackjack_stats = contract.get_blackjack_stats();
        assert_eq!(blackjack_stats.total_players_joined, 2);
    }

    #[test]
    fn test_betting_and_token_burning() {
        // Similar to test_dynamic_storage_costs but for betting amounts
        let mut context = get_context(accounts(1), NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED), 0);
        testing_env!(context.clone());
        
        let mut contract = CardsContract::new(accounts(0));
        
        // Setup player with tokens
        contract.storage_deposit(Some(accounts(1)));
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context.clone());
        contract.claim();
        
        // Take seat
        contract.take_seat(1);
        
        // Set game to betting state (as admin)
        context.predecessor_account_id = accounts(0);
        testing_env!(context.clone());
        contract.game_mode(GameState::Betting);
        
        // Test different bet amounts
        context.predecessor_account_id = accounts(1);
        testing_env!(context.clone());
        
        let initial_balance = contract.get_balance(&accounts(1));
        let initial_supply = contract.total_supply;
        
        // Small bet
        let bet_placed = contract.bet(10);
        assert!(bet_placed);
        
        // Verify token burning
        assert_eq!(contract.get_balance(&accounts(1)), initial_balance - 10);
        assert_eq!(contract.total_supply, initial_supply - 10); // Supply reduced by burn
        assert_eq!(contract.total_cards_burned, 10);
        assert_eq!(contract.blackjack_stats.total_tokens_burned_betting, 10);
        
        // Verify player state
        let player_view = contract.get_seat_player(1).unwrap();
        assert_eq!(player_view.total_burned_this_round, 10);
        assert_eq!(player_view.hands.len(), 1);
        assert_eq!(player_view.hands[0].bet_amount, 10);
        assert_eq!(player_view.hands[0].hand_index, 1);
        
        // Check pending bet signals (seat-based)
        let pending_bets = contract.get_bets_signals(1); // seat 1
        assert_eq!(pending_bets.len(), 1);
        assert_eq!(pending_bets[0].amount, 10);
        assert_eq!(pending_bets[0].seat_number, 1);
    }

    #[test]  
    fn test_full_game_round() {
        // Integration test of full game round: join -> bet -> win -> payout
        let mut context = get_context(accounts(1), NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED), 0);
        testing_env!(context.clone());
        
        let mut contract = CardsContract::new(accounts(0));
        
        // Setup player
        contract.storage_deposit(Some(accounts(1)));
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context.clone());
        contract.claim();
        
        // Take seat
        contract.take_seat(1);
        
        // Start betting phase (as admin)
        context.predecessor_account_id = accounts(0);
        testing_env!(context.clone());
        contract.game_mode(GameState::Betting);
        
        // Player bets
        context.predecessor_account_id = accounts(1);
        testing_env!(context.clone());
        
        let balance_before_bet = contract.get_balance(&accounts(1));
        contract.bet(50);
        assert_eq!(contract.get_balance(&accounts(1)), balance_before_bet - 50);
        
        // Admin distributes winnings (player wins double)
        context.predecessor_account_id = accounts(0);
        testing_env!(context);
        
        let distribution = WinningsDistribution {
            round_number: 1,
            distributions: vec![
                PlayerWinning {
                    account_id: accounts(1),
                    seat_number: 1,
                    bet_amount: 50,
                    winnings: 100, // Won double their bet
                    result: HandResult::Win,
                    hand_index: 0,
                }
            ],
            timestamp: 0,
            total_minted: 100,
        };
        
        let distributed = contract.distribute_winnings(distribution);
        assert!(distributed);
        
        // Verify winnings (player should have original balance - bet + winnings)
        let final_balance = contract.get_balance(&accounts(1));
        assert_eq!(final_balance, balance_before_bet - 50 + 100); // -50 bet +100 winnings = +50 net
        
        // Verify contract stats updated
        let stats = contract.get_contract_stats();
        assert_eq!(stats.total_burned, 50); // Bet was burned
        assert_eq!(stats.total_supply, balance_before_bet + 50); // Net increase due to winnings
        
        let blackjack_stats = contract.get_blackjack_stats();
        assert_eq!(blackjack_stats.total_tokens_burned_betting, 50);
        assert_eq!(blackjack_stats.total_winnings_distributed, 100);
        assert_eq!(blackjack_stats.total_hands_dealt, 1);
        
        // Verify signals were cleared (seat-based)
        assert_eq!(contract.get_bets_signals(1).len(), 0);
        assert_eq!(contract.get_moves_signals(1).len(), 0);
    }

    #[test]
    fn test_admin_functions_integration() {
        // Test admin functions work together
        let mut context = get_context(accounts(0), NearToken::from_near(0), 0); // Owner
        testing_env!(context.clone());
        
        let mut contract = CardsContract::new(accounts(0));
        
        // Admin controls game state
        let advanced = contract.game_mode(GameState::Betting);
        assert!(advanced);
        
        let game_state = contract.get_game_state();
        assert_eq!(game_state.state, GameState::Betting);
        
        // Owner can pause globally
        context.predecessor_account_id = accounts(0);
        testing_env!(context.clone());
        contract.global_pause("Emergency maintenance".to_string());
        assert!(contract.is_globally_paused.unwrap_or(false));
        
        // Operations should be blocked
        context.predecessor_account_id = accounts(2);
        context.attached_deposit = NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED);
        testing_env!(context.clone());
        
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contract.storage_deposit(None)
        }));
        assert!(result.is_err()); // Should panic due to global pause
        
        // Owner resumes
        context.predecessor_account_id = accounts(0);
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context);
        contract.global_resume();
        assert!(!contract.is_globally_paused.unwrap_or(true));
    }

    #[test]
    fn test_purchase_tiers_and_validation() {
        // Similar to storage cost tests but for purchase validation
        let mut context = get_context(accounts(1), NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED), 0);
        testing_env!(context.clone());
        
        let mut contract = CardsContract::new(accounts(0));
        
        // Setup storage
        contract.storage_deposit(Some(accounts(1)));
        
        // Test different purchase tiers
        let tiers = contract.get_purchase_tiers();
        assert!(tiers.len() >= 4); // Should have at least 4 tiers
        
        // Test each tier
        for (tier_index, tier) in tiers.iter().enumerate().take(2) { // Test first 2 tiers
            context.attached_deposit = tier.near_cost;
            testing_env!(context.clone());
            
            let initial_balance = contract.get_balance(&accounts(1));
            let purchased = contract.purchase(tier_index as u8);
            
            assert_eq!(purchased, tier.cards_amount);
            assert_eq!(contract.get_balance(&accounts(1)), initial_balance + tier.cards_amount);
        }
        
        // Test invalid tier
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context);
        
        let result = std::panic::catch_unwind(|| {
            contract.purchase(99) // Invalid tier
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_error_conditions() {
        // Test various error conditions
        let context = get_context(accounts(1), NearToken::from_near(0), 0);
        testing_env!(context);
        
        let mut contract = CardsContract::new(accounts(0));
        
        // Test operations without storage deposit
        let result = std::panic::catch_unwind(|| {
            contract.claim()
        });
        assert!(result.is_err()); // Should fail - no storage
        
        // Test game operations without taking seat first
        let bet_placed = contract.bet(50);
        assert!(!bet_placed); // Should return false, not panic
        
        let joined = contract.take_seat(0); // Invalid seat
        assert!(!joined); // Should return false, not panic
        
        // Test invalid seat numbers
        let joined = contract.take_seat(4); // Invalid seat
        assert!(!joined);
    }

    // Import specific test modules
    use tokens::tests as token_tests;
    use game::tests as blackjack_tests;
}
