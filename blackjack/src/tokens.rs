use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, log, require,
    serde::{Deserialize, Serialize},
    AccountId, NearToken, Promise,
};
use schemars::JsonSchema;
use crate::{CardsContract, events::emit_event};

/// Custom serialization for NearToken to make it JsonSchema compatible
pub mod near_token_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(token: &NearToken, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Serialize::serialize(&token.as_yoctonear().to_string(), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NearToken, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <String as Deserialize>::deserialize(deserializer)?;
        let yocto = s.parse::<u128>().map_err(serde::de::Error::custom)?;
        Ok(NearToken::from_yoctonear(yocto))
    }
}

/// Custom serialization for Option<NearToken>
pub mod near_token_option_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(token: &Option<NearToken>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match token {
            Some(t) => Serialize::serialize(&Some(t.as_yoctonear().to_string()), serializer),
            None => Serialize::serialize(&None::<String>, serializer),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<NearToken>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt_s = <Option<String> as Deserialize>::deserialize(deserializer)?;
        match opt_s {
            Some(s) => {
                let yocto = s.parse::<u128>().map_err(serde::de::Error::custom)?;
                Ok(Some(NearToken::from_yoctonear(yocto)))
            }
            None => Ok(None),
        }
    }
}

/// Time constants (all in nanoseconds)
pub const MINUTE_IN_NS: u64 = 60_000_000_000; // 1 minute
pub const HOUR_IN_NS: u64 = 3_600_000_000_000; // 1 hour  
pub const DAY_IN_NS: u64 = 86_400_000_000_000; // 24 hours

/// User account data
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct UserAccount {
    /// Current card balance
    pub balance: u128,
    /// Timestamp of last claim (nanoseconds)
    pub last_claim_time: u64,
    /// Whether user has deposited storage
    pub storage_deposited: bool,
    /// Total cards claimed by this user
    pub total_claimed: u128,
    /// Total cards purchased by this user  
    pub total_purchased: u128,
    /// Total cards burned by this user
    pub total_burned: u128,
    /// Registration timestamp
    pub registered_at: u64,
}

/// Contract configuration
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractConfig {
    /// Daily claim amount (default: 1000)
    pub daily_claim_amount: u128,
    /// Claim interval in nanoseconds (1 minute = 60_000_000_000)
    pub claim_interval: u64,
    /// Purchase rates (cards per NEAR)
    pub purchase_rates: Vec<PurchaseTier>,
    /// Valid burn amounts
    pub valid_burn_amounts: Vec<u128>,
}

/// Purchase tier definition
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct PurchaseTier {
    /// Amount of NEAR required
    #[serde(with = "near_token_serde")]
    #[schemars(with = "String")]
    pub near_cost: NearToken,
    /// Cards received for this payment
    pub cards_amount: u128,
    /// Display name for this tier
    pub name: String,
}

/// Contract statistics view
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractStats {
    pub total_supply: u128,
    pub total_claimed: u128,
    pub total_purchased: u128,
    pub total_burned: u128,
    pub circulating_supply: u128,
    pub total_users: u64,
    pub active_users: u64, // Users with balance > 0
}

/// User statistics view
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct UserStats {
    pub balance: u128,
    pub last_claim_time: u64,
    pub next_claim_available: u64,
    pub can_claim_now: bool,
    pub total_claimed: u128,
    pub total_purchased: u128,
    pub total_burned: u128,
    pub registered_at: u64,
    pub storage_deposited: bool,
    #[serde(with = "near_token_serde")]
    #[schemars(with = "String")]
    pub storage_deposit_amount: NearToken,
    pub storage_used: u128, // bytes
    pub storage_available: u128, // bytes
}

/// Claim eligibility check (gas-free view function)
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct ClaimEligibility {
    pub can_claim: bool,
    pub reason: String,
    pub next_claim_time: u64,
    pub seconds_until_claim: u64,
    pub claim_amount: u128,
    pub current_balance: u128,
}

/// Storage management structure
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct StorageBalance {
    #[serde(with = "near_token_serde")]
    #[schemars(with = "String")]
    pub total: NearToken,      // Total deposited
    #[serde(with = "near_token_serde")]
    #[schemars(with = "String")]
    pub available: NearToken,  // Available for withdrawal
}

