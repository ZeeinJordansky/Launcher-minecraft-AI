use anyhow::{anyhow, bail, Context, Result};
use crossbeam_channel::Sender;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

const MICROSOFT_DEVICE_CODE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MICROSOFT_TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBOX_USER_AUTH: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XBOX_XSTS: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";
const MINECRAFT_ENTITLEMENTS: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const REFRESH_TOKEN_SERVICE: &str = "minty-launcher-microsoft";
const REFRESH_TOKEN_USER: &str = "primary-account";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub uuid: String,
    pub xuid: String,
    #[serde(skip)]
    pub access_token: String,
}

#[derive(Debug, Clone)]
pub struct DeviceCodeInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum AuthEvent {
    DeviceCode(DeviceCodeInfo),
    Connected(Account),
    Failed(String),
}

#[derive(Deserialize)]
struct OAuthToken {
    access_token: String,
    refresh_token: Option<String>,
    #[allow(dead_code)]
    expires_in: Option<u64>,
}

#[derive(Deserialize)]
struct XboxResponse {
    Token: String,
    DisplayClaims: XboxDisplayClaims,
}

#[derive(Deserialize)]
struct XboxDisplayClaims {
    xui: Vec<XboxUserClaim>,
}

#[derive(Deserialize)]
struct XboxUserClaim {
    uhs: String,
    #[serde(default)]
    xuid: String,
}

#[derive(Deserialize)]
struct MinecraftLoginResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct MinecraftProfile {
    id: String,
    name: String,
}

pub fn login_device(client_id: &str, events: Sender<AuthEvent>) -> Result<Account> {
    let client = http_client()?;
    let device: serde_json::Value = client
        .post(MICROSOFT_DEVICE_CODE)
        .form(&[
            ("client_id", client_id),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()?
        .error_for_status()?
        .json()?;

    let code = device["device_code"].as_str().ok_or_else(|| anyhow!("Microsoft не вернул device_code"))?;
    let user_code = device["user_code"].as_str().unwrap_or_default().to_owned();
    let verification_uri = device["verification_uri"].as_str().unwrap_or("https://microsoft.com/devicelogin").to_owned();
    let message = device["message"].as_str().unwrap_or("Откройте страницу Microsoft и введите код").to_owned();
    let interval = device["interval"].as_u64().unwrap_or(5).max(2);
    let expires_in = device["expires_in"].as_u64().unwrap_or(900);
    events.send(AuthEvent::DeviceCode(DeviceCodeInfo {
        user_code,
        verification_uri,
        message,
    })).ok();

    let deadline = std::time::Instant::now() + Duration::from_secs(expires_in);
    let token = loop {
        if std::time::Instant::now() >= deadline {
            bail!("Истёк срок действия кода Microsoft. Повторите вход.");
        }
        std::thread::sleep(Duration::from_secs(interval));
        let response = client.post(MICROSOFT_TOKEN).form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id),
            ("device_code", code),
        ]).send()?;
        let status = response.status();
        let body: serde_json::Value = response.json().unwrap_or_else(|_| json!({}));
        if status.is_success() {
            break serde_json::from_value::<OAuthToken>(body)?;
        }
        match body["error"].as_str().unwrap_or_default() {
            "authorization_pending" => continue,
            "slow_down" => std::thread::sleep(Duration::from_secs(5)),
            "authorization_declined" => bail!("Вход Microsoft был отклонён."),
            "expired_token" => bail!("Код Microsoft истёк. Повторите вход."),
            "bad_verification_code" => bail!("Microsoft отклонил код подтверждения."),
            other => bail!("Ошибка Microsoft OAuth: {}", if other.is_empty() { body.to_string() } else { other.to_owned() }),
        }
    };

    if let Some(refresh) = token.refresh_token.as_deref() {
        store_refresh_token(refresh)?;
    }
    authenticate_minecraft(&client, &token.access_token)
}

pub fn refresh_saved_account(client_id: &str) -> Result<Account> {
    let refresh = load_refresh_token()?.ok_or_else(|| anyhow!("Сохранённый аккаунт не найден"))?;
    let client = http_client()?;
    let token_response = client.post(MICROSOFT_TOKEN).form(&[
        ("client_id", client_id),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.as_str()),
        ("scope", "XboxLive.signin offline_access"),
    ]).send()?;
    if !token_response.status().is_success() {
        clear_refresh_token();
        bail!("Не удалось обновить сессию Microsoft. Войдите снова.");
    }
    let token: OAuthToken = token_response.json()?;
    if let Some(refresh) = token.refresh_token.as_deref() {
        store_refresh_token(refresh)?;
    }
    authenticate_minecraft(&client, &token.access_token)
}

pub fn sign_out() {
    clear_refresh_token();
}

fn authenticate_minecraft(client: &Client, microsoft_access_token: &str) -> Result<Account> {
    let xbox_user: XboxResponse = client.post(XBOX_USER_AUTH).json(&json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", microsoft_access_token)
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    })).send()?.error_for_status()?.json().context("Xbox Live не принял токен Microsoft")?;

    let xsts: XboxResponse = client.post(XBOX_XSTS).json(&json!({
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbox_user.Token] },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    })).send()?.error_for_status()?.json().context("Xbox XSTS не выдал игровой токен")?;

    let claim = xsts.DisplayClaims.xui.first().ok_or_else(|| anyhow!("Xbox XSTS не вернул данные пользователя"))?;
    let identity = format!("XBL3.0 x={};{}", claim.uhs, xsts.Token);
    let login: MinecraftLoginResponse = client.post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": identity }))
        .send()?.error_for_status().context("Нет доступа к Minecraft Services")?.json()?;

    let entitlement: serde_json::Value = client.get(MINECRAFT_ENTITLEMENTS)
        .bearer_auth(&login.access_token).send()?.error_for_status()?
        .json().context("Не удалось проверить владение Minecraft")?;
    if entitlement["items"].as_array().map(|v| v.is_empty()).unwrap_or(true) {
        bail!("Для этого Microsoft-аккаунта не обнаружена лицензия Minecraft Java Edition.");
    }

    let profile: MinecraftProfile = client.get(MINECRAFT_PROFILE)
        .bearer_auth(&login.access_token).send()?.error_for_status()
        .context("У аккаунта нет профиля Minecraft Java Edition")?.json()?;

    Ok(Account {
        name: profile.name,
        uuid: profile.id,
        xuid: claim.xuid.clone(),
        access_token: login.access_token,
    })
}

fn http_client() -> Result<Client> {
    Ok(Client::builder().user_agent(concat!("MintyLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(90)).build()?)
}

fn store_refresh_token(token: &str) -> Result<()> {
    let entry = keyring::Entry::new(REFRESH_TOKEN_SERVICE, REFRESH_TOKEN_USER)?;
    entry.set_password(token).context("Не удалось сохранить токен в системном хранилище учётных данных Windows")
}

fn load_refresh_token() -> Result<Option<String>> {
    let entry = keyring::Entry::new(REFRESH_TOKEN_SERVICE, REFRESH_TOKEN_USER)?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn clear_refresh_token() {
    if let Ok(entry) = keyring::Entry::new(REFRESH_TOKEN_SERVICE, REFRESH_TOKEN_USER) {
        let _ = entry.delete_credential();
    }
}
