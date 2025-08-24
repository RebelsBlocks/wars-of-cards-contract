use near_sdk::json_types::{U64, U128};
use near_sdk::store::{LookupMap, Vector, IterableSet}; 
use near_sdk::{env, near, AccountId, NearToken, require}; 
use near_sdk::Promise;

// Storage cost calculation based on actual bytes used
// NEAR storage staking: 1E19 yoctoNEAR per byte (100KB per 1 NEAR)
const STORAGE_COST_PER_BYTE: u128 = 10_000_000_000_000_000_000; // 1E19 yoctoNEAR

// Helper function to calculate storage cost for a message
fn calculate_storage_cost(account_id: &AccountId, message: &String) -> NearToken {
    // Estimate bytes for this specific message:
    let account_id_bytes = account_id.as_str().len() as u128;
    let message_bytes = message.len() as u128;
    let timestamp_bytes = 8u128; // U64
    let storage_paid_bytes = 32u128; // U128 as string
    let struct_overhead = 50u128; // Borsh serialization + Vector overhead
    
    let total_bytes = account_id_bytes + message_bytes + timestamp_bytes + storage_paid_bytes + struct_overhead;
    let cost_yocto = total_bytes * STORAGE_COST_PER_BYTE;
    
    // Add 20% safety margin for protocol changes and indexing overhead
    let cost_with_margin = cost_yocto * 120 / 100;
    
    // Return actual calculated cost
    NearToken::from_yoctonear(cost_with_margin)
}

// Message structure - using U128 for storage_paid to handle JSON serialization properly
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Chatter {
    pub account_id: AccountId,  // xyz.testnet lub hex
    pub message: String,        // plain text
    pub timestamp: U64,         // Block timestamp
    pub storage_paid: U128,     // Storage cost in yoctoNEAR as string for JSON
}

// Define the contract structure
#[near(contract_state)]
pub struct Contract {
    chatters: Vector<Chatter>,  // wszystkie wpisy
    // Map of account_id -> deposited balance
    storage_deposits: LookupMap<AccountId, NearToken>,
    // Set of unique users who posted - dla count_chatter
    unique_chatters: IterableSet<AccountId>,  // Changed: UnorderedSet -> IterableSet
    // Total storage fees collected
    total_storage_fees: NearToken,
}

impl Default for Contract {
    fn default() -> Self {
        panic!("Contract should be initialized before usage")
    }
}

// Implement the contract structure
#[near]
impl Contract {
    #[init]
    pub fn new() -> Self {
        // Ensure the contract is not already initialized
        assert!(!env::state_exists(), "Contract is already initialized");
        
        Self {
            chatters: Vector::new(b"chatters".to_vec()),
            storage_deposits: LookupMap::new(b"storage_deposits".to_vec()),
            unique_chatters: IterableSet::new(b"unique_chatters".to_vec()),  // Changed: UnorderedSet -> IterableSet
            total_storage_fees: NearToken::from_yoctonear(0),
        }
    }

    // Public Method - Deposit NEAR tokens for storage fees
    #[payable]
    pub fn deposit_storage(&mut self) {
        let deposit_amount = env::attached_deposit();
        let sender = env::predecessor_account_id();
        
        require!(deposit_amount > NearToken::from_yoctonear(0), "Deposit amount must be greater than 0");
        
        let zero_token = NearToken::from_yoctonear(0);
        let current_balance = self.storage_deposits.get(&sender).unwrap_or(&zero_token);
        let new_balance = current_balance.saturating_add(deposit_amount);
        
        self.storage_deposits.insert(sender.clone(), new_balance);
        
        env::log_str(&format!("User {} deposited {} NEAR for storage. New balance: {} NEAR", 
            sender, deposit_amount.as_near(), new_balance.as_near()));
    }

    // Public Method - Withdraw remaining deposited storage fees
    pub fn withdraw_remain_storage(&mut self, amount: Option<U128>) -> U128 {
        let sender = env::predecessor_account_id();
        let zero_token = NearToken::from_yoctonear(0);
        let current_balance = self.storage_deposits.get(&sender).unwrap_or(&zero_token);
        
        require!(*current_balance > NearToken::from_yoctonear(0), "No storage deposit to withdraw");
        
        let withdraw_amount = if let Some(amount) = amount {
            NearToken::from_yoctonear(amount.into())
        } else {
            *current_balance
        };
        
        require!(withdraw_amount <= *current_balance, "Insufficient balance to withdraw");
        
        let remaining_balance = current_balance.saturating_sub(withdraw_amount);
        
        if remaining_balance == NearToken::from_yoctonear(0) {
            self.storage_deposits.remove(&sender);
        } else {
            self.storage_deposits.insert(sender.clone(), remaining_balance);
        }
        
        // Transfer the tokens back to the user
        Promise::new(sender.clone()).transfer(withdraw_amount);
        
        env::log_str(&format!("User {} withdrew {} NEAR. Remaining balance: {} NEAR", 
            sender, withdraw_amount.as_near(), remaining_balance.as_near()));
        
        U128(withdraw_amount.as_yoctonear())
    }

