use cosmwasm_std::{
    log, to_binary, from_binary, Api, Binary, BankMsg, Coin, Env, Extern, HandleResponse, HandleResult, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128, HumanAddr, CanonicalAddr, CosmosMsg
};

use crate::msg::{ConfigResponse, ExistsResponse, PoolSizeResponse, HandleMsg, HandleReceiveMsg, InitMsg, QueryMsg, RedeemHandleMsg};
use crate::state::{Config, Pair, save, load, may_load, remove, POOL_SIZE_KEY, SNIP20_ADDRESS_KEY, SNIP20_HASH_KEY, CONFIG_KEY, PRNG_SEED_KEY};

use crate::rand::{sha_256, Prng};

use sha2::{Digest};
use std::convert::TryInto;


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
        operator: deps.api.canonical_address(&msg.operator)?,
        active: true,


        fee: msg.fee,
        op_share: msg.op_share,

    };

    if config.fee <= config.op_share {
        return Err(StdError::generic_err(
            "The operator share must be less than the total fee",
        ));
    }

    let prng_seed: Vec<u8> = sha_256(base64::encode(msg.entropy).as_bytes()).to_vec();

    // Initial pool size of 0
    let pool_init: u16 = 0;

    save(&mut deps.storage, POOL_SIZE_KEY, &pool_init)?;
    save(&mut deps.storage, PRNG_SEED_KEY, &prng_seed)?;
    save(&mut deps.storage, CONFIG_KEY, &config)?;
    save(&mut deps.storage, SNIP20_HASH_KEY, &msg.sscrt_hash)?;
    save(&mut deps.storage, SNIP20_ADDRESS_KEY, &msg.sscrt_addr)?;


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
        HandleMsg::FinalizeSeed { tx_key , sender } => finalize_seed(deps, env, tx_key, sender),
        HandleMsg::ExitPool { tx_key } => exit_pool(deps, env, tx_key),
        HandleMsg::ChangeFee { new_fee, new_op_share } => change_fee(deps, env, new_fee, new_op_share),
        HandleMsg::ChangeAdmin { new_admin } => change_admin(deps, env, new_admin),
        HandleMsg::ChangeOperator { new_operator } => change_admin(deps, env, new_operator),
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
            } => {
                seed_wallet(
                    deps,
                    env,
                    &mut config,
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
    gas_amount: Uint128
) -> StdResult<HandleResponse> {


    if !config.active {
        return Err(StdError::generic_err(
            "Transfers are currently disabled",
        ));
    }


    //Generate exit key
    let prng_seed: Vec<u8> = load(&mut deps.storage, PRNG_SEED_KEY)?;

    //Stored Entropy
    let new_data: String = format!("{:?}+{}+{}+{}+{}", prng_seed, gas_amount, &env.block.height, &env.block.time, &env.message.sender);

    let hashvalue = sha2::Sha256::digest(new_data.as_bytes());
    let hash: [u8; 32] = hashvalue.as_slice().try_into().expect("Wrong length");

    //Exported Entropy
    let export_data: String = format!("{:?}+{}", prng_seed, gas_amount);

    let export_hashvalue = sha2::Sha256::digest(export_data.as_bytes());
    let export_hash: [u8; 32] = export_hashvalue.as_slice().try_into().expect("Wrong length");

    
    



    let mut msg_list: Vec<CosmosMsg> = vec![];


    let snip20_address: HumanAddr = load(&deps.storage, SNIP20_ADDRESS_KEY)?;
    let callback_code_hash: String = load(&deps.storage, &SNIP20_HASH_KEY)?;


    let padding: Option<String> = None;


    // Admin Fee
    let amount: Uint128 = (config.fee - config.op_share)?;
    let fee_recipient: HumanAddr = deps.api.human_address(&config.admin)?;
    let cosmos_msg = transfer_msg(
        fee_recipient,
        amount,
        padding.clone(),
        BLOCK_SIZE,
        callback_code_hash.clone(),
        snip20_address.clone(),
    )?;
    msg_list.push(cosmos_msg);



    // Operator Fee
    let amount = config.op_share;
    let fee_recipient: HumanAddr = deps.api.human_address(&config.operator)?;
    let cosmos_msg = transfer_msg(
        fee_recipient,
        amount,
        padding.clone(),
        BLOCK_SIZE,
        callback_code_hash.clone(),
        snip20_address.clone(),
    )?;
    msg_list.push(cosmos_msg);


    // Store pending tx
    

    let new_pair = Pair {
        gas: gas_amount.u128()
    };


    //save(&mut deps.storage, &export_hash, &new_pair)?;
    save(&mut deps.storage, PRNG_SEED_KEY, &hash.to_vec())?;

    let tx_key_string = hex::encode(&export_hash);


    save(&mut deps.storage, tx_key_string.as_bytes(), &new_pair)?;



    // Adjust pool size
    let mut pool_size: u16 = load(&deps.storage, POOL_SIZE_KEY)?;
    pool_size = pool_size + 1;
    save(&mut deps.storage, POOL_SIZE_KEY, &pool_size)?;
    


    Ok(HandleResponse {
        messages: msg_list,
        log: vec![
            log("tx_code", tx_key_string),
        ],
        data: None,
    })
}







pub fn finalize_seed<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    tx_key: String,
    sender: HumanAddr,
) -> StdResult<HandleResponse> {

    let config: Config = load(&deps.storage, CONFIG_KEY)?;


    if env.message.sender != deps.api.human_address(&config.operator)?{
        return Err(StdError::generic_err(
            "You are not a verified operator",
        ));
    }


    if !config.active {
        return Err(StdError::generic_err(
            "Transfers are currently disabled",
        ));
    }


    let tx_data_wrapped: Option<Pair> = may_load(&deps.storage, tx_key.as_bytes())?;
    let tx_data: Pair;
    if tx_data_wrapped == None {
        return Err(StdError::generic_err(
            "There are no pending transactions with this key.",
        ));
    }
    else {
        tx_data = tx_data_wrapped.unwrap();
    }


    let mut msg_list: Vec<CosmosMsg> = vec![];

    let snip20_address: HumanAddr = load(&deps.storage, SNIP20_ADDRESS_KEY)?;
    let callback_code_hash: String = load(&deps.storage, &SNIP20_HASH_KEY)?;


    let padding: Option<String> = None;



    

    
    let redeem_msg = RedeemHandleMsg::Redeem {
        amount: Uint128::from(tx_data.gas),
        denom: Some("uscrt".to_string()),
        padding
    };

    let cosmos_msg = redeem_msg.to_cosmos_msg(
        callback_code_hash,
        snip20_address,
        None,
    )?;
    msg_list.push(cosmos_msg);



    let withdrawal_coins: Vec<Coin> = vec![Coin {
        denom: "uscrt".to_string(),
        amount: Uint128::from(tx_data.gas),
    }];

    let cosmos_msg = CosmosMsg::Bank(BankMsg::Send {
        from_address: env.contract.address.clone(),
        to_address: sender,
        amount: withdrawal_coins,
    });
    msg_list.push(cosmos_msg);


    remove(&mut deps.storage, tx_key.as_bytes());




    // Adjust pool size
    let mut pool_size: u16 = load(&deps.storage, POOL_SIZE_KEY)?;
    pool_size = pool_size - 1;
    save(&mut deps.storage, POOL_SIZE_KEY, &pool_size)?;



    Ok(HandleResponse {
        messages: msg_list,
        log: vec![],
        data: None,
    })
}









