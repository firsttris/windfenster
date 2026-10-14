//! Who may make an account: everyone, only with an invitation from an admin, or nobody (the
//! admin page's setting). An invitation is a link with a token, used once.

use wf_core::admin::invites;
use wf_core::admin::settings::SignupMode;

use super::hash;
use crate::{ApiError, AppState};

/// Whether an account may be made now, with this invitation if one is needed.
pub(crate) async fn allowed(s: &AppState, invite: Option<&str>) -> Result<(), ApiError> {
    match s.settings.get().signup {
        SignupMode::Open => Ok(()),
        SignupMode::Closed => Err(ApiError::SignupClosed),
        SignupMode::Invite => {
            let token = invite.ok_or(ApiError::SignupClosed)?;
            if invites::usable(&s.db, &hash(token), wf_core::time::now()).await? {
                Ok(())
            } else {
                Err(ApiError::SignupClosed)
            }
        }
    }
}

/// The account was made: its invitation, if it needed one, is used up.
pub(crate) async fn made(
    s: &AppState,
    invite: Option<&str>,
    account_id: i64,
) -> Result<(), ApiError> {
    if s.settings.get().signup == SignupMode::Invite
        && let Some(token) = invite
        && !invites::take(&s.db, &hash(token), account_id, wf_core::time::now()).await?
    {
        // Used by someone else in the meantime: the account goes again
        wf_core::account::delete(&s.db, account_id).await?;
        return Err(ApiError::SignupClosed);
    }
    Ok(())
}