    // Public Method - Add message po chatter 
    pub fn add_message_po_chatter(&mut self, message: String) {
        let sender = env::predecessor_account_id();
        
        require!(!message.is_empty(), "Message cannot be empty");
        require!(message.len() <= 1000, "Message too long (max 1000 characters)");
        
        // Calculate actual storage cost for this specific message
        let storage_cost = calculate_storage_cost(&sender, &message);
        
        let zero_token = NearToken::from_yoctonear(0);
        let current_balance = self.storage_deposits.get(&sender).unwrap_or(&zero_token);
        require!(*current_balance >= storage_cost, 
            format!("Insufficient storage deposit. Required: {} NEAR, Available: {} NEAR", 
                storage_cost.as_near(), current_balance.as_near()));
        
        // Deduct storage cost from user's deposit
        let remaining_balance = current_balance.saturating_sub(storage_cost);
        
        // Handle zero balance case
        if remaining_balance == NearToken::from_yoctonear(0) {
            self.storage_deposits.remove(&sender);
        } else {
            self.storage_deposits.insert(sender.clone(), remaining_balance);
        }
        
        // Add to total storage fees
        self.total_storage_fees = self.total_storage_fees.saturating_add(storage_cost);
        
        // Add user to unique chatters set
        self.unique_chatters.insert(sender.clone());
        
        let chatter = Chatter {
            account_id: sender.clone(),
            message,
            timestamp: U64(env::block_timestamp()),
            storage_paid: U128(storage_cost.as_yoctonear()),
        };

        self.chatters.push(chatter);
        
        env::log_str(&format!("Chatter added by {}. Storage cost: {} NEAR (calculated). Remaining balance: {} NEAR", 
            sender, storage_cost.as_near(), remaining_balance.as_near()));
    }

    // Public Method - Get messages 
    pub fn get_messages(&self, limit: Option<U64>) -> Vec<Chatter> {
        let limit = u64::from(limit.unwrap_or(U64(100))); // default 100
        let limit = std::cmp::min(limit, 100) as u32; // Max 100 messages per call, convert to u32
        
        let total_messages = self.chatters.len();
        if total_messages == 0 {
            return vec![];
        }
        
        // Get latest messages (from the end)
        let start_index = if total_messages > limit {
            total_messages - limit
        } else {
            0
        };

        self.chatters
            .iter()
            .skip(start_index as usize)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev() // Reverse to show newest first
            .collect()
    }

    // Public Method - Count chatter (licznik ile unikalnych użytkowników)
    pub fn count_chatter(&self) -> U64 {
        U64(self.unique_chatters.len() as u64)
    }

    // Public Method - Get total number of messages
    pub fn total_messages(&self) -> U64 {
        U64(self.chatters.len() as u64)
    }

    // Public Method - Get user's storage deposit balance
    pub fn get_storage_balance(&self, account_id: AccountId) -> U128 {
        let zero_token = NearToken::from_yoctonear(0);
        let balance = self.storage_deposits.get(&account_id).unwrap_or(&zero_token);
        U128(balance.as_yoctonear())
    }

    // Public Method - Get minimum storage cost per message (example for small message)
    pub fn get_min_storage_cost(&self) -> U128 {
        // Calculate cost for a minimal message
        let example_account = "user.testnet".parse().unwrap();
        let minimal_message = "x".to_string();
        let cost = calculate_storage_cost(&example_account, &minimal_message);
        U128(cost.as_yoctonear())
    }
    
    // Public Method - Preview storage cost for a specific message (before posting)
    pub fn preview_storage_cost(&self, account_id: AccountId, message: String) -> U128 {
        require!(!message.is_empty(), "Message cannot be empty");
        require!(message.len() <= 1000, "Message too long (max 1000 characters)");
        
        let cost = calculate_storage_cost(&account_id, &message);
        U128(cost.as_yoctonear())
    }

