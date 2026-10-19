//! The ceremonies and the account: sign up (a name, then a passkey), sign in (offered in the
//! name field or by button), more passkeys and removing them, sign out, rename, delete.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use webauthn_rs::prelude::{
    DiscoverableKey, Passkey, PublicKeyCredential, RegisterPublicKeyCredential, Uuid,
};
use wf_core::account::passkeys::{self as keys, PasskeySummary, Removal};
use wf_core::account::{self, Account};

use super::{Passkeys, Pending, SignedIn, clear_cookie, hash, start_session, token};
use crate::{ApiError, AppState};

/// Options for the browser's WebAuthn call (`navigator.credentials.create` or `.get`, as
/// webauthn-rs writes them) and the ceremony they belong to.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PasskeyOptions {
    pub ceremony: String,
    #[ts(type = "unknown")]
    pub options: serde_json::Value,
}

/// The browser's answer to a ceremony: the credential as `@simplewebauthn/browser` returns it.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PasskeyAnswer {
    pub ceremony: String,
    #[ts(type = "unknown")]
    pub credential: serde_json::Value,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SignUp {
    pub name: String,
    /// The invitation's token, when sign-up needs one
    #[serde(default)]
    #[ts(optional)]
    pub invite: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SignIn {
    /// Offered in the name field while the page is open (conditional UI), not by a button
    pub autofill: bool,
}

fn passkeys(s: &AppState) -> Result<&Passkeys, ApiError> {
    s.passkeys.as_deref().ok_or(ApiError::PasskeysOff)
}

fn options(ceremony: String, o: impl Serialize) -> Result<Json<PasskeyOptions>, ApiError> {
    Ok(Json(PasskeyOptions {
        ceremony,
        options: serde_json::to_value(o).map_err(ApiError::internal)?,
    }))
}

/// Creation options asking for a discoverable passkey, so the browser can offer it without a
/// name (webauthn-rs asks for none, and does not export the types to say so).
fn discoverable(mut o: Json<PasskeyOptions>) -> Json<PasskeyOptions> {
    o.options["publicKey"]["authenticatorSelection"] = serde_json::json!({
        "residentKey": "required",
        "requireResidentKey": true,
        "userVerification": "required",
    });
    o
}

fn stored(p: &Passkey) -> Result<(String, String), ApiError> {
    let id = hex::encode(p.cred_id().as_ref() as &[u8]);
    Ok((id, serde_json::to_string(p).map_err(ApiError::internal)?))
}

fn parse<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> Result<T, ApiError> {
    serde_json::from_value(v).map_err(|_| ApiError::PasskeyRejected)
}

/// `POST /api/auth/signup`: a name; the options to make a passkey for it.
pub async fn sign_up(
    State(s): State<AppState>,
    Json(req): Json<SignUp>,
) -> Result<Json<PasskeyOptions>, ApiError> {
    let p = passkeys(&s)?;
    let name = account::clean_name(&req.name).ok_or(ApiError::InvalidName)?;
    super::signup::allowed(&s, req.invite.as_deref()).await?;
    let handle = Uuid::new_v4();
    let (ccr, state) = p
        .webauthn
        .start_passkey_registration(handle, &name, &name, None)
        .map_err(ApiError::internal)?;
    let id = p.begin(Pending::SignUp {
        handle,
        name,
        state,
        invite: req.invite,
    });
    options(id.await, ccr).map(discoverable)
}

/// `POST /api/auth/signup/finish`: the new passkey; the account, signed in.
pub async fn finish_sign_up(
    State(s): State<AppState>,
    Json(a): Json<PasskeyAnswer>,
) -> Result<(HeaderMap, Json<Account>), ApiError> {
    let p = passkeys(&s)?;
    let pending = p.take(&a.ceremony).await.ok_or(ApiError::PasskeyRejected)?;
    let Pending::SignUp {
        handle,
        name,
        state,
        invite,
    } = pending.as_ref()
    else {
        return Err(ApiError::PasskeyRejected);
    };
    let cred: RegisterPublicKeyCredential = parse(a.credential)?;
    let passkey = p
        .webauthn
        .finish_passkey_registration(&cred, state)
        .map_err(|_| ApiError::PasskeyRejected)?;
    let (key_id, json) = stored(&passkey)?;
    let now = wf_core::time::now();
    super::signup::allowed(&s, invite.as_deref()).await?;
    let id = account::create(&s.db, &handle.to_string(), name, &key_id, &json, now).await?;
    super::signup::made(&s, invite.as_deref(), id).await?;
    let headers = start_session(&s, id).await?;
    Ok((
        headers,
        Json(Account {
            id,
            name: name.clone(),
            email: None,
            email_confirmed: false,
            passkeys: 1,
            admin: false,
        }),
    ))
}

/// `POST /api/auth/signin`: options to sign in with any passkey of this site.
pub async fn sign_in(
    State(s): State<AppState>,
    Json(req): Json<SignIn>,
) -> Result<Json<PasskeyOptions>, ApiError> {
    let p = passkeys(&s)?;
    let (rcr, state) = p
        .webauthn
        .start_discoverable_authentication()
        .map_err(ApiError::internal)?;
    let id = p.begin(Pending::SignIn(state)).await;
    let mut o = options(id, rcr)?;
    // webauthn-rs asks for the autofill; a button asks with the browser's own dialog.
    if !req.autofill
        && let Some(m) = o.options.as_object_mut()
    {
        m.remove("mediation");
    }
    Ok(o)
}

/// `POST /api/auth/signin/finish`: the passkey's signature; the account, signed in.
pub async fn finish_sign_in(
    State(s): State<AppState>,
    Json(a): Json<PasskeyAnswer>,
) -> Result<(HeaderMap, Json<Account>), ApiError> {
    let p = passkeys(&s)?;
    let pending = p.take(&a.ceremony).await.ok_or(ApiError::PasskeyRejected)?;
    let Pending::SignIn(state) = pending.as_ref() else {
        return Err(ApiError::PasskeyRejected);
    };
    let cred: PublicKeyCredential = parse(a.credential)?;
    let (handle, _) = p
        .webauthn
        .identify_discoverable_authentication(&cred)
        .map_err(|_| ApiError::PasskeyRejected)?;
    let (account_id, stored_keys) = account::by_handle(&s.db, &handle.to_string())
        .await?
        .ok_or(ApiError::PasskeyRejected)?;
    let mut keys: Vec<Passkey> = stored_keys
        .iter()
        .filter_map(|k| serde_json::from_str(k).ok())
        .collect();
    let discoverable: Vec<DiscoverableKey> = keys.iter().map(DiscoverableKey::from).collect();
    let result = p
        .webauthn
        .finish_discoverable_authentication(&cred, state.clone(), &discoverable)
        .map_err(|_| ApiError::PasskeyRejected)?;
    let now = wf_core::time::now();
    for k in keys.iter_mut().filter(|k| k.cred_id() == result.cred_id()) {
        let changed = k.update_credential(&result) == Some(true);
        let (key_id, json) = stored(k)?;
        account::used_passkey(&s.db, &key_id, changed.then_some(json.as_str()), now).await?;
    }
    let headers = start_session(&s, account_id).await?;
    let me = account::get(&s.db, account_id)
        .await?
        .ok_or(ApiError::PasskeyRejected)?;
    Ok((headers, Json(me)))
}

/// `POST /api/auth/passkeys`: options to add a passkey to the signed-in account (another
/// device or password manager).
pub async fn add_passkey(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<PasskeyOptions>, ApiError> {
    let p = passkeys(&s)?;
    let (handle, name, keys) = account::handle_and_passkeys(&s.db, me.id)
        .await?
        .ok_or(ApiError::SignedOut)?;
    let handle = Uuid::parse_str(&handle).map_err(ApiError::internal)?;
    let exclude = keys
        .iter()
        .filter_map(|k| serde_json::from_str::<Passkey>(k).ok())
        .map(|k| k.cred_id().clone())
        .collect();
    let (ccr, state) = p
        .webauthn
        .start_passkey_registration(handle, &name, &name, Some(exclude))
        .map_err(ApiError::internal)?;
    let id = p
        .begin(Pending::AddPasskey {
            account_id: me.id,
            state,
        })
        .await;
    options(id, ccr).map(discoverable)
}

/// `POST /api/auth/passkeys/finish`: the added passkey; the account.
pub async fn finish_add_passkey(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Json(a): Json<PasskeyAnswer>,
) -> Result<Json<Account>, ApiError> {
    let p = passkeys(&s)?;
    let pending = p.take(&a.ceremony).await.ok_or(ApiError::PasskeyRejected)?;
    let Pending::AddPasskey { account_id, state } = pending.as_ref() else {
        return Err(ApiError::PasskeyRejected);
    };
    if *account_id != me.id {
        return Err(ApiError::PasskeyRejected);
    }
    let cred: RegisterPublicKeyCredential = parse(a.credential)?;
    let passkey = p
        .webauthn
        .finish_passkey_registration(&cred, state)
        .map_err(|_| ApiError::PasskeyRejected)?;
    let (key_id, json) = stored(&passkey)?;
    account::add_passkey(&s.db, me.id, &key_id, &json, wf_core::time::now()).await?;
    Ok(Json(Account {
        passkeys: me.passkeys + 1,
        ..me
    }))
}

/// `GET /api/me/passkeys`: the signed-in account's passkeys, oldest first.
pub async fn list_passkeys(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<Vec<PasskeySummary>>, ApiError> {
    Ok(Json(keys::list(&s.db, me.id).await?))
}

/// `DELETE /api/me/passkeys/{id}`: one passkey less; the account. The last way to sign in stays
/// (`last_sign_in`). The passkey stays in its device, it no longer signs in here; sessions it
/// started stay too (signing out is per device).
pub async fn remove_passkey(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Path(id): Path<String>,
) -> Result<Json<Account>, ApiError> {
    match keys::remove(&s.db, me.id, &id).await? {
        Removal::Removed => Ok(Json(Account {
            passkeys: me.passkeys - 1,
            ..me
        })),
        Removal::NotFound => Err(ApiError::NotFound),
        Removal::LastSignIn => Err(ApiError::LastSignIn),
    }
}

/// `GET /api/me`: the signed-in account, or null.
pub async fn me(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let me = match token(&headers) {
        Some(t) => account::of_session(&s.db, &hash(&t), wf_core::time::now()).await?,
        None => None,
    };
    Ok(([(axum::http::header::CACHE_CONTROL, "no-store")], Json(me)))
}

/// `POST /api/auth/signout`: ends this session.
pub async fn sign_out(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    if let Some(t) = token(&headers) {
        account::end_session(&s.db, &hash(&t)).await?;
    }
    Ok((StatusCode::NO_CONTENT, clear_cookie(&s)))
}

/// `PATCH /api/me`: a new name.
pub async fn rename_me(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Json(req): Json<SignUp>,
) -> Result<Json<Account>, ApiError> {
    let name = account::clean_name(&req.name).ok_or(ApiError::InvalidName)?;
    account::rename(&s.db, me.id, &name).await?;
    Ok(Json(Account { name, ..me }))
}

/// `DELETE /api/me`: the account, its passkeys, sessions and favourites. The passkeys stay in
/// the user's devices; they no longer sign in here.
pub async fn delete_me(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<impl IntoResponse, ApiError> {
    account::delete(&s.db, me.id).await?;
    Ok((StatusCode::NO_CONTENT, clear_cookie(&s)))
}
