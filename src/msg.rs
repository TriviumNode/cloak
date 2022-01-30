use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use secret_toolkit::utils::{HandleCallback};



#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    /// Recipient of fees and able to make adjustments
    pub admin: HumanAddr,

    /// Cost of every use
    pub fee: Uint128,

    /// Number of deposits before all transactions go through
    pub min_stack: u8,
    pub max_stack: u8,

    pub sscrt_addr: HumanAddr,
    pub sscrt_hash: String,


    pub entropy: String,

}


#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleReceiveMsg {
    ReceiveSeed {
        recipient: HumanAddr,
     },
}



#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RedeemHandleMsg {
    Redeem {
        amount: Uint128,
        denom: Option<String>,
        padding: Option<String>,
    },
}

impl HandleCallback for RedeemHandleMsg {
    const BLOCK_SIZE: usize = 256;
}





#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    /// Receive Snip20 Payment
    Receive {
        sender: HumanAddr,
        from: HumanAddr,
        amount: Uint128,
        #[serde(default)]
        msg: Option<Binary>,
    },
    ExitPool {
    },
    ChangeFee {
        new_fee: Uint128,
    },
    ChangeStackSize {
        new_stack_max: u8,
        new_stack_min: u8,
    },
    ChangeAdmin {
        new_admin: HumanAddr,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetCount returns the current count as a json-encoded number
    GetConfig {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub active: bool,
    pub stack_min: u8,
    pub stack_max: u8,
    pub fee: Uint128
}