/// Storage deposit bounds
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct StorageBounds {
    #[serde(with = "near_token_serde")]
    #[schemars(with = "String")]
    pub min: NearToken,        // Minimum required deposit
    #[serde(with = "near_token_option_serde")]
    #[schemars(with = "Option<String>")]
    pub max: Option<NearToken>, // Maximum allowed deposit (None = unlimited)
}

/// Admin configuration update payload
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub struct AdminConfigUpdate {
    pub daily_claim_amount: Option<u128>,
    pub claim_interval: Option<u64>,
    pub purchase_rates: Option<Vec<PurchaseTier>>,
}

/// Events for logging
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum CardEvent {
    Claim {
        account_id: AccountId,
        amount: u128,
        timestamp: u64,
    },
    Purchase {
        account_id: AccountId,
        amount: u128,
        cost: NearToken,
        timestamp: u64,
    },
    Burn {
        account_id: AccountId,
        amount: u128,
        timestamp: u64,
    },
    StorageDeposit {
        account_id: AccountId,
        amount: NearToken,
        timestamp: u64,
    },
    StorageWithdraw {
        account_id: AccountId,
        amount: NearToken,
        timestamp: u64,
    },
    ConfigUpdate {
        field: String,
        old_value: String,
        new_value: String,
        updated_by: AccountId,
        timestamp: u64,
    },
}

impl Default for UserAccount {
    fn default() -> Self {
        Self {
            balance: 0,
            last_claim_time: 0,
            storage_deposited: false,
            total_claimed: 0,
            total_purchased: 0,
            total_burned: 0,
            registered_at: env::block_timestamp(),
        }
    }
}

impl Default for ContractConfig {
    fn default() -> Self {
        Self {
            daily_claim_amount: 1000,
            claim_interval: MINUTE_IN_NS, // 1 minute for dev/testnet, change to DAY_IN_NS for mainnet
            purchase_rates: vec![
                PurchaseTier {
                    near_cost: NearToken::from_near(1),
                    cards_amount: 1000,
                    name: "Basic Pack".to_string(),
                },
                PurchaseTier {
                    near_cost: NearToken::from_near(2),
                    cards_amount: 2200,
                    name: "Value Pack".to_string(),
                },
                PurchaseTier {
                    near_cost: NearToken::from_near(5),
                    cards_amount: 7000,
                    name: "Premium Pack".to_string(),
                },
                PurchaseTier {
                    near_cost: NearToken::from_near(10),
                    cards_amount: 20000,
                    name: "Ultimate Pack".to_string(),
                },
            ],
            valid_burn_amounts: vec![10, 30, 50, 100],
        }
    }
}

// ========================================
// TOKEN MANAGEMENT FUNCTIONS
// ========================================

/// Deposit storage for user account
pub fn storage_deposit(contract: &mut CardsContract, account_id: Option<AccountId>) -> StorageBalance {
    use crate::storage::calculate_user_storage_cost;
    
    let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
    let deposit = env::attached_deposit();
    
    // Calculate actual storage cost for this specific account
    let required_storage = calculate_user_storage_cost(&account_id);
    
    require!(
        deposit >= required_storage,
        format!("Minimum storage deposit is {} NEAR for account {}", 
            required_storage.as_near(), account_id)
    );

    // Get existing deposit or create new
    let current_deposit = contract.storage_deposits.get(&account_id).unwrap_or(NearToken::from_near(0));
    let new_total = NearToken::from_yoctonear(current_deposit.as_yoctonear() + deposit.as_yoctonear());
    
    // Update storage deposit
    contract.storage_deposits.insert(&account_id, &new_total);
    
    // Create or update user account
    let mut user = contract.accounts.get(&account_id).unwrap_or_default();
    user.storage_deposited = true;
    if user.registered_at == 0 {
        user.registered_at = env::block_timestamp();
    }
    contract.accounts.insert(&account_id, &user);

    // Log event
    emit_event(CardEvent::StorageDeposit {
        account_id: account_id.clone(),
        amount: deposit,
        timestamp: env::block_timestamp(),
    });

    log!("Storage deposited: {} by {} (required: {})", 
        deposit.as_near(), account_id, required_storage.as_near());

    StorageBalance {
        total: new_total,
        available: NearToken::from_yoctonear(
            new_total.as_yoctonear().saturating_sub(required_storage.as_yoctonear())
        ),
    }
}

