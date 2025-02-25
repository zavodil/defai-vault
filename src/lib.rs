use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near, near_bindgen, AccountId, BorshStorageKey, Gas, NearSchema, NearToken,
    PanicOnDefault, Promise, PromiseOrValue, Timestamp,
};
use std::cmp::PartialEq;
use std::str::FromStr;

mod events;
mod leaderboard;
mod token_receiver;

use leaderboard::{Leaderboard, LeaderboardItem};

type Balance = u128;
type TwitterId = u128;

const DEFAULT_LOCKTIME_IN_MS: u64 = 86_400_000;
const MAX_ASSETS_IN_CAPITAL_ALLOCATION: usize = 7;
const GAS_FT_TRANSFER_CALL: Gas = Gas::from_tgas(25);
const GAS_FT_TRANSFER: Gas = Gas::from_tgas(2);
const GAS_WITHDRAW_CAPITAL: Gas = Gas::from_tgas(10);
const MIN_NEAR_DEPOSIT: NearToken = NearToken::from_millinear(10);
const USDC_CONTRACT_ID: &str = "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1";
const MIN_USDC_DEPOSIT: u128 = 100_000; // 0.1 USDC
const INTENTS_CONTRACT_ID: &str = "intents.near";

#[ext_contract(ext_ft)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn ft_transfer_call(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>, msg: String);
}

#[near(serializers = [json, borsh])]
#[derive(PanicOnDefault)]
pub struct AssetPosition {
    pub token_id: AccountId,
    pub amount: Balance,
}

#[near(serializers = [json])]
#[derive(PanicOnDefault)]
pub struct AssetPositionOutput {
    pub token_id: AccountId,
    pub amount: U128,
}


#[derive(BorshDeserialize, BorshSerialize, Serialize, NearSchema, PartialEq)]
#[borsh(crate = "near_sdk::borsh")]
#[serde(crate = "near_sdk::serde")]
pub enum CapitalAllocationStatus {
    Active,
    Withdrawn,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, NearSchema)]
#[borsh(crate = "near_sdk::borsh")]
#[serde(crate = "near_sdk::serde")]
pub struct CapitalAllocation {
    pub owner_id: AccountId,
    pub status: CapitalAllocationStatus,
    pub positions: Vec<AssetPosition>,
    pub entry_timestamp: Timestamp,
    pub exit_timestamp: Timestamp,
    pub entry_value: AssetPosition,
    pub exit_value: Option<AssetPosition>,
}

#[derive(Deserialize, PanicOnDefault)]
#[serde(crate = "near_sdk::serde")]
pub struct CapitalAllocationInput {
    pub owner_id: AccountId,
    pub entry_value: AssetPosition,
}