pub fn exit_pool<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    tx_key: String,
) -> StdResult<HandleResponse> { 


    let tx_data_wrapped: Option<Pair> = may_load(&deps.storage, tx_key.as_bytes())?;
    let tx_data: Pair;
    if tx_data_wrapped == None {
        return Err(StdError::generic_err(
            "There are no pending transactions with this key.",
        ));
    }
    else {
        tx_data = tx_data_wrapped.unwrap();
    }



    let mut msg_list: Vec<CosmosMsg> = vec![];
    
    let snip20_address: HumanAddr = load(&deps.storage, SNIP20_ADDRESS_KEY)?;
    let callback_code_hash: String = load(&deps.storage, SNIP20_HASH_KEY)?;

    let padding: Option<String> = None;
    
    let amount = Uint128::from(tx_data.gas);
    let recipient: HumanAddr = env.message.sender;
    let cosmos_msg = transfer_msg(
        recipient.clone(),
        amount,
        padding,
        BLOCK_SIZE,
        callback_code_hash,
        snip20_address,
    )?;
    msg_list.push(cosmos_msg);



    remove(&mut deps.storage, tx_key.as_bytes());



    // Adjust pool size
    let mut pool_size: u16 = load(&deps.storage, POOL_SIZE_KEY)?;
    pool_size = pool_size - 1;
    save(&mut deps.storage, POOL_SIZE_KEY, &pool_size)?;

    

    Ok(HandleResponse {
        messages: msg_list,
        log: vec![],
        data: None,
    })
}







