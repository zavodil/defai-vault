use crate::*;

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub enum TokenReceiverAction {
    Deposit { twitter_id: U128, input_tweet_id: Option<U128> },
    AddCapital { capital_id: u64 },
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let token_in = env::predecessor_account_id();

        let message: TokenReceiverAction =
            serde_json::from_str(&msg).expect("Failed to parse message");

        match message {
            TokenReceiverAction::Deposit { twitter_id, input_tweet_id } => {
                if token_in == USDC_CONTRACT_ID {
                    self.deposit_usdc(twitter_id, sender_id, amount.0, input_tweet_id);
                }
            }
            TokenReceiverAction::AddCapital { capital_id } => {
                assert_eq!(
                    sender_id, self.agent_account_id,
                    "Only agent can add capital"
                );
                self.add_position(capital_id, token_in, amount.0);
            }
        }

        PromiseOrValue::Value(U128(0))
    }
}