/// Withdraw unused storage deposit
pub fn storage_withdraw(contract: &mut CardsContract, amount: Option<NearToken>) -> StorageBalance {
    use crate::storage::calculate_user_storage_cost;
    
    let account_id = env::predecessor_account_id();
    let current_deposit = contract.storage_deposits.get(&account_id)
        .expect("No storage deposit found");

    let required_storage = calculate_user_storage_cost(&account_id);
    let available = current_deposit.as_yoctonear().saturating_sub(required_storage.as_yoctonear());
    let withdraw_amount = amount.map_or(available, |a| a.as_yoctonear().min(available));
    
    require!(withdraw_amount > 0, "No funds available for withdrawal");

    let new_deposit = NearToken::from_yoctonear(current_deposit.as_yoctonear() - withdraw_amount);
    contract.storage_deposits.insert(&account_id, &new_deposit);

    // Log event
    emit_event(CardEvent::StorageWithdraw {
        account_id: account_id.clone(),
        amount: NearToken::from_yoctonear(withdraw_amount),
        timestamp: env::block_timestamp(),
    });

    // Transfer withdrawn amount
    Promise::new(account_id).transfer(NearToken::from_yoctonear(withdraw_amount));

    StorageBalance {
        total: new_deposit,
        available: NearToken::from_yoctonear(new_deposit.as_yoctonear().saturating_sub(required_storage.as_yoctonear())),
    }
}

/// Get storage balance for account
pub fn storage_balance_of(contract: &CardsContract, account_id: &AccountId) -> Option<StorageBalance> {
    use crate::storage::calculate_user_storage_cost;
    
    contract.storage_deposits.get(account_id).map(|total| {
        let required_storage = calculate_user_storage_cost(account_id);
        StorageBalance {
            total,
            available: NearToken::from_yoctonear(
                total.as_yoctonear().saturating_sub(required_storage.as_yoctonear())
            ),
        }
    })
}

/// Get storage bounds
pub fn storage_balance_bounds(_contract: &CardsContract) -> StorageBounds {
    use crate::storage::STORAGE_DEPOSIT_REQUIRED;
    
    StorageBounds {
        min: NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED), // Conservative minimum
        max: None,
    }
}

/// Get exact storage cost for a specific account
pub fn get_storage_cost_for_account(_contract: &CardsContract, account_id: &AccountId) -> NearToken {
    use crate::storage::calculate_user_storage_cost;
    calculate_user_storage_cost(account_id)
}

/// Claim daily cards
pub fn claim_daily_cards(contract: &mut CardsContract) -> u128 {
    let account_id = env::predecessor_account_id();
    
    require!(
        has_sufficient_storage(contract, &account_id),
        "Storage deposit required. Call storage_deposit() first."
    );

    let mut user = contract.accounts.get(&account_id)
        .expect("User account not found");

    let current_time = env::block_timestamp();
    let time_since_last = current_time - user.last_claim_time;
    
    require!(
        time_since_last >= contract.config.claim_interval,
        format!("Must wait {} seconds between claims", 
            (contract.config.claim_interval - time_since_last) / 1_000_000_000)
    );

    // Update user stats
    user.balance += contract.config.daily_claim_amount;
    user.last_claim_time = current_time;
    user.total_claimed += contract.config.daily_claim_amount;
    
    // Update contract stats
    contract.total_supply += contract.config.daily_claim_amount;
    contract.total_cards_claimed += contract.config.daily_claim_amount;
    
    // Save user
    contract.accounts.insert(&account_id, &user);

    // Log event
    emit_event(CardEvent::Claim {
        account_id: account_id.clone(),
        amount: contract.config.daily_claim_amount,
        timestamp: current_time,
    });

    log!("Daily claim: {} cards claimed by {}", contract.config.daily_claim_amount, account_id);

    contract.config.daily_claim_amount
}