#[derive(BorshDeserialize, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct TwitterNearAccount {
    pub twitter_id: TwitterId,
    pub near_account_id: AccountId,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    agent: String,
    agent_account_id: AccountId,

    locktime: u64,

    near_deposits: LookupMap<TwitterNearAccount, Balance>,
    usdc_deposits: LookupMap<TwitterNearAccount, Balance>,

    leaderboard: Leaderboard,

    capital: LookupMap<u64, CapitalAllocation>,
    next_capital_id: u64,
}

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    NearDeposits,
    UsdcDeposits,
    CapitalAllocations,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(agent: String, agent_account_id: AccountId, locktime: Option<u64>) -> Self {
        // time to lock capital in ms
        let locktime = locktime.unwrap_or(DEFAULT_LOCKTIME_IN_MS);
        Self {
            agent,
            agent_account_id,

            locktime,

            near_deposits: LookupMap::new(StorageKey::NearDeposits),
            usdc_deposits: LookupMap::new(StorageKey::UsdcDeposits),

            leaderboard: Leaderboard::default(),

            capital: LookupMap::new(StorageKey::CapitalAllocations),
            next_capital_id: 0,
        }
    }

    #[private]
    pub fn set_agent(&mut self, agent: String) {
        self.agent = agent;
    }

    #[private]
    pub fn set_agent_account_id(&mut self, agent_account_id: AccountId) {
        self.agent_account_id = agent_account_id;
    }

    #[private]
    pub fn set_locktime(&mut self, locktime: u64) {
        self.locktime = locktime;
    }

    pub fn get_locktime(&self) -> u64 {
        self.locktime.clone()
    }

    #[payable]
    pub fn deposit_near(&mut self, twitter_id: U128) {
        let deposit = env::attached_deposit();
        let near_account_id = env::predecessor_account_id();

        let key = TwitterNearAccount {
            twitter_id: twitter_id.0,
            near_account_id: near_account_id.clone(),
        };

        let balance = self.near_deposits.get(&key).unwrap_or(0);
        let new_balance = balance + deposit.as_yoctonear();
        assert!(
            new_balance >= MIN_NEAR_DEPOSIT.as_yoctonear(),
            "Deposit must be at least 0.01 NEAR"
        );

        events::emit::run_agent(
            &self.agent,
            &serde_json::json!(
                {
                    "action": "deposit_near".to_string(),
                    "account_id": near_account_id,
                    "twitter_id": twitter_id,
                    "deposit": deposit.to_string(),
                }
            )
            .to_string(),
        );

        self.near_deposits.insert(&key, &new_balance);
    }

    pub fn get_near_balance(&self, twitter_id: U128, near_account_id: AccountId) -> U128 {
        if let Some(account) = self.near_deposits.get(&TwitterNearAccount {
            twitter_id: twitter_id.0,
            near_account_id,
        }) {
            U128::from(account)
        } else {
            U128(0)
        }
    }
    pub fn get_usdc_balance(&self, twitter_id: U128, near_account_id: AccountId) -> U128 {
        if let Some(account) = self.usdc_deposits.get(&TwitterNearAccount {
            twitter_id: twitter_id.0,
            near_account_id,
        }) {
            U128::from(account)
        } else {
            U128(0)
        }
    }

    pub fn withdraw_near(&mut self, twitter_id: U128, near_account_id: AccountId) {
        self.assert_agent();
        let key = TwitterNearAccount {
            twitter_id: twitter_id.0,
            near_account_id,
        };
        let balance = self
            .near_deposits
            .get(&key)
            .expect("Account not found");
        assert!(balance > 0, "No balance to withdraw");
        self.near_deposits.insert(&key, &0);
        Promise::new(self.agent_account_id.clone()).transfer(NearToken::from_yoctonear(balance));
    }

    pub fn withdraw_usdc(&mut self, twitter_id: U128, near_account_id: AccountId, amount: Option<U128>) {
        self.assert_agent();
        let key = TwitterNearAccount {
            twitter_id: twitter_id.0,
            near_account_id,
        };
        let balance = self
            .usdc_deposits
            .get(&key)
            .expect("Twitter account not found");
        assert!(balance > 0, "No balance to withdraw");
        let amount = amount.unwrap_or(U128::from(balance));
        assert!(balance >= amount.0, "Not enough balance to withdraw");
        self.usdc_deposits.insert(&key, &(balance - amount.0));

        ext_ft::ext(AccountId::from_str(USDC_CONTRACT_ID).unwrap())
            .with_static_gas(GAS_FT_TRANSFER)
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .ft_transfer(self.agent_account_id.clone(), U128::from(amount.0), None);
    }

    pub fn get_capital_allocation(
        &self,
        capital_id: u64,
    ) -> (bool, AccountId, Timestamp, Vec<AssetPositionOutput>) {
        let capital = self
            .capital
            .get(&capital_id)
            .expect("Capital Allocation not found");

        (
            capital.status == CapitalAllocationStatus::Active,
            capital.owner_id,
            capital.exit_timestamp,
            capital.positions.iter().map(|p| AssetPositionOutput {
                token_id: p.token_id.clone(),
                amount: U128::from(p.amount),
            }).collect(),
        )
    }

    pub fn create_capital_allocation(&mut self, owner_id: AccountId, entry_amount: U128, entry_token_id: Option<AccountId>) -> u64 {
        self.assert_agent();

        let capital = CapitalAllocation {
            owner_id,
            status: CapitalAllocationStatus::Active,
            positions: vec![],
            entry_timestamp: env::block_timestamp_ms(),
            exit_timestamp: env::block_timestamp_ms() + DEFAULT_LOCKTIME_IN_MS,
            entry_value: AssetPosition {
                token_id: entry_token_id.unwrap_or(AccountId::from_str(USDC_CONTRACT_ID).unwrap()),
                amount: entry_amount.0,
            },
            exit_value: None,
        };

        let capital_id = self.next_capital_id;
        self.capital.insert(&capital_id, &capital);
        self.next_capital_id += 1;

        capital_id
    }

    pub fn withdraw_capital(&mut self, capital_id: u64) {
        self.assert_agent();

        let mut capital = self
            .capital
            .get(&capital_id)
            .expect("Capital Allocation not found");
        assert!(
            capital.status == CapitalAllocationStatus::Active,
            "Capital Allocation already withdrawn"
        );
        assert!(
            capital.exit_timestamp >= env::block_timestamp_ms(),
            "Capital Allocation not yet matured"
        );

        capital.status = CapitalAllocationStatus::Withdrawn;
        self.capital.insert(&capital_id, &capital);

        let gas_to_spend =
            GAS_WITHDRAW_CAPITAL.as_gas() + GAS_FT_TRANSFER.as_gas() * capital.positions.len() as u64;
        assert!(
            env::prepaid_gas().as_gas() >= gas_to_spend,
            "Not enough gas to withdraw capital"
        );

        for position in capital.positions.iter() {
            ext_ft::ext(position.token_id.clone())
                .with_static_gas(GAS_FT_TRANSFER_CALL)
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .ft_transfer_call(
                    AccountId::from_str(INTENTS_CONTRACT_ID).unwrap(),
                    U128::from(position.amount),
                    None,
                    "".to_string()
                );
        }
    }

    pub fn set_capital_exit_value(&mut self, capital_id: u64, exit_amount: U128, exit_token_id: Option<AccountId>) {
        let exit_token_id = exit_token_id.unwrap_or(AccountId::from_str(USDC_CONTRACT_ID).unwrap());

        self.assert_agent();
        let mut capital = self
            .capital
            .get(&capital_id)
            .expect("Capital Allocation not found");

        assert!(
            capital.status == CapitalAllocationStatus::Withdrawn,
            "Capital Allocation was not withdrawn"
        );


        assert_eq!(
            capital.entry_value.token_id, exit_token_id,
            "Exit value token must match entry value token"
        );

        // calculate profit/loss in percents as u128, store profit: bool, percent: u128
        let if_profit = exit_amount.0 >= capital.entry_value.amount;
        let profit_loss_in_percent = if if_profit {
            (exit_amount.0 - capital.entry_value.amount) * 100 / capital.entry_value.amount
        } else {
            (capital.entry_value.amount - exit_amount.0) * 100 / capital.entry_value.amount
        };

        self.leaderboard.add_item(
            LeaderboardItem {
                near_account_id: capital.owner_id.clone(),
                value: profit_loss_in_percent,
                capital_id,
            },
            if_profit,
        );

        capital.exit_value = Some(AssetPosition {
            token_id: exit_token_id,
            amount: exit_amount.0,
        });

        self.capital.insert(&capital_id, &capital);
    }

    pub fn get_leaderboard(&self) -> (Vec<LeaderboardItem>, Vec<LeaderboardItem>) {
        (self.leaderboard.profit.clone(), self.leaderboard.loss.clone())
    }

    pub fn get_capital(&self, capital_id: u64) -> CapitalAllocation {
        self.capital.get(&capital_id).expect("Capital Allocation not found")
    }
}

