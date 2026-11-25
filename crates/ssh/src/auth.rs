//! Proving who the user is (specs/oneterm, "M7 in detail"): `none` first, then the agent, the usual
//! key files, the password (by `password` or `keyboard-interactive`), by the option `Auth`.

use std::path::Path;
use std::sync::Arc;

use russh::MethodKind;
use russh::client::{AuthResult, Handle, Handler, KeyboardInteractiveAuthResponse};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, load_secret_key};

use crate::prompt::{Ask, Prompt};
use crate::{Auth, Hop, Options};

const KEY_FILES: [&str; 3] = ["id_ed25519", "id_ecdsa", "id_rsa"];

/// Why a connection ends when a question was given up.
pub(crate) const CANCELLED: &str = "cancelled";

pub(crate) async fn authenticate<H: Handler>(
    handle: &mut Handle<H>,
    hop: &Hop,
    options: &Options,
    prompt: &Prompt,
) -> Result<(), String> {
    let user = hop.target.user.as_str();
    // RSA signs with the hash the server names, SHA-512 when it names none.
    let rsa = handle.best_supported_rsa_hash().await.ok().flatten().unwrap_or(Some(HashAlg::Sha512));
    let auth = &hop.auth;
    // As OpenSSH: `none` first. A server that needs nothing of this user (an account with no
    // password) lets it in; else it says what it takes.
    let offered = match handle.authenticate_none(user).await.map_err(|e| e.to_string())? {
        AuthResult::Success => return Ok(()),
        AuthResult::Failure { remaining_methods, .. } => remaining_methods,
    };
    // A server of PAM takes the password as answers to its prompts.
    let interactive =
        offered.contains(&MethodKind::KeyboardInteractive) && !offered.contains(&MethodKind::Password);
    if matches!(auth, Auth::Auto | Auth::Agent) && agent(handle, user, &options.agent, rsa).await {
        return Ok(());
    }
    let files: Vec<_> = match auth {
        Auth::Auto => {
            KEY_FILES.iter().map(|name| options.keys.join(name)).filter(|it| it.is_file()).collect()
        }
        Auth::Key(path) => vec![path.clone()],
        _ => Vec::new(),
    };
    for file in files {
        match key(handle, user, &file, rsa, prompt).await {
            Ok(true) => return Ok(()),
            // Of the usual files, one that does not open is passed over.
            Err(why) if why == CANCELLED || matches!(auth, Auth::Key(_)) => return Err(why),
            _ => {}
        }
    }
    if matches!(auth, Auth::Auto | Auth::Password) {
        for again in [false, true, true] {
            let ask = Ask::Password { target: hop.target.label(), again };
            let password = zeroize::Zeroizing::new(prompt.ask(ask).await.ok_or(CANCELLED)?);
            let signed_in = match interactive {
                true => answer_prompts(handle, user, &password).await,
                false => handle.authenticate_password(user, password.as_str()).await.map(|it| it.success()),
            };
            if signed_in.map_err(|e| e.to_string())? {
                return Ok(());
            }
        }
    }
    Err("permission denied".into())
}

/// `keyboard-interactive`, each prompt answered with the password (an empty one too).
/// ponytail: a second factor asked as a prompt gets the password too; ask each prompt of the
/// user when a server needs that.
async fn answer_prompts<H: Handler>(
    handle: &mut Handle<H>,
    user: &str,
    password: &str,
) -> Result<bool, russh::Error> {
    let mut said = handle.authenticate_keyboard_interactive_start(user, None).await?;
    // A server asks again after answers (an empty request before success is usual); a few
    // rounds at most.
    for _ in 0..4 {
        match said {
            KeyboardInteractiveAuthResponse::Success => return Ok(true),
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers = vec![password.to_string(); prompts.len()];
                said = handle.authenticate_keyboard_interactive_respond(answers).await?;
            }
        }
    }
    Ok(false)
}

/// Each key the agent holds, six at most.
async fn agent<H: Handler>(handle: &mut Handle<H>, user: &str, pipe: &str, rsa: Option<HashAlg>) -> bool {
    let Ok(mut agent) = AgentClient::connect_named_pipe(pipe).await else { return false };
    let Ok(keys) = agent.request_identities().await else { return false };
    for identity in keys.into_iter().take(6) {
        let AgentIdentity::PublicKey { key, .. } = identity else { continue };
        let hash = if key.algorithm().is_rsa() { rsa } else { None };
        if let Ok(AuthResult::Success) = handle.authenticate_publickey_with(user, key, hash, &mut agent).await
        {
            return true;
        }
    }
    false
}

/// A key file, its passphrase asked (three times) when it has one; an error when it does not
/// open.
async fn key<H: Handler>(
    handle: &mut Handle<H>,
    user: &str,
    file: &Path,
    rsa: Option<HashAlg>,
    prompt: &Prompt,
) -> Result<bool, String> {
    let mut key = load_secret_key(file, None);
    if matches!(key, Err(russh::keys::Error::KeyIsEncrypted)) {
        for again in [false, true, true] {
            let ask = Ask::Passphrase { file: file.to_path_buf(), again };
            let passphrase = zeroize::Zeroizing::new(prompt.ask(ask).await.ok_or(CANCELLED)?);
            // Whatever goes wrong with a passphrase given, it was the wrong one.
            if let Ok(opened) = load_secret_key(file, Some(&passphrase)) {
                key = Ok(opened);
                break;
            }
        }
    }
    let key = match key {
        Ok(key) => key,
        Err(russh::keys::Error::KeyIsEncrypted) => {
            return Err(format!("wrong passphrase for {}", file.display()));
        }
        Err(e) => return Err(format!("could not read {}: {e}", file.display())),
    };
    let key = PrivateKeyWithHashAlg::new(Arc::new(key), rsa);
    let said = handle.authenticate_publickey(user, key).await.map_err(|e| e.to_string())?;
    Ok(said.success())
}