/// Purchase cards with NEAR deposit
/// tier_index: 0=Basic, 1=Value, 2=Premium, 3=Ultimate
pub fn purchase_cards(contract: &mut CardsContract, tier_index: u8) -> u128 {
    let account_id = env::predecessor_account_id();
    
    require!(
        has_sufficient_storage(contract, &account_id),
        "Storage deposit required. Call storage_deposit() first."
    );

    // Get tier by index (0-3)
    require!(
        (tier_index as usize) < contract.config.purchase_rates.len(),
        format!("Invalid tier index {}. Valid range: 0-{}", tier_index, contract.config.purchase_rates.len() - 1)
    );
    
    let tier = &contract.config.purchase_rates[tier_index as usize];
    let deposit = env::attached_deposit();

    // Verify the attached deposit matches the tier cost
    require!(
        deposit >= tier.near_cost,
        format!("Insufficient deposit. Required: {} NEAR, Attached: {} NEAR", 
            tier.near_cost.as_near(), 
            deposit.as_near())
    );

    // CRITICAL FIX: Update state BEFORE external calls to prevent re-entrancy
    // Get or create user
    let mut user = contract.accounts.get(&account_id).unwrap_or_default();
    if !user.storage_deposited {
        user.storage_deposited = true;
        user.registered_at = env::block_timestamp();
    }

    // Update user stats
    user.balance = user.balance.checked_add(tier.cards_amount)
        .expect("Balance overflow in purchase_cards");
    user.total_purchased = user.total_purchased.checked_add(tier.cards_amount)
        .expect("Total purchased overflow");
    
    // Update contract stats
    contract.total_supply = contract.total_supply.checked_add(tier.cards_amount)
        .expect("Total supply overflow");
    contract.total_cards_purchased = contract.total_cards_purchased.checked_add(tier.cards_amount)
        .expect("Total cards purchased overflow");
    
    // Save user BEFORE external calls
    contract.accounts.insert(&account_id, &user);

    // EXTERNAL CALLS AFTER STATE CHANGES
    // If user overpaid, refund the excess
    if deposit > tier.near_cost {
        let excess = NearToken::from_yoctonear(deposit.as_yoctonear() - tier.near_cost.as_yoctonear());
        Promise::new(account_id.clone()).transfer(excess);
    }

    // Transfer payment to contract owner
    Promise::new(contract.owner_id.clone()).transfer(tier.near_cost);

    // Log event
    emit_event(CardEvent::Purchase {
        account_id: account_id.clone(),
        amount: tier.cards_amount,
        cost: tier.near_cost,
        timestamp: env::block_timestamp(),
    });

    log!("Purchase: {} cards bought by {} for {} NEAR (tier {})", 
        tier.cards_amount, account_id, tier.near_cost.as_near(), tier_index);

    tier.cards_amount
}

/// Burn cards (destroy them permanently)
pub fn burn_cards(contract: &mut CardsContract, amount: u128) {
    let account_id = env::predecessor_account_id();
    
    // Enhanced validation
    require!(
        contract.config.valid_burn_amounts.contains(&amount),
        format!("Invalid burn amount. Valid amounts: {:?}", contract.config.valid_burn_amounts)
    );

    let mut user = contract.accounts.get(&account_id)
        .expect("User account not found");

    require!(user.balance >= amount, "Insufficient card balance");

    // Overflow protection
    require!(
        contract.total_supply >= amount,
        "Cannot burn more than total supply"
    );

    // Update user stats with checked arithmetic
    user.balance = user.balance.checked_sub(amount)
        .expect("Balance underflow in burn_cards");
    user.total_burned = user.total_burned.checked_add(amount)
        .expect("Total burned overflow");
    
    // Update contract stats (reduce total supply)
    contract.total_supply = contract.total_supply.checked_sub(amount)
        .expect("Total supply underflow");
    contract.total_cards_burned = contract.total_cards_burned.checked_add(amount)
        .expect("Total cards burned overflow");
    
    // Save user
    contract.accounts.insert(&account_id, &user);

    // Log event
    emit_event(CardEvent::Burn {
        account_id: account_id.clone(),
        amount,
        timestamp: env::block_timestamp(),
    });

    log!("Burn: {} cards burned by {}", amount, account_id);
}

// ========================================
// VIEW FUNCTIONS
// ========================================