impl Contract {
    fn assert_agent(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.agent_account_id,
            "Only agent can call this method"
        );
    }

    pub fn deposit_usdc(
        &mut self,
        twitter_id: U128,
        near_account_id: AccountId,
        amount: u128,
        input_tweet_id: Option<U128>,
    ) {
        let key = TwitterNearAccount {
            twitter_id: twitter_id.0,
            near_account_id: near_account_id.clone(),
        };

        let balance = self.usdc_deposits.get(&key).unwrap_or(0);

        let new_balance = balance + amount;
        assert!(
            new_balance >= MIN_USDC_DEPOSIT,
            "Deposit must be at least 0.1 USDC"
        );

        events::emit::run_agent(
            &self.agent,
            &serde_json::json!(
                {
                    "action": "deposit_usdc".to_string(),
                    "account_id": near_account_id,
                    "twitter_id": twitter_id,
                    "deposit": amount.to_string(),
                    "tweet_id": input_tweet_id.unwrap_or(U128(0)),
                }
            )
            .to_string(),
        );

        self.usdc_deposits.insert(&key, &new_balance);
    }

    fn add_position(&mut self, capital_id: u64, token_id: AccountId, amount: Balance) {
        let mut capital = self
            .capital
            .get(&capital_id)
            .expect("Capital Allocation not found");
        assert!(
            capital.positions.len() < MAX_ASSETS_IN_CAPITAL_ALLOCATION,
            "Too many assets in Capital Allocation"
        );

        events::emit::run_agent(
            &self.agent,
            &serde_json::json!(
                {
                    "action": "add_position".to_string(),
                    "capital_id": capital_id,
                    "token_id": token_id.to_string(),
                    "amount": amount.to_string(),
                }
            )
            .to_string(),
        );

        let position = AssetPosition { token_id, amount };

        capital.positions.push(position);

        self.capital.insert(&capital_id, &capital);
    }
}