    // Public Method - Health check
    pub fn health_check(&self) -> String {
        format!("Total messages: {}, Unique chatters: {}, Total storage fees: {} NEAR", 
            self.chatters.len(),
            self.unique_chatters.len(), 
            self.total_storage_fees.as_near())
    }

    // Get messages by specific user
    pub fn get_messages_by_user(&self, account_id: AccountId, limit: Option<U64>) -> Vec<Chatter> {
        let limit = u64::from(limit.unwrap_or(U64(50))) as usize;
        
        self.chatters
            .iter()
            .filter(|chatter| chatter.account_id == account_id)
            .rev() // newest first
            .take(limit)
            .cloned()
            .collect()
    }

    // Check if user has posted before
    pub fn is_chatter(&self, account_id: AccountId) -> bool {
        self.unique_chatters.contains(&account_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, VMContext};

    fn get_context(predecessor_account_id: AccountId) -> VMContext {
        VMContextBuilder::new()
            .predecessor_account_id(predecessor_account_id)
            .attached_deposit(NearToken::from_yoctonear(0))
            .build()
    }

    #[test]
    fn test_guestbook_flow() {
        let mut context = get_context(accounts(0));
        context.attached_deposit = NearToken::from_near(1); // 1 NEAR deposit
        testing_env!(context);
        
        let mut contract = Contract::new();
        
        // Test deposit
        contract.deposit_storage();
        let balance = contract.get_storage_balance(accounts(0));
        assert_eq!(balance.0, NearToken::from_near(1).as_yoctonear());
        
        // Test preview cost
        let test_message = "Hello, this is my first message!".to_string();
        let preview_cost = contract.preview_storage_cost(accounts(0), test_message.clone());
        assert!(preview_cost.0 > 0); // Should have some cost
        
        // Test add message
        contract.add_message_po_chatter(test_message.clone());
        
        // Check counters
        assert_eq!(contract.total_messages(), U64(1));
        assert_eq!(contract.count_chatter(), U64(1)); // one unique user
        assert!(contract.is_chatter(accounts(0)));
        
        // Check messages
        let messages = contract.get_messages(None);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message, test_message);
        assert_eq!(messages[0].account_id, accounts(0));
        assert!(messages[0].timestamp.0 > 0); // Check timestamp exists
        
        // Verify storage cost was calculated and stored correctly
        assert!(messages[0].storage_paid.0 > 0);
    }

    #[test]
    fn test_multiple_users() {
        let mut contract = Contract::new();
        
        // User 1 posts
        let mut context = get_context(accounts(0));
        context.attached_deposit = NearToken::from_near(1);
        testing_env!(context);
        contract.deposit_storage();
        contract.add_message_po_chatter("Message from user 1".to_string());
        
        // User 2 posts
        let mut context = get_context(accounts(1));
        context.attached_deposit = NearToken::from_near(1);
        testing_env!(context);
        contract.deposit_storage();
        contract.add_message_po_chatter("Message from user 2".to_string());
        
        // Check counters
        assert_eq!(contract.total_messages(), U64(2));
        assert_eq!(contract.count_chatter(), U64(2)); // two unique users
        
        // Check messages
        let messages = contract.get_messages(None);
        assert_eq!(messages.len(), 2);
        // Newest first
        assert_eq!(messages[0].message, "Message from user 2");
        assert_eq!(messages[0].account_id, accounts(1));
        assert_eq!(messages[1].message, "Message from user 1");
        assert_eq!(messages[1].account_id, accounts(0));
    }

    #[test]
    fn test_dynamic_storage_costs() {
        let contract = Contract::new();
        
        // Test different message lengths should have different costs
        let short_message = "Hi".to_string();
        let long_message = "A".repeat(500); // 500 character message
        
        let short_cost = contract.preview_storage_cost(accounts(0), short_message);
        let long_cost = contract.preview_storage_cost(accounts(0), long_message);
        
        // Long message should cost more than short message
        assert!(long_cost.0 > short_cost.0);
        
        // Both should have some cost (real storage cost)
        assert!(short_cost.0 > 0);
        assert!(long_cost.0 > 0);
    }
    
    #[test]
    fn test_real_storage_cost_calculation() {
        let contract = Contract::new();
        
        // Test that even tiny messages have real calculated cost (no artificial minimum)
        let tiny_message = "x".to_string();
        let cost = contract.preview_storage_cost(accounts(0), tiny_message);
        
        // Should be real calculated cost, not artificial minimum
        assert!(cost.0 > 0);
        assert!(cost.0 < NearToken::from_millinear(1).as_yoctonear()); // Should be less than 0.001 NEAR for tiny message
    }
}
