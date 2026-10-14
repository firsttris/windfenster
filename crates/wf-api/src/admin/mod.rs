//! The admin page's API under `/api/admin`: only for accounts with the admin role, made on the
//! server with `windfenster admin`. Every change an admin makes is written to the admin log.

pub mod users;

use axum::Router;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::routing::{get, post, put};
use wf_core::account::Account;

use crate::auth::SignedIn;
use crate::{ApiError, AppState};

/// A signed-in admin; anyone else is refused (`signed_out` or `forbidden`).
pub struct Admin(pub Account);

impl FromRequestParts<AppState> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, s: &AppState) -> Result<Self, ApiError> {
        let SignedIn(me) = SignedIn::from_request_parts(parts, s).await?;
        if !me.admin {
            return Err(ApiError::Forbidden);
        }
        Ok(Admin(me))
    }
}

impl Admin {
    /// Writes an action to the admin log.
    pub async fn log(
        &self,
        s: &AppState,
        action: &str,
        target: Option<&str>,
    ) -> Result<(), ApiError> {
        wf_core::admin::log::add(&s.db, wf_core::time::now(), Some(self.0.id), action, target)
            .await?;
        Ok(())
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(users::list))
        .route("/users/{id}", get(users::detail).delete(users::delete))
        .route("/users/{id}/role", put(users::role))
        .route("/users/{id}/disable", post(users::disable))
        .route("/users/{id}/enable", post(users::enable))
        .route("/users/{id}/signout", post(users::sign_out))
        .route("/users/{id}/export", get(users::export))
        .route("/users/{id}/confirm-email", post(users::confirm_email))
        .route("/log", get(users::log))
}
