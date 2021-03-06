use hex_view::HexView;
use rand::{self, Rng};
use uuid::Uuid;

#[cfg(test)]
use fake_redis::FakeConnection as Connection;
#[cfg(not(test))]
use redis::{self, Commands, Connection};

use crate::{
    error::{self, *},
    types::*,
};

const NEXT_USER_ID: &str = "next_user_id";

const USER_ID_SALT: &str = "user_id_salt";

pub fn hash(data: &str, salt: &str) -> String {
    format!(
        "{:x}",
        HexView::from(&argon2rs::argon2i_simple(&data, &salt))
    )
}

fn generate_salt() -> String {
    if cfg!(test) {
        "00000000".to_string()
    } else {
        let mut rng = rand::thread_rng();
        rng.gen::<u64>().to_string()
    }
}

fn get_next_id<RV: std::str::FromStr>(
    c: &mut Connection,
    next_key: &str,
    salt_key: &str,
) -> Result<RV> {
    let id: u32 = c.incr(next_key, 1)?;
    let salt: String = match c.exists(salt_key) {
        Ok(true) => c.get(salt_key)?,
        _ => {
            let s = generate_salt();
            c.set(salt_key, s.clone())?;
            s
        }
    };
    RV::from_str(&hash(&id.to_string(), &salt)).map_err(|_| {
        ServerError::new(
            error::INTERNAL_ERROR,
            "Creation of hashed id failed, can't be",
        )
    })
}

pub fn get_next_user_id(c: &mut Connection) -> Result<UserId> {
    get_next_id(c, NEXT_USER_ID, USER_ID_SALT)
}

pub fn get_next_store_id() -> StoreId {
    StoreId::new(
        (*Uuid::new_v4()
            .to_hyphenated_ref()
            .encode_lower(&mut Uuid::encode_buffer()))
        .to_string(),
    )
}

pub fn get_next_aisle_id() -> AisleId {
    AisleId(
        (*Uuid::new_v4()
            .to_hyphenated_ref()
            .encode_lower(&mut Uuid::encode_buffer()))
        .to_string(),
    )
}

pub fn get_next_product_id() -> ProductId {
    ProductId(
        (*Uuid::new_v4()
            .to_hyphenated_ref()
            .encode_lower(&mut Uuid::encode_buffer()))
        .to_string(),
    )
}

#[cfg(test)]
pub mod tests {
    pub const HASH_1: &str = "26a9dc4bed936c6ad9944f209790626d18f0b797233fd18465ecef1d1fd16686";
    pub const HASH_2: &str = "1dca54016dd7aadeaa82c84a0be2e2829b299de8472ff4e51bcbdc540f242a69";
    pub const HASH_3: &str = "ad9d3d3a33b5b0b29edf5ac27a63392fa5d1d1b03da1ebb96941d7d7cfd59c3a";
}
