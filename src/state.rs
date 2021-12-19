use std::any::type_name;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use secret_toolkit::{
    serialization::{Bincode2, Json, Serde},
    storage::{AppendStore, AppendStoreMut},
};


use cosmwasm_std::{Api, BlockInfo, CanonicalAddr, ReadonlyStorage, StdError, StdResult, Storage, Uint128};
//use cosmwasm_storage::{PrefixedStorage, ReadonlyPrefixedStorage};

pub static CONFIG_KEY: &[u8] = b"config";
pub static STACK_KEY: &[u8] = b"stack";

/// prefix for the storage of snip20 address
pub const SNIP20_ADDRESS_KEY: &[u8] = b"sscrt";
/// Storage for storing the hash of the snip20 contract
pub const SNIP20_HASH_KEY: &[u8] = b"callback";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    // Permissions to edit rates
    pub admin: CanonicalAddr,
    // Marks whether txs are allowed to be sent
    pub active: bool,

    // Number of inputs before pool is allowed to transfer stack
    pub stack_size: u8,

    // Minimum amount of funds that can be sent through the contract
    pub fee: Uint128,


}

/// Pair of the recipient address and the gas amount they are sending
#[derive(Serialize, Deserialize, Clone, JsonSchema, PartialEq, Debug)]
pub struct  Pair {
    pub recipient: CanonicalAddr,
    pub gas: Uint128
}


/// Returns StdResult<T> from retrieving the item with the specified key.  Returns a
/// StdError::NotFound if there is no item with that key
///
/// # Arguments
///
/// * `storage` - a reference to the storage this item is in
/// * `key` - a byte slice representing the key that accesses the stored item
pub fn load<T: DeserializeOwned, S: ReadonlyStorage>(storage: &S, key: &[u8]) -> StdResult<T> {
    Bincode2::deserialize(
        &storage
            .get(key)
            .ok_or_else(|| StdError::not_found(type_name::<T>()))?,
    )
}


/// Returns StdResult<()> resulting from saving an item to storage
///
/// # Arguments
///
/// * `storage` - a mutable reference to the storage this item should go to
/// * `key` - a byte slice representing the key to access the stored item
/// * `value` - a reference to the item to store
pub fn save<T: Serialize, S: Storage>(storage: &mut S, key: &[u8], value: &T) -> StdResult<()> {
    storage.set(key, &Bincode2::serialize(value)?);
    Ok(())
}