/// Check if user can claim cards (gas-free)
pub fn check_claim_eligibility(contract: &CardsContract, account_id: &AccountId) -> ClaimEligibility {
    let current_time = env::block_timestamp();
    
    if let Some(user) = contract.accounts.get(account_id) {
        if !user.storage_deposited {
            return ClaimEligibility {
                can_claim: false,
                reason: "Storage deposit required. Call storage_deposit() first.".to_string(),
                next_claim_time: 0,
                seconds_until_claim: 0,
                claim_amount: 0,
                current_balance: user.balance,
            };
        }
        
        let time_since_last = current_time - user.last_claim_time;
        if time_since_last < contract.config.claim_interval {
            let next_claim = user.last_claim_time + contract.config.claim_interval;
            let seconds_remaining = (next_claim - current_time) / 1_000_000_000;
            
            return ClaimEligibility {
                can_claim: false,
                reason: format!("Must wait {} seconds between claims", seconds_remaining),
                next_claim_time: next_claim,
                seconds_until_claim: seconds_remaining,
                claim_amount: contract.config.daily_claim_amount,
                current_balance: user.balance,
            };
        }
        
        ClaimEligibility {
            can_claim: true,
            reason: "Ready to claim!".to_string(),
            next_claim_time: current_time + contract.config.claim_interval,
            seconds_until_claim: 0,
            claim_amount: contract.config.daily_claim_amount,
            current_balance: user.balance,
        }
        
    } else {
        ClaimEligibility {
            can_claim: false,
            reason: "Account not registered. Call storage_deposit() first.".to_string(),
            next_claim_time: 0,
            seconds_until_claim: 0,
            claim_amount: 0,
            current_balance: 0,
        }
    }
}

/// Get user card balance
pub fn get_balance(contract: &CardsContract, account_id: &AccountId) -> u128 {
    contract.accounts.get(account_id)
        .map_or(0, |user| user.balance)
}

/// Get detailed user statistics
pub fn get_user_stats(contract: &CardsContract, account_id: &AccountId) -> Option<UserStats> {
    use crate::storage::calculate_user_storage_cost;
    
    let user = contract.accounts.get(account_id)?;
    let storage_deposit = contract.storage_deposits.get(account_id)
        .unwrap_or(NearToken::from_near(0));
    
    let required_storage = calculate_user_storage_cost(account_id);
    let available_for_purchases = storage_deposit.as_yoctonear().saturating_sub(required_storage.as_yoctonear());
    
    Some(UserStats {
        balance: user.balance,
        last_claim_time: user.last_claim_time,
        next_claim_available: user.last_claim_time + contract.config.claim_interval,
        can_claim_now: can_user_claim(contract, account_id),
        total_claimed: user.total_claimed,
        total_purchased: user.total_purchased,
        total_burned: user.total_burned,
        registered_at: user.registered_at,
        storage_deposited: user.storage_deposited,
        storage_deposit_amount: storage_deposit,
        storage_used: required_storage.as_yoctonear(),
        storage_available: available_for_purchases,
    })
}

/// Get contract statistics
pub fn get_contract_stats(contract: &CardsContract) -> ContractStats {
    let mut active_users = 0;
    let mut total_users = 0;
    
    for (_, user) in contract.accounts.iter() {
        total_users += 1;
        if user.balance > 0 {
            active_users += 1;
        }
    }
    
    ContractStats {
        total_supply: contract.total_supply,
        total_claimed: contract.total_cards_claimed,
        total_purchased: contract.total_cards_purchased,
        total_burned: contract.total_cards_burned,
        circulating_supply: contract.total_supply.saturating_sub(contract.total_cards_burned),
        total_users,
        active_users,
    }
}

/// Get purchase tiers
pub fn get_purchase_tiers(contract: &CardsContract) -> &Vec<PurchaseTier> {
    &contract.config.purchase_rates
}

/// Get tier info by index (0-3)
pub fn get_tier_info(contract: &CardsContract, tier_index: u8) -> Option<&PurchaseTier> {
    contract.config.purchase_rates.get(tier_index as usize)
}

/// Get valid burn amounts
pub fn get_valid_burn_amounts(contract: &CardsContract) -> &Vec<u128> {
    &contract.config.valid_burn_amounts
}

/// Get contract configuration
pub fn get_config(contract: &CardsContract) -> &ContractConfig {
    &contract.config
}

