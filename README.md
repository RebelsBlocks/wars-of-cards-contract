# NEAR Chatter Contract

A smart contract for NEAR Protocol that allows users to leave messages in a public chat. Users pay for their own storage costs based on actual message size.

## 🎯 Features

- **Dynamic Storage Costs** - Pay only for the storage your message actually uses
- **Fair Pricing** - No artificial minimums, real costs based on NEAR storage staking (1E19 yoctoNEAR per byte)
- **Storage Management** - Users deposit NEAR tokens for storage, withdraw remaining balance anytime
- **Message History** - View all messages with pagination
- **User Statistics** - Track unique users and total messages

## 💰 Storage Costs

Your cost depends on your message length:

- **Short message (50 chars)**: ~0.0016 NEAR
- **Medium message (200 chars)**: ~0.0034 NEAR  
- **Long message (500 chars)**: ~0.0072 NEAR
- **Maximum message (1000 chars)**: ~0.0132 NEAR

*Costs include 20% safety margin for protocol overhead*

## 🚀 Quick Start

### Build Contract

```bash
# Modern way (recommended)
cargo near build

# Manual way (if needed)
cargo build --target wasm32-unknown-unknown --release --package chatter
```

### Deploy Contract

**New NEAR CLI (recommended):**
```bash
# Deploy with initialization
near contract deploy your-contract.testnet use-file target/near/chatter.wasm with-init-call new json-args '{}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' network-config testnet now
```

**Old NEAR CLI:**
```bash
# Deploy
near deploy --wasmFile target/near/chatter.wasm --accountId your-contract.testnet

# Initialize
near call your-contract.testnet new '{}' --accountId your-contract.testnet
```

### Basic Usage

```bash
# 1. Deposit storage (0.01 NEAR covers ~7-8 messages)
near call your-contract.testnet deposit_storage '{}' --accountId alice.testnet --deposit 0.01

# 2. Check cost before posting (optional)
near view your-contract.testnet preview_storage_cost '{"account_id": "alice.testnet", "message": "Hello world!"}'

# 3. Post message
near call your-contract.testnet add_message_po_chatter '{"message": "Hello from the guestbook!"}' --accountId alice.testnet

# 4. View messages
near view your-contract.testnet get_messages '{"limit": "10"}'

# 5. Check your remaining balance
near view your-contract.testnet get_storage_balance '{"account_id": "alice.testnet"}'

# 6. Withdraw remaining balance
near call your-contract.testnet withdraw_remain_storage '{}' --accountId alice.testnet
```

## 📋 Contract API

### State-Changing Methods (require transaction)

#### `deposit_storage()`
Deposit NEAR tokens for storage fees.
- **Payable**: Attach NEAR tokens
- **Example**: `--deposit 0.01`

#### `add_message_po_chatter(message: String)`  
Post a message to the guestbook.
- **Parameters**: `message` (max 1000 characters)
- **Cost**: Dynamically calculated based on message size
- **Example**: `'{"message": "Hello world!"}'`

#### `withdraw_remain_storage(amount?: U128)`
Withdraw remaining storage balance.
- **Parameters**: `amount` (optional U128 as string, withdraws all if not specified)
- **Returns**: Amount withdrawn as U128
- **Example**: `'{"amount": "1000000000000000000000000"}'` or `'{}'` for all

### View Methods (free to call)

#### `get_messages(limit?: U64)`
Get recent messages (newest first).
- **Parameters**: `limit` (default: 100, max: 100)
- **Returns**: Array of `Chatter` objects

#### `preview_storage_cost(account_id: AccountId, message: String)`
Preview storage cost before posting.
- **Returns**: Cost in yoctoNEAR as U128 string

#### `get_storage_balance(account_id: AccountId)`
Get user's current storage balance.
- **Returns**: Balance in yoctoNEAR as U128 string

#### `total_messages()`
Get total number of messages.
- **Returns**: Number as U64 string

#### `count_chatter()`  
Get number of unique users who posted.
- **Returns**: Number as U64 string

#### `is_chatter(account_id: AccountId)`
Check if user has posted before.
- **Returns**: Boolean

#### `get_messages_by_user(account_id: AccountId, limit?: U64)`
Get messages from specific user.
- **Returns**: Array of `Chatter` objects

#### `health_check()`
Get contract status and statistics.
- **Returns**: Status string

#### `get_min_storage_cost()`
Get minimum storage cost (for smallest possible message).
- **Returns**: Cost in yoctoNEAR as U128 string

## 📱 Frontend Integration

### Setup
```javascript
import { Contract } from 'near-api-js';

const contract = new Contract(account, 'your-contract.testnet', {
  viewMethods: [
    'get_messages', 'preview_storage_cost', 'get_storage_balance', 
    'total_messages', 'count_chatter', 'health_check', 'get_min_storage_cost',
    'get_messages_by_user', 'is_chatter'
  ],
  changeMethods: [
    'deposit_storage', 'add_message_po_chatter', 'withdraw_remain_storage'
  ]
});
```

### Usage Examples
```javascript
// Deposit storage
await contract.deposit_storage({}, "300000000000000", utils.format.parseNearAmount("0.01"));

// Preview cost
const cost = await contract.preview_storage_cost({
  account_id: wallet.accountId,
  message: "Hello world!"
});
console.log(`Cost: ${utils.format.formatNearAmount(cost)} NEAR`);

// Post message  
await contract.add_message_po_chatter(
  { message: "Hello from frontend!" },
  "300000000000000" // 300 TGas
);

// Get messages
const messages = await contract.get_messages({ limit: "50" });

// Check if user posted before
const isChatter = await contract.is_chatter({ account_id: wallet.accountId });

// Get user's messages only
const userMessages = await contract.get_messages_by_user({ 
  account_id: wallet.accountId, 
  limit: "20" 
});
```

## 🏗️ Data Structures

### Chatter Object
```typescript
interface Chatter {
  account_id: string;      // "alice.testnet"
  message: string;         // "Hello world!"
  timestamp: string;       // "1640995200000000000" (nanoseconds as U64 string)
  storage_paid: string;    // "1500000000000000000000" (yoctoNEAR as U128 string)
}
```

## ⚡ Gas Costs

- **deposit_storage**: ~2-3 TGas
- **add_message_po_chatter**: ~5-10 TGas  
- **withdraw_remain_storage**: ~3-5 TGas
- **View methods**: Free

*Always attach extra gas for safety (300 TGas recommended)*

## 🧪 Testing

```bash
# Run tests
cargo test

# Run with logs
cargo test -- --nocapture

# Build and test in one go
cargo near build && cargo test
```

## 📊 Storage Calculation

The contract calculates storage cost using:
```
Cost = (account_id_bytes + message_bytes + metadata_bytes + overhead) × 1E19 × 1.2
```

Where:
- `account_id_bytes`: Length of your account ID
- `message_bytes`: Length of your message
- `metadata_bytes`: Timestamp + storage_paid field (~40 bytes)  
- `overhead`: Borsh serialization + Vector overhead (~50 bytes)
- `1E19`: NEAR storage cost per byte (10^19 yoctoNEAR)
- `1.2`: 20% safety margin

## 🛡️ Security Features

- **Early validation** on all inputs
- **Reentrancy protection** - state updated before external calls
- **Balance checks** before message posting
- **Gas optimization** with efficient storage prefixes
- **No artificial limits** - pay only real storage costs
- **Modern SDK** - uses `IterableSet` for better performance

## 📄 License

MIT License
