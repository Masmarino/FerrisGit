use std::time::Duration;

use reqwest::{Method, RequestBuilder, Response, StatusCode, redirect};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{CliError, Result};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Asset uploads and downloads go up to 100 MiB, on connections nobody here can vouch for.
pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// A typed client of the REST API. Redirects aren't followed, so a token is only ever sent to the instance it was meant for.
pub struct Client {
    http: reqwest::Client,
    instance: String,
    token: Option<String>,
}

impl Client {
    pub fn new(instance: &str, token: Option<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("fgit/", env!("CARGO_PKG_VERSION")))
            .redirect(redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| CliError::Other(format!("could not set up HTTP: {e}")))?;
        Ok(Self {
            http,
            instance: instance.to_string(),
            token,
        })
    }

    pub fn instance(&self) -> &str {
        &self.instance
    }

    /// `path` starts below `/api`.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let builder = self
            .http
            .request(method, format!("{}/api{path}", self.instance));
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    pub async fn send(&self, request: RequestBuilder) -> Result<Response> {
        let response = request.send().await.map_err(|e| self.unreachable(e))?;
        check(response).await
    }

    fn unreachable(&self, error: reqwest::Error) -> CliError {
        CliError::Server(format!("could not reach {}: {error}", self.instance))
    }

    async fn json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = self.send(request.timeout(REQUEST_TIMEOUT)).await?;
        let text = response.text().await.map_err(|e| self.unreachable(e))?;
        serde_json::from_str(&text).map_err(|e| {
            CliError::Server(format!(
                "unexpected answer from {} (is it the same version as fgit?): {e}",
                self.instance
            ))
        })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.json(self.request(Method::GET, path)).await
    }

    pub async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.json(self.request(Method::POST, path).json(body)).await
    }

    pub async fn patch<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.json(self.request(Method::PATCH, path).json(body))
            .await
    }

    pub async fn put<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.json(self.request(Method::PUT, path).json(body)).await
    }

    /// For the routes that answer with no body, or with one this command doesn't use.
    pub async fn post_empty(&self, path: &str) -> Result<()> {
        self.send(self.request(Method::POST, path).timeout(REQUEST_TIMEOUT))
            .await
            .map(drop)
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        self.send(self.request(Method::DELETE, path).timeout(REQUEST_TIMEOUT))
            .await
            .map(drop)
    }
}

/// Errors are `{"error": "…"}`, except the ones axum raises by itself before a handler runs, which are plain text.
pub(crate) fn error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_string))
        .unwrap_or_else(|| body.trim().chars().take(500).collect())
}

pub(crate) fn error_for(status: StatusCode, message: String) -> CliError {
    let message = crate::output::clean(&message);
    match status.as_u16() {
        401 => CliError::Auth(format!(
            "{message}. Sign in again with `fgit auth login`, or check {}",
            crate::config::TOKEN_ENV
        )),
        404 => CliError::NotFound(format!("not found: {message}")),
        500.. => CliError::Server(format!("the server answered {status}: {message}")),
        _ => CliError::Refused(message),
    }
}

async fn check(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    if status.is_redirection() {
        // Usually http:// on an instance that wants https://. Following it could hand the token to someone else.
        return Err(CliError::Refused(format!(
            "the server answered with a redirect ({status}) to {}. Use that address instead",
            response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("somewhere else")
        )));
    }
    let body = response.text().await.unwrap_or_default();
    Err(error_for(status, error_message(&body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_comes_from_the_error_field_or_the_plain_text() {
        assert_eq!(error_message("{\"error\":\"nope\"}"), "nope");
        assert_eq!(
            error_message("Invalid URL: bad uuid\n"),
            "Invalid URL: bad uuid"
        );
    }

    #[test]
    fn each_status_lands_on_its_exit_code() {
        let code = |status: u16| {
            error_for(StatusCode::from_u16(status).unwrap(), "m".to_string()).exit_code()
        };
        assert_eq!(code(400), 5);
        assert_eq!(code(401), 3);
        assert_eq!(code(403), 5);
        assert_eq!(code(404), 4);
        assert_eq!(code(409), 5);
        assert_eq!(code(422), 5);
        assert_eq!(code(429), 5);
        assert_eq!(code(500), 6);
        assert_eq!(code(503), 6);
    }

    #[test]
    fn a_server_message_can_not_drive_the_terminal() {
        let error = error_for(StatusCode::BAD_REQUEST, "bad\x1b[2J".to_string());
        assert!(!error.to_string().contains('\x1b'));
    }
}