#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[allow(unused_imports)]
    use near_contract_standards::fungible_token::{metadata::FT_METADATA_SPEC, Balance};
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    use super::*;

    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    fn get_contract() -> Contract {
        Contract::new(
            "agent.near".to_string(),
            accounts(1).into(),
            None,
        )
    }

    #[test]
    fn test_new() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let contract = get_contract();
        assert!(contract.leaderboard.profit.is_empty());
        assert!(contract.leaderboard.loss.is_empty());
    }

    #[test]
    fn test_deposit_near() {
        let mut context = get_context(accounts(1));
        testing_env!(context
            .attached_deposit(NearToken::from_near(1))
            .build());
        let mut contract = get_contract();

        let twitter_id = U128(1845765845647056907);
        contract.deposit_near(twitter_id);
        let balance = contract.get_near_balance(twitter_id, accounts(1));
        assert_eq!(balance.0, NearToken::from_near(1).as_yoctonear());
    }

    #[test]
    fn test_withdraw_near() {
        let mut context = get_context(accounts(1));
        testing_env!(context
            .attached_deposit(NearToken::from_near(1))
            .build());
        let mut contract = get_contract();

        let twitter_id = U128(1845765845647056907);
        contract.deposit_near(twitter_id);
        contract.withdraw_near(twitter_id, accounts(1));
        let balance = contract.get_near_balance(twitter_id, accounts(1));
        assert_eq!(balance.0, 0);
    }

    #[test]
    fn test_deposit_usdc() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = get_contract();

        let deposit = NearToken::from_millinear(567).as_yoctonear();

        let twitter_id = U128(1845765845647056907);
        contract.deposit_usdc(twitter_id, accounts(3), deposit, None);
        let balance = contract.get_usdc_balance(twitter_id, accounts(3));
        assert_eq!(balance.0, deposit);
    }

    #[test]
    fn test_create_capital_allocation() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = get_contract();

        let capital_id = contract.create_capital_allocation(accounts(1), U128::from(1000), Some(accounts(2)));
        let (active, owner_id, _, positions) = contract.get_capital_allocation(capital_id);
        assert!(active);
        assert_eq!(owner_id, accounts(1));
        assert_eq!(positions.len(), 0);
    }

    #[test]
    fn test_withdraw_capital() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = get_contract();

        contract.create_capital_allocation(accounts(1), U128::from(1000), Some(accounts(2)));
        contract.withdraw_capital(0);
        let (active, _, _, _) = contract.get_capital_allocation(0);
        assert!(!active);
    }
    #[test]
    fn test_full_leaderboard() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = get_contract();

        // 100% profit deal
        let capital_id = contract.create_capital_allocation(accounts(1), U128::from(1000), None);
        contract.withdraw_capital(capital_id);

        contract.set_capital_exit_value(capital_id, U128::from(2000), None);
        let (active, _, _, _) = contract.get_capital_allocation(0);
        assert!(!active);

        // add leaderboard checks
        assert_eq!(contract.leaderboard.profit.len(), 1);
        assert_eq!(contract.leaderboard.loss.len(), 0);

        assert_eq!(contract.leaderboard.profit[0].value, 100);
        assert_eq!(contract.leaderboard.profit[0].capital_id, capital_id);

        // 33% profit deal
        let capital_id = contract.create_capital_allocation(accounts(1), U128::from(100), None);
        contract.withdraw_capital(capital_id);

        contract.set_capital_exit_value(capital_id, U128::from(133), None);
        let (active, _, _, _) = contract.get_capital_allocation(0);
        assert!(!active);

        // add leaderboard checks
        assert_eq!(contract.leaderboard.profit.len(), 2);
        assert_eq!(contract.leaderboard.loss.len(), 0);

        assert_eq!(contract.leaderboard.profit[0].value, 100);
        assert_eq!(contract.leaderboard.profit[1].value, 33);
        assert_eq!(contract.leaderboard.profit[1].capital_id, capital_id);


        // - 50% profit deal
        let capital_id = contract.create_capital_allocation(accounts(1), U128::from(1000), None);
        contract.withdraw_capital(capital_id);

        contract.set_capital_exit_value(capital_id, U128::from(500), None);
        let (active, _, _, _) = contract.get_capital_allocation(1);
        assert!(!active);

        assert_eq!(contract.leaderboard.profit.len(), 2);
        assert_eq!(contract.leaderboard.loss[0].value, 50);
        assert_eq!(contract.leaderboard.loss[0].capital_id, capital_id);

        // 0% profit deal
        let capital_id = contract.create_capital_allocation(accounts(1), U128::from(500), None);
        contract.withdraw_capital(capital_id);

        contract.set_capital_exit_value(capital_id, U128::from(500), None);
        let (active, _, _, _) = contract.get_capital_allocation(1);
        assert!(!active);

        assert_eq!(contract.leaderboard.profit.len(), 3);
        assert_eq!(contract.leaderboard.profit[2].value, 0);
        assert_eq!(contract.leaderboard.profit[2].capital_id, capital_id);
    }
}