/// Update contract configuration (Owner only)
pub fn update_config(contract: &mut CardsContract, update: AdminConfigUpdate) {
    contract.assert_owner();
    
    let timestamp = env::block_timestamp();
    
    if let Some(new_amount) = update.daily_claim_amount {
        let old_amount = contract.config.daily_claim_amount;
        contract.config.daily_claim_amount = new_amount;
        
        emit_event(CardEvent::ConfigUpdate {
            field: "daily_claim_amount".to_string(),
            old_value: old_amount.to_string(),
            new_value: new_amount.to_string(),
            updated_by: env::predecessor_account_id(),
            timestamp,
        });
    }
    
    if let Some(new_interval) = update.claim_interval {
        let old_interval = contract.config.claim_interval;
        contract.config.claim_interval = new_interval;
        
        emit_event(CardEvent::ConfigUpdate {
            field: "claim_interval".to_string(),
            old_value: format!("{}s", old_interval / 1_000_000_000),
            new_value: format!("{}s", new_interval / 1_000_000_000),
            updated_by: env::predecessor_account_id(),
            timestamp,
        });
    }
    
    if let Some(new_rates) = update.purchase_rates {
        contract.config.purchase_rates = new_rates;
        
        emit_event(CardEvent::ConfigUpdate {
            field: "purchase_rates".to_string(),
            old_value: "updated".to_string(),
            new_value: "updated".to_string(),
            updated_by: env::predecessor_account_id(),
            timestamp,
        });
    }
    
    log!("Contract configuration updated by {}", env::predecessor_account_id());
}

// ========================================
// INTERNAL HELPER FUNCTIONS
// ========================================

/// Check if user has sufficient storage deposited
pub fn has_sufficient_storage(contract: &CardsContract, account_id: &AccountId) -> bool {
    use crate::storage::calculate_user_storage_cost;
    
    let deposit = contract.storage_deposits.get(account_id).unwrap_or(NearToken::from_near(0));
    let required = calculate_user_storage_cost(account_id);
    deposit.as_yoctonear() >= required.as_yoctonear()
}

/// Check if user can claim based on last claim time
pub fn can_user_claim(contract: &CardsContract, account_id: &AccountId) -> bool {
    if let Some(user) = contract.accounts.get(account_id) {
        if !user.storage_deposited {
            return false;
        }
        
        let current_time = env::block_timestamp();
        let time_since_last_claim = current_time - user.last_claim_time;
        
        time_since_last_claim >= contract.config.claim_interval
    } else {
        false
    }
}

// ========================================
// TESTS MODULE
// ========================================

#[cfg(test)]
pub mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, VMContext};
    use crate::storage::STORAGE_DEPOSIT_REQUIRED;

    fn get_context(predecessor: AccountId) -> VMContext {
        VMContextBuilder::new()
            .current_account_id(accounts(0))
            .predecessor_account_id(predecessor)
            .build()
    }

    #[test]
    pub fn test_claim_cards() {
        let mut context = get_context(accounts(1));
        context.attached_deposit = NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED);
        testing_env!(context);
        
        let mut contract = crate::CardsContract::new(accounts(0));
        storage_deposit(&mut contract, None);
        
        let claimed = claim_daily_cards(&mut contract);
        assert_eq!(claimed, 1000);
        assert_eq!(get_balance(&contract, &accounts(1)), 1000);
        assert_eq!(contract.total_supply, 1000);
        assert_eq!(contract.total_cards_claimed, 1000);
    }

    #[test]
    pub fn test_purchase_cards_basic() {
        let mut context = get_context(accounts(1));
        testing_env!(context.clone());
        
        let mut contract = crate::CardsContract::new(accounts(0));
        
        // First deposit storage
        context.attached_deposit = NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED);
        testing_env!(context.clone());
        storage_deposit(&mut contract, None);
        
        // Now purchase Basic Pack (tier 0) with direct NEAR payment
        context.attached_deposit = NearToken::from_near(1); // Exact tier cost
        testing_env!(context.clone());
        let purchased = purchase_cards(&mut contract, 0);
        assert_eq!(purchased, 1000);
        assert_eq!(get_balance(&contract, &accounts(1)), 1000);
        assert_eq!(contract.total_cards_purchased, 1000);
    }

    #[test]
    pub fn test_burn_cards() {
        let mut context = get_context(accounts(1));
        context.attached_deposit = NearToken::from_yoctonear(STORAGE_DEPOSIT_REQUIRED);
        testing_env!(context);
        
        let mut contract = crate::CardsContract::new(accounts(0));
        storage_deposit(&mut contract, None);
        claim_daily_cards(&mut contract);
        
        // Burn some cards
        burn_cards(&mut contract, 10);
        assert_eq!(get_balance(&contract, &accounts(1)), 990); // 1000 - 10
        assert_eq!(contract.total_cards_burned, 10);
        assert_eq!(contract.total_supply, 990); // Supply reduced by burn
    }
}