pub fn new_entropy(env: &Env, seed: &[u8], entropy: &[u8])-> [u8;32]{
    // 16 here represents the lengths in bytes of the block height and time.
    let entropy_len = 16 + env.message.sender.len() + entropy.len();
    let mut rng_entropy = Vec::with_capacity(entropy_len);
    rng_entropy.extend_from_slice(&env.block.height.to_be_bytes());
    rng_entropy.extend_from_slice(&env.block.time.to_be_bytes());
    rng_entropy.extend_from_slice(&env.message.sender.0.as_bytes());
    rng_entropy.extend_from_slice(entropy);

    let mut rng = Prng::new(seed, &rng_entropy);

    rng.rand_bytes()
}







// ADMIN COMMANDS

pub fn change_fee<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    new_fee: Uint128,
    new_op_share: Uint128
) -> StdResult<HandleResponse> {
    let mut config: Config = load(&deps.storage, CONFIG_KEY)?;  
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    if config.admin != sender_raw {
        return Err(StdError::generic_err(
            "This function is only usable by the Admin",
        ));
    }

    if new_fee <= new_op_share {
        return Err(StdError::generic_err(
            "The operator share must be less than the total fee",
        ));
    }


    config.fee = new_fee;
    config.op_share = new_op_share;


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



pub fn change_operator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    new_operator: HumanAddr
) -> StdResult<HandleResponse> {
    let mut config: Config = load(&deps.storage, CONFIG_KEY)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    if config.admin != sender_raw {
        return Err(StdError::generic_err(
            "This function is only usable by the Admin",
        ));
    }

    config.operator = deps.api.canonical_address(&new_operator)?;

    save(&mut deps.storage, CONFIG_KEY, &config)?;



    Ok(HandleResponse::default())
}



pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::GetExists { tx_key } => to_binary(&query_tx_exists(deps, tx_key)?),
        QueryMsg::GetPoolSize {} => to_binary(&query_pool_size(deps)?),
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<ConfigResponse> {
    let config: Config = load(&deps.storage, CONFIG_KEY)?;


    Ok(ConfigResponse { active: config.active, fee: config.fee })
}



fn query_tx_exists<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>, tx_key: String,) -> StdResult<ExistsResponse> {
    
    let exists: bool;

    let tx_data_wrapped: Option<Pair> = may_load(&deps.storage, tx_key.as_bytes())?;
    if tx_data_wrapped == None {
        exists = false;
    }
    else {
        exists = true;
    }

    Ok(ExistsResponse { exists })
}


fn query_pool_size<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<PoolSizeResponse> {
    
    let pool_size: u16 = load(&deps.storage, POOL_SIZE_KEY)?;


    Ok(PoolSizeResponse { pool_size })
}
