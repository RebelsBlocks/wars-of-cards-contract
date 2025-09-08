use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    collections::{UnorderedMap, LookupMap},
    env, log, near_bindgen, require,
    serde::{Deserialize, Serialize},
    AccountId, NearToken, PanicOnDefault,
};
use schemars::JsonSchema;

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
    // TOKEN SYSTEM (existing functionality)
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
    // BLACKJACK SYSTEM (new functionality)
    // ========================================
    /// Active game tables (table_id -> GameTable)
    pub game_tables: UnorderedMap<String, GameTable>,
    /// Player signals pending backend processing (table_id -> Vec<signals>)
    pub pending_bets: LookupMap<String, Vec<BetSignal>>,
    pub pending_moves: LookupMap<String, Vec<MoveSignal>>,
    /// Game configuration
    pub game_config: GameConfig,
    /// Statistics for blackjack
    pub blackjack_stats: BlackjackStats,
    /// Current nonce for generating unique table IDs
    pub table_id_nonce: u64,
    
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
    pub active_tables: u32,
    pub total_players_joined: u64,
}

impl Default for BlackjackStats {
    fn default() -> Self {
        Self {
            total_games_played: 0,
            total_hands_dealt: 0,
            total_tokens_burned_betting: 0,
            total_winnings_distributed: 0,
            active_tables: 0,
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
            storage_deposits: UnorderedMap::new(b"s"),
            config: ContractConfig::default(),
            
            // Blackjack system
            game_tables: UnorderedMap::new(b"t"),
            pending_bets: LookupMap::new(b"p"),
            pending_moves: LookupMap::new(b"m"),
            game_config: GameConfig::default(),
            blackjack_stats: BlackjackStats::default(),
            table_id_nonce: 0,
            
            // Shared
            owner_id: owner_id.clone(),
            game_admins,
            
            // Global pause system
            is_globally_paused: Some(false),
            pause_reason: None,
        }
    }

    // ========================================
    // TOKEN FUNCTIONS (delegate to tokens module)
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

    /// Get exact storage cost for a specific account
    pub fn get_storage_cost_for_account(&self, account_id: &AccountId) -> NearToken {
        tokens::get_storage_cost_for_account(self, account_id)
    }

    /// Claim daily cards
    pub fn claim_daily_cards(&mut self) -> u128 {
        self.assert_not_paused();
        tokens::claim_daily_cards(self)
    }

    /// Purchase cards with NEAR
    #[payable]
    pub fn purchase_cards(&mut self, tier_index: u8) -> u128 {
        self.assert_not_paused();
        tokens::purchase_cards(self, tier_index)
    }

