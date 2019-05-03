use argon2rs;
use hex_view::HexView;
use rand::{self, Rng};
use redis::{self, Commands};

use crate::db;
use crate::error::{self, Result, ServerError};
use crate::session::AuthInfo;
use crate::token::Token;
use crate::types::*;
use crate::user;

const NEXT_USER_ID: &str = "next_user_id";
const USER_PWD: &str = "password";
const USER_MAIL: &str = "email";
const USER_SALT_M: &str = "salt_mail";
const USER_SALT_P: &str = "salt_password";
const USER_NAME: &str = "username";
const USER_AUTH: &str = "auth";
const USERS_LIST: &str = "users";

fn hash(data: &str, salt: &str) -> String {
    format!(
        "{:x}",
        HexView::from(&argon2rs::argon2i_simple(&data, &salt))
    )
}

fn user_key(user_id: &UserId) -> String {
    format!("user:{}", **user_id)
}

fn gen_auth(rng: &mut rand::rngs::ThreadRng) -> String {
    let mut auth = [0u8; 32];
    rng.fill(&mut auth[..]);
    format!("{:x}", HexView::from(&auth))
}

pub fn save_user(user: &user::User) -> Result<Token> {
    let c = db::get_connection()?;
    let norm_username = user.username.to_lowercase();
    if c.hexists(USERS_LIST, &norm_username)? {
        Err(ServerError::new(
            error::USERNAME_TAKEN,
            &format!("Username {} is not available.", &user.username),
        ))
    } else {
        let mut rng = rand::thread_rng();
        let auth = gen_auth(&mut rng);
        let salt_mail = rng.gen::<u64>().to_string();
        let salt_pwd = rng.gen::<u64>().to_string();
        let hashed_pwd = hash(&user.password, &salt_pwd);
        let hashed_mail = hash(&user.email, &salt_mail);

        let user_id = UserId(c.incr(NEXT_USER_ID, 1)?);
        c.hset_multiple(
            &user_key(&user_id),
            &[
                (USER_NAME, &user.username),
                (USER_MAIL, &hashed_mail),
                (USER_PWD, &hashed_pwd),
                (USER_SALT_M, &salt_mail),
                (USER_SALT_P, &salt_pwd),
                (USER_AUTH, &auth),
            ],
        )?;
        c.hset(USERS_LIST, &norm_username, *user_id)?;
        db::sessions::store_session(&auth, &user_id)?;
        Ok(auth.into())
    }
}

pub fn delete_user(auth: &Auth) -> Result<()> {
    let c = db::get_connection()?;
    let user_id = db::sessions::get_user_id(&c, auth)?;
    let user_key = user_key(&user_id);
    let username: String = c.hget(&user_key, USER_NAME)?;
    db::stores::delete_all_user_stores(&auth)?;
    c.hdel(USERS_LIST, username.to_lowercase())?;
    db::sessions::delete_all_user_sessions(auth)?;
    Ok(c.del(&user_key)?)
}

pub fn verify_password(auth_info: &AuthInfo) -> Result<(Token, UserId)> {
    let c = db::get_connection()?;
    let user_id = UserId(
        c.hget(USERS_LIST, &auth_info.username.to_lowercase())
            .or_else(|_| {
                Err(ServerError::new(
                    error::INVALID_USER_OR_PWD,
                    "Invalid usename or password",
                ))
            })?,
    );
    let user_key = user_key(&user_id);
    let salt_pwd: String = c.hget(&user_key, USER_SALT_P)?;
    let stored_pwd: String = c.hget(&user_key, USER_PWD)?;
    let hashed_pwd = hash(&auth_info.password, &salt_pwd);
    if hashed_pwd == stored_pwd {
        let auth: String = c.hget(&user_key, USER_AUTH)?;
        Ok((auth.into(), user_id))
    } else {
        Err(ServerError::new(
            error::INVALID_USER_OR_PWD,
            "Invalid usename or password",
        ))
    }
}

pub fn regen_auth(c: &redis::Connection, user_id: &UserId) -> Result<()> {
    let mut rng = rand::thread_rng();
    c.hset(&user_key(user_id), USER_AUTH, gen_auth(&mut rng))?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::db::tests::*;

    pub fn gen_user() -> user::User {
        user::User {
            username: "toto".to_string(),
            password: "pwd".to_string(),
            email: "m@m.com".to_string(),
        }
    }

    pub fn store_user_for_test() {
        let user = gen_user();
        let res = save_user(&user);
        if res.is_err() {
            dbg!(&res);
        }
        assert_eq!(true, res.is_ok());
    }

    pub fn store_user_for_test_with_reset() {
        reset_db();
        store_user_for_test();
    }

    #[test]
    fn store_user_test() {
        store_user_for_test_with_reset();
        let user = gen_user();
        let c = db::get_connection().unwrap();
        assert_eq!(Ok(true), c.exists("user:1"));
        assert_eq!(Ok(true), c.exists("sessions:1"));
        assert_eq!(Ok(1), c.get("next_user_id"));
        assert_eq!(Ok(true), c.hexists("users", "toto"));
        assert_eq!(Ok(1), c.hget("users", "toto"));

        assert_eq!(
            Ok(true),
            c.hexists(USERS_LIST, &user.username.to_lowercase())
        );
    }

    #[test]
    fn store_user_exists_test() {
        store_user_test();
        let mut user = gen_user();
        let res = save_user(&user);
        if res.is_ok() {
            dbg!(&res);
        }
        assert_eq!(false, res.is_ok());
        user.username = "ToTo".to_string(); // username uniqueness should be case insensitive
        let res = save_user(&user);
        if res.is_ok() {
            dbg!(&res);
        }
        assert_eq!(false, res.is_ok());
    }

    #[test]
    fn login_test() {
        store_user_test();

        let login = AuthInfo {
            username: "toto".to_string(),
            password: "pwd".to_string(),
        };
        let res = verify_password(&login);
        if res.is_err() {
            dbg!(&res);
        }
        assert_eq!(true, res.is_ok());

        let login = AuthInfo {
            username: "toto".to_string(),
            password: "pwdb".to_string(),
        };
        let res = verify_password(&login);
        if res.is_ok() {
            dbg!(&res);
        }
        assert_eq!(false, res.is_ok());

        let login = AuthInfo {
            username: "tato".to_string(),
            password: "pwd".to_string(),
        };
        let res = verify_password(&login);
        if res.is_ok() {
            dbg!(&res);
        }
        assert_eq!(false, res.is_ok());
    }

    #[test]
    fn delete_user_test() {
        store_user_test();
        let c = db::get_connection().unwrap();
        let auth: String = c.hget(&user_key(&UserId(1)), USER_AUTH).unwrap();
        let auth = Auth(&auth);
        assert_eq!(Ok(()), delete_user(&auth));
        let res: bool = c.exists(USERS_LIST).unwrap();
        assert_eq!(false, res);
        store_user_test();
        let mut user = gen_user();
        user.username = "tata".to_string();
        let res = save_user(&user);
        if res.is_err() {
            dbg!(&res);
        }
        assert_eq!(true, res.is_ok());
        let auth: String = c.hget(&user_key(&UserId(1)), USER_AUTH).unwrap();
        let auth = Auth(&auth);
        assert_eq!(Ok(()), delete_user(&auth));
        let res: bool = c.hexists(USERS_LIST, &user.username).unwrap();
        assert_eq!(true, res);
        let res: bool = c.hexists(USERS_LIST, "toto").unwrap();
        assert_eq!(false, res);
        let res: bool = c.exists("user:1").unwrap();
        assert_eq!(false, res);
    }
}
