use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use secret_toolkit::utils::{HandleCallback};



#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    /// Recipient of fees and able to make adjustments
    pub admin: HumanAddr,

    pub operator: HumanAddr,

    /// Cost of every use
    pub fee: Uint128,
    pub op_share: Uint128,


    pub sscrt_addr: HumanAddr,
    pub sscrt_hash: String,


    pub entropy: String,

}


#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleReceiveMsg {
    ReceiveSeed {
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
    FinalizeSeed {
        tx_key: String,
        sender: HumanAddr,
    },
    ExitPool {
        tx_key: String
    },
    ChangeFee {
        new_fee: Uint128,
        new_op_share: Uint128,
    },
    ChangeAdmin {
        new_admin: HumanAddr,
    },
    ChangeOperator {
        new_operator: HumanAddr,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetCount returns the current count as a json-encoded number
    GetConfig {},
    GetExists {
        tx_key: String
    },
    GetPoolSize {}
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub active: bool,
    pub fee: Uint128
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ExistsResponse {
    pub exists: bool
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolSizeResponse {
    pub pool_size: u16
}