    /// Burn cards (used for betting)
    pub fn burn_cards(&mut self, amount: u128) {
        self.assert_not_paused();
        
        let account_id = env::predecessor_account_id();
        require!(
            crate::tokens::has_sufficient_storage(self, &account_id),
            "Storage deposit required. Call storage_deposit() first."
        );
        
        tokens::burn_cards(self, amount)
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
    pub fn get_config(&self) -> &ContractConfig {
        tokens::get_config(self)
    }

    /// Update contract configuration (Owner only)
    pub fn update_config(&mut self, update: AdminConfigUpdate) {
        tokens::update_config(self, update)
    }

    // ========================================
    // BLACKJACK FUNCTIONS
    // ========================================

    /// Create a new game table
    pub fn create_game_table(&mut self, table_id: Option<String>) -> String {
        game::table::create_table(self, table_id)
    }

    /// Join a game table at specific seat
    pub fn join_game_table(&mut self, table_id: String, seat_number: u8) -> bool {
        self.assert_not_paused();
        game::player::join_table(self, table_id, seat_number)
    }

    /// Leave a game table
    pub fn leave_game_table(&mut self, table_id: String) -> bool {
        game::player::leave_table(self, table_id)
    }

    /// Place a bet (burns tokens)
    pub fn place_bet(&mut self, table_id: String, amount: u128) -> bool {
        self.assert_not_paused();
        game::action::place_bet(self, table_id, amount)
    }

    /// Signal a move (hit, stand, double, split)
    pub fn signal_move(&mut self, table_id: String, move_type: PlayerMove, hand_index: Option<u8>) -> bool {
        self.assert_not_paused();
        game::action::signal_move(self, table_id, move_type, hand_index)
    }

    /// Distribute winnings (admin/backend only)
    pub fn distribute_winnings(&mut self, distribution: WinningsDistribution) -> bool {
        self.assert_admin();
        game::action::distribute_winnings(self, distribution)
    }

    /// Advance game state (backend trigger)
    pub fn advance_game_state(&mut self, table_id: String, new_state: GameState) -> bool {
        self.assert_admin();
        game::admin::advance_game_state(self, table_id, new_state)
    }

    // ========================================
    // VIEW FUNCTIONS (Blackjack)
    // ========================================

    /// Get game table information
    pub fn get_game_table(&self, table_id: &String) -> Option<GameTableView> {
        game::table::get_table_view(self, table_id)
    }

    /// Get all active tables
    pub fn get_active_tables(&self) -> Vec<GameTableView> {
        game::table::get_active_tables(self)
    }

    /// Get pending bet signals (for backend polling)
    pub fn get_pending_bets(&self, table_id: &String) -> Vec<BetSignal> {
        self.pending_bets.get(table_id).unwrap_or_default()
    }

    /// Get pending move signals (for backend polling)
    pub fn get_pending_moves(&self, table_id: &String) -> Vec<MoveSignal> {
        self.pending_moves.get(table_id).unwrap_or_default()
    }

    /// Clear processed signals (backend calls after processing)
    pub fn clear_processed_signals(&mut self, table_id: String, bet_count: u8, move_count: u8) {
        self.assert_admin();
        game::admin::clear_signals(self, table_id, bet_count, move_count)
    }

    /// Get blackjack statistics
    pub fn get_blackjack_stats(&self) -> &BlackjackStats {
        &self.blackjack_stats
    }

    /// Find available table with open seats
    pub fn find_available_table(&self) -> Option<GameTableView> {
        game::table::find_available_table(self)
    }

    // ========================================
    // ADMIN FUNCTIONS
    // ========================================

    /// Add game admin
    pub fn add_game_admin(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.game_admins.insert(&account_id, &true);
        log!("Added game admin: {}", account_id);
    }

    /// Remove game admin
    pub fn remove_game_admin(&mut self, account_id: &AccountId) {
        self.assert_owner();
        self.game_admins.remove(account_id);
        log!("Removed game admin: {}", account_id);
    }

    /// Update game configuration
    pub fn update_game_config(&mut self, config: GameConfig) {
        self.assert_admin();
        self.game_config = config;
        log!("Game configuration updated by {}", env::predecessor_account_id());
    }

    /// Close a game table (emergency)
    pub fn close_table(&mut self, table_id: String, reason: String) {
        self.assert_admin();
        game::admin::close_table(self, table_id, reason)
    }

    /// Kick specific player by account ID (admin only)
    /// For when player times out or needs to be removed
    pub fn kick_player_by_account(&mut self, account_id: AccountId, reason: String) -> bool {
        self.assert_admin();
        
        // Find the single table (since you only have one)
        let table_id = self.get_single_table_id();
        
        match table_id {
            Some(id) => game::admin::kick_player(self, id, account_id, reason),
            None => {
                log!("No tables found for kicking player {}", account_id);
                false
            }
        }
    }
    
    /// Clear all pending signals (emergency cleanup)
    pub fn clear_all_pending_signals(&mut self, table_id: String) {
        self.assert_admin();
        
        let bet_count = self.pending_bets.get(&table_id).map_or(0, |v| v.len()) as u8;
        let move_count = self.pending_moves.get(&table_id).map_or(0, |v| v.len()) as u8;
        
        // Clear all signals
        self.pending_bets.insert(&table_id, &Vec::new());
        self.pending_moves.insert(&table_id, &Vec::new());
        
        log!("Admin cleared {} bet signals and {} move signals", bet_count, move_count);
        
        self.emit_event(BlackjackEvent::SignalsCleared {
            table_id,
            bet_signals_cleared: bet_count,
            move_signals_cleared: move_count,
            timestamp: env::block_timestamp(),
        });
    }
    
    /// Force end round and refund all bets (emergency)
    pub fn emergency_end_round_with_refunds(&mut self, table_id: String, reason: String) -> u8 {
        self.assert_admin();
        
        let refunded = game::admin::emergency_refund_table(self, table_id.clone());
        
        self.emit_event(BlackjackEvent::EmergencyRefund {
            table_id,
            reason,
            players_refunded: refunded,
            timestamp: env::block_timestamp(),
        });
        
        refunded
    }
    
    /// Get single table ID (helper since you only have one table)
    pub fn get_single_table_id(&self) -> Option<String> {
        self.game_tables.keys().next()
    }
    
    /// Auto-clear processed signals after round completion
    /// Called by backend after each round
    pub fn cleanup_round_signals(&mut self, table_id: String, round_number: u64) {
        self.assert_admin();
        
        // Verify this is for current/completed round
        if let Some(table) = self.game_tables.get(&table_id) {
            require!(
                round_number >= table.round_number, 
                "Cannot clear signals for future rounds"
            );
            
            // Clear all signals since round is complete
            self.pending_bets.insert(&table_id, &Vec::new());
            self.pending_moves.insert(&table_id, &Vec::new());
            
            log!("Cleaned up signals for completed round {}", round_number);
        }
    }
    
    /// Global pause for contract upgrades (owner only)
    pub fn global_pause(&mut self, reason: String) {
        self.assert_owner();
        
        self.is_globally_paused = Some(true);
        self.pause_reason = Some(reason.clone());
        
        // Pause the single table
        if let Some(table_id) = self.get_single_table_id() {
            game::admin::set_table_active(self, table_id, false);
        }
        
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
        
        // Resume the single table
        if let Some(table_id) = self.get_single_table_id() {
            game::admin::set_table_active(self, table_id, true);
        }
        
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

    /// Generate unique table ID
    pub fn generate_table_id(&mut self) -> String {
        self.table_id_nonce += 1;
        format!("table-{}", self.table_id_nonce)
    }

    /// Emit event for logging (internal only)
    fn emit_event<T: Serialize>(&self, event: T) {
        events::emit_event(event)
    }

    // ========================================
    // ENHANCED UTILITY FUNCTIONS
    // ========================================


    /// Enhanced validation for all operations (internal only)
    fn validate_user_operation(&self, account_id: &AccountId, required_tokens: Option<u128>) -> bool {
        // Check storage
        if !crate::tokens::has_sufficient_storage(self, account_id) {
            return false;
        }

        // Check balance if required
        if let Some(required) = required_tokens {
            if self.get_balance(account_id) < required {
                return false;
            }
        }

        // Check if user exists
        if self.accounts.get(account_id).is_none() {
            return false;
        }

        true
    }

    /// Batch operation for gas efficiency
    pub fn batch_burn_cards(&mut self, burns: Vec<(AccountId, u128)>) -> Vec<bool> {
        let mut results = Vec::new();
        
        for (account_id, amount) in burns {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Validate
                if !self.validate_user_operation(&account_id, Some(amount)) {
                    return false;
                }

                if !self.config.valid_burn_amounts.contains(&amount) {
                    return false;
                }

                // Execute burn
                if let Some(mut user) = self.accounts.get(&account_id) {
                    if user.balance >= amount {
                        user.balance -= amount;
                        user.total_burned += amount;
                        
                        self.total_supply = self.total_supply.saturating_sub(amount);
                        self.total_cards_burned += amount;
                        
                        self.accounts.insert(&account_id, &user);
                        return true;
                    }
                }
                false
            }));

            results.push(result.unwrap_or(false));
        }
        
