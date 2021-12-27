use cosmwasm_std::{
    to_binary, from_binary, Api, Binary, BankMsg, Coin, Env, Extern, HandleResponse, HandleResult, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128, HumanAddr, CanonicalAddr, CosmosMsg
};

use crate::msg::{ConfigResponse, HandleMsg, HandleReceiveMsg, InitMsg, QueryMsg, RedeemHandleMsg};
use crate::state::{Config, Pair, save, load, STACK_KEY, SNIP20_ADDRESS_KEY, SNIP20_HASH_KEY, CONFIG_KEY};


//Snip 20 usage
use secret_toolkit::{snip20::handle::{register_receive_msg,transfer_msg}, 
utils::{HandleCallback}};


/// pad handle responses and log attributes to blocks of 256 bytes to prevent leaking info based on
/// response size
pub const BLOCK_SIZE: usize = 256;



pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let config = Config {
        admin: deps.api.canonical_address(&msg.admin)?,
        active: true,
        stack_size: msg.stack,
        fee: msg.fee,

    };

    let stack: Vec<Pair> = vec![];

    save(&mut deps.storage, CONFIG_KEY, &config)?;
    save(&mut deps.storage, SNIP20_HASH_KEY, &msg.sscrt_hash)?;
    save(&mut deps.storage, SNIP20_ADDRESS_KEY, &msg.sscrt_addr)?;
    save(&mut deps.storage, STACK_KEY, &stack)?;


    Ok(InitResponse {
        messages: vec![
            register_receive_msg(
                env.contract_code_hash,
                None,
                BLOCK_SIZE,
                msg.sscrt_hash,
                msg.sscrt_addr
            )?
        ],
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Receive { sender, from, amount, msg } => receive(deps, env, sender, from, amount, msg),
        HandleMsg::ChangeFee { new_fee } => change_fee(deps, env, new_fee),
        HandleMsg::ChangeStackSize { new_stack_size } => change_stack_size(deps, env, new_stack_size),
        HandleMsg::ChangeAdmin { new_admin } => change_admin(deps, env, new_admin),
    }
}


/// For receiving SNIP20s
pub fn receive<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    _sender: HumanAddr,
    _from: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> HandleResult {
    let snip20_address: HumanAddr = load(&deps.storage, SNIP20_ADDRESS_KEY)?;
    let config: Config = load(&deps.storage, CONFIG_KEY)?;

    if env.message.sender != snip20_address {
        return Err(StdError::generic_err(
            "Address is not correct snip contract",
        ));
    }

    if amount <= config.fee  {
        return Err(StdError::generic_err(
            "You have not reached the minumum amount for a transaction",
        ));
    }

    let mut config: Config = load(&deps.storage, CONFIG_KEY)?;

    let gas_amount = (amount - config.fee)?;

    if let Some(bin_msg) = msg {
        match from_binary(&bin_msg)? {
            HandleReceiveMsg::ReceiveSeed {
                recipient,
            } => {
                seed_wallet(
                    deps,
                    env,
                    &mut config,
                    recipient,
                    gas_amount,      
                )
            }
        }
     } else {
        Err(StdError::generic_err("data should be given"))
     }
}






pub fn seed_wallet<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    config: &mut Config,
    recipient: HumanAddr,
    gas_amount: Uint128
) -> StdResult<HandleResponse> {


    if !config.active {
        return Err(StdError::generic_err(
            "Transfers are currently disabled",
        ));
    }

    
    

    let mut msg_list: Vec<CosmosMsg> = vec![];

    let snip20_address: HumanAddr = load(&deps.storage, SNIP20_ADDRESS_KEY)?;
    let callback_code_hash: String = load(&deps.storage, &SNIP20_HASH_KEY)?;


    let padding: Option<String> = None;
    let block_size = BLOCK_SIZE;


    // Admin Fee
    let amount = config.fee;
    let fee_recipient: HumanAddr = deps.api.human_address(&config.admin)?;
    let cosmos_msg = transfer_msg(
        fee_recipient,
        amount,
        padding.clone(),
        block_size.clone(),
        callback_code_hash.clone(),
        snip20_address.clone(),
    )?;
    msg_list.push(cosmos_msg);


    // Store pending tx
    let mut stack: Vec<Pair> = load(&deps.storage, &STACK_KEY)?;

    let new_pair = Pair {
        recipient: deps.api.canonical_address(&recipient)?,
        gas: gas_amount
    };

    stack.push(new_pair);


    // If stack is ready to send through all tx
    if stack.len() >= config.stack_size as usize {

        // Convert all funds to SCRT
        let mut redeemable_funds: u128 = 0;
        for pair in stack.iter() {
            redeemable_funds = redeemable_funds + pair.gas.u128();
        }

        let redeem_msg = RedeemHandleMsg::Redeem {
            amount: Uint128::from(redeemable_funds),
            denom: Some("uscrt".to_string()),
            padding
        };

        let cosmos_msg = redeem_msg.to_cosmos_msg(
            callback_code_hash,
            snip20_address,
            None,
        )?;
        msg_list.push(cosmos_msg);


        // Send all SCRT
        for pair in stack.iter() {

            let withdrawal_coins: Vec<Coin> = vec![Coin {
                denom: "uscrt".to_string(),
                amount: pair.gas,
            }];

            let cosmos_msg = CosmosMsg::Bank(BankMsg::Send {
                from_address: env.contract.address.clone(),
                to_address: deps.api.human_address(&pair.recipient)?,
                amount: withdrawal_coins,
            });
            msg_list.push(cosmos_msg);
        }

        stack = vec![];


    }
    save(&mut deps.storage, STACK_KEY, &stack)?;    




    Ok(HandleResponse::default())
}









// ADMIN COMMANDS

pub fn change_fee<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    new_fee: Uint128,
) -> StdResult<HandleResponse> {
    let mut config: Config = load(&deps.storage, CONFIG_KEY)?;  
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    if config.admin != sender_raw {
        return Err(StdError::generic_err(
            "This function is only usable by the Admin",
        ));
    }

    config.fee = new_fee;

    save(&mut deps.storage, CONFIG_KEY, &config)?;


    Ok(HandleResponse::default())
}



pub fn change_stack_size<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    new_stack_size: u8,
) -> StdResult<HandleResponse> {
    let mut config: Config = load(&deps.storage, CONFIG_KEY)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    if config.admin != sender_raw {
        return Err(StdError::generic_err(
            "This function is only usable by the Admin",
        ));
    }

    config.stack_size = new_stack_size;

    save(&mut deps.storage, CONFIG_KEY, &config)?;


    Ok(HandleResponse::default())
}



pub fn change_admin<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    new_admin: HumanAddr
) -> StdResult<HandleResponse> {
    let mut config: Config = load(&deps.storage, CONFIG_KEY)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    if config.admin != sender_raw {
        return Err(StdError::generic_err(
            "This function is only usable by the Admin",
        ));
    }

    config.admin = deps.api.canonical_address(&new_admin)?;

    save(&mut deps.storage, CONFIG_KEY, &config)?;



    Ok(HandleResponse::default())
}






pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<ConfigResponse> {
    let config: Config = load(&deps.storage, CONFIG_KEY)?;


    Ok(ConfigResponse { active: config.active, stack_size: config.stack_size, fee: config.fee })
}
