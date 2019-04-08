use failure::{self, Fail};
use warp::{self, http::StatusCode, Filter, Rejection, Reply};

mod consts;
mod db;
mod error;
mod helpers;
mod session;
mod store;
mod token;
mod types;
mod user;

// Reset the DB, only available in debug compilation
fn nuke() -> Result<impl warp::reply::Reply, warp::reject::Rejection> {
    if cfg!(debug_assertions) {
        let c = db::get_connection().expect("should have connection");
        let _: () = redis::cmd("FLUSHDB").query(&c).expect("error on flush");
        Ok(warp::reply())
    } else {
        Err(warp::reject::not_found())
    }
}

fn main() {
    // GET /nuke
    let nuke = warp::path("nuke").and_then(|| nuke());

    // POST /user
    let create_user = warp::path("user").and(warp::body::json()).and_then(|obj| {
        user::create_user(obj)
            .and_then(|token| Ok(warp::reply::json(&token)))
            .or_else(|e| Err(warp::reject::custom(e.compat())))
    });

    // POST /login
    let login = warp::path("login").and(warp::body::json()).and_then(|obj| {
        session::login(obj)
            .and_then(|token| Ok(warp::reply::json(&token)))
            .or_else(|e| Err(warp::reject::custom(e.compat())))
    });

    // POST /logout
    let logout = warp::path("logout")
        .and(warp::header::<String>("session_token"))
        .and_then(|auth| {
            session::logout(auth)
                .and_then(|()| Ok(warp::reply()))
                .or_else(|e| Err(warp::reject::custom(e.compat())))
        });

    // DELETE /user
    let delete_user = warp::path("user")
        .and(warp::header::<String>("session_token"))
        .and_then(|auth| {
            user::delete_user(auth)
                .and_then(|()| Ok(warp::reply()))
                .or_else(|e| Err(warp::reject::custom(e.compat())))
        });

    // POST /store
    let create_store = warp::path("store")
        .and(warp::body::json())
        .and(warp::header::<String>("session_token"))
        .and_then(|obj, auth| {
            store::create_store(auth, obj)
                .and_then(|store_id| Ok(warp::reply::json(&store_id)))
                .or_else(|e| Err(warp::reject::custom(e.compat())))
        });

    let post_routes = warp::post2()
        .and(create_user)
        .or(login)
        .or(logout)
        .or(create_store)
        .recover(customize_error);
    let get_routes = warp::get2().and(nuke);
    let del_routes = warp::delete2().and(delete_user);

    println!("Efficio's ready for requests...");
    warp::serve(post_routes.or(get_routes).or(del_routes)).run(([127, 0, 0, 1], 3030));
}

fn customize_error(err: Rejection) -> Result<impl Reply, Rejection> {
    if let Some(server_error) = err.find_cause::<failure::Compat<error::ServerError>>() {
        let json = warp::reply::json(&server_error.get_ref().clone());
        Ok(warp::reply::with_status(json, StatusCode::OK))
    } else {
        Err(err)
    }
}