        results
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
        
        let claimed = contract.claim_daily_cards();
        assert_eq!(claimed, 1000);
        assert_eq!(contract.get_balance(&accounts(1)), 1000);
        
        // 4. Create game table ✅
        let table_id = contract.create_game_table(Some("test-table".to_string()));
        assert_eq!(table_id, "test-table");
        
        // 5. Join table ✅
        let joined = contract.join_game_table(table_id.clone(), 1);
        assert!(joined);
        
        // 6. Check game state ✅
        let table_view = contract.get_game_table(&table_id).unwrap();
        assert_eq!(table_view.state, GameState::WaitingForPlayers);
        assert_eq!(table_view.players.len(), 1);
        assert_eq!(table_view.players[0].account_id, accounts(1));
        assert_eq!(table_view.players[0].seat_number, 1);
        
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
        contract.claim_daily_cards(); // Get 1000 tokens
        
        // Player 2 setup
        context.predecessor_account_id = accounts(2);
        context.attached_deposit = NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED);
        testing_env!(context.clone());
        contract.storage_deposit(Some(accounts(2)));
        
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context.clone());
        contract.claim_daily_cards(); // Get 1000 tokens
        
        // Create table and both players join
        let table_id = contract.create_game_table(Some("multi-player".to_string()));
        
        // Player 1 joins
        context.predecessor_account_id = accounts(1);
        testing_env!(context.clone());
        let joined1 = contract.join_game_table(table_id.clone(), 1);
        assert!(joined1);
        
        // Player 2 joins
        context.predecessor_account_id = accounts(2);
        testing_env!(context.clone());
        let joined2 = contract.join_game_table(table_id.clone(), 2);
        assert!(joined2);
        
        // Check table state
        let table_view = contract.get_game_table(&table_id).unwrap();
        assert_eq!(table_view.players.len(), 2);
        assert_eq!(table_view.available_seats, vec![3]); // Only seat 3 left
        
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
        contract.claim_daily_cards();
        
        // Create and join table
        let table_id = contract.create_game_table(Some("betting-test".to_string()));
        contract.join_game_table(table_id.clone(), 1);
        
        // Set table to betting state (as admin)
        context.predecessor_account_id = accounts(0);
        testing_env!(context.clone());
        contract.advance_game_state(table_id.clone(), GameState::Betting);
        
        // Test different bet amounts
        context.predecessor_account_id = accounts(1);
        testing_env!(context.clone());
        
        let initial_balance = contract.get_balance(&accounts(1));
        let initial_supply = contract.total_supply;
        
        // Small bet
        let bet_placed = contract.place_bet(table_id.clone(), 10);
        assert!(bet_placed);
        
        // Verify token burning
        assert_eq!(contract.get_balance(&accounts(1)), initial_balance - 10);
        assert_eq!(contract.total_supply, initial_supply - 10); // Supply reduced by burn
        assert_eq!(contract.total_cards_burned, 10);
        assert_eq!(contract.blackjack_stats.total_tokens_burned_betting, 10);
        
        // Verify player state
        let table_view = contract.get_game_table(&table_id).unwrap();
        assert_eq!(table_view.players[0].burned_tokens, 10);
        
        // Check pending bet signals
        let pending_bets = contract.get_pending_bets(&table_id);
        assert_eq!(pending_bets.len(), 1);
        assert_eq!(pending_bets[0].amount, 10);
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
        contract.claim_daily_cards();
        
        // Create game scenario
        let table_id = contract.create_game_table(Some("full-game".to_string()));
        contract.join_game_table(table_id.clone(), 1);
        
        // Start betting phase (as admin)
        context.predecessor_account_id = accounts(0);
        testing_env!(context.clone());
        contract.advance_game_state(table_id.clone(), GameState::Betting);
        
        // Player bets
        context.predecessor_account_id = accounts(1);
        testing_env!(context.clone());
        
        let balance_before_bet = contract.get_balance(&accounts(1));
        contract.place_bet(table_id.clone(), 50);
        assert_eq!(contract.get_balance(&accounts(1)), balance_before_bet - 50);
        
        // Admin distributes winnings (player wins double)
        context.predecessor_account_id = accounts(0);
        testing_env!(context);
        
        let distribution = WinningsDistribution {
            table_id: table_id.clone(),
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
        
        // Verify signals were cleared
        assert_eq!(contract.get_pending_bets(&table_id).len(), 0);
        assert_eq!(contract.get_pending_moves(&table_id).len(), 0);
    }

    #[test]
    fn test_admin_functions_integration() {
        // Test admin functions work together
        let mut context = get_context(accounts(0), NearToken::from_near(0), 0); // Owner
        testing_env!(context.clone());
        
        let mut contract = CardsContract::new(accounts(0));
        
        // Add game admin
        contract.add_game_admin(accounts(1));
        assert!(contract.game_admins.get(&accounts(1)).unwrap_or(false));
        
        // Admin creates table
        context.predecessor_account_id = accounts(1);
        testing_env!(context.clone());
        let table_id = contract.create_game_table(Some("admin-table".to_string()));
        
        // Admin controls game state
        let advanced = contract.advance_game_state(table_id.clone(), GameState::Betting);
        assert!(advanced);
        
        let table_view = contract.get_game_table(&table_id).unwrap();
        assert_eq!(table_view.state, GameState::Betting);
        
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
            let purchased = contract.purchase_cards(tier_index as u8);
            
            assert_eq!(purchased, tier.cards_amount);
            assert_eq!(contract.get_balance(&accounts(1)), initial_balance + tier.cards_amount);
        }
        
        // Test invalid tier
        context.attached_deposit = NearToken::from_near(0);
        testing_env!(context);
        
        let result = std::panic::catch_unwind(|| {
            contract.purchase_cards(99) // Invalid tier
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
            contract.claim_daily_cards()
        });
        assert!(result.is_err()); // Should fail - no storage
        
        let result = std::panic::catch_unwind(|| {
            contract.burn_cards(10)
        });
        assert!(result.is_err()); // Should fail - no storage
        
        // Test game operations on non-existent table
        let bet_placed = contract.place_bet("fake-table".to_string(), 50);
        assert!(!bet_placed); // Should return false, not panic
        
        let joined = contract.join_game_table("fake-table".to_string(), 1);
        assert!(!joined); // Should return false, not panic
        
        // Test invalid seat numbers
        let table_id = contract.create_game_table(None);
        let joined = contract.join_game_table(table_id, 0); // Invalid seat
        assert!(!joined);
        
        let joined = contract.join_game_table(table_id, 4); // Invalid seat
        assert!(!joined);
    }

    // Import specific test modules
    use tokens::tests as token_tests;
    use game::tests as blackjack_tests;
}