use crate::*;
const MAX_LEADERBOARD_LENGTH: usize = 5;

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct LeaderboardItem {
    pub near_account_id: AccountId,
    pub value: u128,
    pub capital_id: u64,
}

#[near(serializers = [json, borsh])]
pub struct Leaderboard {
    pub profit: Vec<LeaderboardItem>,
    pub loss: Vec<LeaderboardItem>,
}

impl Leaderboard {
    pub fn new() -> Self {
        Self {
            profit: vec![],
            loss: vec![],
        }
    }

    pub fn add_item(&mut self, item: LeaderboardItem, is_profit: bool) {
        let list = if is_profit {
            &mut self.profit
        } else {
            &mut self.loss
        };

        if list.len() < MAX_LEADERBOARD_LENGTH || item.clone().value > list.last().unwrap().value {
            list.push(item);
            list.sort_by(|a, b| b.value.cmp(&a.value));
            if list.len() > MAX_LEADERBOARD_LENGTH {
                list.pop();
            }
        }
    }
}

impl Default for Leaderboard {
    fn default() -> Self {
        Self::new()
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
            None
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
    fn test_add_item_profit() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = get_contract();

        let item = LeaderboardItem {
            near_account_id: AccountId::from_str("bob.near").unwrap(),
            value: 100,
            capital_id: 1,
        };
        contract.leaderboard.add_item(item.clone(), true);
        assert_eq!(contract.leaderboard.profit.len(), 1usize);

        assert_eq!(contract.leaderboard.profit[0].value, item.value);
    }

    #[test]
    fn test_add_item_loss() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = get_contract();

        let item = LeaderboardItem {
            near_account_id: AccountId::from_str("bob.near").unwrap(),
            value: 100,
            capital_id: 1,
        };
        contract.leaderboard.add_item(item.clone(), false);
        assert_eq!(contract.leaderboard.loss.len(), 1usize);
        assert_eq!(contract.leaderboard.loss[0].value, item.value);
    }

    #[test]
    fn test_add_item_exceeds_max_length() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = get_contract();

        for i in 0..8 {
            let item = LeaderboardItem {
                near_account_id: AccountId::from_str(format!("user{}.near", i).as_str()).unwrap(),
                value: 100 + i as u128,
                capital_id: i as u64,
            };
            contract.leaderboard.add_item(item, true);

            let item = LeaderboardItem {
                near_account_id: AccountId::from_str(format!("user{}.near", i).as_str()).unwrap(),
                value: 110 + i as u128,
                capital_id: i as u64,
            };
            contract.leaderboard.add_item(item, false);
        }
        assert_eq!(contract.leaderboard.profit.len(), MAX_LEADERBOARD_LENGTH);
        assert_eq!(contract.leaderboard.profit[0].value, 107);
        assert_eq!(contract.leaderboard.profit[4].value, 103);

        assert_eq!(contract.leaderboard.loss.len(), MAX_LEADERBOARD_LENGTH);
        assert_eq!(contract.leaderboard.loss[0].value, 117);
        assert_eq!(contract.leaderboard.loss[4].value, 113);
    }
}