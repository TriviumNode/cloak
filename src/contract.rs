use cosmwasm_std::{
    to_binary, from_binary, log, Api, Binary, BankMsg, Coin, Env, Extern, HandleResponse, HandleResult, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128, HumanAddr, CanonicalAddr, CosmosMsg
};

use crate::msg::{ConfigResponse, HandleMsg, HandleReceiveMsg, InitMsg, QueryMsg, RedeemHandleMsg};
use crate::state::{Config, Pair, save, load, STACK_KEY, STACK_SIZE_KEY, SNIP20_ADDRESS_KEY, SNIP20_HASH_KEY, CONFIG_KEY, PRNG_SEED_KEY};

use crate::rand::{sha_256, Prng};
use rand_chacha::ChaChaRng;
use rand::{RngCore, SeedableRng};

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



        min_stack: msg.min_stack,
        max_stack: msg.max_stack,

        fee: msg.fee,

    };

    let prng_seed: Vec<u8> = sha_256(base64::encode(msg.entropy).as_bytes()).to_vec();

    let stack: Vec<Pair> = vec![];

    save(&mut deps.storage, PRNG_SEED_KEY, &prng_seed)?;
    save(&mut deps.storage, CONFIG_KEY, &config)?;
    save(&mut deps.storage, SNIP20_HASH_KEY, &msg.sscrt_hash)?;
    save(&mut deps.storage, SNIP20_ADDRESS_KEY, &msg.sscrt_addr)?;
    save(&mut deps.storage, STACK_KEY, &stack)?;
    save(&mut deps.storage, STACK_SIZE_KEY, &msg.max_stack)?;


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
        HandleMsg::ExitPool { } => exit_pool(deps, env),
        HandleMsg::ChangeFee { new_fee } => change_fee(deps, env, new_fee),
        HandleMsg::ChangeStackSize { new_stack_max, new_stack_min } => change_stack_size(deps, env, new_stack_max, new_stack_min),
        HandleMsg::ChangeAdmin { new_admin } => change_admin(deps, env, new_admin),
        HandleMsg::ForcePool { } => force_pool(deps, env),
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

    let mut stack_size: u8 = load(&deps.storage, STACK_SIZE_KEY)?;
    let snip20_address: HumanAddr = load(&deps.storage, SNIP20_ADDRESS_KEY)?;
    let callback_code_hash: String = load(&deps.storage, &SNIP20_HASH_KEY)?;


    let padding: Option<String> = None;


    // Admin Fee
    let amount = config.fee;
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


    // Store pending tx
    let mut stack: Vec<Pair> = load(&deps.storage, STACK_KEY)?;

    let new_pair = Pair {
        recipient: deps.api.canonical_address(&recipient)?,
        sender: deps.api.canonical_address(&env.message.sender)?,
        gas: gas_amount
    };

    stack.push(new_pair);


    // If stack is ready to send through all tx
    if stack.len() >= stack_size as usize {

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

        //Assign new value to stack_size
        let prng_seed: Vec<u8> = load(&mut deps.storage, PRNG_SEED_KEY)?;
        let random_seed  = new_entropy(&env,prng_seed.as_ref(),prng_seed.as_ref());
        let mut rng = ChaChaRng::from_seed(random_seed);

        stack_size =(rng.next_u32() % (config.max_stack as u32 - config.min_stack as u32 + 1) + config.min_stack as u32 ) as u8;
        save(&mut deps.storage, STACK_SIZE_KEY, &stack_size)?;

    }
    save(&mut deps.storage, STACK_KEY, &stack)?;    




    Ok(HandleResponse {
        messages: msg_list,
        log: vec![],
        data: None,
    })
}




pub fn exit_pool<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> { 

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;



    let mut stack: Vec<Pair> = load(&deps.storage, &STACK_KEY)?;

    let mut returnable_funds: u128 = 0;
    let mut n: usize = 0;

    while n < stack.len() {
        if stack[n].recipient == sender_raw{
            returnable_funds = returnable_funds + stack[n].gas.u128();
            stack.swap_remove(n);
        }
        else {
            n=n+1;
        }
    }

    save(&mut deps.storage, STACK_KEY, &stack)?;


    let mut msg_list: Vec<CosmosMsg> = vec![];
    
    let snip20_address: HumanAddr = load(&deps.storage, SNIP20_ADDRESS_KEY)?;
    let callback_code_hash: String = load(&deps.storage, SNIP20_HASH_KEY)?;

    let padding: Option<String> = None;
    let block_size = BLOCK_SIZE;
    
    let amount = Uint128::from(returnable_funds);
    let recipient: HumanAddr = deps.api.human_address(&sender_raw)?;
    let cosmos_msg = transfer_msg(
        recipient,
        amount,
        padding,
        block_size,
        callback_code_hash,
        snip20_address,
    )?;
    msg_list.push(cosmos_msg);

    


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
    new_stack_max: u8,
    new_stack_min: u8
) -> StdResult<HandleResponse> {
    let mut config: Config = load(&deps.storage, CONFIG_KEY)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    if config.admin != sender_raw {
        return Err(StdError::generic_err(
            "This function is only usable by the Admin",
        ));
    }

    config.max_stack = new_stack_max;
    config.min_stack = new_stack_min;

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



// If pool needs to be pushed through prior to an update
pub fn force_pool<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let config: Config = load(&deps.storage, CONFIG_KEY)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    if config.admin != sender_raw {
        return Err(StdError::generic_err(
            "This function is only usable by the Admin",
        ));
    }

    

    let mut msg_list: Vec<CosmosMsg> = vec![];

    let snip20_address: HumanAddr = load(&deps.storage, SNIP20_ADDRESS_KEY)?;
    let callback_code_hash: String = load(&deps.storage, &SNIP20_HASH_KEY)?;


    let padding: Option<String> = None;


    // Convert all funds to SCRT
    let mut stack: Vec<Pair> = load(&deps.storage, STACK_KEY)?;
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

    //Assign new value to stack_size
    let prng_seed: Vec<u8> = load(&mut deps.storage, PRNG_SEED_KEY)?;
    let random_seed  = new_entropy(&env,prng_seed.as_ref(),prng_seed.as_ref());
    let mut rng = ChaChaRng::from_seed(random_seed);

    let stack_size =(rng.next_u32() % (config.max_stack as u32 - config.min_stack as u32 + 1) + config.min_stack as u32 ) as u8;
    save(&mut deps.storage, STACK_SIZE_KEY, &stack_size)?;


    save(&mut deps.storage, STACK_KEY, &stack)?;    




    Ok(HandleResponse {
        messages: msg_list,
        log: vec![],
        data: None,
    })




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


    Ok(ConfigResponse { active: config.active, stack_max: config.max_stack, stack_min: config.min_stack, fee: config.fee })
}

